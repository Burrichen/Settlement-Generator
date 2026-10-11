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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub generation_method: GenerationMethod,
    /// Kept even while the method is Automatic, so switching back to Manual
    /// restores the previous choice.
    pub generation_presentation: GenerationPresentation,
    /// Ignore tag-based result eligibility; see [`GenerationOptions`].
    pub god_mode: bool,
    /// Show applied tags during generation and in saved settlements.
    pub tag_view: bool,
    pub theme: Theme,
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

    pub fn generation_options(&self) -> GenerationOptions {
        GenerationOptions {
            god_mode: self.god_mode,
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
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "generation_method": "manual",
                "generation_presentation": "one_table",
                "god_mode": true,
                "tag_view": true,
                "theme": "old_timey_dark"
            })
        );
    }
}
