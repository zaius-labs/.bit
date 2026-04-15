use clap::Args;
use std::error::Error;
use std::path::Path;

use bit_store::{store_delete, BitStore};
use crate::discover;

#[derive(Args)]
pub struct DeleteArgs {
    /// Entity reference: @Entity:id
    pub entity_ref: String,
    /// Path to .bitstore file (auto-discovered if omitted)
    #[arg(short, long)]
    pub store: Option<String>,
}

pub fn run(args: &DeleteArgs) -> Result<(), Box<dyn Error>> {
    let store_path = discover::resolve_store(args.store.as_deref())
        .map_err(|e| -> Box<dyn Error> { e.into() })?;

    let mut store = BitStore::open(Path::new(&store_path))?;

    let entity_ref = args.entity_ref.trim_start_matches('@');
    let (entity, id) = entity_ref
        .split_once(':')
        .ok_or("Expected @Entity:id format")?;

    let deleted = store_delete(&mut store, entity, id)?;
    store.flush()?;

    if deleted {
        eprintln!("Deleted @{}:{}", entity, id);
    } else {
        eprintln!("Not found: @{}:{}", entity, id);
        std::process::exit(1);
    }
    Ok(())
}
