use ingestor_core::{ingest_files, IngestOptions};
use pretty_assertions::assert_eq;

fn load_fixture(path: &str) -> Vec<u8> {
    std::fs::read(path).expect("fixture")
}

#[test]
fn deterministic_chunking_across_runs() {
    let fixtures = vec![
        ("fixtures/sample.md", "docs/sample.md"),
        ("fixtures/sample.json", "data/sample.json"),
        ("fixtures/sample.js", "src/sample.js"),
        ("fixtures/sample.html", "web/sample.html"),
        ("fixtures/sample.txt", "notes/sample.txt"),
    ];

    let files_a: Vec<ingestor_core::FileInput> = fixtures
        .iter()
        .map(|(fixture, path)| ingestor_core::FileInput {
            path: (*path).to_string(),
            data: load_fixture(&format!("{}/tests/{}", env!("CARGO_MANIFEST_DIR"), fixture)),
            mtime_ms: None,
            fingerprint_sha256: None,
        })
        .collect();
    let files_b = files_a.clone();

    let options = IngestOptions::default();
    let index_a = ingest_files(files_a, options.clone());
    let index_b = ingest_files(files_b, options);

    assert_eq!(index_a.index_id, index_b.index_id);
    assert_eq!(index_a.chunks.len(), index_b.chunks.len());
    let refs_a: Vec<(String, String, String)> = index_a
        .chunks
        .iter()
        .map(|c| (c.id.clone(), c.short_id.clone(), c.slug.clone()))
        .collect();
    let refs_b: Vec<(String, String, String)> = index_b
        .chunks
        .iter()
        .map(|c| (c.id.clone(), c.short_id.clone(), c.slug.clone()))
        .collect();
    assert_eq!(refs_a, refs_b);
}

#[test]
fn html_strips_script_content() {
    let input = ingestor_core::FileInput {
        path: "web/attack.html".to_string(),
        data: load_fixture(&format!("{}/tests/fixtures/sample.html", env!("CARGO_MANIFEST_DIR"))),
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    let combined = index
        .chunks
        .iter()
        .map(|c| c.content.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!combined.contains("console.log"));
}

#[test]
fn enforces_size_limits() {
    let data = vec![b'a'; 1024];
    let input = ingestor_core::FileInput {
        path: "notes/large.txt".to_string(),
        data,
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let mut options = IngestOptions::default();
    options.max_file_bytes = 512;
    let index = ingest_files(vec![input], options);
    assert!(index.files.is_empty());
    assert!(!index.warnings.is_empty());
    assert_eq!(index.warnings[0].code, "max_file_bytes");
}

#[test]
fn ingests_images_as_assets_without_utf8_decode() {
    let input = ingestor_core::FileInput {
        path: "assets/screenshot.png".to_string(),
        data: vec![0, 1, 2, 3, 4, 5],
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    assert_eq!(index.files.len(), 1);
    assert!(index.warnings.is_empty());
    assert_eq!(index.files[0].kind, ingestor_core::ChunkKind::Image);
    assert_eq!(index.chunks.len(), 1);
    assert_eq!(index.chunks[0].kind, ingestor_core::ChunkKind::Image);
    assert!(index.chunks[0].asset_path.as_deref().unwrap_or("").starts_with("images/"));
}

#[test]
fn ingests_png_paths_with_spaces_as_image() {
    let path = "chatgpt-apps/Screenshot 2026-01-06 at 02-36-22 SDKs - Model Context Protocol.png";
    let input = ingestor_core::FileInput {
        path: path.to_string(),
        data: vec![0, 1, 2, 3, 4, 5],
        mtime_ms: None,
        fingerprint_sha256: None,
    };
    let index = ingest_files(vec![input], IngestOptions::default());
    assert!(index.warnings.is_empty());
    assert_eq!(index.files.len(), 1);
    assert_eq!(index.files[0].path, path);
    assert_eq!(index.files[0].kind, ingestor_core::ChunkKind::Image);
    assert_eq!(index.chunks.len(), 1);
    assert_eq!(index.chunks[0].kind, ingestor_core::ChunkKind::Image);
}

#[test]
fn selective_update_keeps_unchanged_paths() {
    let options = IngestOptions::default();
    let prev = ingest_files(
        vec![
            ingestor_core::FileInput {
                path: "docs/a.md".to_string(),
                data: b"# A\n\nHello\n".to_vec(),
                mtime_ms: None,
                fingerprint_sha256: Some("fp-a".to_string()),
            },
            ingestor_core::FileInput {
                path: "docs/b.md".to_string(),
                data: b"# B\n\nOld\n".to_vec(),
                mtime_ms: None,
                fingerprint_sha256: Some("fp-b".to_string()),
            },
        ],
        options.clone(),
    );

    let updated = ingestor_core::update_index_selective(
        prev,
        vec![ingestor_core::FileInput {
            path: "docs/b.md".to_string(),
            data: b"# B\n\nNew\n".to_vec(),
            mtime_ms: None,
            fingerprint_sha256: Some("fp-b2".to_string()),
        }],
        vec!["docs/a.md".to_string()],
        options,
    );

    let paths: Vec<String> = updated.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(paths, vec!["docs/a.md".to_string(), "docs/b.md".to_string()]);
    assert_eq!(updated.warnings.len(), 0);
}
