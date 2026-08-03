//! Templates written by `tink init` (ZEN + AGENTS reference).

pub const ZEN_FILENAME: &str = "ZEN.md";

/// Canonical Zen text — kept identical to the repo's `ZEN.md`.
pub const ZEN: &str = include_str!("../ZEN.md");

pub const ZEN_REFERENCE: &str = "\
## Maintainability

Follow the maintainability principles in [ZEN.md](ZEN.md).
";

pub const ZEN_REFERENCE_MARKER: &str = "[ZEN.md](ZEN.md)";

pub const TWOTINK_SOURCE: &str = "jon-devlapaz/twotink";
pub const TWOTINK_SKILLS: &[&str] = &["skill-scout", "skill-eval-loop"];
