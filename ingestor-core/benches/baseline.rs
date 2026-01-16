use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use ingestor_core::{
    ingest_files, search, build_inverted_index, compute_stats, FileInput, IngestOptions,
    SearchFilters, IndexFile,
};

#[cfg(feature = "embeddings")]
use ingestor_core::{
    embeddings::{generate_embedding, generate_embeddings, cosine_similarity},
    hybrid_search, vector_search,
};

// Test data generators
fn create_test_file(path: &str, size_kb: usize) -> FileInput {
    let content = format!("// Test file: {}\n{}", path, "fn test() { println!(\"hello\"); }\n".repeat(size_kb * 10));
    FileInput {
        path: path.to_string(),
        data: content.into_bytes(),
        mtime_ms: Some(1234567890),
        fingerprint_sha256: None,
    }
}

fn create_test_index(file_count: usize, file_size_kb: usize) -> IndexFile {
    let files: Vec<FileInput> = (0..file_count)
        .map(|i| create_test_file(&format!("src/file_{}.rs", i), file_size_kb))
        .collect();

    let options = IngestOptions {
        chunk_target_chars: 4000,
        chunk_max_chars: 8000,
        max_file_bytes: 10 * 1024 * 1024,
        max_total_bytes: 50 * 1024 * 1024,
        max_chunks_per_file: 2000,
    };

    ingest_files(files, options)
}

// Benchmark: Index creation
fn bench_index_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("index_creation");

    for (file_count, file_size_kb) in [(10, 1), (50, 2), (100, 5)] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}files_{}kb", file_count, file_size_kb)),
            &(file_count, file_size_kb),
            |b, &(fc, fs)| {
                b.iter(|| {
                    let files: Vec<FileInput> = (0..fc)
                        .map(|i| create_test_file(&format!("src/file_{}.rs", i), fs))
                        .collect();

                    let options = IngestOptions {
                        chunk_target_chars: 4000,
                        chunk_max_chars: 8000,
                        max_file_bytes: 10 * 1024 * 1024,
                        max_total_bytes: 50 * 1024 * 1024,
                        max_chunks_per_file: 2000,
                    };

                    black_box(ingest_files(files, options))
                });
            },
        );
    }

    group.finish();
}

// Benchmark: BM25 search
fn bench_search_bm25(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_bm25");

    // Pre-create index
    let index = create_test_index(50, 2);

    for query in ["function", "test println", "hello world"] {
        group.bench_with_input(
            BenchmarkId::from_parameter(query),
            &query,
            |b, q| {
                b.iter(|| {
                    black_box(search(
                        &index,
                        q,
                        SearchFilters::default(),
                        10,
                    ))
                });
            },
        );
    }

    group.finish();
}

// Benchmark: Inverted index build
fn bench_inverted_index_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("inverted_index_build");

    for chunk_count in [100, 500, 1000] {
        let index = create_test_index(chunk_count / 10, 1);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}chunks", chunk_count)),
            &index.chunks,
            |b, chunks| {
                b.iter(|| {
                    black_box(build_inverted_index(chunks))
                });
            },
        );
    }

    group.finish();
}

// Benchmark: Stats computation
fn bench_stats_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("stats_computation");

    for file_count in [10, 50, 100] {
        let index = create_test_index(file_count, 2);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}files", file_count)),
            &(index.files.clone(), index.chunks.clone()),
            |b, (files, chunks)| {
                b.iter(|| {
                    black_box(compute_stats(files, chunks))
                });
            },
        );
    }

    group.finish();
}

// Benchmark: Memory usage (serialize/deserialize)
fn bench_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");

    let index = create_test_index(50, 2);

    group.bench_function("serialize_index", |b| {
        b.iter(|| {
            black_box(serde_json::to_vec(&index).unwrap())
        });
    });

    let serialized = serde_json::to_vec(&index).unwrap();
    group.bench_function("deserialize_index", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<IndexFile>(&serialized).unwrap())
        });
    });

    group.finish();
}

// Phase 5: Embedding generation benchmarks
#[cfg(feature = "embeddings")]
fn bench_embedding_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("embedding_generation");

    let short_text = "function hello() { return 'world'; }";
    let medium_text = "// Complex function\nfunction processData(items) {\n  return items.map(item => {\n    return transform(item);\n  }).filter(x => x != null);\n}";
    let long_text = format!("{}\n{}", medium_text.repeat(5), short_text.repeat(10));

    group.bench_function("generate_single_short", |b| {
        b.iter(|| black_box(generate_embedding(short_text)));
    });

    group.bench_function("generate_single_medium", |b| {
        b.iter(|| black_box(generate_embedding(medium_text)));
    });

    group.bench_function("generate_single_long", |b| {
        b.iter(|| black_box(generate_embedding(&long_text)));
    });

    let chunk_texts: Vec<&str> = (0..100).map(|_| short_text).collect();
    group.bench_function("generate_batch_100", |b| {
        b.iter(|| black_box(generate_embeddings(&chunk_texts)));
    });

    group.finish();
}

// Phase 5: Vector search benchmarks
#[cfg(feature = "embeddings")]
fn bench_vector_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search");

    let mut index = create_test_index(50, 2);

    // Generate embeddings
    let chunk_texts: Vec<&str> = index.chunks.iter().map(|c| c.content.as_str()).collect();
    let embeddings = generate_embeddings(&chunk_texts);
    let query_embedding = generate_embedding("function test");

    group.bench_function("vector_search_50chunks", |b| {
        b.iter(|| {
            black_box(vector_search(
                &index.chunks,
                &index.chunk_refs,
                &embeddings,
                &query_embedding,
                &SearchFilters::default(),
                10,
            ))
        });
    });

    group.finish();
}

// Phase 5: Hybrid search benchmarks
#[cfg(feature = "embeddings")]
fn bench_hybrid_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_search");

    let mut index = create_test_index(50, 2);

    // Generate embeddings
    let chunk_texts: Vec<&str> = index.chunks.iter().map(|c| c.content.as_str()).collect();
    let embeddings = generate_embeddings(&chunk_texts);

    for query in ["function", "test println", "error handling"] {
        let query_embedding = generate_embedding(query);
        group.bench_with_input(
            BenchmarkId::from_parameter(query),
            &(query, &query_embedding),
            |b, (q, qe)| {
                b.iter(|| {
                    black_box(hybrid_search(
                        &index.chunks,
                        &index.inverted_index,
                        &index.chunk_refs,
                        &embeddings,
                        q,
                        qe,
                        &SearchFilters::default(),
                        10,
                    ))
                });
            },
        );
    }

    group.finish();
}

// Phase 5: Cosine similarity benchmarks
#[cfg(feature = "embeddings")]
fn bench_cosine_similarity(c: &mut Criterion) {
    let mut group = c.benchmark_group("cosine_similarity");

    let embedding1 = generate_embedding("function test");
    let embedding2 = generate_embedding("function hello");

    group.bench_function("cosine_similarity_384dim", |b| {
        b.iter(|| black_box(cosine_similarity(&embedding1, &embedding2)));
    });

    group.finish();
}

#[cfg(feature = "embeddings")]
criterion_group!(
    benches,
    bench_index_creation,
    bench_search_bm25,
    bench_inverted_index_build,
    bench_stats_computation,
    bench_serialization,
    bench_embedding_generation,
    bench_vector_search,
    bench_hybrid_search,
    bench_cosine_similarity
);

#[cfg(not(feature = "embeddings"))]
criterion_group!(
    benches,
    bench_index_creation,
    bench_search_bm25,
    bench_inverted_index_build,
    bench_stats_computation,
    bench_serialization
);

criterion_main!(benches);
