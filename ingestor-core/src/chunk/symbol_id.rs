use crate::model::{AstNodeKind, LanguageId};

pub(crate) fn make_symbol_id(
    language: &LanguageId,
    relative_path: &str,
    qualified_name: &str,
    ast_kind: Option<AstNodeKind>,
) -> String {
    let lang = language_label(language);
    let suffix = symbol_suffix(ast_kind);
    format!("generic {lang} {relative_path}/{qualified_name}{suffix}")
}

fn language_label(language: &LanguageId) -> String {
    match language {
        LanguageId::TypeScript => "typescript".to_string(),
        LanguageId::JavaScript => "javascript".to_string(),
        LanguageId::CSharp => "csharp".to_string(),
        LanguageId::Other(other) => other.clone(),
        other => format!("{:?}", other).to_ascii_lowercase(),
    }
}

fn symbol_suffix(ast_kind: Option<AstNodeKind>) -> &'static str {
    match ast_kind {
        Some(AstNodeKind::Function | AstNodeKind::Method | AstNodeKind::Test) => "().",
        _ => ".",
    }
}

#[cfg(test)]
mod tests {
    use super::make_symbol_id;
    use crate::{AstNodeKind, LanguageId};

    #[test]
    fn function_ids_use_callable_suffix() {
        let symbol_id = make_symbol_id(
            &LanguageId::Rust,
            "src/exec.rs",
            "codex_exec",
            Some(AstNodeKind::Function),
        );
        assert_eq!(symbol_id, "generic rust src/exec.rs/codex_exec().");
    }

    #[test]
    fn type_ids_use_plain_suffix() {
        let symbol_id = make_symbol_id(
            &LanguageId::Rust,
            "src/auth.rs",
            "Auth",
            Some(AstNodeKind::Type),
        );
        assert_eq!(symbol_id, "generic rust src/auth.rs/Auth.");
    }
}
