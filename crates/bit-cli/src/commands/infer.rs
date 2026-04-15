use clap::Args;
use std::error::Error;
use std::path::Path;

use bit_store::{infer_schema, render_inferred_schema, BitStore};

#[derive(Args)]
pub struct InferArgs {
    /// Path to .bitstore file
    pub store: String,
    /// Entity type: @Entity
    pub entity: String,
}

pub fn run(args: &InferArgs) -> Result<(), Box<dyn Error>> {
    let mut store = BitStore::open(Path::new(&args.store))?;

    let entity = args.entity.trim_start_matches('@');
    let records: Vec<serde_json::Value> = store
        .list_entities(entity)?
        .into_iter()
        .map(|(_, v)| v)
        .collect();

    if records.is_empty() {
        eprintln!("No @{} records found", entity);
        return Ok(());
    }

    let schema = infer_schema(entity, &records);
    let bit_text = render_inferred_schema(&schema);

    println!("{}", bit_text);
    eprintln!(
        "Inferred schema from {} @{} records ({} fields)",
        schema.record_count, entity, schema.field_count
    );
    Ok(())
}
