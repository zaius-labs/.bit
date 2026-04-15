use clap::Args;
use std::collections::HashMap;
use std::error::Error;

use bit_core::gate::GateContext;
use bit_core::index::DocIndex;
use bit_core::mutate::RecordStore;
use bit_core::schema::SchemaRegistry;

use super::super::read_input;

#[derive(Args)]
pub struct CheckArgs {
    /// File to check (use "-" for stdin)
    pub file: String,
}

pub fn run(args: &CheckArgs) -> Result<(), Box<dyn Error>> {
    let source = read_input(&args.file)?;

    let doc = bit_core::parse_source(&source).map_err(|e| {
        eprintln!("parse error: {}", e);
        e
    })?;

    let schemas = SchemaRegistry::new();
    let store = RecordStore::new();
    let gate_context = GateContext {
        store: RecordStore::new(),
        index: DocIndex::default(),
        vars: HashMap::new(),
        completed_tasks: Vec::new(),
        task_results: HashMap::new(),
        submitted_forms: Vec::new(),
        task_scores: HashMap::new(),
    };
    let config = HashMap::new();
    let root_path = if args.file != "-" { &args.file } else { "." };

    let result =
        bit_core::check::execute_checks(&doc, root_path, &schemas, &store, &gate_context, &config);

    let json = serde_json::to_string_pretty(&result)?;
    println!("{json}");

    if result.failed > 0 {
        eprintln!(
            "{} passed, {} failed, {} skipped",
            result.passed, result.failed, result.skipped
        );
        std::process::exit(1);
    } else {
        eprintln!("{} passed, {} skipped", result.passed, result.skipped);
    }

    Ok(())
}
