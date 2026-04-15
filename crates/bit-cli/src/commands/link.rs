use clap::Args;
use std::error::Error;
use std::path::Path;

use bit_store::{BitStore, EntityLinker};

#[derive(Args)]
pub struct LinkArgs {
    /// Path to .bitstore file
    pub store: String,
    /// Text mention to resolve
    pub mention: String,
}

pub fn run(args: &LinkArgs) -> Result<(), Box<dyn Error>> {
    let mut store = BitStore::open(Path::new(&args.store))?;

    // Build linker from all entity types in the store
    let mut linker = EntityLinker::new();
    let entity_types = store.list_entity_types()?;

    let mut total = 0usize;
    for entity_type in &entity_types {
        let records = store.list_entities(entity_type)?;
        for (id, _) in &records {
            linker.register_entity(entity_type, id);
            linker.register_alias(id, entity_type, id);
        }
        total += records.len();
    }
    linker.build_aliases();

    eprintln!(
        "Loaded {} entities across {} types",
        total,
        entity_types.len()
    );

    match linker.resolve(&args.mention) {
        Some(link) => {
            println!("@{}:{}", link.entity_type, link.entity_id);
            println!("  method:     {:?}", link.method);
            println!("  confidence: {:.3}", link.confidence);
        }
        None => {
            println!("No match for \"{}\"", args.mention);
        }
    }

    Ok(())
}
