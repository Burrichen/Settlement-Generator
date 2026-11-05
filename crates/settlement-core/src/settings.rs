//! User settings shared by both interfaces.

use serde::{Deserialize, Serialize};

use crate::engine::GenerationOptions;

/// How a settlement is generated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationMethod {
    #[default]
    Automatic,
    Manual,
}

/// How manual generation is presented. Only meaningful when the method is
/// [`GenerationMethod::Manual`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationPresentation {
    #[default]
    Incremental,
    OneTable,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    ModernDark,
    OldTimeyDark,
}

/// A named set of accent colours (buttons, selection, focus). Each theme
/// keeps its own choice; maps keep their own colours. The colours themselves
/// belong to each interface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccentPalette {
    /// Rust red with an amber focus: Modern Dark's own accent.
    #[default]
    Ember,
    /// Aged gold: Old-Timey Dark's own accent.
    Brass,
    Amethyst,
    Verdigris,
    Lapis,
    Moss,
    Glacier,
}

impl AccentPalette {
    /// Every palette, in the order the interfaces offer them.
    pub const ALL: [AccentPalette; 7] = [
        AccentPalette::Ember,
        AccentPalette::Brass,
        AccentPalette::Amethyst,
        AccentPalette::Verdigris,
        AccentPalette::Lapis,
        AccentPalette::Moss,
        AccentPalette::Glacier,
    ];

    pub fn name(self) -> &'static str {
        match self {
            AccentPalette::Ember => "Ember",
            AccentPalette::Brass => "Brass",
            AccentPalette::Amethyst => "Amethyst",
            AccentPalette::Verdigris => "Verdigris",
            AccentPalette::Lapis => "Lapis",
            AccentPalette::Moss => "Moss",
            AccentPalette::Glacier => "Glacier",
        }
    }

    /// The palette `step` places along `ALL`, wrapping at either end.
    pub fn cycle(self, step: isize) -> AccentPalette {
        let len = Self::ALL.len() as isize;
        let at = Self::ALL.iter().position(|p| *p == self).unwrap_or(0) as isize;
        Self::ALL[(at + step).rem_euclid(len) as usize]
    }
}

impl Theme {
    /// The accent a theme uses until the user picks another.
    pub fn default_accent(self) -> AccentPalette {
        match self {
            Theme::ModernDark => AccentPalette::Ember,
            Theme::OldTimeyDark => AccentPalette::Brass,
        }
    }
}

fn default_modern_dark_accent() -> AccentPalette {
    Theme::ModernDark.default_accent()
}

fn default_old_timey_dark_accent() -> AccentPalette {
    Theme::OldTimeyDark.default_accent()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub generation_method: GenerationMethod,
    /// Kept even while the method is Automatic, so switching back to Manual
    /// restores the previous choice.
    pub generation_presentation: GenerationPresentation,
    /// Ignore tag-based result eligibility; see [`GenerationOptions`].
    pub god_mode: bool,
    /// Show applied tags during generation and in saved settlements.
    pub tag_view: bool,
    /// Every settlement gets one extra Notable Location: a Tavern / Inn with 4
    /// spare rooms. See [`GenerationOptions`].
    #[serde(default)]
    pub bare_minimum_buildings: bool,
    pub theme: Theme,
    /// Advanced: show how every generated value was reached (each roll,
    /// modifier and scale move) during generation.
    #[serde(default)]
    pub roll_details: bool,
    /// Accent colours while the Modern Dark theme is active.
    #[serde(default = "default_modern_dark_accent")]
    pub modern_dark_accent: AccentPalette,
    /// Accent colours while the Old-Timey Dark theme is active.
    #[serde(default = "default_old_timey_dark_accent")]
    pub old_timey_dark_accent: AccentPalette,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            generation_method: GenerationMethod::default(),
            generation_presentation: GenerationPresentation::default(),
            god_mode: false,
            tag_view: false,
            bare_minimum_buildings: false,
            theme: Theme::default(),
            roll_details: false,
            modern_dark_accent: default_modern_dark_accent(),
            old_timey_dark_accent: default_old_timey_dark_accent(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("Generation Presentation is disabled while Generation Method is Automatic")]
    PresentationDisabled,
}

impl Settings {
    /// Whether the presentation setting is enabled. While the method is
    /// Automatic it is disabled completely: shown greyed out, not editable,
    /// and ignored, but its saved value is kept for when Manual is chosen.
    pub fn presentation_applies(&self) -> bool {
        self.generation_method == GenerationMethod::Manual
    }

    /// Changes the presentation; refused while the method is Automatic.
    pub fn set_generation_presentation(
        &mut self,
        presentation: GenerationPresentation,
    ) -> Result<(), SettingsError> {
        if !self.presentation_applies() {
            return Err(SettingsError::PresentationDisabled);
        }
        self.generation_presentation = presentation;
        Ok(())
    }

