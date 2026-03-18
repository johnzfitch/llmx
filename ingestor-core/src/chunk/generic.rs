use super::language::{LanguageAdapter, ParseRequest, ParseResult};
use super::legacy::{finalize_chunks, ChunkDraft};
use crate::model::{AstNodeKind, ChunkKind, LanguageId, ResolutionTier};

pub(crate) struct GenericTreeSitterAdapter;

impl LanguageAdapter for GenericTreeSitterAdapter {
    fn language_id(&self) -> Option<LanguageId> {
        None
    }

    fn resolution_tier(&self) -> ResolutionTier {
        ResolutionTier::GenericTreeSitter
    }

    fn supports(&self, request: &ParseRequest<'_>) -> bool {
        request.kind != ChunkKind::JavaScript && request.language.is_some()
    }

    fn parse(&self, request: &ParseRequest<'_>) -> Option<ParseResult> {
        let chunks = parse_generic_chunks(request.path, request.text, request.kind)?;
        Some(ParseResult {
            chunks,
            resolution_tier: self.resolution_tier(),
        })
    }
}

#[cfg(not(feature = "treesitter"))]
fn parse_generic_chunks(_path: &str, _text: &str, _kind: ChunkKind) -> Option<Vec<crate::Chunk>> {
    None
}

#[cfg(feature = "treesitter")]
fn parse_generic_chunks(path: &str, text: &str, kind: ChunkKind) -> Option<Vec<crate::Chunk>> {
    use tree_sitter::Parser;

    let language = select_language(path)?;
    let mut parser = Parser::new();
    parser.set_language(language).ok()?;
    let tree = parser.parse(text, None)?;
    let root = tree.root_node();
    let mut drafts = Vec::new();
    collect_drafts(root, text, kind, None, &mut drafts);
    if drafts.is_empty() {
        return None;
    }
    Some(finalize_chunks(path, drafts))
}

#[cfg(feature = "treesitter")]
fn collect_drafts(
    node: tree_sitter::Node,
    text: &str,
    kind: ChunkKind,
    scope: Option<&str>,
    drafts: &mut Vec<ChunkDraft>,
) {
    let next_scope = scope_name(node, text).or_else(|| extract_symbol_name(node, text));

    if let Some(draft) = draft_from_node(node, text, kind, scope) {
        drafts.push(draft);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_drafts(child, text, kind, next_scope.as_deref().or(scope), drafts);
    }
}

#[cfg(feature = "treesitter")]
fn draft_from_node(
    node: tree_sitter::Node,
    text: &str,
    kind: ChunkKind,
    scope: Option<&str>,
) -> Option<ChunkDraft> {
    let ast_kind = classify_node_kind(node)?;
    let symbol = extract_symbol_name(node, text)?;
    let start_line = node.start_position().row + 1;
    let end_line = node.end_position().row + 1;
    let content = text.get(node.start_byte()..node.end_byte())?.trim().to_string();
    if content.is_empty() {
        return None;
    }

    let qualified_name = scope
        .map(|parent| format!("{parent}::{symbol}"))
        .or_else(|| Some(symbol.clone()));

    let visibility = extract_visibility(node, text);
    let doc_summary = extract_doc_comment(node, text);
    let signature = extract_signature(node, text);
    let module_path = scope.map(str::to_string);
    let symbol_tail = Some(symbol.clone());

    Some(ChunkDraft {
        kind,
        start_line,
        end_line,
        content,
        heading_path: Vec::new(),
        symbol: Some(symbol),
        address: None,
        ast_kind: Some(ast_kind),
        qualified_name,
        signature,
        parent_symbol: scope.map(str::to_string),
        imports: Vec::new(),
        exports: Vec::new(),
        calls: Vec::new(),
        type_refs: Vec::new(),
        doc_summary,
        symbol_id: None,
        symbol_tail,
        module_path,
        visibility,
    })
}

#[cfg(feature = "treesitter")]
fn classify_node_kind(node: tree_sitter::Node) -> Option<AstNodeKind> {
    use AstNodeKind::{Class, Constant, Enum, Function, Interface, Method, Module, Type, Variable};

    match node.kind() {
        "function_item" | "function_definition" | "function_declaration" | "function_definition_item" => Some(Function),
        "method_definition" | "method_declaration" => Some(Method),
        "class_declaration" | "class_definition" | "namespace_declaration" => Some(Class),
        "struct_item" | "struct_specifier" | "type_alias_declaration" | "type_declaration" | "type_spec" => Some(Type),
        "interface_declaration" | "trait_item" => Some(Interface),
        "mod_item" | "module" | "module_declaration" | "namespace_definition" | "namespace_alias_definition" => Some(Module),
        "enum_item" | "enum_declaration" | "enum_specifier" => Some(Enum),
        "const_item" | "const_declaration" => Some(Constant),
        "static_item" | "field_declaration" | "variable_declaration" | "declaration" => Some(Variable),
        "impl_item" => None,
        _ => None,
    }
}

