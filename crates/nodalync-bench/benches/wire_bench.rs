use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use nodalync_wire::{
    encode_payload, decode_payload, encode_message, decode_message, create_message,
    AnnouncePayload, SearchPayload, SearchFilters, PingPayload, MessageType,
};
use nodalync_types::{ContentType, L1Summary};
use nodalync_crypto::{generate_identity, peer_id_from_public_key, content_hash};

fn create_sample_announce_payload(size: usize) -> AnnouncePayload {
    let content_data = vec![b'A'; size];
    let hash = content_hash(&content_data);
    let l1_summary = L1Summary::empty(hash);
    
    AnnouncePayload {
        hash,
        content_type: ContentType::L0,
        title: format!("Test Content {}", size),
        l1_summary,
        price: 100,
        addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
        publisher_peer_id: None,
    }
}

fn bench_announce_payload_serialization(c: &mut Criterion) {
    let announce = create_sample_announce_payload(1024);
    
    c.bench_function("announce_payload_serialize", |b| {
        b.iter(|| {
            let serialized = encode_payload(black_box(&announce));
            black_box(serialized);
        });
    });
    
    let serialized = encode_payload(&announce).unwrap();
    c.bench_function("announce_payload_deserialize", |b| {
        b.iter(|| {
            let deserialized: Result<AnnouncePayload, _> = decode_payload(black_box(&serialized));
            black_box(deserialized);
        });
    });
}

fn bench_search_payload_serialization(c: &mut Criterion) {
    let search_filters = SearchFilters {
        content_types: Some(vec![ContentType::L0, ContentType::L1]),
        max_price: Some(1000),
        min_reputation: None,
        created_after: None,
        created_before: None,
        tags: None,
    };
    
    let search_payload = SearchPayload {
        query: "test query for benchmarking performance".to_string(),
        filters: Some(search_filters),
        limit: 100,
        offset: 0,
    };
    
    c.bench_function("search_payload_serialize", |b| {
        b.iter(|| {
            let serialized = encode_payload(black_box(&search_payload));
            black_box(serialized);
        });
    });
    
    let serialized = encode_payload(&search_payload).unwrap();
    c.bench_function("search_payload_deserialize", |b| {
        b.iter(|| {
            let deserialized: Result<SearchPayload, _> = decode_payload(black_box(&serialized));
            black_box(deserialized);
        });
    });
}

fn bench_ping_payload_serialization(c: &mut Criterion) {
    let ping_payload = PingPayload { nonce: 12345 };
    
    c.bench_function("ping_payload_serialize", |b| {
        b.iter(|| {
            let serialized = encode_payload(black_box(&ping_payload));
            black_box(serialized);
        });
    });
    
    let serialized = encode_payload(&ping_payload).unwrap();
    c.bench_function("ping_payload_deserialize", |b| {
        b.iter(|| {
            let deserialized: Result<PingPayload, _> = decode_payload(black_box(&serialized));
            black_box(deserialized);
        });
    });
}

fn bench_full_message_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_message_throughput");
    
    for size in [256, 1024, 4096, 16384, 65536].iter() {
        let (private_key, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);
        let payload = create_sample_announce_payload(*size);
        let payload_bytes = encode_payload(&payload).unwrap();
        let timestamp = 1640995200000u64;
        
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _size| {
            b.iter(|| {
                let message = create_message(
                    MessageType::Announce,
                    black_box(payload_bytes.clone()),
                    black_box(peer_id),
                    black_box(timestamp),
                    black_box(&private_key),
                );
                let wire_bytes = encode_message(black_box(&message));
                black_box(wire_bytes);
            });
        });
    }
    group.finish();
}

