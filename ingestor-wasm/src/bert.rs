use burn::module::Module;
use burn::nn::{
    Dropout, DropoutConfig, Embedding, EmbeddingConfig, Gelu, LayerNorm, LayerNormConfig, Linear,
    LinearConfig,
};
use burn::tensor::module::attention;
use burn::tensor::{backend::Backend, Bool, Int, Tensor};

const VOCAB_SIZE: usize = 30_522;
const HIDDEN_SIZE: usize = 384;
const NUM_ATTENTION_HEADS: usize = 12;
const NUM_HIDDEN_LAYERS: usize = 12;
const INTERMEDIATE_SIZE: usize = 1_536;
const MAX_POSITION_EMBEDDINGS: usize = 512;
const TYPE_VOCAB_SIZE: usize = 2;
const LAYER_NORM_EPS: f64 = 1e-12;
const DROPOUT_PROB: f64 = 0.1;

pub type Model<B> = BertModel<B>;

#[derive(Module, Debug)]
pub struct BertEmbeddings<B: Backend> {
    pub word_embeddings: Embedding<B>,
    pub position_embeddings: Embedding<B>,
    pub token_type_embeddings: Embedding<B>,
    pub layer_norm: LayerNorm<B>,
    pub dropout: Dropout,
}

impl<B: Backend> BertEmbeddings<B> {
    pub fn new(device: &B::Device) -> Self {
        let word_embeddings = EmbeddingConfig::new(VOCAB_SIZE, HIDDEN_SIZE).init(device);
        let position_embeddings =
            EmbeddingConfig::new(MAX_POSITION_EMBEDDINGS, HIDDEN_SIZE).init(device);
        let token_type_embeddings = EmbeddingConfig::new(TYPE_VOCAB_SIZE, HIDDEN_SIZE).init(device);
        let layer_norm = LayerNormConfig::new(HIDDEN_SIZE)
            .with_epsilon(LAYER_NORM_EPS)
            .init(device);
        let dropout = DropoutConfig::new(DROPOUT_PROB).init();

        Self {
            word_embeddings,
            position_embeddings,
            token_type_embeddings,
            layer_norm,
            dropout,
        }
    }

    pub fn forward(
        &self,
        input_ids: Tensor<B, 2, Int>,
        token_type_ids: Tensor<B, 2, Int>,
    ) -> Tensor<B, 3> {
        let [batch_size, seq_len] = input_ids.dims();
        let device = input_ids.device();

        let position_ids = Tensor::<B, 1, Int>::arange(0..seq_len as i64, &device)
            .unsqueeze::<2>()
            .expand([batch_size, seq_len]);

        let word_embeddings = self.word_embeddings.forward(input_ids);
        let position_embeddings = self.position_embeddings.forward(position_ids);
        let token_type_embeddings = self.token_type_embeddings.forward(token_type_ids);

        let embeddings = word_embeddings + position_embeddings + token_type_embeddings;
        let embeddings = self.layer_norm.forward(embeddings);
        self.dropout.forward(embeddings)
    }
}

#[derive(Module, Debug)]
pub struct BertSelfAttention<B: Backend> {
    pub query: Linear<B>,
    pub key: Linear<B>,
    pub value: Linear<B>,
    pub dropout: Dropout,
}

impl<B: Backend> BertSelfAttention<B> {
    pub fn new(device: &B::Device) -> Self {
        let query = LinearConfig::new(HIDDEN_SIZE, HIDDEN_SIZE).init(device);
        let key = LinearConfig::new(HIDDEN_SIZE, HIDDEN_SIZE).init(device);
        let value = LinearConfig::new(HIDDEN_SIZE, HIDDEN_SIZE).init(device);
        let dropout = DropoutConfig::new(DROPOUT_PROB).init();

        Self {
            query,
            key,
            value,
            dropout,
        }
    }

