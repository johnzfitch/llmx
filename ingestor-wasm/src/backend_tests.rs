#[cfg(test)]
mod tests {
    use crate::bert::{BertModel, BertSelfAttention};
    use burn::tensor::{backend::Backend, Bool, Int, Tensor, TensorData};
    #[cfg(feature = "ndarray-backend")]
    use burn_ndarray::{NdArray, NdArrayDevice};
    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    use burn::module::Module;
    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    use burn::record::{FullPrecisionSettings, Recorder};
    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    use burn_import::safetensors::{AdapterType, LoadArgs, SafetensorsFileRecorder};
    #[cfg(feature = "wgpu-backend")]
    use burn_wgpu::{Wgpu, WgpuDevice};
    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    use std::path::Path;
    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    use std::path::PathBuf;

    const HIDDEN_SIZE: usize = 384;
    const NUM_ATTENTION_HEADS: usize = 12;
    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    const SAFETENSORS_PATH: &str = "models/arctic-embed-s.safetensors";
    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    const RUN_WGPU_ENV: &str = "LLMX_RUN_WGPU_TESTS";
    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    const BACKEND_MSE_ENV: &str = "LLMX_BACKEND_MSE_MAX";

    fn make_hidden<B: Backend>(device: &B::Device, batch: usize, seq: usize) -> Tensor<B, 3> {
        let total = batch * seq * HIDDEN_SIZE;
        let data: Vec<f32> = (0..total)
            .map(|idx| idx as f32 / total as f32)
            .collect();
        Tensor::from_data(TensorData::new(data, [batch, seq, HIDDEN_SIZE]), device)
    }

    fn make_attention_mask<B: Backend>(
        device: &B::Device,
        batch: usize,
        seq: usize,
    ) -> Tensor<B, 4, Bool> {
        let total = batch * NUM_ATTENTION_HEADS * seq * seq;
        let data = vec![false; total];
        Tensor::from_data(
            TensorData::new(data, [batch, NUM_ATTENTION_HEADS, seq, seq]),
            device,
        )
    }

    fn make_ids_mask<B: Backend>(
        device: &B::Device,
        batch: usize,
        seq: usize,
    ) -> (Tensor<B, 2, Int>, Tensor<B, 2, Int>) {
        let total = batch * seq;
        let ids: Vec<i64> = (0..total).map(|idx| (idx % 256) as i64 + 1).collect();
        let mask: Vec<i64> = vec![1; total];
        let input_ids = Tensor::<B, 2, Int>::from_ints(TensorData::new(ids, [batch, seq]), device);
        let attention_mask =
            Tensor::<B, 2, Int>::from_ints(TensorData::new(mask, [batch, seq]), device);
        (input_ids, attention_mask)
    }

    #[cfg(feature = "ndarray-backend")]
    #[test]
    fn attention_head_reshape_shapes_ndarray() {
        let device = NdArrayDevice::default();
        let attention = BertSelfAttention::<NdArray<f32>>::new(&device);

        for (batch, seq) in [(1usize, 4usize), (2, 7), (3, 11)] {
            let hidden = make_hidden::<NdArray<f32>>(&device, batch, seq);
            let mask = make_attention_mask::<NdArray<f32>>(&device, batch, seq);
            let output = attention.forward(hidden, &mask);
            let dims = output.dims();
            assert_eq!(
                dims,
                [batch, seq, HIDDEN_SIZE],
                "Unexpected output shape for batch {batch} seq {seq}"
            );
        }
    }

    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    #[test]
    fn wgpu_matches_ndarray_outputs() {
        let flag = std::env::var(RUN_WGPU_ENV).unwrap_or_default().to_ascii_lowercase();
        if flag != "1" && flag != "true" {
            eprintln!("Skipping WGPU backend test; set {RUN_WGPU_ENV}=1 to run.");
            return;
        }

        if !Path::new(SAFETENSORS_PATH).exists() {
            panic!("Missing safetensors at {SAFETENSORS_PATH}");
        }

        let mse_max = std::env::var(BACKEND_MSE_ENV)
            .ok()
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(1e-3);

        let device_cpu = NdArrayDevice::default();
        let device_gpu = WgpuDevice::default();

        let model_cpu = load_model_from_safetensors::<NdArray<f32>>(&device_cpu);
        let model_gpu = load_model_from_safetensors::<Wgpu>(&device_gpu);

        let (input_ids_cpu, mask_cpu) = make_ids_mask::<NdArray<f32>>(&device_cpu, 1, 8);
        let (input_ids_gpu, mask_gpu) = make_ids_mask::<Wgpu>(&device_gpu, 1, 8);

        let output_cpu = model_cpu.forward(input_ids_cpu, mask_cpu);
        let output_gpu = model_gpu.forward(input_ids_gpu, mask_gpu);

        let cpu_vec = output_cpu
            .into_data()
            .to_vec::<f32>()
            .expect("Failed to read NdArray output");
        let gpu_vec = output_gpu
            .into_data()
            .to_vec::<f32>()
            .expect("Failed to read WGPU output");

        assert_eq!(
            cpu_vec.len(),
            gpu_vec.len(),
            "Backend output length mismatch"
        );

        let mse = cpu_vec
            .iter()
            .zip(&gpu_vec)
            .map(|(a, b)| {
                let diff = a - b;
                diff * diff
            })
            .sum::<f32>()
            / cpu_vec.len() as f32;

        assert!(mse.is_finite(), "Backend MSE is not finite");
        assert!(
            mse <= mse_max,
            "Backend MSE {mse:.6} exceeds threshold {mse_max:.6}"
        );
    }

    #[cfg(all(feature = "ndarray-backend", feature = "wgpu-backend"))]
    fn load_model_from_safetensors<B: Backend>(device: &B::Device) -> BertModel<B> {
        let load_args = LoadArgs::new(PathBuf::from(SAFETENSORS_PATH))
            .with_adapter_type(AdapterType::PyTorch)
            .with_key_remap("^bert\\.(.*)$", "$1")
            .with_key_remap("^model\\.(.*)$", "$1")
            .with_key_remap("attention\\.self\\.(.*)$", "attention.self_attn.$1")
            .with_key_remap("^LayerNorm\\.(.*)$", "layer_norm.$1")
            .with_key_remap("\\.LayerNorm\\.", ".layer_norm.");

        let record: <BertModel<B> as Module<B>>::Record =
            SafetensorsFileRecorder::<FullPrecisionSettings>::default()
                .load(load_args, device)
                .expect("Failed to load safetensors record");

        BertModel::new(device).load_record(record)
    }
}