fn bench_message_decode_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_decode_throughput");
    
    for size in [256, 1024, 4096, 16384, 65536].iter() {
        let (private_key, public_key) = generate_identity();
        let peer_id = peer_id_from_public_key(&public_key);
        let payload = create_sample_announce_payload(*size);
        let payload_bytes = encode_payload(&payload).unwrap();
        let timestamp = 1640995200000u64;
        
        let message = create_message(
            MessageType::Announce,
            payload_bytes,
            peer_id,
            timestamp,
            &private_key,
        );
        let wire_bytes = encode_message(&message).unwrap();
        
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _size| {
            b.iter(|| {
                let decoded = decode_message(black_box(&wire_bytes));
                black_box(decoded);
            });
        });
    }
    group.finish();
}

fn bench_cbor_vs_json(c: &mut Criterion) {
    let announce_payload = create_sample_announce_payload(2048);
    
    let mut group = c.benchmark_group("serialization_format_comparison");
    
    group.bench_function("cbor_serialize", |b| {
        b.iter(|| {
            let serialized = encode_payload(black_box(&announce_payload));
            black_box(serialized);
        });
    });
    
    group.bench_function("json_serialize", |b| {
        b.iter(|| {
            let serialized = serde_json::to_vec(black_box(&announce_payload));
            black_box(serialized);
        });
    });
    
    let cbor_data = encode_payload(&announce_payload).unwrap();
    let json_data = serde_json::to_vec(&announce_payload).unwrap();
    
    group.bench_function("cbor_deserialize", |b| {
        b.iter(|| {
            let deserialized: Result<AnnouncePayload, _> = decode_payload(black_box(&cbor_data));
            black_box(deserialized);
        });
    });
    
    group.bench_function("json_deserialize", |b| {
        b.iter(|| {
            let deserialized: Result<AnnouncePayload, _> = serde_json::from_slice(black_box(&json_data));
            black_box(deserialized);
        });
    });
    
    group.finish();
    
    // Report size comparison
    println!("CBOR size: {} bytes, JSON size: {} bytes, CBOR is {:.1}% smaller", 
             cbor_data.len(), json_data.len(), 
             100.0 * (json_data.len() - cbor_data.len()) as f64 / json_data.len() as f64);
}

fn bench_different_message_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_types_comparison");
    let (private_key, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);
    let timestamp = 1640995200000u64;
    
    // Ping message (small)
    let ping = PingPayload { nonce: 42 };
    let ping_bytes = encode_payload(&ping).unwrap();
    group.bench_function("ping_message_encode", |b| {
        b.iter(|| {
            let msg = create_message(
                MessageType::Ping,
                black_box(ping_bytes.clone()),
                black_box(peer_id),
                black_box(timestamp),
                black_box(&private_key),
            );
            let wire = encode_message(black_box(&msg));
            black_box(wire);
        });
    });
    
    // Search message (medium)
    let search_filters = SearchFilters {
        content_types: Some(vec![ContentType::L0]),
        max_price: Some(1000),
        min_reputation: None,
        created_after: None,
        created_before: None,
        tags: None,
    };
    
    let search = SearchPayload {
        query: "test search query".to_string(),
        filters: Some(search_filters),
        limit: 50,
        offset: 0,
    };
    let search_bytes = encode_payload(&search).unwrap();
    group.bench_function("search_message_encode", |b| {
        b.iter(|| {
            let msg = create_message(
                MessageType::Search,
                black_box(search_bytes.clone()),
                black_box(peer_id),
                black_box(timestamp),
                black_box(&private_key),
            );
            let wire = encode_message(black_box(&msg));
            black_box(wire);
        });
    });
    
    // Announce message (large)
    let announce = create_sample_announce_payload(4096);
    let announce_bytes = encode_payload(&announce).unwrap();
    group.bench_function("announce_message_encode", |b| {
        b.iter(|| {
            let msg = create_message(
                MessageType::Announce,
                black_box(announce_bytes.clone()),
                black_box(peer_id),
                black_box(timestamp),
                black_box(&private_key),
            );
            let wire = encode_message(black_box(&msg));
            black_box(wire);
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_announce_payload_serialization,
    bench_search_payload_serialization,
    bench_ping_payload_serialization,
    bench_full_message_throughput,
    bench_message_decode_throughput,
    bench_cbor_vs_json,
    bench_different_message_types
);

criterion_main!(benches);