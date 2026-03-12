//! Phase 7: Query intelligence — intent classification, synonym expansion, and adaptive routing.
//!
//! This module classifies incoming queries by intent (symbol lookup, semantic question,
//! keyword grep) and adjusts search engine weights accordingly. No ML required — pure
//! pattern matching against query structure.

use crate::model::QueryIntent;

/// Weights for each search engine, keyed by engine name.
/// Higher weight = more influence in RRF fusion.
#[derive(Debug, Clone)]
pub struct EngineWeights {
    pub bm25: f32,
    pub dense: f32,
    pub symbol: f32,
}

impl Default for EngineWeights {
    fn default() -> Self {
        Self {
            bm25: 1.0,
            dense: 1.0,
            symbol: 0.5,
        }
    }
}

// ── Intent Classification ───────────────────────────────────────────────────

/// Classify a query into an intent category using structural heuristics.
///
/// Rules (evaluated in order):
/// 1. If it looks like a symbol name → Symbol
/// 2. If it's natural language (stopwords, sentence structure) → Semantic
/// 3. Otherwise → Keyword
pub fn classify_intent(query: &str) -> QueryIntent {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return QueryIntent::Keyword;
    }

    if looks_like_symbol(trimmed) {
        return QueryIntent::Symbol;
    }

    if is_natural_language(trimmed) {
        return QueryIntent::Semantic;
    }

    QueryIntent::Keyword
}

/// Check if a query looks like a code symbol.
///
/// Matches: camelCase, snake_case, PascalCase, qualified::names, Foo.bar,
/// and patterns with no spaces that contain separators.
fn looks_like_symbol(q: &str) -> bool {
    // No spaces → could be a symbol
    let has_spaces = q.contains(' ');

    if !has_spaces {
        // Qualified names: foo::bar, foo.bar, foo/bar
        if q.contains("::") || (q.contains('.') && !q.ends_with('.')) {
            return true;
        }
        // snake_case with at least one underscore between alnum chars
        if q.contains('_') && q.chars().filter(|c| *c == '_').count() >= 1 {
            let parts: Vec<&str> = q.split('_').collect();
            if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty()) {
                return true;
            }
        }
        // camelCase: lowercase followed by uppercase somewhere in the string
        if has_camel_case_boundary(q) {
            return true;
        }
        // Single token that's all alphanumeric and starts with uppercase (PascalCase type name)
        if q.len() >= 2
            && q.chars().next().map_or(false, |c| c.is_ascii_uppercase())
            && q.chars().skip(1).any(|c| c.is_ascii_lowercase())
            && q.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return true;
        }
    }

    false
}

/// Check if there's a camelCase boundary (lowercase→uppercase) in the string.
fn has_camel_case_boundary(s: &str) -> bool {
    let bytes = s.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i - 1].is_ascii_lowercase() && bytes[i].is_ascii_uppercase() {
            return true;
        }
    }
    false
}

/// Common English stopwords that signal natural language.
const STOPWORDS: &[&str] = &[
    "how", "what", "where", "when", "why", "which", "who", "does", "is", "are",
    "the", "a", "an", "in", "on", "at", "to", "for", "of", "with", "by",
    "from", "that", "this", "it", "do", "can", "should", "would", "could",
    "find", "show", "list", "get", "all", "any", "about",
];

/// Check if a query is natural language (has stopwords, multiple words, sentence structure).
fn is_natural_language(q: &str) -> bool {
    let words: Vec<&str> = q.split_whitespace().collect();

    // Need at least 2 words for natural language
    if words.len() < 2 {
        return false;
    }

    // Check for stopwords
    let stopword_count = words
        .iter()
        .filter(|w| STOPWORDS.contains(&w.to_ascii_lowercase().as_str()))
        .count();

    // If ≥1 stopword in a 2-word query, or ≥2 in longer queries → natural language
    if words.len() <= 3 && stopword_count >= 1 {
        return true;
    }
    if stopword_count >= 2 {
        return true;
    }

    // Starts with a question word
    if let Some(first) = words.first() {
        let lower = first.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "how" | "what" | "where" | "when" | "why" | "which" | "who" | "does" | "is" | "are"
                | "can" | "should" | "find" | "show"
        ) {
            return true;
        }
    }

    false
}

// ── Engine Weight Routing ───────────────────────────────────────────────────

/// Get engine weights for a given intent.
///
/// These weights are applied as multipliers in RRF fusion:
/// - Higher weight → more influence on final ranking
/// - Symbol intent → heavily weights symbol search
/// - Semantic intent → heavily weights dense embeddings
/// - Keyword intent → heavily weights BM25
pub fn weights_for_intent(intent: QueryIntent) -> EngineWeights {
    match intent {
        QueryIntent::Symbol => EngineWeights {
            bm25: 0.3,
            dense: 0.2,
            symbol: 2.0,
        },
        QueryIntent::Semantic => EngineWeights {
            bm25: 0.5,
            dense: 1.5,
            symbol: 0.3,
        },
        QueryIntent::Keyword => EngineWeights {
            bm25: 1.5,
            dense: 0.3,
            symbol: 0.5,
        },
        QueryIntent::Auto => EngineWeights::default(),
    }
}

