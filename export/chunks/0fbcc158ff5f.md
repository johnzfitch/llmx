---
chunk_index: 1065
ref: "0fbcc158ff5f"
id: "0fbcc158ff5f68334c270608431d540661be0a17aefc139fb4b389160567a1a5"
slug: "chunk-l128-271"
path: "/home/zack/dev/llmx/ingestor-core/src/chunk.rs"
kind: "text"
lines: [128, 271]
token_estimate: 1100
content_sha256: "fc07f3088e3491dab116360f743e599ff0e7f6d3a4a406adbd612c0490ba1ce3"
compacted: false
heading_path: []
symbol: null
address: null
asset_path: null
---

buf.push(line.to_string());
        if !in_fence && buffer_len(&buf) >= options.chunk_max_chars {
            flush_chunk(&mut drafts, &mut buf, &current_heading, ChunkFlushParams {
                start_line,
                end_line: line_no,
                kind: ChunkKind::Markdown,
                symbol: None,
                address: None,
            });
            start_line = line_no + 1;
        }
    }

    flush_chunk(&mut drafts, &mut buf, &current_heading, ChunkFlushParams {
        start_line,
        end_line: line_count(text),
        kind: ChunkKind::Markdown,
        symbol: None,
        address: None,
    });
    drafts
}

fn chunk_text(text: &str, options: &IngestOptions) -> Vec<ChunkDraft> {
    let lines: Vec<&str> = text.lines().collect();
    let mut drafts = Vec::new();
    let mut buf: Vec<String> = Vec::new();
    let mut start_line = 1;
    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        if line.trim().is_empty() && !buf.is_empty() {
            if buffer_len(&buf) >= options.chunk_target_chars {
                flush_chunk(&mut drafts, &mut buf, &[], ChunkFlushParams {
                    start_line,
                    end_line: line_no,
                    kind: ChunkKind::Text,
                    symbol: None,
                    address: None,
                });
                start_line = line_no + 1;
            } else {
                buf.push(line.to_string());
            }
            continue;
        }
        buf.push((*line).to_string());
        if buffer_len(&buf) >= options.chunk_max_chars {
            flush_chunk(&mut drafts, &mut buf, &[], ChunkFlushParams {
                start_line,
                end_line: line_no,
                kind: ChunkKind::Text,
                symbol: None,
                address: None,
            });
            start_line = line_no + 1;
        }
    }
    flush_chunk(&mut drafts, &mut buf, &[], ChunkFlushParams {
        start_line,
        end_line: line_count(text),
        kind: ChunkKind::Text,
        symbol: None,
        address: None,
    });
    drafts
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "lt;" => Some('<'),
        "gt;" => Some('>'),
        "amp;" => Some('&'),
        "quot;" => Some('"'),
        "apos;" => Some('\''),
        "nbsp;" => Some(' '),
        "#34;" => Some('"'),
        "#39;" => Some('\''),
        _ => {
            if let Some(rest) = entity.strip_prefix("#x") {
                let hex = rest.strip_suffix(';')?;
                let value = u32::from_str_radix(hex, 16).ok()?;
                char::from_u32(value)
            } else if let Some(rest) = entity.strip_prefix("#X") {
                let hex = rest.strip_suffix(';')?;
                let value = u32::from_str_radix(hex, 16).ok()?;
                char::from_u32(value)
            } else if let Some(rest) = entity.strip_prefix('#') {
                let dec = rest.strip_suffix(';')?;
                let value = dec.parse::<u32>().ok()?;
                char::from_u32(value)
            } else {
                None
            }
        }
    }
}

fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '&' {
            out.push(ch);
            continue;
        }
        let mut entity = String::new();
        while let Some(&next) = chars.peek() {
            entity.push(next);
            chars.next();
            if next == ';' {
                break;
            }
            if entity.len() > 16 {
                break;
            }
        }
        if let Some(decoded) = decode_html_entity(&entity) {
            out.push(decoded);
        } else {
            out.push('&');
            out.push_str(&entity);
        }
    }
    out
}

fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = false;
    for ch in input.chars() {
        let is_ws = ch.is_whitespace() || ch == '\u{00a0}';
        if is_ws {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}