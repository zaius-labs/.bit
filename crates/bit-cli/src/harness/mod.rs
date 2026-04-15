pub mod claude_code;
pub mod generic;

use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum HarnessKind {
    ClaudeCode,
    Generic,
}

/// Detect harness by walking up from `dir` looking for `.claude/` directory.
pub fn detect_harness(dir: &Path) -> HarnessKind {
    let mut current = Some(dir.to_path_buf());
    while let Some(d) = current {
        if d.join(".claude").is_dir() {
            return HarnessKind::ClaudeCode;
        }
        current = d.parent().map(|p| p.to_path_buf());
    }
    HarnessKind::Generic
}
