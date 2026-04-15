use clap::Args;
use std::error::Error;
use std::path::{Path, PathBuf};

use crate::harness::{self, HarnessKind};
use super::annotate::{AnnotationEvent, read_annotation_file, append_annotation};

#[derive(Args)]
pub struct ApplyArgs {
    /// Directory containing .bit files to apply
    pub dir: String,

    /// Override harness detection (claude-code, generic)
    #[arg(long)]
    pub harness: Option<String>,

    /// Path to write annotation sidecar (NDJSON). When set, enables annotation mode.
    #[arg(long)]
    pub annotate: Option<String>,

    /// Path to SpanIndex file (.span.json) to merge annotations into.
    #[arg(long)]
    pub span_index: Option<String>,

    /// Minimum confidence to include (0.0–1.0). Skips constructs below this threshold.
    #[arg(long, default_value = "0.0")]
    pub confidence: f32,

    /// Show what would happen without executing.
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(args: &ApplyArgs) -> Result<(), Box<dyn Error>> {
    let dir = Path::new(&args.dir).canonicalize().map_err(|e| {
        eprintln!("error: cannot resolve directory '{}': {}", args.dir, e);
        e
    })?;

    let kind = match &args.harness {
        Some(name) => match name.as_str() {
            "claude-code" | "claude" => HarnessKind::ClaudeCode,
            "generic" => HarnessKind::Generic,
            other => {
                eprintln!("error: unknown harness '{}'", other);
                return Err(format!("unknown harness: {}", other).into());
            }
        },
        None => harness::detect_harness(&dir),
    };

    eprintln!("Detected harness: {:?}", kind);

    // If --annotate is set, run annotation merge path after staging
    let annotation_result = if let Some(ref annotate_path) = args.annotate {
        let annotate_path = PathBuf::from(annotate_path);

        if args.dry_run {
            eprintln!("[dry-run] Would write annotations to {:?}", annotate_path);
            run_annotation_dry_run(&dir, &annotate_path, args.confidence)?;
        } else {
            run_annotation_merge(&dir, &annotate_path, args.span_index.as_deref(), args.confidence)?;
        }
        true
    } else {
        false
    };

    // Always run the original staging behavior (unless dry-run)
    if !args.dry_run {
        match kind {
            HarnessKind::ClaudeCode => harness::claude_code::apply(&dir)?,
            HarnessKind::Generic => harness::generic::apply(&dir)?,
        }
    }

    if annotation_result {
        eprintln!("Annotation merge complete.");
    }

    Ok(())
}

fn timestamp() -> String {
    // Use a simple epoch seconds timestamp (avoids chrono dependency)
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", secs)
}

fn run_annotation_dry_run(
    dir: &Path,
    annotation_path: &Path,
    _min_confidence: f32,
) -> Result<(), Box<dyn Error>> {
    let existing = read_annotation_file(annotation_path);
    eprintln!("[dry-run] Existing annotations: {}", existing.len());

    // List .bit files that would be processed
    let bit_files = find_bit_files(dir);
    eprintln!("[dry-run] .bit files found: {}", bit_files.len());
    for f in &bit_files {
        eprintln!("  {:?}", f);
    }

    Ok(())
}

fn run_annotation_merge(
    dir: &Path,
    annotation_path: &Path,
    span_index_path: Option<&str>,
    min_confidence: f32,
) -> Result<(), Box<dyn Error>> {
    let existing = read_annotation_file(annotation_path);
    let existing_ids: std::collections::HashSet<String> = existing.iter()
        .map(|e| e.construct_id().to_string())
        .collect();

    eprintln!("Loaded {} existing annotations", existing_ids.len());

    // Find .bit files in the directory
    let bit_files = find_bit_files(dir);
    eprintln!("Found {} .bit files", bit_files.len());

    let mut merged_count = 0usize;

    // For each .bit file, scan for constructs with nl_confidence above threshold
    // and emit ImplSkipped events for constructs below threshold
    for bit_file in &bit_files {
        let source = match std::fs::read_to_string(bit_file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: could not read {:?}: {}", bit_file, e);
                continue;
            }
        };

        // Parse the file for construct names (simple heuristic: look for define:/flow:/etc. lines)
        let constructs = extract_construct_ids_from_source(&source);

        for (construct_id, confidence) in constructs {
            if existing_ids.contains(&construct_id) {
                continue; // Already annotated
            }

            if confidence < min_confidence {
                let event = AnnotationEvent::ImplSkipped {
                    construct_id,
                    reason: format!("confidence {:.2} below threshold {:.2}", confidence, min_confidence),
                    timestamp: timestamp(),
                };
                append_annotation(annotation_path, &event)?;
                merged_count += 1;
            }
        }
    }

    // If span_index_path is provided, merge ImplComplete events into it
    if let Some(si_path) = span_index_path {
        let si_path = Path::new(si_path);
        merge_into_span_index(si_path, annotation_path)?;
    }

    eprintln!("Wrote {} new annotation events", merged_count);
    Ok(())
}

