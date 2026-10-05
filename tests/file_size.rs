//! Fails when a Rust source file passes the 1,000-line limit.
//! Files already over the limit are listed in `ALLOWED` with their current size:
//! they may shrink, not grow. Lower or delete an entry after splitting a file.

use std::fs;
use std::path::{Path, PathBuf};

const LIMIT: usize = 1000;
const ALLOWED: &[(&str, usize)] = &[
    ("tests/acceptance.rs", 8252),
    ("src/skillsets.rs", 1667),
    ("src/lib.rs", 1492),
    ("src/skills.rs", 1196),
    ("src/manifest.rs", 1057),
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn source_files_stay_within_the_line_limit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in ["src", "tests"] {
        rust_files(&root.join(dir), &mut files);
    }
    let mut problems = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(root)
            .expect("under root")
            .to_string_lossy()
            .replace('\\', "/");
        let lines = fs::read_to_string(path)
            .expect("utf-8 source")
            .lines()
            .count();
        match ALLOWED.iter().find(|(name, _)| *name == rel) {
            Some((_, cap)) if lines > *cap => problems.push(format!(
                "{rel}: {lines} lines; allowed {cap} until split, and it may not grow"
            )),
            None if lines > LIMIT => problems.push(format!("{rel}: {lines} lines; limit {LIMIT}")),
            _ => {}
        }
    }
    for (name, _) in ALLOWED {
        if !root.join(name).is_file() {
            problems.push(format!(
                "{name}: listed in ALLOWED but missing; remove the entry"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "split the file or move code to its owner:\n  {}",
        problems.join("\n  ")
    );
}
