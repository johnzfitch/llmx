mod generic;
mod language;
mod legacy;
mod registry;
#[cfg(test)]
mod symbol_id;

use crate::model::{Chunk, ChunkKind, IngestOptions};

pub fn chunk_file(path: &str, text: &str, kind: ChunkKind, options: &IngestOptions) -> Vec<Chunk> {
    registry::parse(path, text, kind, options)
        .map(|mut result| {
            for chunk in &mut result.chunks {
                chunk.resolution_tier = result.resolution_tier;
            }
            result.chunks
        })
        .unwrap_or_else(|| legacy::chunk_file(path, text, kind, options))
}
