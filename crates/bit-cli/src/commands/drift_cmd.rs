use clap::Args;
use std::error::Error;
use std::path::Path;

use bit_store::{BitStore, DriftBaseline};

#[derive(Args)]
pub struct DriftArgs {
    /// Path to .bitstore file
    pub store: String,
    /// Entity type: @Entity
    pub entity: String,
    /// Split ratio for baseline (first N% used as baseline)
    #[arg(short, long, default_value = "50")]
    pub split: usize,
}

pub fn run(args: &DriftArgs) -> Result<(), Box<dyn Error>> {
    let mut store = BitStore::open(Path::new(&args.store))?;

    let entity = args.entity.trim_start_matches('@');
    let records = store.list_entities(entity)?;

    if records.len() < 4 {
        eprintln!(
            "Need at least 4 @{} records for drift detection, found {}",
            entity,
            records.len()
        );
        return Ok(());
    }

    let split_pct = args.split.clamp(10, 90);
    let split_idx = records.len() * split_pct / 100;

    let baseline_records: Vec<serde_json::Value> = records[..split_idx]
        .iter()
        .map(|(_, v)| v.clone())
        .collect();
    let test_records: Vec<serde_json::Value> = records[split_idx..]
        .iter()
        .map(|(_, v)| v.clone())
        .collect();

    eprintln!(
        "Baseline: {} records, Test: {} records ({}% split)",
        baseline_records.len(),
        test_records.len(),
        split_pct
    );

    let baseline = DriftBaseline::build(entity, &baseline_records);
    let alerts = baseline.detect(entity, &test_records);

    if alerts.is_empty() {
        println!("No drift detected for @{}", entity);
    } else {
        println!("{} drift alerts for @{}:", alerts.len(), entity);
        for alert in &alerts {
            println!(
                "  [{:?}] severity={:.2} — {}",
                alert.drift_type, alert.severity, alert.description
            );
        }
    }

    Ok(())
}