    pub fn forward(
        &self,
        hidden: Tensor<B, 3>,
        attention_mask: &Tensor<B, 4, Bool>,
    ) -> Tensor<B, 3> {
        let [batch_size, seq_len, _] = hidden.dims();
        let head_dim = HIDDEN_SIZE / NUM_ATTENTION_HEADS;

        let query = self.query.forward(hidden.clone());
        let key = self.key.forward(hidden.clone());
        let value = self.value.forward(hidden);

        let query = query
            .reshape([batch_size, seq_len, NUM_ATTENTION_HEADS, head_dim])
            .swap_dims(1, 2);
        let key = key
            .reshape([batch_size, seq_len, NUM_ATTENTION_HEADS, head_dim])
            .swap_dims(1, 2);
        let value = value
            .reshape([batch_size, seq_len, NUM_ATTENTION_HEADS, head_dim])
            .swap_dims(1, 2);

        let context = attention(query, key, value, Some(attention_mask.clone()));
        context
            .swap_dims(1, 2)
            .reshape([batch_size, seq_len, HIDDEN_SIZE])
    }
}

#[derive(Module, Debug)]
pub struct BertSelfOutput<B: Backend> {
    pub dense: Linear<B>,
    pub layer_norm: LayerNorm<B>,
    pub dropout: Dropout,
}

impl<B: Backend> BertSelfOutput<B> {
    pub fn new(device: &B::Device) -> Self {
        let dense = LinearConfig::new(HIDDEN_SIZE, HIDDEN_SIZE).init(device);
        let layer_norm = LayerNormConfig::new(HIDDEN_SIZE)
            .with_epsilon(LAYER_NORM_EPS)
            .init(device);
        let dropout = DropoutConfig::new(DROPOUT_PROB).init();

        Self {
            dense,
            layer_norm,
            dropout,
        }
    }

    pub fn forward(&self, hidden: Tensor<B, 3>, input: Tensor<B, 3>) -> Tensor<B, 3> {
        let hidden = self.dense.forward(hidden);
        let hidden = self.dropout.forward(hidden);
        let hidden = hidden + input;
        self.layer_norm.forward(hidden)
    }
}

#[derive(Module, Debug)]
pub struct BertAttention<B: Backend> {
    pub self_attn: BertSelfAttention<B>,
    pub output: BertSelfOutput<B>,
}

impl<B: Backend> BertAttention<B> {
    pub fn new(device: &B::Device) -> Self {
        Self {
            self_attn: BertSelfAttention::new(device),
            output: BertSelfOutput::new(device),
        }
    }

    pub fn forward(
        &self,
        hidden: Tensor<B, 3>,
        attention_mask: &Tensor<B, 4, Bool>,
    ) -> Tensor<B, 3> {
        let context = self.self_attn.forward(hidden.clone(), attention_mask);
        self.output.forward(context, hidden)
    }
}

#[derive(Module, Debug)]
pub struct BertIntermediate<B: Backend> {
    pub dense: Linear<B>,
    pub intermediate_act_fn: Gelu,
}

impl<B: Backend> BertIntermediate<B> {
    pub fn new(device: &B::Device) -> Self {
        let dense = LinearConfig::new(HIDDEN_SIZE, INTERMEDIATE_SIZE).init(device);
        let intermediate_act_fn = Gelu::new();

        Self {
            dense,
            intermediate_act_fn,
        }
    }

    pub fn forward(&self, hidden: Tensor<B, 3>) -> Tensor<B, 3> {
        let hidden = self.dense.forward(hidden);
        self.intermediate_act_fn.forward(hidden)
    }
}

#[derive(Module, Debug)]
pub struct BertOutput<B: Backend> {
    pub dense: Linear<B>,
    pub layer_norm: LayerNorm<B>,
    pub dropout: Dropout,
}

impl<B: Backend> BertOutput<B> {
    pub fn new(device: &B::Device) -> Self {
        let dense = LinearConfig::new(INTERMEDIATE_SIZE, HIDDEN_SIZE).init(device);
        let layer_norm = LayerNormConfig::new(HIDDEN_SIZE)
            .with_epsilon(LAYER_NORM_EPS)
            .init(device);
        let dropout = DropoutConfig::new(DROPOUT_PROB).init();

        Self {
            dense,
            layer_norm,
            dropout,
        }
    }