// ── Synonym Expansion ───────────────────────────────────────────────────────

/// Code-specific synonym table.
///
/// Maps common abbreviations to their full forms. Applied at query time
/// before BM25 scoring to bridge vocabulary gaps.
static SYNONYMS: &[(&str, &[&str])] = &[
    ("auth", &["authentication", "authorize", "login", "signin"]),
    ("authn", &["authentication"]),
    ("authz", &["authorization"]),
    ("config", &["configuration", "settings", "options", "preferences"]),
    ("cfg", &["config", "configuration"]),
    ("db", &["database", "storage", "persistence", "datastore"]),
    ("err", &["error", "exception", "failure", "fault"]),
    ("req", &["request", "http"]),
    ("res", &["response", "reply", "result"]),
    ("resp", &["response"]),
    ("fn", &["function"]),
    ("func", &["function"]),
    ("impl", &["implementation", "implements"]),
    ("init", &["initialize", "initialization", "setup"]),
    ("msg", &["message"]),
    ("param", &["parameter", "argument"]),
    ("params", &["parameters", "arguments"]),
    ("arg", &["argument", "parameter"]),
    ("args", &["arguments", "parameters"]),
    ("ret", &["return", "result"]),
    ("val", &["value"]),
    ("var", &["variable"]),
    ("env", &["environment"]),
    ("dir", &["directory", "folder"]),
    ("fs", &["filesystem", "file"]),
    ("io", &["input", "output"]),
    ("ctx", &["context"]),
    ("cb", &["callback"]),
    ("async", &["asynchronous"]),
    ("sync", &["synchronous"]),
    ("chan", &["channel"]),
    ("conn", &["connection"]),
    ("sess", &["session"]),
    ("tok", &["token"]),
    ("str", &["string"]),
    ("buf", &["buffer"]),
    ("len", &["length"]),
    ("idx", &["index"]),
    ("iter", &["iterator", "iterate"]),
    ("num", &["number"]),
    ("fmt", &["format"]),
    ("util", &["utility", "utilities"]),
    ("utils", &["utilities"]),
    ("cmd", &["command"]),
    ("exec", &["execute", "execution"]),
    ("proc", &["process"]),
    ("pkg", &["package"]),
    ("dep", &["dependency"]),
    ("deps", &["dependencies"]),
    ("lib", &["library"]),
    ("src", &["source"]),
    ("dst", &["destination"]),
    ("srv", &["server", "service"]),
    ("svc", &["service"]),
    ("api", &["interface", "endpoint"]),
    ("ui", &["interface", "frontend"]),
    ("ux", &["experience"]),
    ("tx", &["transaction", "transmit"]),
    ("rx", &["receive"]),
    ("mux", &["multiplexer"]),
    ("tls", &["transport", "security", "ssl"]),
    ("jwt", &["token", "authentication"]),
    ("oauth", &["authentication", "authorization"]),
    ("http", &["request", "web"]),
    ("ws", &["websocket"]),
    ("rpc", &["remote", "call"]),
    ("grpc", &["remote", "call"]),
    ("orm", &["database", "model"]),
    ("crud", &["create", "read", "update", "delete"]),
    ("ssr", &["server", "rendering"]),
    ("csr", &["client", "rendering"]),
];

/// Expand a query by adding synonyms for known code abbreviations.
///
/// Returns a list of additional terms to search for alongside the original query.
/// Does NOT replace the original terms — only adds alternatives.
pub fn expand_synonyms(query: &str) -> Vec<String> {
    let words: Vec<String> = query
        .split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .collect();

    let mut expansions = Vec::new();

    for word in &words {
        for &(abbrev, full_forms) in SYNONYMS {
            if word == abbrev {
                for &full in full_forms {
                    if !words.contains(&full.to_string()) {
                        expansions.push(full.to_string());
                    }
                }
            }
        }
    }

    expansions
}

/// Generate symbol name variations for cross-convention matching.
///
/// "verifyToken" → ["verify_token", "VerifyToken", "VERIFY_TOKEN"]
/// "verify_token" → ["verifyToken", "VerifyToken", "VERIFY_TOKEN"]
pub fn symbol_variations(symbol: &str) -> Vec<String> {
    let mut variations = Vec::new();

    if has_camel_case_boundary(symbol) {
        // camelCase/PascalCase → snake_case
        let snake = camel_to_snake(symbol);
        if snake != symbol {
            variations.push(snake.clone());
            variations.push(snake.to_ascii_uppercase());
        }
    } else if symbol.contains('_') {
        // snake_case → camelCase and PascalCase
        let camel = snake_to_camel(symbol);
        let pascal = snake_to_pascal(symbol);
        if camel != symbol {
            variations.push(camel);
        }
        if pascal != symbol {
            variations.push(pascal);
        }
        variations.push(symbol.to_ascii_uppercase());
    }

    variations
}

fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b.is_ascii_uppercase() {
            if i > 0
                && bytes[i - 1].is_ascii_lowercase()
            {
                out.push('_');
            } else if i > 0
                && i + 1 < bytes.len()
                && bytes[i - 1].is_ascii_uppercase()
                && bytes[i + 1].is_ascii_lowercase()
            {
                out.push('_');
            }
            out.push(b.to_ascii_lowercase() as char);
        } else {
            out.push(b as char);
        }
    }
    out
}

fn snake_to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut capitalize_next = false;
    for (i, ch) in s.chars().enumerate() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            out.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else if i == 0 {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn snake_to_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            out.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

// ── Match Explanation ───────────────────────────────────────────────────────

/// Generate a human-readable explanation of why a result matched.
pub fn explain_match(
    engines: &[(&str, f32)], // (engine_name, score_from_that_engine)
    symbol: Option<&str>,
    query: &str,
) -> String {
    if engines.is_empty() {
        return String::new();
    }

    // Find the primary contributing engine
    let (primary_engine, primary_score) = engines
        .iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap();

    let secondary: Vec<&&str> = engines
        .iter()
        .filter(|(name, _)| name != primary_engine)
        .filter(|(_, score)| *score > 0.0)
        .map(|(name, _)| name)
        .collect();

    let mut reason = match *primary_engine {
        "symbol" => {
            if let Some(sym) = symbol {
                format!("Symbol match: {}", sym)
            } else {
                "Symbol match".to_string()
            }
        }
        "dense" => format!("Semantic similarity: {:.2}", primary_score),
        "bm25" => format!("Keyword match for '{}'", truncate_query(query, 40)),
        other => format!("{} match", other),
    };

    if !secondary.is_empty() {
        let also = secondary
            .iter()
            .map(|s| **s)
            .collect::<Vec<&str>>()
            .join(", ");
        reason.push_str(&format!(" (also matched: {})", also));
    }

    reason
}

fn truncate_query(q: &str, max: usize) -> String {
    if q.len() <= max {
        q.to_string()
    } else {
        format!("{}...", &q[..max])
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_symbol_queries() {
        assert_eq!(classify_intent("getUserById"), QueryIntent::Symbol);
        assert_eq!(classify_intent("verify_token"), QueryIntent::Symbol);
        assert_eq!(classify_intent("auth::jwt::Claims"), QueryIntent::Symbol);
        assert_eq!(classify_intent("Foo.bar"), QueryIntent::Symbol);
        assert_eq!(classify_intent("MyClass"), QueryIntent::Symbol);
        assert_eq!(classify_intent("HTMLParser"), QueryIntent::Symbol);
    }

    #[test]
    fn test_classify_semantic_queries() {
        assert_eq!(classify_intent("how does authentication work"), QueryIntent::Semantic);
        assert_eq!(classify_intent("what is the error handling strategy"), QueryIntent::Semantic);
        assert_eq!(classify_intent("find all database connections"), QueryIntent::Semantic);
        assert_eq!(classify_intent("where is the config loaded"), QueryIntent::Semantic);
    }

    #[test]
    fn test_classify_keyword_queries() {
        assert_eq!(classify_intent("TODO"), QueryIntent::Keyword);
        assert_eq!(classify_intent("FIXME"), QueryIntent::Keyword);
        assert_eq!(classify_intent("unsafe"), QueryIntent::Keyword);
        assert_eq!(classify_intent("error handling"), QueryIntent::Keyword);
    }

    #[test]
    fn test_synonym_expansion() {
        let expansions = expand_synonyms("auth error");
        assert!(expansions.contains(&"authentication".to_string()));
        assert!(expansions.contains(&"login".to_string()));
        // "error" has no entry in SYNONYMS, so no expansion for it
    }

    #[test]
    fn test_symbol_variations() {
        let vars = symbol_variations("verifyToken");
        assert!(vars.contains(&"verify_token".to_string()));

        let vars = symbol_variations("verify_token");
        assert!(vars.contains(&"verifyToken".to_string()));
        assert!(vars.contains(&"VerifyToken".to_string()));
    }

    #[test]
    fn test_camel_to_snake() {
        assert_eq!(camel_to_snake("getUserById"), "get_user_by_id");
        assert_eq!(camel_to_snake("HTMLParser"), "html_parser");
        assert_eq!(camel_to_snake("XMLHttpRequest"), "xml_http_request");
    }

    #[test]
    fn test_snake_to_camel() {
        assert_eq!(snake_to_camel("get_user_by_id"), "getUserById");
        assert_eq!(snake_to_camel("html_parser"), "htmlParser");
    }

    #[test]
    fn test_weights_for_intent() {
        let w = weights_for_intent(QueryIntent::Symbol);
        assert!(w.symbol > w.bm25);
        assert!(w.symbol > w.dense);

        let w = weights_for_intent(QueryIntent::Semantic);
        assert!(w.dense > w.bm25);
        assert!(w.dense > w.symbol);

        let w = weights_for_intent(QueryIntent::Keyword);
        assert!(w.bm25 > w.dense);
        assert!(w.bm25 > w.symbol);
    }
}
