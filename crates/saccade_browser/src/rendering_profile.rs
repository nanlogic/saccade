use std::env;
use std::fmt;
use std::str::FromStr;

use anyhow::{Result, anyhow};
use servo::Preferences;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderingProfile {
    ServoSafe,
    ServoModern,
    ChromeReference,
}

#[derive(Debug, Clone)]
pub struct RenderingProfileSettings {
    pub profile: RenderingProfile,
    pub layout_grid_enabled: bool,
    pub legacy_grid_override: Option<bool>,
}

impl RenderingProfile {
    pub fn name(self) -> &'static str {
        match self {
            Self::ServoSafe => "servo-safe",
            Self::ServoModern => "servo-modern",
            Self::ChromeReference => "chrome-reference",
        }
    }

    pub fn engine(self) -> &'static str {
        match self {
            Self::ServoSafe | Self::ServoModern => "servo",
            Self::ChromeReference => "chrome",
        }
    }

    pub fn default_layout_grid_enabled(self) -> bool {
        match self {
            Self::ServoSafe | Self::ChromeReference => false,
            Self::ServoModern => true,
        }
    }

    pub fn resolve(requested: Option<Self>) -> Result<RenderingProfileSettings> {
        Self::resolve_with_default(requested, Self::ServoSafe)
    }

    pub fn resolve_with_default(
        requested: Option<Self>,
        default_profile: Self,
    ) -> Result<RenderingProfileSettings> {
        let profile = if let Some(profile) = requested {
            profile
        } else if let Ok(value) = env::var("SACCADE_RENDERING_PROFILE") {
            value.parse()?
        } else {
            default_profile
        };
        let legacy_grid_override = legacy_grid_override_from_env()?;
        let layout_grid_enabled =
            legacy_grid_override.unwrap_or_else(|| profile.default_layout_grid_enabled());
        Ok(RenderingProfileSettings {
            profile,
            layout_grid_enabled,
            legacy_grid_override,
        })
    }
}

impl FromStr for RenderingProfile {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "servo-safe" | "safe" => Ok(Self::ServoSafe),
            "servo-modern" | "modern" => Ok(Self::ServoModern),
            "chrome-reference" | "chrome" | "reference" => Ok(Self::ChromeReference),
            other => Err(anyhow!(
                "unknown rendering profile `{other}`; expected servo-safe, servo-modern, or chrome-reference"
            )),
        }
    }
}

impl fmt::Display for RenderingProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl RenderingProfileSettings {
    pub fn servo_preferences(&self) -> Preferences {
        let mut preferences = Preferences::default();
        preferences.layout_grid_enabled = self.layout_grid_enabled;
        preferences
    }

    pub fn fallback_recommended(&self) -> &'static str {
        "chrome-reference"
    }

    pub fn experimental_prefs(&self) -> Vec<&'static str> {
        if self.layout_grid_enabled {
            vec!["layout.grid.enabled"]
        } else {
            Vec::new()
        }
    }
}

fn legacy_grid_override_from_env() -> Result<Option<bool>> {
    let Ok(value) = env::var("SACCADE_SERVO_GRID") else {
        return Ok(None);
    };
    let value = value.trim().to_ascii_lowercase();
    match value.as_str() {
        "1" | "true" | "yes" | "on" | "enabled" => Ok(Some(true)),
        "0" | "false" | "no" | "off" | "disabled" => Ok(Some(false)),
        other => Err(anyhow!(
            "invalid SACCADE_SERVO_GRID value `{other}`; expected on/off or true/false"
        )),
    }
}
