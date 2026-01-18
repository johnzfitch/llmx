# Burn Code Review Follow-up

## Deployment Checklist

1) Set the environment variable:
```
export LLMX_EMBEDDING_MODEL_URL="https://your-cdn.com/arctic-embed-s-q8.bin"
```

2) Deploy to staging and test across browsers:
- Chrome/Edge (WebGPU)
- Firefox/Safari (CPU fallback)

3) Update SHA-256 hashes if models change:
- Model: /home/zack/dev/llmx/ingestor-wasm/src/model_loader.rs
- Tokenizer: /home/zack/dev/llmx/ingestor-wasm/src/embeddings_burn.rs

## Medium Issues Remaining (Future Optimization)

- Tensor cloning in attention (minor performance)
- Model config flexibility
- Cancellation support

## Quantization Validation (Opt-in Test)

To run the post-build validation test:
```
LLMX_VALIDATE_QUANT=1 cargo test -p ingestor-wasm
```

Optional threshold override:
```
LLMX_VALIDATE_QUANT=1 LLMX_QUANT_MSE_MAX=0.1 cargo test -p ingestor-wasm
```

## Model Config Flexibility Note

The Burn `#[derive(Module)]` macro requires all fields to implement `Module`, so
runtime or generic config fields are not supported without a manual `Module`
implementation or generated model modules per config.

## Recommendation

Deploy to staging for browser testing.
