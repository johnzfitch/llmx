#[cfg(test)]
mod tests {
    use crate::bert::BertModel;
    use burn::module::{Module, Quantizer};
    use burn::record::{BinFileRecorder, FullPrecisionSettings, Recorder};
    use burn::tensor::backend::Backend as BurnBackend;
    use burn::tensor::quantization::{
        Calibration, QTensorPrimitive, QuantLevel, QuantParam, QuantValue,
    };
    use burn::tensor::{Int, Tensor, TensorData};
    use burn_ndarray::{NdArray, NdArrayDevice};
    use burn_store::{ModuleSnapshot, PyTorchToBurnAdapter, SafetensorsStore};
    use std::path::PathBuf;

    const BASE_SAFETENSORS_PATH: &str = "models/mdbr-leaf-ir.safetensors";
    const DENSE_SAFETENSORS_PATH: &str = "models/mdbr-leaf-ir-dense.safetensors";
    const MODEL_BIN_PATH: &str = "models/mdbr-leaf-ir.bin";
    const VALIDATE_ENV: &str = "LLMX_VALIDATE_QUANT";
    const MSE_ENV: &str = "LLMX_QUANT_MSE_MAX";
    const BIN_MSE_ENV: &str = "LLMX_BIN_MSE_MAX";

    fn load_model_from_safetensors<B: burn::tensor::backend::Backend>(
        device: &B::Device,
    ) -> BertModel<B> {
        let mut base_store = SafetensorsStore::from_file(PathBuf::from(BASE_SAFETENSORS_PATH))
            .with_from_adapter(PyTorchToBurnAdapter)
            .with_key_remapping("^bert\\.(.*)$", "$1")
            .with_key_remapping("^model\\.(.*)$", "$1")
            .with_key_remapping("attention\\.self\\.(.*)$", "attention.self_attn.$1")
            .with_key_remapping("^LayerNorm\\.(.*)$", "layer_norm.$1")
            .with_key_remapping("\\.LayerNorm\\.", ".layer_norm.")
            .allow_partial(true);

        let mut dense_store = SafetensorsStore::from_file(PathBuf::from(DENSE_SAFETENSORS_PATH))
            .with_from_adapter(PyTorchToBurnAdapter)
            .with_key_remapping("^linear\\.(.*)$", "dense.$1")
            .allow_partial(true);

        let mut model = BertModel::<B>::new(device);
        model
            .load_from(&mut base_store)
            .expect("Failed to load base safetensors model");
        model
            .load_from(&mut dense_store)
            .expect("Failed to load dense safetensors model");
        model
    }

    #[test]
    fn quantized_model_mse_smoke() {
        let flag = std::env::var(VALIDATE_ENV).unwrap_or_default().to_ascii_lowercase();
        if flag != "1" && flag != "true" {
            eprintln!("Skipping quantization validation; set {VALIDATE_ENV}=1 to run.");
            return;
        }

        if !std::path::Path::new(BASE_SAFETENSORS_PATH).exists() {
            panic!("Missing safetensors at {BASE_SAFETENSORS_PATH}");
        }
        if !std::path::Path::new(DENSE_SAFETENSORS_PATH).exists() {
            panic!("Missing dense safetensors at {DENSE_SAFETENSORS_PATH}");
        }

        let mse_max = std::env::var(MSE_ENV)
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.1);

        type Backend = NdArray<f32>;
        let device = NdArrayDevice::default();

        let model_full = load_model_from_safetensors::<Backend>(&device);

        let scheme =
            <<Backend as BurnBackend>::QuantizedTensorPrimitive as QTensorPrimitive>::default_scheme()
                .with_value(QuantValue::Q8S)
                .with_level(QuantLevel::Tensor)
                .with_param(QuantParam::F32);
        let mut quantizer = Quantizer {
            calibration: Calibration::MinMax,
            scheme,
        };
        let model_quant = load_model_from_safetensors::<Backend>(&device);
        let model_quant = model_quant.quantize_weights(&mut quantizer);

        let mut cases: Vec<(&str, Vec<Vec<i64>>, Vec<Vec<i64>>)> = Vec::new();
        cases.push((
            "short-padded",
            vec![vec![101, 2003, 2023, 102, 0, 0, 0, 0]],
            vec![vec![1, 1, 1, 1, 0, 0, 0, 0]],
        ));
        cases.push(("single-token", vec![vec![101]], vec![vec![1]]));

        let medium_ids: Vec<i64> = (0..32).map(|i| 100 + i as i64).collect();
        cases.push((
            "medium-32",
            vec![medium_ids.clone()],
            vec![vec![1; 32]],
        ));

        let long_ids: Vec<i64> = (0..512).map(|i| 100 + (i % 200) as i64).collect();
        cases.push((
            "full-512",
            vec![long_ids],
            vec![vec![1; 512]],
        ));

