use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use nodalync_crypto::{generate_identity, sign, verify, PrivateKey, PublicKey, Signature, peer_id_from_public_key, peer_id_to_string};

fn bench_identity_generation(c: &mut Criterion) {
    c.bench_function("identity_generation", |b| {
        b.iter(|| {
            let (private_key, public_key) = generate_identity();
            black_box((private_key, public_key));
        });
    });
}

fn bench_signing(c: &mut Criterion) {
    let (private_key, _) = generate_identity();
    let message = b"Hello, world! This is a test message for signing benchmarks.";
    
    c.bench_function("message_signing", |b| {
        b.iter(|| {
            let signature = sign(&private_key, black_box(message));
            black_box(signature);
        });
    });
}

fn bench_signature_verification(c: &mut Criterion) {
    let (private_key, public_key) = generate_identity();
    let message = b"Hello, world! This is a test message for verification benchmarks.";
    let signature = sign(&private_key, message);
    
    c.bench_function("signature_verification", |b| {
        b.iter(|| {
            let result = verify(black_box(&public_key), black_box(message), black_box(&signature));
            black_box(result);
        });
    });
}

fn bench_signing_throughput(c: &mut Criterion) {
    let (private_key, _) = generate_identity();
    let mut group = c.benchmark_group("signing_throughput");
    
    for size in [64, 256, 1024, 4096, 16384].iter() {
        let message = vec![0u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let signature = sign(&private_key, black_box(&message));
                black_box(signature);
            });
        });
    }
    group.finish();
}

fn bench_verification_throughput(c: &mut Criterion) {
    let (private_key, public_key) = generate_identity();
    let mut group = c.benchmark_group("verification_throughput");
    
    for size in [64, 256, 1024, 4096, 16384].iter() {
        let message = vec![0u8; *size];
        let signature = sign(&private_key, &message);
        
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let result = verify(black_box(&public_key), black_box(&message), black_box(&signature));
                black_box(result);
            });
        });
    }
    group.finish();
}

fn bench_public_key_serialization(c: &mut Criterion) {
    let (_, public_key) = generate_identity();
    let peer_id = peer_id_from_public_key(&public_key);
    
    c.bench_function("peer_id_to_string", |b| {
        b.iter(|| {
            let string = peer_id_to_string(black_box(&peer_id));
            black_box(string);
        });
    });
    
    let peer_id_string = peer_id_to_string(&peer_id);
    c.bench_function("peer_id_from_string", |b| {
        b.iter(|| {
            let result = nodalync_crypto::peer_id_from_string(black_box(&peer_id_string));
            black_box(result);
        });
    });
}

criterion_group!(
    benches,
    bench_identity_generation,
    bench_signing,
    bench_signature_verification,
    bench_signing_throughput,
    bench_verification_throughput,
    bench_public_key_serialization
);

criterion_main!(benches);