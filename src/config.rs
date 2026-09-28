// SPDX-License-Identifier: MPL-2.0

use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Band {
    Ghz24,
    Ghz5,
}

impl Band {
    pub fn nm_value(self) -> &'static str {
        match self {
            Band::Ghz24 => "bg",
            Band::Ghz5 => "a",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub ssid: String,
    pub password: String,
    pub band: Band,
    pub idle_minutes: u32,
}

/// No built-in network name or password: the user sets both on first use.
impl Default for Settings {
    fn default() -> Self {
        Self { ssid: String::new(), password: String::new(), band: Band::Ghz24, idle_minutes: 10 }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.ssid.is_empty() || self.ssid.len() > 32 {
            return Err(crate::fl!("err-ssid"));
        }
        if !(8..=63).contains(&self.password.chars().count()) {
            return Err(crate::fl!("err-password"));
        }
        if self.idle_minutes > 240 {
            return Err(crate::fl!("err-idle"));
        }
        Ok(())
    }
}

/// The part persisted in cosmic-config. The password is never stored here: it lives in the NM profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct Stored {
    pub ssid: String,
    pub band: Band,
    pub idle_minutes: u32,
}

impl Default for Stored {
    fn default() -> Self {
        let d = Settings::default();
        Self { ssid: d.ssid, band: d.band, idle_minutes: d.idle_minutes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A user-provided, valid configuration (there are no built-in network credentials).
    fn valid() -> Settings {
        Settings { ssid: "MyNetwork".into(), password: "test-password".into(), ..Settings::default() }
    }

    #[test]
    fn validation_errors_come_from_the_fluent_loader() {
        // The global loader is never re-selected in tests, so it is in the fallback language (English).
        let bad = Settings { ssid: String::new(), ..valid() };
        assert_eq!(bad.validate(), Err("SSID must be 1 to 32 bytes".to_string()));
        let bad = Settings { password: "1234567".into(), ..valid() };
        assert_eq!(bad.validate(), Err("Password must be 8 to 63 characters".to_string()));
        let bad = Settings { idle_minutes: 241, ..valid() };
        assert_eq!(bad.validate(), Err("Auto-off: maximum 240 minutes".to_string()));
    }

    #[test]
    fn defaults_require_configuration() {
        let d = Settings::default();
        assert!(d.ssid.is_empty(), "no built-in network name");
        assert!(d.password.is_empty(), "no built-in password");
        assert!(d.validate().is_err(), "the user must configure it first");
        assert_eq!(d.band.nm_value(), "bg");
        assert_eq!(d.idle_minutes, 10);
        assert!(Stored::default().ssid.is_empty());
    }

    #[test]
    fn rejects_bad_values() {
        let ok = valid();
        assert!(Settings { ssid: "".into(), ..ok.clone() }.validate().is_err());
        assert!(Settings { ssid: "x".repeat(33), ..ok.clone() }.validate().is_err());
        assert!(Settings { password: "1234567".into(), ..ok.clone() }.validate().is_err());
        assert!(Settings { password: "x".repeat(64), ..ok.clone() }.validate().is_err());
        assert!(Settings { idle_minutes: 241, ..ok.clone() }.validate().is_err());
        assert!(Settings { idle_minutes: 0, ..ok }.validate().is_ok());
    }

    #[test]
    fn band_nm_values() {
        assert_eq!(Band::Ghz24.nm_value(), "bg");
        assert_eq!(Band::Ghz5.nm_value(), "a");
    }
}