/// Merge ImplComplete events from annotation sidecar into a .span.json SpanIndex file.
pub fn merge_into_span_index(
    span_index_path: &Path,
    annotation_path: &Path,
) -> Result<(), Box<dyn Error>> {
    use bit_nl_core::{SpanIndex, AnnotationMergeEntry};

    // Load or create SpanIndex
    let mut span_index = if span_index_path.exists() {
        let json = std::fs::read_to_string(span_index_path)?;
        SpanIndex::from_json(&json).unwrap_or_default()
    } else {
        SpanIndex::default()
    };

    // Collect ImplComplete events
    let events = read_annotation_file(annotation_path);
    let entries: Vec<AnnotationMergeEntry> = events.iter()
        .filter_map(|e| {
            if let AnnotationEvent::ImplComplete { construct_id, file, function, start_line, .. } = e {
                Some(AnnotationMergeEntry {
                    construct_id: construct_id.clone(),
                    file: file.clone(),
                    function: function.clone(),
                    start_line: *start_line,
                })
            } else {
                None
            }
        })
        .collect();

    let updated = span_index.merge_annotations(&entries);
    eprintln!("Merged {} impl locations into SpanIndex", updated);

    // Save updated SpanIndex
    std::fs::write(span_index_path, span_index.to_json())?;

    Ok(())
}

fn find_bit_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "bit").unwrap_or(false) {
                files.push(path);
            }
        }
    }
    files
}

/// Extract (construct_id, confidence) pairs from .bit source text.
/// Simple heuristic: looks for confidence gate annotations.
fn extract_construct_ids_from_source(source: &str) -> Vec<(String, f32)> {
    let mut results = Vec::new();
    let mut current_construct: Option<String> = None;
    let mut current_conf = 1.0f32;

    for line in source.lines() {
        let trimmed = line.trim();

        // Detect construct start: define:@Name, [!] name, flow: name, gate: name
        if let Some(name) = parse_construct_name(trimmed) {
            // Save previous
            if let Some(id) = current_construct.take() {
                results.push((id, current_conf));
                current_conf = 1.0;
            }
            current_construct = Some(name);
        }

        // Detect confidence gate: gate: confidence_0.71
        if trimmed.starts_with("gate: confidence_") {
            if let Some(conf_str) = trimmed.strip_prefix("gate: confidence_") {
                if let Ok(c) = conf_str.parse::<f32>() {
                    current_conf = c;
                }
            }
        }
    }

    if let Some(id) = current_construct {
        results.push((id, current_conf));
    }

    results
}

fn parse_construct_name(line: &str) -> Option<String> {
    // define:@EntityName
    if let Some(rest) = line.strip_prefix("define:@") {
        let name = rest.split_whitespace().next().unwrap_or("unknown");
        return Some(format!("define_{}", name.to_lowercase()));
    }
    // [!] task name  or [o] task name
    if line.starts_with("[!]") || line.starts_with("[o]") || line.starts_with("[x]") {
        let name = line[3..].trim().split_whitespace().take(3).collect::<Vec<_>>().join("_");
        return Some(format!("task_{}", name.to_lowercase()));
    }
    // flow: name
    if let Some(rest) = line.strip_prefix("flow:") {
        let name = rest.trim().split_whitespace().next().unwrap_or("unknown");
        return Some(format!("flow_{}", name.to_lowercase()));
    }
    // gate: name (but not gate: confidence_N)
    if let Some(rest) = line.strip_prefix("gate:") {
        let name = rest.trim().split_whitespace().next().unwrap_or("unknown");
        if !name.starts_with("confidence_") && name != "UNKNOWN" {
            return Some(format!("gate_{}", name.to_lowercase()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_extract_construct_ids_define() {
        let source = "define:@User\n  name: string\n";
        let constructs = extract_construct_ids_from_source(source);
        assert_eq!(constructs.len(), 1);
        assert_eq!(constructs[0].0, "define_user");
        assert!((constructs[0].1 - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_extract_construct_ids_task() {
        let source = "[!] user login\n  gate: confidence_0.71\n";
        let constructs = extract_construct_ids_from_source(source);
        assert_eq!(constructs.len(), 1);
        assert_eq!(constructs[0].0, "task_user_login");
        assert!((constructs[0].1 - 0.71).abs() < 0.01);
    }

    #[test]
    fn test_extract_construct_ids_empty() {
        let constructs = extract_construct_ids_from_source("# just a comment\n");
        assert!(constructs.is_empty());
    }

    #[test]
    fn test_find_bit_files_empty_dir() {
        let dir = std::env::temp_dir().join("test_bit_apply_empty");
        fs::create_dir_all(&dir).unwrap();
        let files = find_bit_files(&dir);
        assert!(files.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_find_bit_files_finds_bit_extension() {
        let dir = std::env::temp_dir().join("test_bit_apply_find");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("test.bit"), "define:@Test").unwrap();
        fs::write(dir.join("ignore.rs"), "fn main() {}").unwrap();
        let files = find_bit_files(&dir);
        assert_eq!(files.len(), 1);
        assert!(files[0].to_str().unwrap().ends_with("test.bit"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_timestamp_is_numeric() {
        let ts = timestamp();
        assert!(ts.parse::<u64>().is_ok(), "timestamp should be numeric: {}", ts);
    }

    #[test]
    fn test_parse_construct_name_define() {
        assert_eq!(
            parse_construct_name("define:@User"),
            Some("define_user".to_string())
        );
    }

    #[test]
    fn test_parse_construct_name_task() {
        assert_eq!(
            parse_construct_name("[!] user login"),
            Some("task_user_login".to_string())
        );
    }

    #[test]
    fn test_parse_construct_name_none() {
        assert_eq!(parse_construct_name("  name: string"), None);
        assert_eq!(parse_construct_name("# comment"), None);
        assert_eq!(parse_construct_name("gate: confidence_0.71"), None);
    }
}
