use std::error::Error;
use std::path::Path;

/// Apply .bit files to a Claude Code harness by copying them to .claude/skills/.
pub fn apply(dir: &Path) -> Result<(), Box<dyn Error>> {
    // Find the .claude/ directory by walking up
    let claude_dir = find_claude_dir(dir).ok_or("Could not find .claude/ directory")?;
    let skills_dir = claude_dir.join("skills");
    std::fs::create_dir_all(&skills_dir)?;

    // Copy all .bit files from dir into .claude/skills/
    let mut count = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("bit") {
            let dest = skills_dir.join(path.file_name().unwrap());
            std::fs::copy(&path, &dest)?;
            eprintln!("  copied {} -> {}", path.display(), dest.display());
            count += 1;
        }
    }

    eprintln!(
        "Applied {} .bit file(s) to Claude Code ({}/)",
        count,
        skills_dir.display()
    );
    Ok(())
}

fn find_claude_dir(dir: &Path) -> Option<std::path::PathBuf> {
    let mut current = Some(dir.to_path_buf());
    while let Some(d) = current {
        let candidate = d.join(".claude");
        if candidate.is_dir() {
            return Some(candidate);
        }
        current = d.parent().map(|p| p.to_path_buf());
    }
    None
}
