use bit_store::BitStore;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::json;
use tempfile::TempDir;

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");

    for size in [100, 1000, 5000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter(|| {
                let dir = TempDir::new().unwrap();
                let path = dir.path().join("bench.bitstore");
                let mut store = BitStore::create(&path).unwrap();
                for i in 0..size {
                    store
                        .insert_entity(
                            "User",
                            &format!("u{}", i),
                            &json!({"name": format!("User {}", i)}),
                        )
                        .unwrap();
                }
                store.flush().unwrap();
            });
        });
    }
    group.finish();
}

fn bench_search(c: &mut Criterion) {
    // Pre-populate store with 5000 entities
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bench.bitstore");
    {
        let mut store = BitStore::create(&path).unwrap();
        for i in 0..5000 {
            store
                .insert_entity(
                    "User",
                    &format!("u{:05}", i),
                    &json!({"name": format!("User {}", i)}),
                )
                .unwrap();
        }
        store.flush().unwrap();
    }

    c.bench_function("search_5k_entities", |b| {
        b.iter(|| {
            let mut store = BitStore::open(&path).unwrap();
            // Search for 100 random entities
            for i in (0..5000).step_by(50) {
                store.get_entity("User", &format!("u{:05}", i)).unwrap();
            }
        });
    });
}

fn bench_scan(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bench.bitstore");
    {
        let mut store = BitStore::create(&path).unwrap();
        for i in 0..5000 {
            store
                .insert_entity(
                    "User",
                    &format!("u{:05}", i),
                    &json!({"name": format!("User {}", i)}),
                )
                .unwrap();
        }
        store.flush().unwrap();
    }

    c.bench_function("scan_all_5k", |b| {
        b.iter(|| {
            let mut store = BitStore::open(&path).unwrap();
            let users = store.list_entities("User").unwrap();
            assert_eq!(users.len(), 5000);
        });
    });
}

criterion_group!(benches, bench_insert, bench_search, bench_scan);
criterion_main!(benches);
