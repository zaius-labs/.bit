use std::error::Error;
use std::path::Path;

/// Apply .bit files for a generic harness by copying to a `.bit-applied/` directory.
pub fn apply(dir: &Path) -> Result<(), Box<dyn Error>> {
    let target = dir.join(".bit-applied");
    std::fs::create_dir_all(&target)?;

    let mut count = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("bit") {
            let dest = target.join(path.file_name().unwrap());
            std::fs::copy(&path, &dest)?;
            eprintln!("  copied {} -> {}", path.display(), dest.display());
            count += 1;
        }
    }

    eprintln!("Applied {} .bit file(s) to {}/", count, target.display());
    Ok(())
}
