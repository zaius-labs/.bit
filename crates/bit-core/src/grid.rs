use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

// --- Public types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridCell {
    pub file_path: String,
    pub line: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridBlock {
    pub file_path: String,
    pub line_count: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridRegion {
    pub dir_path: String,
    pub total_lines: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub depth: usize,
    pub children: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridIndex {
    pub blocks: Vec<GridBlock>,
    pub regions: Vec<GridRegion>,
    pub total_lines: usize,
    pub total_files: usize,
    pub width: f64,
    pub height: f64,
}

// --- Internal types ---

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Debug)]
struct FileEntry {
    path: String,
    line_count: usize,
}

#[derive(Debug)]
struct DirEntry {
    path: String,
    files: Vec<FileEntry>,
    subdirs: Vec<DirEntry>,
    total_lines: usize,
}

// --- Constants ---

const IGNORE_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "_build",
    ".svelte-kit",
    "build",
    "dist",
    ".vite",
    "deps",
];

const MAX_LINES_PER_FILE: usize = 10_000;

// --- Workspace scanning ---

fn scan_workspace(root: &Path, ignore: &[&str]) -> DirEntry {
    let mut files = Vec::new();
    let mut subdirs = Vec::new();
    let mut total_lines: usize = 0;

    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => {
            return DirEntry {
                path: root.to_string_lossy().to_string(),
                files,
                subdirs,
                total_lines: 0,
            };
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.') && ignore.contains(&name_str.as_ref()) {
            continue;
        }
        if ignore.contains(&name_str.as_ref()) {
            continue;
        }

        if path.is_dir() {
            let sub = scan_workspace(&path, ignore);
            if sub.total_lines > 0 {
                total_lines = total_lines.saturating_add(sub.total_lines);
                subdirs.push(sub);
            }
        } else if path.is_file() {
            let lines = count_lines(&path);
            if lines > 0 {
                total_lines = total_lines.saturating_add(lines);
                files.push(FileEntry {
                    path: path.to_string_lossy().to_string(),
                    line_count: lines,
                });
            }
        }
    }

    DirEntry {
        path: root.to_string_lossy().to_string(),
        files,
        subdirs,
        total_lines,
    }
}

fn count_lines(path: &Path) -> usize {
    // Skip known binary extensions
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let binary_exts = [
            "png", "jpg", "jpeg", "gif", "bmp", "ico", "webp", "svg", "pdf", "zip", "gz", "tar",
            "bz2", "xz", "7z", "rar", "exe", "dll", "so", "dylib", "a", "o", "obj", "lib", "bin",
            "dat", "db", "sqlite", "wasm", "ttf", "otf", "woff", "woff2", "eot", "mp3", "mp4",
            "avi", "mov", "wav", "ogg", "flac", "beam", "pyc", "class", "rlib",
        ];
        if binary_exts.contains(&ext.to_lowercase().as_str()) {
            return 0;
        }
    }

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return 0,
    };

    // Check for binary content: if first 8KB contain a null byte, treat as binary
    let check_len = bytes.len().min(8192);
    if bytes[..check_len].contains(&0) {
        return 0;
    }

    let count = bytes.iter().filter(|&&b| b == b'\n').count();
    count.min(MAX_LINES_PER_FILE)
}

// --- Squarified treemap layout ---

/// An item to be laid out: name + area value
#[derive(Debug, Clone)]
struct LayoutItem {
    name: String,
    area: f64,
}

/// Compute the worst aspect ratio for a row of items laid out along the short side.
/// `row_areas` are the normalized areas (in pixel^2) of items in the row.
/// `side` is the length of the side along which the row is laid out.
fn worst_ratio(row_areas: &[f64], side: f64) -> f64 {
    if row_areas.is_empty() || side <= 0.0 {
        return f64::MAX;
    }
    let sum: f64 = row_areas.iter().sum();
    let row_length = sum / side; // the "thickness" of the row

    let mut worst = 0.0_f64;
    for &a in row_areas {
        let item_side = a / row_length;
        let ratio = if row_length > item_side {
            row_length / item_side
        } else {
            item_side / row_length
        };
        worst = worst.max(ratio);
    }
    worst
}

