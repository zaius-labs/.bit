use bit_store::{collapse, BitStore};
use serde_json::json;
use std::time::Instant;
use tempfile::TempDir;

#[test]
fn stress_insert_10k_entities() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("stress.bitstore");
    let mut store = BitStore::create(&path).unwrap();

    let start = Instant::now();
    for i in 0..10_000u32 {
        store
            .insert_entity(
                "User",
                &format!("u{:05}", i),
                &json!({"name": format!("User {}", i), "score": i}),
            )
            .unwrap();
    }
    store.flush().unwrap();
    let insert_time = start.elapsed();

    // Verify all findable
    let start = Instant::now();
    for i in 0..10_000u32 {
        let val = store.get_entity("User", &format!("u{:05}", i)).unwrap();
        assert!(val.is_some(), "Missing u{:05}", i);
    }
    let search_time = start.elapsed();

    eprintln!("10K insert: {:?}", insert_time);
    eprintln!("10K search: {:?}", search_time);
    eprintln!("Pages: {}", store.page_count());
}

#[test]
fn stress_insert_delete_query() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("stress.bitstore");
    let mut store = BitStore::create(&path).unwrap();

    // Insert 5000
    for i in 0..5000u32 {
        store
            .insert_entity(
                "Item",
                &format!("i{:05}", i),
                &json!({"value": i, "active": i % 2 == 0}),
            )
            .unwrap();
    }

    // Delete even-numbered ones (2500)
    for i in (0..5000u32).step_by(2) {
        store.delete_entity("Item", &format!("i{:05}", i)).unwrap();
    }

    // Query remaining (should be 2500)
    let remaining = store.list_entities("Item").unwrap();
    assert_eq!(remaining.len(), 2500);

    // Verify deleted ones are gone
    for i in (0..5000u32).step_by(2) {
        assert!(store
            .get_entity("Item", &format!("i{:05}", i))
            .unwrap()
            .is_none());
    }

    // Verify kept ones are present
    for i in (1..5000u32).step_by(2) {
        assert!(store
            .get_entity("Item", &format!("i{:05}", i))
            .unwrap()
            .is_some());
    }

    store.flush().unwrap();
    eprintln!(
        "Pages after 5K insert + 2.5K delete: {}",
        store.page_count()
    );
}

#[test]
#[ignore] // requires overflow pages for large blob values — tracked as future work
fn stress_collapse_100_files() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();

    for i in 0..100 {
        let content = format!(
            "\
define:@Entity{i}
    name: \"\"!
    value: 0#

mutate:@Entity{i}:inst1
    name: Instance 1 of Entity{i}
    value: {val}

[!] Task A for Entity{i}
[x] Task B for Entity{i}
",
            i = i,
            val = i * 10
        );
        std::fs::write(src.join(format!("file_{:03}.bit", i)), content).unwrap();
    }

    let out = dir.path().join("test.bitstore");
    let start = Instant::now();
    let mut store = collapse(&src, &out).unwrap();
    let collapse_time = start.elapsed();

    eprintln!("Collapse 100 files: {:?}", collapse_time);
    eprintln!("Blobs: {}", store.count_blobs().unwrap());
    eprintln!("Pages: {}", store.page_count());

    // Verify all blobs present
    assert_eq!(store.count_blobs().unwrap(), 100);
}

#[test]
fn stress_reopen_persistence() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("persist.bitstore");

    // Create and insert
    {
        let mut store = BitStore::create(&path).unwrap();
        for i in 0..1000u32 {
            store
                .insert_entity(
                    "Record",
                    &format!("r{:04}", i),
                    &json!({"data": format!("payload {}", i)}),
                )
                .unwrap();
        }
        store.flush().unwrap();
    }

    // Reopen and verify
    {
        let mut store = BitStore::open(&path).unwrap();
        for i in 0..1000u32 {
            let val = store.get_entity("Record", &format!("r{:04}", i)).unwrap();
            assert!(val.is_some(), "Missing r{:04} after reopen", i);
        }
    }
}

#[test]
fn stress_mixed_entity_types() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("mixed.bitstore");
    let mut store = BitStore::create(&path).unwrap();

    // Insert across multiple entity types
    for i in 0..500 {
        store
            .insert_entity("User", &format!("u{}", i), &json!({"type": "user"}))
            .unwrap();
        store
            .insert_entity("Team", &format!("t{}", i), &json!({"type": "team"}))
            .unwrap();
        store
            .insert_entity("Project", &format!("p{}", i), &json!({"type": "project"}))
            .unwrap();
    }
    store.flush().unwrap();

    assert_eq!(store.list_entities("User").unwrap().len(), 500);
    assert_eq!(store.list_entities("Team").unwrap().len(), 500);
    assert_eq!(store.list_entities("Project").unwrap().len(), 500);
}
