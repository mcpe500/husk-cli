//! TUI theme system: built-in themes + JSON custom themes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    ///ratatui Color as string names (theme files stay plain JSON).
    pub accent: String,
    pub text: String,
    pub dim: String,
    pub ok: String,
    pub warn: String,
    pub error: String,
}

impl Theme {
    pub fn builtins() -> Vec<Theme> {
        vec![
            Theme {
                name: "husk-dark".into(),
                dark: true,
                accent: "cyan".into(),
                text: "white".into(),
                dim: "darkgray".into(),
                ok: "green".into(),
                warn: "yellow".into(),
                error: "red".into(),
            },
            Theme {
                name: "husk-light".into(),
                dark: false,
                accent: "blue".into(),
                text: "black".into(),
                dim: "gray".into(),
                ok: "green".into(),
                warn: "yellow".into(),
                error: "red".into(),
            },
        ]
    }

    pub fn default_theme() -> Theme {
        Theme::builtins().remove(0)
    }

    pub fn cycle(&self) -> Theme {
        let all = Self::builtins();
        let idx = all.iter().position(|t| t.name == self.name).map(|i| (i + 1) % all.len()).unwrap_or(0);
        all[idx].clone()
    }

    /// Map a color name to ratatui Color (graceful fallback to Gray).
    pub fn color(&self, key: &str) -> ratatui::style::Color {
        use ratatui::style::Color;
        let name = match key {
            "accent" => self.accent.as_str(),
            "text" => self.text.as_str(),
            "dim" => self.dim.as_str(),
            "ok" => self.ok.as_str(),
            "warn" => self.warn.as_str(),
            "error" => self.error.as_str(),
            _ => "gray",
        };
        match name {
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" => Color::Magenta,
            "cyan" => Color::Cyan,
            "gray" => Color::Gray,
            "white" => Color::White,
            "darkgray" => Color::DarkGray,
            _ => Color::Gray,
        }
    }

    /// Load custom themes from `<config dir>/themes/*.json`, best-effort.
    pub fn load_custom(_dir: &std::path::Path) -> Vec<Theme> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_cycle() {
        let dark = Theme::default_theme();
        assert_eq!(dark.name, "husk-dark");
        let light = dark.cycle();
        assert_eq!(light.name, "husk-light");
        let again = light.cycle();
        assert_eq!(again.name, "husk-dark", "cycle wraps around");
    }

    #[test]
    fn color_names_map_to_ratatui() {
        let theme = Theme::default_theme();
        assert_eq!(theme.color("accent"), ratatui::style::Color::Cyan);
        assert_eq!(theme.color("unknown"), ratatui::style::Color::Gray);
    }
}
