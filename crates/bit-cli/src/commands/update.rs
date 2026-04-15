use clap::Args;
use std::error::Error;
use std::path::Path;

use bit_store::{store_update, BitStore};
use crate::discover;

#[derive(Args)]
pub struct UpdateArgs {
    /// Entity reference: @Entity:id
    pub entity_ref: String,
    /// Field values: key=value pairs
    pub fields: Vec<String>,
    /// Path to .bitstore file (auto-discovered if omitted)
    #[arg(short, long)]
    pub store: Option<String>,
}

pub fn run(args: &UpdateArgs) -> Result<(), Box<dyn Error>> {
    let store_path = discover::resolve_store(args.store.as_deref())
        .map_err(|e| -> Box<dyn Error> { e.into() })?;

    let mut store = BitStore::open(Path::new(&store_path))?;

    let entity_ref = args.entity_ref.trim_start_matches('@');
    let (entity, id) = entity_ref
        .split_once(':')
        .ok_or("Expected @Entity:id format")?;

    let fields: Vec<(&str, &str)> = args
        .fields
        .iter()
        .filter_map(|f| f.split_once('='))
        .collect();

    let updated = store_update(&mut store, entity, id, &fields)?;
    store.flush()?;

    if updated {
        eprintln!("Updated @{}:{}", entity, id);
    } else {
        eprintln!("Not found: @{}:{}", entity, id);
        std::process::exit(1);
    }
    Ok(())
}
