use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use nodalync_store::{NodeState, NodeStateConfig, ContentStore, ManifestStore};
use nodalync_types::{ContentType, L1Summary, Manifest, Metadata};
use nodalync_crypto::{generate_identity, peer_id_from_public_key, content_hash};
use nodalync_wire::AnnouncePayload;
use tempfile::TempDir;

fn create_temp_state() -> (NodeState, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let config = NodeStateConfig::new(temp_dir.path());
    let state = NodeState::open(config).unwrap();
    (state, temp_dir)
}

fn create_sample_content(id: u64, size: usize) -> Vec<u8> {
    format!("Content {} with {} bytes of data: {}", id, size, "X".repeat(size.saturating_sub(50))).into_bytes()
}

fn create_sample_manifest(id: u64, content_size: usize) -> (Manifest, Vec<u8>) {
    let (_, public_key) = generate_identity();
    let owner = peer_id_from_public_key(&public_key);
    let content = create_sample_content(id, content_size);
    let hash = content_hash(&content);
    let metadata = Metadata::new(&format!("Test Content {}", id), content.len() as u64);
    let manifest = Manifest::new_l0(hash, owner, metadata, 1640995200 + id as u64);
    (manifest, content)
}

fn bench_content_storage(c: &mut Criterion) {
    let (state, _temp_dir) = create_temp_state();
    let content = create_sample_content(1, 1024);
    
    c.bench_function("store_content", |b| {
        b.iter_with_setup(
            || {
                let (state, _temp_dir) = create_temp_state();
                let content = create_sample_content(rand::random(), 1024);
                (state, content, _temp_dir)
            },
            |(mut state, content, _temp_dir)| {
                let hash = state.content.store(black_box(&content)).unwrap();
                black_box(hash);
            }
        );
    });
    
    // Store content first for retrieval benchmark
    let mut state_mut = state;
    let hash = state_mut.content.store(&content).unwrap();
    
    c.bench_function("retrieve_content", |b| {
        b.iter(|| {
            let retrieved = state_mut.content.load(black_box(&hash));
            black_box(retrieved);
        });
    });
}

fn bench_content_storage_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("content_storage_throughput");
    
    for size in [256, 1024, 4096, 16384, 65536].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter_with_setup(
                || {
                    let (state, _temp_dir) = create_temp_state();
                    let content = create_sample_content(1, size);
                    (state, content, _temp_dir)
                },
                |(mut state, content, _temp_dir)| {
                    let hash = state.content.store(black_box(&content)).unwrap();
                    black_box(hash);
                }
            );
        });
    }
    group.finish();
}

fn bench_manifest_storage(c: &mut Criterion) {
    let (state, _temp_dir) = create_temp_state();
    let (manifest, _content) = create_sample_manifest(1, 1024);
    
    c.bench_function("store_manifest", |b| {
        b.iter_with_setup(
            || {
                let (mut state, _temp_dir) = create_temp_state();
                let (manifest, _) = create_sample_manifest(rand::random(), 1024);
                (state, manifest, _temp_dir)
            },
            |(mut state, manifest, _temp_dir)| {
                state.manifests.store(black_box(&manifest)).unwrap();
            }
        );
    });
    
    // Store manifest first for retrieval benchmark
    let mut state_mut = state;
    state_mut.manifests.store(&manifest).unwrap();
    
    c.bench_function("retrieve_manifest", |b| {
        b.iter(|| {
            let retrieved = state_mut.manifests.load(black_box(&manifest.hash));
            black_box(retrieved);
        });
    });
}

fn bench_batch_content_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_content_operations");
    
    for count in [10, 50, 100, 500].iter() {
        group.bench_with_input(BenchmarkId::new("store_batch", count), count, |b, &count| {
            b.iter_with_setup(
                || {
                    let (state, _temp_dir) = create_temp_state();
                    let contents: Vec<_> = (0..count).map(|i| create_sample_content(i as u64, 512)).collect();
                    (state, contents, _temp_dir)
                },
                |(mut state, contents, _temp_dir)| {
                    for content in contents {
                        let hash = state.content.store(black_box(&content)).unwrap();
                        black_box(hash);
                    }
                }
            );
        });
        
        group.bench_with_input(BenchmarkId::new("retrieve_batch", count), count, |b, &count| {
            b.iter_with_setup(
                || {
                    let (mut state, _temp_dir) = create_temp_state();
                    let mut content_hashes = Vec::new();
                    
                    // Store all contents first
                    for i in 0..count {
                        let content = create_sample_content(i as u64, 512);
                        let hash = state.content.store(&content).unwrap();
                        content_hashes.push(hash);
                    }
                    
                    (state, content_hashes, _temp_dir)
                },
                |(state, content_hashes, _temp_dir)| {
                    for hash in &content_hashes {
                        let retrieved = state.content.load(black_box(hash));
                        black_box(retrieved);
                    }
                }
            );
        });
    }
    group.finish();
}

