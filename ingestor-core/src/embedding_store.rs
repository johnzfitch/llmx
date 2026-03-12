use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const EMBEDDING_MAGIC: &[u8; 8] = b"LLMXEMB2";
const EMBEDDING_VERSION: u32 = 1;
const HEADER_LEN: usize = 8 + 4 + 4 + 4;

pub fn sidecar_path(storage_dir: &Path, index_id: &str) -> PathBuf {
    storage_dir.join(format!("{index_id}.emb.bin"))
}

pub fn write_sidecar(path: &Path, embeddings: Option<&[Vec<f32>]>) -> Result<()> {
    match embeddings {
        Some(embeddings) => {
            let bytes = encode_embeddings(embeddings)?;
            let temp = path.with_file_name(format!(
                "{}.tmp",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("embeddings.bin")
            ));
            fs::write(&temp, bytes).with_context(|| {
                format!("Failed to write embedding sidecar {}", temp.display())
            })?;
            fs::rename(&temp, path).with_context(|| {
                format!(
                    "Failed to rename embedding sidecar {} -> {}",
                    temp.display(),
                    path.display()
                )
            })?;
        }
        None => {
            if path.exists() {
                fs::remove_file(path).with_context(|| {
                    format!("Failed to delete stale embedding sidecar {}", path.display())
                })?;
            }
        }
    }

    Ok(())
}

pub fn read_sidecar(path: &Path) -> Result<Option<Vec<Vec<f32>>>> {
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(path)
        .with_context(|| format!("Failed to read embedding sidecar {}", path.display()))?;
    decode_embeddings(&bytes).map(Some)
}

fn encode_embeddings(embeddings: &[Vec<f32>]) -> Result<Vec<u8>> {
    let count = embeddings.len();
    let dimension = embeddings.first().map(|row| row.len()).unwrap_or(0);

    for (index, row) in embeddings.iter().enumerate() {
        if row.len() != dimension {
            bail!(
                "Embedding row {} has dimension {}, expected {}",
                index,
                row.len(),
                dimension
            );
        }
    }

    let mut bytes = Vec::with_capacity(HEADER_LEN + count * dimension * std::mem::size_of::<f32>());
    bytes.extend_from_slice(EMBEDDING_MAGIC);
    bytes.extend_from_slice(&EMBEDDING_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(count as u32).to_le_bytes());
    bytes.extend_from_slice(&(dimension as u32).to_le_bytes());

    for row in embeddings {
        for value in row {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    Ok(bytes)
}

fn decode_embeddings(bytes: &[u8]) -> Result<Vec<Vec<f32>>> {
    if bytes.len() < HEADER_LEN {
        bail!("Embedding sidecar is truncated");
    }
    if &bytes[..8] != EMBEDDING_MAGIC {
        bail!("Embedding sidecar has invalid magic header");
    }

    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if version != EMBEDDING_VERSION {
        bail!("Unsupported embedding sidecar version {version}");
    }

    let count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let dimension = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let expected = HEADER_LEN + count * dimension * std::mem::size_of::<f32>();
    if bytes.len() != expected {
        bail!(
            "Embedding sidecar length mismatch: expected {} bytes, got {} bytes",
            expected,
            bytes.len()
        );
    }

    let mut embeddings = Vec::with_capacity(count);
    let mut offset = HEADER_LEN;
    for _ in 0..count {
        let mut row = Vec::with_capacity(dimension);
        for _ in 0..dimension {
            let value = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            row.push(value);
            offset += 4;
        }
        embeddings.push(row);
    }

    Ok(embeddings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_embeddings() {
        let embeddings = vec![vec![0.1, 0.2, 0.3], vec![0.4, 0.5, 0.6]];
        let bytes = encode_embeddings(&embeddings).unwrap();
        let decoded = decode_embeddings(&bytes).unwrap();
        assert_eq!(decoded, embeddings);
    }
}
