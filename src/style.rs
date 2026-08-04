//! Terminal styling for CLI output (`anstyle` + TTY / `NO_COLOR`).

use std::io::IsTerminal;

use anstyle::{AnsiColor, Color, Effects, Style};
use clap::builder::styling::Styles;

type Ansi = Color;

const GREEN: Ansi = Color::Ansi(AnsiColor::Green);
const RED: Ansi = Color::Ansi(AnsiColor::Red);
const YELLOW: Ansi = Color::Ansi(AnsiColor::Yellow);
const CYAN: Ansi = Color::Ansi(AnsiColor::Cyan);
const WHITE: Ansi = Color::Ansi(AnsiColor::White);

/// Clap help/error styles.
pub const CLAP_STYLES: Styles = Styles::styled()
    .header(Style::new().fg_color(Some(CYAN)).effects(Effects::BOLD))
    .usage(Style::new().fg_color(Some(CYAN)).effects(Effects::BOLD))
    .literal(Style::new().fg_color(Some(GREEN)).effects(Effects::BOLD))
    .placeholder(Style::new().fg_color(Some(WHITE)))
    .error(Style::new().fg_color(Some(RED)).effects(Effects::BOLD))
    .valid(Style::new().fg_color(Some(GREEN)).effects(Effects::BOLD))
    .invalid(Style::new().fg_color(Some(YELLOW)).effects(Effects::BOLD));

/// Whether ANSI styling is enabled for a given output stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CliStyle {
    enabled: bool,
}

impl CliStyle {
    #[cfg(test)]
    pub fn plain() -> Self {
        Self { enabled: false }
    }

    #[cfg(test)]
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

    #[cfg(test)]
    pub fn enabled(self) -> bool {
        self.enabled
    }

    fn paint(self, style: Style, text: impl std::fmt::Display) -> String {
        if !self.enabled {
            return text.to_string();
        }
        format!("{}{}{}", style.render(), text, style.render_reset())
    }

    pub fn success(self, text: impl std::fmt::Display) -> String {
        self.paint(Style::new().fg_color(Some(GREEN)).effects(Effects::BOLD), text)
    }

    pub fn error(self, text: impl std::fmt::Display) -> String {
        self.paint(Style::new().fg_color(Some(RED)).effects(Effects::BOLD), text)
    }

    pub fn warn(self, text: impl std::fmt::Display) -> String {
        self.paint(Style::new().fg_color(Some(YELLOW)).effects(Effects::BOLD), text)
    }

    pub fn muted(self, text: impl std::fmt::Display) -> String {
        self.paint(Style::new().fg_color(Some(WHITE)).effects(Effects::DIMMED), text)
    }

    pub fn accent(self, text: impl std::fmt::Display) -> String {
        self.paint(Style::new().fg_color(Some(CYAN)), text)
    }
}

fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

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
    }

    #[test]
    fn color_enabled_respects_no_color() {
        assert_eq!(color_enabled(), std::env::var_os("NO_COLOR").is_none());
    }
}
