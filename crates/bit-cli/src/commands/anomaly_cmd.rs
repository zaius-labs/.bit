use clap::Args;
use std::error::Error;
use std::path::Path;

use bit_store::{AnomalyDetector, BitStore};

#[derive(Args)]
pub struct AnomalyArgs {
    /// Path to .bitstore file
    pub store: String,
    /// Entity type: @Entity
    pub entity: String,
    /// Anomaly score threshold (0.0-1.0)
    #[arg(short, long, default_value = "0.3")]
    pub threshold: f64,
}

pub fn run(args: &AnomalyArgs) -> Result<(), Box<dyn Error>> {
    let mut store = BitStore::open(Path::new(&args.store))?;

    let entity = args.entity.trim_start_matches('@');
    let records = store.list_entities(entity)?;

    if records.is_empty() {
        eprintln!("No @{} records found", entity);
        return Ok(());
    }

    let training_data: Vec<serde_json::Value> = records.iter().map(|(_, v)| v.clone()).collect();

    let mut detector = AnomalyDetector::new();
    detector.train(&training_data);

    let anomalies = detector.detect_anomalies(&records, args.threshold);

    if anomalies.is_empty() {
        println!(
            "No anomalies detected in {} @{} records (threshold={:.2})",
            records.len(),
            entity,
            args.threshold
        );
    } else {
        println!(
            "{} anomalies in {} @{} records:",
            anomalies.len(),
            records.len(),
            entity
        );
        for a in &anomalies {
            println!("  {} (score={:.3})", a.entity_key, a.anomaly_score);
            for f in &a.anomalous_fields {
                println!("    {}: {} — {}", f.field, f.value, f.reason);
            }
        }
    }

    Ok(())
}
