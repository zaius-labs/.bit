use clap::Args;
use std::error::Error;
use std::path::Path;

use bit_store::{BitStore, SearchIndex};
use crate::discover;

#[derive(Args)]
pub struct SearchArgs {
    /// Search query (or path to .bitstore if store is provided as second form)
    pub query: String,
    /// Optional: explicit path to .bitstore file (auto-discovered if omitted)
    #[arg(short, long)]
    pub store: Option<String>,
    /// Maximum number of results
    #[arg(short, long, default_value = "10")]
    pub limit: usize,
}

pub fn run(args: &SearchArgs) -> Result<(), Box<dyn Error>> {
    let store_path = discover::resolve_store(args.store.as_deref())
        .map_err(|e| -> Box<dyn Error> { e.into() })?;

    let mut store = BitStore::open(Path::new(&store_path))?;

    let mut index = SearchIndex::new();
    let entity_types = store.list_entity_types()?;

    let mut total = 0usize;
    for entity_type in &entity_types {
        let records = store.list_entities(entity_type)?;
        for (id, record) in &records {
            let key = format!("@{}:{}", entity_type, id);
            index.index_document(&key, record);
        }
        total += records.len();
    }

    eprintln!("Indexed {} entities", total);

    let results = index.search(&args.query);

    if results.is_empty() {
        println!("No results for \"{}\"", args.query);
    } else {
        let shown = results.len().min(args.limit);
        for (key, score) in results.iter().take(args.limit) {
            println!("{:<40} score={:.4}", key, score);
        }
        if results.len() > shown {
            eprintln!("... and {} more", results.len() - shown);
        }
    }

    Ok(())
}
