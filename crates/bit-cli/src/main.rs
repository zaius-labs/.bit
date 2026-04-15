use clap::Parser;
use std::io::Read;

mod commands;
pub mod discover;
mod harness;

#[derive(Parser)]
#[command(name = "bit", about = "The .bit language toolkit", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Parse a .bit file and output JSON AST
    Parse(commands::parse::ParseArgs),
    /// Format a .bit file
    Fmt(commands::fmt::FmtArgs),
    /// Validate a .bit file against a schema
    Validate(commands::validate::ValidateArgs),
    /// Render a .bit AST back to .bit text
    Render(commands::render::RenderArgs),
    /// Query .bit files or a .bitstore and output matching entities as JSON
    Query(commands::query::QueryArgs),
    /// Run checks defined in a .bit file
    Check(commands::check::CheckArgs),
    /// Collapse .bit files into a .bitstore archive
    Collapse(commands::collapse::CollapseArgs),
    /// Expand a .bitstore archive into .bit files
    Expand(commands::expand::ExpandArgs),
    /// Compare a .bitstore against expanded files on disk
    Status(commands::status::StatusArgs),
    /// Watch a directory for .bit file changes (NDJSON output)
    Watch(commands::watch::WatchArgs),
    /// Convert between JSON/Markdown and .bit format
    Convert(commands::convert::ConvertArgs),
    /// Initialize a new .bit project
    Init(commands::init::InitArgs),
    /// Apply .bit files to a detected harness
    Apply(commands::apply::ApplyArgs),
    /// Insert an entity into a .bitstore
    Insert(commands::insert::InsertArgs),
    /// Update an entity in a .bitstore
    Update(commands::update::UpdateArgs),
    /// Delete an entity from a .bitstore
    Delete(commands::delete::DeleteArgs),
    /// Show summary info about a .bitstore
    Info(commands::info::InfoArgs),
    /// Show page map of a .bitstore
    Pages(commands::pages::PagesArgs),
    /// Infer a .bit schema from entity data in a store
    Infer(commands::infer::InferArgs),
    /// Suggest autocomplete values for an entity field
    Suggest(commands::suggest::SuggestArgs),
    /// Detect schema and distribution drift in entity data
    Drift(commands::drift_cmd::DriftArgs),
    /// Resolve a text mention to a known entity
    Link(commands::link::LinkArgs),
    /// Full-text BM25 search across all entities in a store
    Search(commands::search_cmd::SearchArgs),
    /// Detect anomalous entity records using statistical analysis
    Anomaly(commands::anomaly_cmd::AnomalyArgs),
}

/// Read input from a file path or stdin (when path is "-").
pub fn read_input(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    if path == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        Ok(std::fs::read_to_string(path)?)
    }
}

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Parse(args) => commands::parse::run(args),
        Commands::Fmt(args) => commands::fmt::run(args),
        Commands::Validate(args) => commands::validate::run(args),
        Commands::Render(args) => commands::render::run(args),
        Commands::Query(args) => commands::query::run(args),
        Commands::Check(args) => commands::check::run(args),
        Commands::Collapse(args) => commands::collapse::run(args),
        Commands::Expand(args) => commands::expand::run(args),
        Commands::Status(args) => commands::status::run(args),
        Commands::Watch(args) => commands::watch::run(args),
        Commands::Convert(args) => commands::convert::run(args),
        Commands::Init(args) => commands::init::run(args),
        Commands::Apply(args) => commands::apply::run(args),
        Commands::Insert(args) => commands::insert::run(args),
        Commands::Update(args) => commands::update::run(args),
        Commands::Delete(args) => commands::delete::run(args),
        Commands::Info(args) => commands::info::run(args),
        Commands::Pages(args) => commands::pages::run(args),
        Commands::Infer(args) => commands::infer::run(args),
        Commands::Suggest(args) => commands::suggest::run(args),
        Commands::Drift(args) => commands::drift_cmd::run(args),
        Commands::Link(args) => commands::link::run(args),
        Commands::Search(args) => commands::search_cmd::run(args),
        Commands::Anomaly(args) => commands::anomaly_cmd::run(args),
    };

    if result.is_err() {
        std::process::exit(1);
    }
}
