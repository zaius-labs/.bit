use clap::Args;
use std::error::Error;
use std::path::Path;

use super::super::read_input;
use crate::discover;

#[derive(Args)]
pub struct QueryArgs {
    /// Query expression (or path to .bitstore when querying a store)
    pub expr: String,

    /// Files to query (or query string when first arg is a .bitstore)
    pub files: Vec<String>,
}

pub fn run(args: &QueryArgs) -> Result<(), Box<dyn Error>> {
    // If the first arg looks like a .bitstore file, use the store query engine
    if args.expr.ends_with(".bitstore") && Path::new(&args.expr).exists() {
        return run_store_query(&args.expr, &args.files.join(" "));
    }

    // If there are no file args, try auto-discover for store query.
    // The expr is treated as the query string.
    if args.files.is_empty() {
        let store_path = discover::resolve_store(None)?;
        return run_store_query(
            store_path.to_str().unwrap_or_default(),
            &args.expr,
        );
    }

    // Otherwise, fall back to the existing DocIndex behavior
    run_doc_query(args)
}

fn run_store_query(store_path: &str, query_str: &str) -> Result<(), Box<dyn Error>> {
    let store_path = Path::new(store_path);
    if query_str.is_empty() {
        return Err("Usage: bit query <store.bitstore> <query>".into());
    }

    let mut store = bit_store::BitStore::open(store_path)?;
    let query = bit_store::parse_query(query_str)?;
    let results = bit_store::execute_query(&mut store, &query)?;

    let output = serde_json::to_string_pretty(&results)?;
    println!("{output}");
    Ok(())
}

fn run_doc_query(args: &QueryArgs) -> Result<(), Box<dyn Error>> {
    let mut all_entities: Vec<serde_json::Value> = Vec::new();

    for file in &args.files {
        let source = read_input(file)?;
        let doc = bit_core::parse_source(&source).map_err(|e| {
            eprintln!("error parsing {}: {}", file, e);
            e
        })?;

        let index = bit_core::build_index(&doc);
        let json_str = serde_json::to_string(&index)?;
        let val: serde_json::Value = serde_json::from_str(&json_str)?;
        all_entities.push(val);
    }

    let output = if all_entities.len() == 1 {
        serde_json::to_string_pretty(&all_entities[0])?
    } else {
        serde_json::to_string_pretty(&all_entities)?
    };
    println!("{output}");

    Ok(())
}