    pub fn forward(&self, hidden: Tensor<B, 3>, input: Tensor<B, 3>) -> Tensor<B, 3> {
        let hidden = self.dense.forward(hidden);
        let hidden = self.dropout.forward(hidden);
        let hidden = hidden + input;
        self.layer_norm.forward(hidden)
    }
}

#[derive(Module, Debug)]
pub struct BertLayer<B: Backend> {
    pub attention: BertAttention<B>,
    pub intermediate: BertIntermediate<B>,
    pub output: BertOutput<B>,
}

impl<B: Backend> BertLayer<B> {
    pub fn new(device: &B::Device) -> Self {
        Self {
            attention: BertAttention::new(device),
            intermediate: BertIntermediate::new(device),
            output: BertOutput::new(device),
        }
    }

    pub fn forward(
        &self,
        hidden: Tensor<B, 3>,
        attention_mask: &Tensor<B, 4, Bool>,
    ) -> Tensor<B, 3> {
        let attention_output = self.attention.forward(hidden, attention_mask);
        let intermediate_output = self.intermediate.forward(attention_output.clone());
        self.output.forward(intermediate_output, attention_output)
    }
}

#[derive(Module, Debug)]
pub struct BertEncoder<B: Backend> {
    pub layer: Vec<BertLayer<B>>,
}

impl<B: Backend> BertEncoder<B> {
    pub fn new(device: &B::Device) -> Self {
        let layer = (0..NUM_HIDDEN_LAYERS)
            .map(|_| BertLayer::new(device))
            .collect();
        Self { layer }
    }

    pub fn forward(
        &self,
        hidden: Tensor<B, 3>,
        attention_mask: &Tensor<B, 4, Bool>,
    ) -> Tensor<B, 3> {
        let mut hidden = hidden;
        for layer in self.layer.iter() {
            hidden = layer.forward(hidden, attention_mask);
        }
        hidden
    }
}

#[derive(Module, Debug)]
pub struct BertModel<B: Backend> {
    pub embeddings: BertEmbeddings<B>,
    pub encoder: BertEncoder<B>,
}

impl<B: Backend> BertModel<B> {
    pub fn new(device: &B::Device) -> Self {
        debug_assert_eq!(HIDDEN_SIZE % NUM_ATTENTION_HEADS, 0);

        Self {
            embeddings: BertEmbeddings::new(device),
            encoder: BertEncoder::new(device),
        }
    }

    pub fn forward(
        &self,
        input_ids: Tensor<B, 2, Int>,
        attention_mask: Tensor<B, 2, Int>,
    ) -> Tensor<B, 3> {
        let [batch_size, seq_len] = input_ids.dims();
        assert!(batch_size > 0, "Batch size must be greater than zero");
        assert!(seq_len > 0, "Sequence length must be greater than zero");
        assert!(
            seq_len <= MAX_POSITION_EMBEDDINGS,
            "Sequence length {seq_len} exceeds maximum {MAX_POSITION_EMBEDDINGS}"
        );
        assert_eq!(
            attention_mask.dims(),
            [batch_size, seq_len],
            "Attention mask shape must match input ids"
        );
        let device = input_ids.device();
        let token_type_ids = Tensor::<B, 2, Int>::zeros([batch_size, seq_len], &device);
        let embedding_output = self.embeddings.forward(input_ids, token_type_ids);
        let attention_mask = build_attention_mask(attention_mask);
        self.encoder.forward(embedding_output, &attention_mask)
    }
}

fn build_attention_mask<B: Backend>(attention_mask: Tensor<B, 2, Int>) -> Tensor<B, 4, Bool> {
    let mask = attention_mask.bool().bool_not();
    mask.unsqueeze_dim::<3>(1).unsqueeze_dim::<4>(2)
}