#[cfg(feature = "treesitter")]
fn extract_symbol_name(node: tree_sitter::Node, text: &str) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|name| text.get(name.start_byte()..name.end_byte()))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

#[cfg(feature = "treesitter")]
fn scope_name(node: tree_sitter::Node, text: &str) -> Option<String> {
    match node.kind() {
        "impl_item" => node
            .child_by_field_name("type")
            .and_then(|ty| text.get(ty.start_byte()..ty.end_byte()))
            .map(|ty| ty.trim().to_string())
            .filter(|ty| !ty.is_empty()),
        _ => extract_symbol_name(node, text),
    }
}

#[cfg(feature = "treesitter")]
fn extract_signature(node: tree_sitter::Node, text: &str) -> Option<String> {
    let slice = text.get(node.start_byte()..node.end_byte())?;
    slice
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

#[cfg(feature = "treesitter")]
fn extract_doc_comment(node: tree_sitter::Node, text: &str) -> Option<String> {
    let mut docs = Vec::new();
    let mut cursor = node.prev_sibling();
    while let Some(prev) = cursor {
        if prev.kind() != "comment" {
            break;
        }
        let raw = text.get(prev.start_byte()..prev.end_byte())?.trim();
        let cleaned = raw
            .trim_start_matches("///")
            .trim_start_matches("//!")
            .trim_start_matches("//")
            .trim_start_matches("/**")
            .trim_end_matches("*/")
            .trim_start_matches('*')
            .trim();
        if !cleaned.is_empty() {
            docs.push(cleaned.to_string());
        }
        cursor = prev.prev_sibling();
    }
    docs.reverse();
    let summary = docs.join(" ");
    if summary.is_empty() {
        None
    } else {
        Some(summary.chars().take(200).collect())
    }
}

#[cfg(feature = "treesitter")]
fn extract_visibility(node: tree_sitter::Node, text: &str) -> Option<crate::Visibility> {
    let slice = text.get(node.start_byte()..node.end_byte())?.trim_start();
    if slice.starts_with("pub(crate)") {
        Some(crate::Visibility::Crate)
    } else if slice.starts_with("pub ") || slice.starts_with("pub(") {
        Some(crate::Visibility::Pub)
    } else {
        None
    }
}

#[cfg(feature = "treesitter")]
fn select_language(path: &str) -> Option<tree_sitter::Language> {
    match crate::util::detect_language(path)? {
        LanguageId::Rust => Some(tree_sitter_rust::language()),
        LanguageId::Python => Some(tree_sitter_python::language()),
        LanguageId::Go => Some(tree_sitter_go::language()),
        LanguageId::Java => Some(tree_sitter_java::language()),
        LanguageId::C => Some(tree_sitter_c::language()),
        LanguageId::Cpp => Some(tree_sitter_cpp::language()),
        LanguageId::CSharp => Some(tree_sitter_c_sharp::language()),
        _ => None,
    }
}

#[cfg(all(test, feature = "treesitter"))]
mod tests {
    use crate::chunk::chunk_file;
    use crate::{AstNodeKind, ChunkKind};

    #[test]
    fn rust_function_is_structurally_chunked() {
        let chunks = chunk_file(
            "src/exec.rs",
            "pub fn codex_exec(input: &str) -> bool { !input.is_empty() }",
            ChunkKind::Unknown,
            &crate::IngestOptions::default(),
        );

        let chunk = chunks
            .iter()
            .find(|chunk| chunk.symbol.as_deref() == Some("codex_exec"))
            .expect("codex_exec chunk");

        assert_eq!(chunk.ast_kind, Some(AstNodeKind::Function));
        assert_eq!(chunk.qualified_name.as_deref(), Some("codex_exec"));
        assert_eq!(chunk.visibility, Some(crate::Visibility::Pub));
        assert_eq!(chunk.resolution_tier, crate::ResolutionTier::GenericTreeSitter);
    }

    #[test]
    fn rust_impl_methods_get_parent_scope() {
        let chunks = chunk_file(
            "src/auth.rs",
            "struct Auth; impl Auth { fn login(&self) {} }",
            ChunkKind::Unknown,
            &crate::IngestOptions::default(),
        );

        let method = chunks
            .iter()
            .find(|chunk| chunk.symbol.as_deref() == Some("login"))
            .expect("login chunk");

        assert_eq!(method.ast_kind, Some(AstNodeKind::Function));
        assert_eq!(method.qualified_name.as_deref(), Some("Auth::login"));
    }
}