/// Lay out a single row of items within bounds.
/// Returns the rectangles for each item and the remaining bounds.
fn layout_row(items: &[LayoutItem], bounds: Rect) -> (Vec<(String, Rect)>, Rect) {
    if items.is_empty() {
        return (Vec::new(), bounds);
    }

    let total_area: f64 = items.iter().map(|i| i.area).sum();

    // Lay out along the shorter side
    let horizontal = bounds.w >= bounds.h;
    let mut rects = Vec::with_capacity(items.len());

    if horizontal {
        // Row is a vertical strip on the left
        let row_width = if bounds.h > 0.0 {
            total_area / bounds.h
        } else {
            bounds.w
        };
        let row_width = row_width.min(bounds.w);

        let mut y = bounds.y;
        for item in items {
            let h = if row_width > 0.0 {
                item.area / row_width
            } else {
                0.0
            };
            rects.push((
                item.name.clone(),
                Rect {
                    x: bounds.x,
                    y,
                    w: row_width,
                    h,
                },
            ));
            y += h;
        }

        let remaining = Rect {
            x: bounds.x + row_width,
            y: bounds.y,
            w: bounds.w - row_width,
            h: bounds.h,
        };
        (rects, remaining)
    } else {
        // Row is a horizontal strip on the top
        let row_height = if bounds.w > 0.0 {
            total_area / bounds.w
        } else {
            bounds.h
        };
        let row_height = row_height.min(bounds.h);

        let mut x = bounds.x;
        for item in items {
            let w = if row_height > 0.0 {
                item.area / row_height
            } else {
                0.0
            };
            rects.push((
                item.name.clone(),
                Rect {
                    x,
                    y: bounds.y,
                    w,
                    h: row_height,
                },
            ));
            x += w;
        }

        let remaining = Rect {
            x: bounds.x,
            y: bounds.y + row_height,
            w: bounds.w,
            h: bounds.h - row_height,
        };
        (rects, remaining)
    }
}

/// Squarified treemap: sort by area desc, greedily add to row while aspect ratio improves.
fn squarify(items: &[LayoutItem], bounds: Rect) -> Vec<(String, Rect)> {
    if items.is_empty() {
        return Vec::new();
    }
    if items.len() == 1 {
        return vec![(items[0].name.clone(), bounds)];
    }

    let total_item_area: f64 = items.iter().map(|i| i.area).sum();
    let bounds_area = bounds.w * bounds.h;
    if total_item_area <= 0.0 || bounds_area <= 0.0 {
        return items
            .iter()
            .map(|i| {
                (
                    i.name.clone(),
                    Rect {
                        x: bounds.x,
                        y: bounds.y,
                        w: 0.0,
                        h: 0.0,
                    },
                )
            })
            .collect();
    }

    // Normalize areas to fit bounds
    let scale = bounds_area / total_item_area;
    let scaled: Vec<LayoutItem> = items
        .iter()
        .map(|i| LayoutItem {
            name: i.name.clone(),
            area: i.area * scale,
        })
        .collect();

    squarify_recursive(&scaled, bounds)
}

fn squarify_recursive(items: &[LayoutItem], bounds: Rect) -> Vec<(String, Rect)> {
    if items.is_empty() {
        return Vec::new();
    }
    if items.len() == 1 {
        return vec![(items[0].name.clone(), bounds)];
    }
    if bounds.w <= 0.0 || bounds.h <= 0.0 {
        return items
            .iter()
            .map(|i| {
                (
                    i.name.clone(),
                    Rect {
                        x: bounds.x,
                        y: bounds.y,
                        w: 0.0,
                        h: 0.0,
                    },
                )
            })
            .collect();
    }

    let short_side = bounds.w.min(bounds.h);

    // Build the row greedily
    let mut row: Vec<LayoutItem> = Vec::new();
    let mut row_areas: Vec<f64> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let mut test_areas = row_areas.clone();
        test_areas.push(item.area);

        if row.is_empty() {
            row.push(item.clone());
            row_areas.push(item.area);
            continue;
        }

        let current_worst = worst_ratio(&row_areas, short_side);
        let new_worst = worst_ratio(&test_areas, short_side);

        if new_worst <= current_worst {
            // Adding improves or maintains aspect ratio
            row.push(item.clone());
            row_areas.push(item.area);
        } else {
            // Adding worsens — lay out current row, recurse on remainder
            let (mut rects, remaining) = layout_row(&row, bounds);
            rects.extend(squarify_recursive(&items[i..], remaining));
            return rects;
        }
    }

    // All items fit in one row
    let (rects, _) = layout_row(&row, bounds);
    rects
}

// --- Directory layout ---

