use clap::Args;
use std::error::Error;
use std::path::Path;

use bit_store::BitStore;
use crate::discover;

#[derive(Args)]
pub struct InfoArgs {
    /// Path to .bitstore file (auto-discovered if omitted)
    pub store: Option<String>,
}

pub fn run(args: &InfoArgs) -> Result<(), Box<dyn Error>> {
    let store_path = discover::resolve_store(args.store.as_deref())
        .map_err(|e| -> Box<dyn Error> { e.into() })?;

    let mut store = BitStore::open(Path::new(&store_path))?;
    let info = store.info()?;

    println!("Pages:     {}", info.page_count);
    println!("Entities:  {}", info.entity_count);
    println!("Tasks:     {}", info.task_count);
    println!("Flows:     {}", info.flow_count);
    println!("Schemas:   {}", info.schema_count);
    println!("Blobs:     {}", info.blob_count);

    Ok(())
}
