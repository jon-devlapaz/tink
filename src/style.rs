//! Terminal styling for user-facing CLI output (`anstyle` + TTY / `NO_COLOR`).
//!
//! Brand palette sampled from the tink mark (deep indigo / mauve on cream),
//! lifted for contrast on typical dark terminals while keeping the same hue.

use std::io::IsTerminal;

use anstyle::{AnsiColor, Color, Effects, RgbColor, Style};
use clap::builder::styling::Styles;

/// Logo deep indigo `#381048`, brightened for dark terminals.
const INDIGO: Color = Color::Rgb(RgbColor(0xB5, 0x7B, 0xC9));
/// Logo mauve `#905098`, brightened for dark terminals.
const MAUVE: Color = Color::Rgb(RgbColor(0xD4, 0xA8, 0xDC));
/// Logo soft lavender-gray `#c8c0c8`.
const LAVENDER: Color = Color::Rgb(RgbColor(0xC8, 0xC0, 0xC8));

/// Clap help/error styles aligned with tink's success/error/warn/accent roles.
pub const CLAP_STYLES: Styles = Styles::styled()
    .header(Style::new().fg_color(Some(INDIGO)).effects(Effects::BOLD))
    .usage(Style::new().fg_color(Some(INDIGO)).effects(Effects::BOLD))
    .literal(Style::new().fg_color(Some(MAUVE)).effects(Effects::BOLD))
    .placeholder(Style::new().fg_color(Some(LAVENDER)))
    .error(
        Style::new()
            .fg_color(Some(Color::Ansi(AnsiColor::Red)))
            .effects(Effects::BOLD),
    )
    .valid(
        Style::new()
            .fg_color(Some(Color::Ansi(AnsiColor::Green)))
            .effects(Effects::BOLD),
    )
    .invalid(Style::new().fg_color(Some(MAUVE)).effects(Effects::BOLD));

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
    .fg_color(Some(Color::Ansi(AnsiColor::Green)))
    .effects(Effects::BOLD);

const ERROR: Style = Style::new()
    .fg_color(Some(Color::Ansi(AnsiColor::Red)))
    .effects(Effects::BOLD);

const WARN: Style = Style::new()
    .fg_color(Some(MAUVE))
    .effects(Effects::BOLD);

const MUTED: Style = Style::new()
    .fg_color(Some(LAVENDER))
    .effects(Effects::DIMMED);

const ACCENT: Style = Style::new().fg_color(Some(INDIGO));

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