fn layout_dir(dir: &DirEntry, bounds: Rect, depth: usize) -> (Vec<GridBlock>, Vec<GridRegion>) {
    let mut blocks = Vec::new();
    let mut regions = Vec::new();

    // Collect all children (files + subdirs) as layout items
    let mut items: Vec<LayoutItem> = Vec::new();
    let mut child_names: Vec<String> = Vec::new();

    // Track which names are files vs subdirs
    let mut file_map: std::collections::HashMap<String, &FileEntry> =
        std::collections::HashMap::new();
    let mut dir_map: std::collections::HashMap<String, &DirEntry> =
        std::collections::HashMap::new();

    for f in &dir.files {
        let area = f.line_count as f64;
        if area > 0.0 {
            items.push(LayoutItem {
                name: f.path.clone(),
                area,
            });
            child_names.push(f.path.clone());
            file_map.insert(f.path.clone(), f);
        }
    }

    for d in &dir.subdirs {
        let area = d.total_lines as f64;
        if area > 0.0 {
            items.push(LayoutItem {
                name: d.path.clone(),
                area,
            });
            child_names.push(d.path.clone());
            dir_map.insert(d.path.clone(), d);
        }
    }

    if items.is_empty() {
        return (blocks, regions);
    }

    // Sort by area descending for squarify
    items.sort_by(|a, b| {
        b.area
            .partial_cmp(&a.area)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let rects = squarify(&items, bounds);

    for (name, rect) in &rects {
        if let Some(file) = file_map.get(name) {
            blocks.push(GridBlock {
                file_path: file.path.clone(),
                line_count: file.line_count,
                x: rect.x,
                y: rect.y,
                width: rect.w,
                height: rect.h,
                region: dir.path.clone(),
            });
        } else if let Some(subdir) = dir_map.get(name) {
            let (sub_blocks, sub_regions) = layout_dir(subdir, *rect, depth + 1);
            blocks.extend(sub_blocks);
            regions.extend(sub_regions);
        }
    }

    // Create region for this directory
    regions.push(GridRegion {
        dir_path: dir.path.clone(),
        total_lines: dir.total_lines,
        x: bounds.x,
        y: bounds.y,
        width: bounds.w,
        height: bounds.h,
        depth,
        children: child_names,
    });

    (blocks, regions)
}

// --- Public entry point ---

pub fn build_grid_index(workspace_path: &str, width: f64, height: f64) -> GridIndex {
    let root = Path::new(workspace_path);
    let tree = scan_workspace(root, IGNORE_DIRS);
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
    };
    let (blocks, regions) = layout_dir(&tree, bounds, 0);

    GridIndex {
        total_lines: tree.total_lines,
        total_files: blocks.len(),
        width,
        height,
        blocks,
        regions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_squarify_single_item() {
        let items = vec![LayoutItem {
            name: "a".into(),
            area: 100.0,
        }];
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let result = squarify(&items, bounds);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "a");
        assert!((result[0].1.w - 10.0).abs() < 0.001);
        assert!((result[0].1.h - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_squarify_two_equal() {
        let items = vec![
            LayoutItem {
                name: "a".into(),
                area: 50.0,
            },
            LayoutItem {
                name: "b".into(),
                area: 50.0,
            },
        ];
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let result = squarify(&items, bounds);
        assert_eq!(result.len(), 2);

        // Total area should equal bounds area
        let total: f64 = result.iter().map(|r| r.1.w * r.1.h).sum();
        assert!((total - 100.0).abs() < 0.01);

        // Each rect should have positive dimensions
        for (_, r) in &result {
            assert!(r.w > 0.0);
            assert!(r.h > 0.0);
        }
    }

    #[test]
    fn test_squarify_no_overlap() {
        let items = vec![
            LayoutItem {
                name: "a".into(),
                area: 60.0,
            },
            LayoutItem {
                name: "b".into(),
                area: 30.0,
            },
            LayoutItem {
                name: "c".into(),
                area: 10.0,
            },
        ];
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let result = squarify(&items, bounds);
        assert_eq!(result.len(), 3);

        // No rect exceeds bounds
        for (_, r) in &result {
            assert!(r.x >= -0.001);
            assert!(r.y >= -0.001);
            assert!(r.x + r.w <= 10.001);
            assert!(r.y + r.h <= 10.001);
            assert!(r.w > 0.0);
            assert!(r.h > 0.0);
        }

        // Total area equals bounds
        let total: f64 = result.iter().map(|r| r.1.w * r.1.h).sum();
        assert!((total - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_count_lines() {
        // Count lines of this source file using CARGO_MANIFEST_DIR for reliable path
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = Path::new(manifest_dir).join("src/grid.rs");
        let lines = count_lines(&path);
        assert!(lines > 10, "Expected > 10 lines, got {}", lines);
    }
}
