//! Terminal styling for user-facing CLI output (`anstyle` + TTY / `NO_COLOR`).

use std::io::IsTerminal;

use anstyle::{AnsiColor, Effects, Style};

/// Whether ANSI styling is enabled for a given output stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CliStyle {
    enabled: bool,
}

impl CliStyle {
    /// Always-plain handle for tests and non-interactive formatters.
    #[allow(dead_code)] // used by unit tests; integration builds omit cfg(test)
    pub fn plain() -> Self {
        Self { enabled: false }
    }

    /// Force ANSI on/off regardless of TTY / `NO_COLOR` (tests / smokes).
    #[allow(dead_code)] // used by unit tests; integration builds omit cfg(test)
    pub fn forced(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn auto_stdout() -> Self {
        Self {
            enabled: color_enabled() && std::io::stdout().is_terminal(),
        }
    }

    pub fn auto_stderr() -> Self {
        Self {
            enabled: color_enabled() && std::io::stderr().is_terminal(),
        }
    }

    #[allow(dead_code)] // used by unit tests; integration builds omit cfg(test)
    pub fn enabled(self) -> bool {
        self.enabled
    }

    pub fn paint(self, style: Style, text: impl std::fmt::Display) -> String {
        if !self.enabled {
            return text.to_string();
        }
        format!("{}{}{}", style.render(), text, style.render_reset())
    }

    pub fn success(self, text: impl std::fmt::Display) -> String {
        self.paint(SUCCESS, text)
    }

    pub fn error(self, text: impl std::fmt::Display) -> String {
        self.paint(ERROR, text)
    }

    pub fn warn(self, text: impl std::fmt::Display) -> String {
        self.paint(WARN, text)
    }

    pub fn muted(self, text: impl std::fmt::Display) -> String {
        self.paint(MUTED, text)
    }

    pub fn accent(self, text: impl std::fmt::Display) -> String {
        self.paint(ACCENT, text)
    }
}

fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

const SUCCESS: Style = Style::new()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Green)))
    .effects(Effects::BOLD);

const ERROR: Style = Style::new()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Red)))
    .effects(Effects::BOLD);

const WARN: Style = Style::new()
    .fg_color(Some(anstyle::Color::Ansi(AnsiColor::Yellow)))
    .effects(Effects::BOLD);

const MUTED: Style = Style::new().effects(Effects::DIMMED);

const ACCENT: Style = Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan)));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_strips_styles() {
        let style = CliStyle::plain();
        assert_eq!(style.success("OK"), "OK");
        assert_eq!(style.error("nope"), "nope");
        assert_eq!(style.accent("tui-design"), "tui-design");
        assert!(!style.enabled());
    }

    #[test]
    fn forced_emits_ansi_and_reset() {
        let style = CliStyle::forced(true);
        let painted = style.success("OK");
        assert!(painted.contains("OK"), "{painted}");
        assert!(painted.contains('\u{1b}'), "{painted}");
        assert_ne!(painted, "OK");
        assert!(painted.ends_with("\u{1b}[0m") || painted.contains("\u{1b}[0m"), "{painted}");
    }

    #[test]
    fn color_enabled_respects_no_color() {
        assert_eq!(color_enabled(), std::env::var_os("NO_COLOR").is_none());
    }
}
