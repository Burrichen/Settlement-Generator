//! Terminal styling. Restrained: typography through bold and dim, one accent
//! colour per theme, and the terminal's own foreground for body text so
//! readability never depends on the theme.

use ratatui::style::{Color, Modifier, Style};
use settlement_core::settings::Theme;

#[derive(Debug, Clone, Copy)]
pub struct Styles {
    pub accent: Color,
    pub favourite: Color,
    pub danger: Color,
    pub ok: Color,
}

impl Styles {
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::ModernDark => Styles {
                accent: Color::LightRed,
                favourite: Color::Yellow,
                danger: Color::Red,
                ok: Color::Green,
            },
            // Brass rather than red; still plain terminal colours.
            Theme::OldTimeyDark => Styles {
                accent: Color::Yellow,
                favourite: Color::LightYellow,
                danger: Color::LightRed,
                ok: Color::Green,
            },
        }
    }

    pub fn title(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    pub fn heading(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn dim(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }

    pub fn disabled(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM | Modifier::CROSSED_OUT)
    }

    pub fn selected(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn key(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    pub fn error(&self) -> Style {
        Style::new().fg(self.danger)
    }

    pub fn good(&self) -> Style {
        Style::new().fg(self.ok)
    }

    pub fn favourite(&self) -> Style {
        Style::new().fg(self.favourite)
    }

    pub fn border(&self) -> Style {
        Style::new().add_modifier(Modifier::DIM)
    }
}