        cases.push((
            "batch-2",
            vec![
                vec![101, 2024, 2023, 2003, 1037, 4012, 102, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                vec![101, 2070, 3978, 2000, 3980, 102, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            ],
            vec![
                vec![1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                vec![1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            ],
        ));

        for (case_name, batch_ids, batch_masks) in cases {
            let batch_size = batch_ids.len();
            assert_eq!(
                batch_size,
                batch_masks.len(),
                "Case {case_name} mask batch size mismatch"
            );
            let seq_len = batch_ids.first().map(|seq| seq.len()).unwrap_or(0);
            assert!(seq_len > 0, "Case {case_name} has empty sequence");
            for (seq, mask) in batch_ids.iter().zip(batch_masks.iter()) {
                assert_eq!(
                    seq.len(),
                    seq_len,
                    "Case {case_name} has inconsistent sequence lengths"
                );
                assert_eq!(
                    mask.len(),
                    seq_len,
                    "Case {case_name} has inconsistent mask lengths"
                );
            }

            let flat_ids: Vec<i64> = batch_ids.into_iter().flatten().collect();
            let flat_masks: Vec<i64> = batch_masks.into_iter().flatten().collect();

            let input_ids = Tensor::<Backend, 2, Int>::from_ints(
                TensorData::new(flat_ids, [batch_size, seq_len]),
                &device,
            );
            let attention_mask = Tensor::<Backend, 2, Int>::from_ints(
                TensorData::new(flat_masks, [batch_size, seq_len]),
                &device,
            );

            let output_full = model_full.forward(input_ids.clone(), attention_mask.clone());
            let output_quant = model_quant.forward(input_ids, attention_mask);

            let diff = (output_full - output_quant).powf_scalar(2.0);
            let mse = diff.mean().into_scalar();
            assert!(mse.is_finite(), "Case {case_name} MSE is not finite");
            assert!(
                mse <= mse_max,
                "Case {case_name} MSE {mse:.6} exceeds threshold {mse_max:.6}"
            );
        }
    }

    #[test]
    fn build_bin_matches_in_memory_quantization() {
        let flag = std::env::var(VALIDATE_ENV).unwrap_or_default().to_ascii_lowercase();
        if flag != "1" && flag != "true" {
            eprintln!("Skipping quantization validation; set {VALIDATE_ENV}=1 to run.");
            return;
        }

        if !std::path::Path::new(BASE_SAFETENSORS_PATH).exists() {
            panic!("Missing safetensors at {BASE_SAFETENSORS_PATH}");
        }
        if !std::path::Path::new(DENSE_SAFETENSORS_PATH).exists() {
            panic!("Missing dense safetensors at {DENSE_SAFETENSORS_PATH}");
        }
        if !std::path::Path::new(MODEL_BIN_PATH).exists() {
            panic!("Missing Burn bin at {MODEL_BIN_PATH}");
        }

        let mse_max = std::env::var(BIN_MSE_ENV)
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(1e-6);

        type Backend = NdArray<f32>;
        let device = NdArrayDevice::default();

        let scheme =
            <<Backend as BurnBackend>::QuantizedTensorPrimitive as QTensorPrimitive>::default_scheme()
                .with_value(QuantValue::Q8S)
                .with_level(QuantLevel::Tensor)
                .with_param(QuantParam::F32);
        let mut quantizer = Quantizer {
            calibration: Calibration::MinMax,
            scheme,
        };

        let model_quant = load_model_from_safetensors::<Backend>(&device);
        let model_quant = model_quant.quantize_weights(&mut quantizer);

        let record_bin: <BertModel<Backend> as Module<Backend>>::Record =
            BinFileRecorder::<FullPrecisionSettings>::default()
                .load(PathBuf::from(MODEL_BIN_PATH), &device)
                .expect("Failed to load Burn bin record");
        let model_bin = BertModel::<Backend>::new(&device).load_record(record_bin);

        let input_ids = Tensor::<Backend, 2, Int>::from_ints(
            TensorData::new(vec![101, 2003, 2023, 102, 0, 0, 0, 0], [1, 8]),
            &device,
        );
        let attention_mask = Tensor::<Backend, 2, Int>::from_ints(
            TensorData::new(vec![1, 1, 1, 1, 0, 0, 0, 0], [1, 8]),
            &device,
        );

        let out_quant = model_quant.forward(input_ids.clone(), attention_mask.clone());
        let out_bin = model_bin.forward(input_ids, attention_mask);

        let diff = (out_quant - out_bin).powf_scalar(2.0);
        let mse = diff.mean().into_scalar();
        assert!(mse.is_finite(), "Bin MSE is not finite");
        assert!(
            mse <= mse_max,
            "Bin record differs from in-memory quantization: MSE {mse:.9} exceeds threshold {mse_max:.9}"
        );
    }
}
