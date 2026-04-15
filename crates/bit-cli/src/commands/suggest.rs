use clap::Args;
use std::error::Error;
use std::path::Path;

use bit_store::{AutocompleteIndex, BitStore};

#[derive(Args)]
pub struct SuggestArgs {
    /// Path to .bitstore file
    pub store: String,
    /// Entity type: @Entity
    pub entity: String,
    /// Field name to get suggestions for
    pub field: String,
    /// Maximum number of suggestions
    #[arg(short, long, default_value = "10")]
    pub limit: usize,
}

pub fn run(args: &SuggestArgs) -> Result<(), Box<dyn Error>> {
    let mut store = BitStore::open(Path::new(&args.store))?;

    let entity = args.entity.trim_start_matches('@');
    let records = store.list_entities(entity)?;

    if records.is_empty() {
        eprintln!("No @{} records found", entity);
        return Ok(());
    }

    let index = AutocompleteIndex::build_from_records(entity, &records);
    let suggestions = index.suggest(entity, &args.field, args.limit);

    if suggestions.is_empty() {
        eprintln!("No suggestions for @{}.{}", entity, args.field);
        return Ok(());
    }

    for s in &suggestions {
        println!(
            "{:<30} conf={:.3}  freq={}",
            s.value, s.confidence, s.frequency
        );
    }
    eprintln!(
        "{} suggestions for @{}.{} (from {} records)",
        suggestions.len(),
        entity,
        args.field,
        records.len()
    );
    Ok(())
}