fn bench_announcement_storage(c: &mut Criterion) {
    let (state, _temp_dir) = create_temp_state();
    let content = create_sample_content(1, 1024);
    let hash = content_hash(&content);
    
    let announcement = AnnouncePayload {
        hash,
        content_type: ContentType::L0,
        title: "Test Content for Benchmarking".to_string(),
        l1_summary: L1Summary::empty(hash),
        price: 100,
        addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
        publisher_peer_id: None,
    };
    
    c.bench_function("store_announcement", |b| {
        b.iter_with_setup(
            || {
                let content = create_sample_content(rand::random(), 1024);
                let hash = content_hash(&content);
                AnnouncePayload {
                    hash,
                    content_type: ContentType::L0,
                    title: format!("Random Content {}", rand::random::<u32>()),
                    l1_summary: L1Summary::empty(hash),
                    price: rand::random::<u64>() % 1000 + 1,
                    addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
                    publisher_peer_id: None,
                }
            },
            |announcement| {
                state.store_announcement(black_box(announcement));
            }
        );
    });
    
    // Store announcement first for retrieval benchmark
    state.store_announcement(announcement.clone());
    
    c.bench_function("retrieve_announcement", |b| {
        b.iter(|| {
            let retrieved = state.get_announcement(black_box(&announcement.hash));
            black_box(retrieved);
        });
    });
}

fn bench_announcement_search(c: &mut Criterion) {
    let (state, _temp_dir) = create_temp_state();
    
    // Populate with test announcements
    for i in 0..1000 {
        let content = create_sample_content(i, 512);
        let hash = content_hash(&content);
        let announcement = AnnouncePayload {
            hash,
            content_type: if i % 3 == 0 { ContentType::L1 } else { ContentType::L0 },
            title: format!("Content {} about {}", i, if i % 5 == 0 { "protocol" } else { "testing" }),
            l1_summary: L1Summary::empty(hash),
            price: (i * 10) as u64,
            addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
            publisher_peer_id: None,
        };
        state.store_announcement(announcement);
    }
    
    c.bench_function("search_announcements_text", |b| {
        b.iter(|| {
            let results = state.search_announcements(black_box("protocol"), None, black_box(50));
            black_box(results);
        });
    });
    
    c.bench_function("search_announcements_filter", |b| {
        b.iter(|| {
            let results = state.search_announcements(black_box(""), Some(black_box(ContentType::L1)), black_box(100));
            black_box(results);
        });
    });
    
    c.bench_function("list_all_announcements", |b| {
        b.iter(|| {
            let results = state.list_announcements();
            black_box(results);
        });
    });
}

fn bench_database_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("database_scaling");
    
    for count in [100, 1000, 5000, 10000].iter() {
        group.bench_with_input(BenchmarkId::new("announcement_count", count), count, |b, &count| {
            b.iter_with_setup(
                || {
                    let (state, _temp_dir) = create_temp_state();
                    
                    // Populate database
                    for i in 0..count {
                        let content = create_sample_content(i, 256);
                        let hash = content_hash(&content);
                        let announcement = AnnouncePayload {
                            hash,
                            content_type: ContentType::L0,
                            title: format!("Content {}", i),
                            l1_summary: L1Summary::empty(hash),
                            price: i as u64,
                            addresses: vec![],
                            publisher_peer_id: None,
                        };
                        state.store_announcement(announcement);
                    }
                    
                    (state, _temp_dir)
                },
                |(state, _temp_dir)| {
                    let count = state.announcement_count();
                    black_box(count);
                }
            );
        });
        
        group.bench_with_input(BenchmarkId::new("search_in_large_db", count), count, |b, &count| {
            b.iter_with_setup(
                || {
                    let (state, _temp_dir) = create_temp_state();
                    
                    // Populate database
                    for i in 0..count {
                        let content = create_sample_content(i, 256);
                        let hash = content_hash(&content);
                        let announcement = AnnouncePayload {
                            hash,
                            content_type: ContentType::L0,
                            title: format!("Content {} {}", i, if i % 100 == 0 { "special" } else { "normal" }),
                            l1_summary: L1Summary::empty(hash),
                            price: i as u64,
                            addresses: vec![],
                            publisher_peer_id: None,
                        };
                        state.store_announcement(announcement);
                    }
                    
                    (state, _temp_dir)
                },
                |(state, _temp_dir)| {
                    let results = state.search_announcements(black_box("special"), None, black_box(10));
                    black_box(results);
                }
            );
        });
    }
    group.finish();
}

fn bench_mixed_workload(c: &mut Criterion) {
    c.bench_function("mixed_content_manifest_workload", |b| {
        b.iter_with_setup(
            || create_temp_state(),
            |(mut state, _temp_dir)| {
                // Store content
                let content = create_sample_content(1, 2048);
                let hash = state.content.store(black_box(&content)).unwrap();
                
                // Create and store manifest
                let (_, public_key) = generate_identity();
                let owner = peer_id_from_public_key(&public_key);
                let metadata = Metadata::new("Mixed Workload Test", content.len() as u64);
                let manifest = Manifest::new_l0(hash, owner, metadata, 1640995200);
                state.manifests.store(black_box(&manifest)).unwrap();
                
                // Store announcement
                let announcement = AnnouncePayload {
                    hash,
                    content_type: ContentType::L0,
                    title: "Mixed Workload Content".to_string(),
                    l1_summary: L1Summary::empty(hash),
                    price: 150,
                    addresses: vec![],
                    publisher_peer_id: None,
                };
                state.store_announcement(black_box(announcement));
                
                // Retrieve everything back
                let retrieved_content = state.content.load(black_box(&hash)).unwrap();
                let retrieved_manifest = state.manifests.load(black_box(&hash)).unwrap();
                let retrieved_announcement = state.get_announcement(black_box(&hash));
                
                black_box((retrieved_content, retrieved_manifest, retrieved_announcement));
            }
        );
    });
}

criterion_group!(
    benches,
    bench_content_storage,
    bench_content_storage_throughput,
    bench_manifest_storage,
    bench_batch_content_operations,
    bench_announcement_storage,
    bench_announcement_search,
    bench_database_scaling,
    bench_mixed_workload
);

criterion_main!(benches);