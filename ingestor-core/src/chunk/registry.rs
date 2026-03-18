use super::generic::GenericTreeSitterAdapter;
use super::language::{LanguageAdapter, ParseRequest, ParseResult};
use crate::model::{ChunkKind, IngestOptions};
use crate::util::detect_language;

struct JavaScriptLegacyAdapter;

impl LanguageAdapter for JavaScriptLegacyAdapter {
    fn language_id(&self) -> Option<crate::LanguageId> {
        None
    }

    fn resolution_tier(&self) -> crate::ResolutionTier {
        crate::ResolutionTier::GenericTreeSitter
    }

    fn supports(&self, request: &ParseRequest<'_>) -> bool {
        request.kind == ChunkKind::JavaScript
    }

    fn parse(&self, request: &ParseRequest<'_>) -> Option<ParseResult> {
        Some(ParseResult {
            chunks: super::legacy::chunk_file(request.path, request.text, request.kind, request.options),
            resolution_tier: self.resolution_tier(),
        })
    }
}

pub(crate) fn parse(path: &str, text: &str, kind: ChunkKind, options: &IngestOptions) -> Option<ParseResult> {
    let request = ParseRequest {
        path,
        text,
        kind,
        options,
        language: detect_language(path),
    };

    let adapters: [&dyn LanguageAdapter; 2] = [&JavaScriptLegacyAdapter, &GenericTreeSitterAdapter];
    for adapter in adapters {
        let _ = adapter.language_id();
        if adapter.supports(&request) {
            if let Some(result) = adapter.parse(&request) {
                return Some(result);
            }
        }
    }

    None
}
