//! Terminal styling for CLI output (`anstyle` + TTY / `NO_COLOR`).

use std::io::IsTerminal;

use anstyle::{AnsiColor, Color, Effects, Style};
use clap::builder::styling::Styles;

type Ansi = Color;

const GREEN: Ansi = Color::Ansi(AnsiColor::Green);
const RED: Ansi = Color::Ansi(AnsiColor::Red);
const YELLOW: Ansi = Color::Ansi(AnsiColor::Yellow);
const CYAN: Ansi = Color::Ansi(AnsiColor::Cyan);
const BLUE: Ansi = Color::Ansi(AnsiColor::Blue);
const MAGENTA: Ansi = Color::Ansi(AnsiColor::Magenta);
const WHITE: Ansi = Color::Ansi(AnsiColor::White);

const RAINBOW: [Ansi; 6] = [RED, YELLOW, GREEN, CYAN, BLUE, MAGENTA];

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

    /// Skill names — magenta so they read apart from cyan paths/accents.
    pub fn skill(self, text: impl std::fmt::Display) -> String {
        self.paint(Style::new().fg_color(Some(MAGENTA)).effects(Effects::BOLD), text)
    }

    /// Per-character rainbow (init ZEN tease). Plain text when styling is off.
    pub fn rainbow(self, text: impl std::fmt::Display) -> String {
        let text = text.to_string();
        if !self.enabled {
            return text;
        }
        let mut out = String::with_capacity(text.len() * 12);
        let mut color_i = 0usize;
        for ch in text.chars() {
            if ch.is_whitespace() {
                out.push(ch);
                continue;
            }
            let style = Style::new()
                .fg_color(Some(RAINBOW[color_i % RAINBOW.len()]))
                .effects(Effects::BOLD);
            out.push_str(&format!("{}{}{}", style.render(), ch, style.render_reset()));
            color_i += 1;
        }
        out
    }

    /// Clickable terminal hyperlink (OSC 8). Plain label when styling is off.
    pub fn link(self, url: &str, text: impl std::fmt::Display) -> String {
        let label = text.to_string();
        if !self.enabled {
            return label;
        }
        let painted = self.paint(
            Style::new()
                .fg_color(Some(CYAN))
                .effects(Effects::UNDERLINE),
            &label,
        );
        format!("\u{1b}]8;;{url}\u{1b}\\{painted}\u{1b}]8;;\u{1b}\\")
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
        assert_eq!(style.skill("manage-tink"), "manage-tink");
        assert_eq!(style.rainbow("ZEN.md"), "ZEN.md");
        assert_eq!(
            style.link("https://github.com/jon-devlapaz/tink-skills", "tink-skills"),
            "tink-skills"
        );
        assert!(!style.enabled());
    }

    #[test]
    fn forced_emits_ansi_and_reset() {
        let style = CliStyle::forced(true);
        let painted = style.success("OK");
        assert!(painted.contains("OK"), "{painted}");
        assert!(painted.contains('\u{1b}'), "{painted}");
        assert_ne!(painted, "OK");
        let skill = style.skill("manage-tink");
        assert!(skill.contains("manage-tink"), "{skill}");
        assert!(skill.contains('\u{1b}'), "{skill}");
        let rainbow = style.rainbow("ZEN.md");
        assert!(rainbow.contains('Z') && rainbow.contains('N'), "{rainbow}");
        assert!(rainbow.contains('\u{1b}'), "{rainbow}");
        assert_ne!(rainbow, "ZEN.md");
        let link = style.link("https://github.com/jon-devlapaz/tink-skills", "tink-skills");
        assert!(link.contains("tink-skills"), "{link}");
        assert!(
            link.contains("\u{1b}]8;;https://github.com/jon-devlapaz/tink-skills\u{1b}\\"),
            "{link}"
        );
    }

    #[test]
    fn color_enabled_respects_no_color() {
        assert_eq!(color_enabled(), std::env::var_os("NO_COLOR").is_none());
    }
}