    /// Checks that moving from `previous` to `self` does not change a
    /// disabled setting.
    pub fn check_change_from(&self, previous: &Settings) -> Result<(), SettingsError> {
        if !self.presentation_applies()
            && self.generation_presentation != previous.generation_presentation
        {
            return Err(SettingsError::PresentationDisabled);
        }
        Ok(())
    }

    /// The presentation in effect, or `None` when generation is automatic.
    pub fn active_presentation(&self) -> Option<GenerationPresentation> {
        self.presentation_applies()
            .then_some(self.generation_presentation)
    }

    /// The accent palette of the active theme.
    pub fn accent(&self) -> AccentPalette {
        match self.theme {
            Theme::ModernDark => self.modern_dark_accent,
            Theme::OldTimeyDark => self.old_timey_dark_accent,
        }
    }

    /// Chooses the accent palette for the active theme only; the other
    /// theme keeps its own.
    pub fn set_accent(&mut self, palette: AccentPalette) {
        match self.theme {
            Theme::ModernDark => self.modern_dark_accent = palette,
            Theme::OldTimeyDark => self.old_timey_dark_accent = palette,
        }
    }

    pub fn generation_options(&self) -> GenerationOptions {
        GenerationOptions {
            god_mode: self.god_mode,
            bare_minimum_buildings: self.bare_minimum_buildings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let s = Settings::default();
        assert_eq!(s.theme, Theme::ModernDark);
        assert_eq!(s.generation_method, GenerationMethod::Automatic);
        assert!(!s.god_mode);
        assert!(!s.tag_view);
        assert_eq!(s.modern_dark_accent, AccentPalette::Ember);
        assert_eq!(s.old_timey_dark_accent, AccentPalette::Brass);
        assert_eq!(s.accent(), AccentPalette::Ember);
    }

    #[test]
    fn each_theme_keeps_its_own_accent() {
        let mut s = Settings::default();
        s.set_accent(AccentPalette::Lapis);
        s.theme = Theme::OldTimeyDark;
        assert_eq!(s.accent(), AccentPalette::Brass);
        s.set_accent(AccentPalette::Moss);
        assert_eq!(s.accent(), AccentPalette::Moss);
        s.theme = Theme::ModernDark;
        assert_eq!(s.accent(), AccentPalette::Lapis);
    }

    #[test]
    fn accent_cycles_and_wraps() {
        assert_eq!(AccentPalette::Ember.cycle(1), AccentPalette::Brass);
        assert_eq!(AccentPalette::Ember.cycle(-1), AccentPalette::Glacier);
        assert_eq!(AccentPalette::Glacier.cycle(1), AccentPalette::Ember);
    }

    #[test]
    fn settings_saved_before_accents_read_the_theme_defaults() {
        let old = serde_json::json!({
            "generation_method": "automatic",
            "generation_presentation": "incremental",
            "god_mode": false,
            "tag_view": false,
            "theme": "old_timey_dark"
        });
        let s: Settings = serde_json::from_value(old).unwrap();
        assert_eq!(s.modern_dark_accent, AccentPalette::Ember);
        assert_eq!(s.old_timey_dark_accent, AccentPalette::Brass);
    }

    #[test]
    fn presentation_is_kept_but_ignored_while_automatic() {
        let mut s = Settings {
            generation_method: GenerationMethod::Manual,
            generation_presentation: GenerationPresentation::OneTable,
            ..Settings::default()
        };
        assert_eq!(
            s.active_presentation(),
            Some(GenerationPresentation::OneTable)
        );

        s.generation_method = GenerationMethod::Automatic;
        assert!(!s.presentation_applies());
        assert_eq!(s.active_presentation(), None);
        assert_eq!(s.generation_presentation, GenerationPresentation::OneTable);
        assert_eq!(
            s.set_generation_presentation(GenerationPresentation::Incremental),
            Err(SettingsError::PresentationDisabled)
        );
        assert_eq!(s.generation_presentation, GenerationPresentation::OneTable);

        s.generation_method = GenerationMethod::Manual;
        assert_eq!(
            s.active_presentation(),
            Some(GenerationPresentation::OneTable)
        );
    }

    #[test]
    fn god_mode_maps_to_generation_options() {
        let s = Settings {
            god_mode: true,
            ..Settings::default()
        };
        assert!(s.generation_options().god_mode);
    }

    #[test]
    fn serialised_names_are_stable() {
        let json = serde_json::to_value(Settings {
            generation_method: GenerationMethod::Manual,
            generation_presentation: GenerationPresentation::OneTable,
            god_mode: true,
            tag_view: true,
            theme: Theme::OldTimeyDark,
            bare_minimum_buildings: true,
            roll_details: true,
            modern_dark_accent: AccentPalette::Verdigris,
            old_timey_dark_accent: AccentPalette::Glacier,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "generation_method": "manual",
                "generation_presentation": "one_table",
                "god_mode": true,
                "tag_view": true,
                "theme": "old_timey_dark",
                "bare_minimum_buildings": true,
                "roll_details": true,
                "modern_dark_accent": "verdigris",
                "old_timey_dark_accent": "glacier"
            })
        );
    }
}
