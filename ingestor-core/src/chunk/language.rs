use crate::model::{Chunk, ChunkKind, IngestOptions, LanguageId, ResolutionTier};

pub(crate) struct ParseRequest<'a> {
    pub path: &'a str,
    pub text: &'a str,
    pub kind: ChunkKind,
    pub options: &'a IngestOptions,
    pub language: Option<LanguageId>,
}

pub(crate) struct ParseResult {
    pub chunks: Vec<Chunk>,
    pub resolution_tier: ResolutionTier,
}

pub(crate) trait LanguageAdapter {
    fn language_id(&self) -> Option<LanguageId>;
    fn resolution_tier(&self) -> ResolutionTier;
    fn supports(&self, request: &ParseRequest<'_>) -> bool;
    fn parse(&self, request: &ParseRequest<'_>) -> Option<ParseResult>;
}
