use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bell {
    #[serde(default = "default_audio")]
    pub audio: AudioBell,
    #[serde(default = "default_urgency")]
    pub urgency: UrgencyHint,
    #[serde(default = "default_notification")]
    pub notification: BellNotification,
    /// Minimum time in milliseconds between two bells. A burst of BEL bytes
    /// (e.g. `cat`-ing a binary file) is coalesced down to one bell per this
    /// window, so the terminal does not ring constantly. Applies to the audible
    /// sound, the urgency hint and the notification alike.
    #[serde(default = "default_min_interval", rename = "min-interval")]
    pub min_interval: u64,
}

/// How the audible bell behaves.
///
/// Deserializes from either a bool (`false`/`true`, kept for backwards
/// compatibility) or a string (`"off"`/`"beep"`/`"system"`):
/// - `false` / `"off"` — no sound.
/// - `true` / `"beep"` — legacy self-synthesized tone (macOS/Windows use the
///   native system beep).
/// - `"system"` — the desktop environment's event sound theme (Linux:
///   libcanberra; macOS/Windows: the native system beep).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioBell {
    Off,
    Beep,
    System,
}

/// Whether the bell sets the window urgency / attention hint (taskbar/dock
/// flash) when it fires in an unfocused window. Accepts a bool or
/// `"on"`/`"off"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrgencyHint {
    Disabled,
    Enabled,
}

/// Whether the bell raises a desktop notification when it fires in an unfocused
/// window. Accepts a bool or `"on"`/`"off"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BellNotification {
    Disabled,
    Enabled,
}

impl UrgencyHint {
    pub fn is_enabled(self) -> bool {
        matches!(self, UrgencyHint::Enabled)
    }
}

impl BellNotification {
    pub fn is_enabled(self) -> bool {
        matches!(self, BellNotification::Enabled)
    }
}

impl Default for Bell {
    fn default() -> Self {
        Bell {
            audio: default_audio(),
            urgency: default_urgency(),
            notification: default_notification(),
            min_interval: default_min_interval(),
        }
    }
}

fn default_audio() -> AudioBell {
    // macOS and Windows beep with the native system sound; Linux integrates
    // with the freedesktop event-sound theme via libcanberra.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        AudioBell::Beep
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        AudioBell::System
    }
}

fn default_urgency() -> UrgencyHint {
    // The window urgency / attention hint is the primary reason a bell is
    // useful on Linux (taskbar/dock flash when unfocused). Leave the existing
    // macOS/Windows behavior untouched.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        UrgencyHint::Disabled
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        UrgencyHint::Enabled
    }
}

fn default_notification() -> BellNotification {
    BellNotification::Disabled
}

fn default_min_interval() -> u64 {
    // 3 seconds: comfortably longer than any bell sound, so an accidental
    // binary `cat` rings at most once every few seconds instead of constantly.
    3_000
}

impl<'de> Deserialize<'de> for AudioBell {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match BoolOrStr::deserialize(deserializer)? {
            BoolOrStr::Bool(true) => Ok(AudioBell::Beep),
            BoolOrStr::Bool(false) => Ok(AudioBell::Off),
            BoolOrStr::Str(s) => match s.to_ascii_lowercase().as_str() {
                "off" | "false" | "none" => Ok(AudioBell::Off),
                "beep" | "true" | "tone" => Ok(AudioBell::Beep),
                "system" => Ok(AudioBell::System),
                other => Err(de::Error::custom(format!(
                    "invalid bell audio value {other:?} (expected false, true, or \"system\")"
                ))),
            },
        }
    }
}

impl Serialize for AudioBell {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            AudioBell::Off => serializer.serialize_bool(false),
            AudioBell::Beep => serializer.serialize_bool(true),
            AudioBell::System => serializer.serialize_str("system"),
        }
    }
}

impl<'de> Deserialize<'de> for UrgencyHint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(if toggle_from(deserializer)? {
            UrgencyHint::Enabled
        } else {
            UrgencyHint::Disabled
        })
    }
}

impl Serialize for UrgencyHint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(self.is_enabled())
    }
}

impl<'de> Deserialize<'de> for BellNotification {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(if toggle_from(deserializer)? {
            BellNotification::Enabled
        } else {
            BellNotification::Disabled
        })
    }
}

impl Serialize for BellNotification {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(self.is_enabled())
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BoolOrStr {
    Bool(bool),
    Str(String),
}

/// Parse an on/off toggle that accepts a TOML bool or a string spelling.
fn toggle_from<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match BoolOrStr::deserialize(deserializer)? {
        BoolOrStr::Bool(b) => Ok(b),
        BoolOrStr::Str(s) => match s.to_ascii_lowercase().as_str() {
            "on" | "true" | "enabled" | "yes" => Ok(true),
            "off" | "false" | "disabled" | "no" | "none" => Ok(false),
            other => Err(de::Error::custom(format!(
                "invalid toggle value {other:?} (expected true or false)"
            ))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Bell {
        toml::from_str(s).unwrap()
    }

    #[test]
    fn audio_accepts_legacy_bools() {
        assert_eq!(parse("audio = true").audio, AudioBell::Beep);
        assert_eq!(parse("audio = false").audio, AudioBell::Off);
    }

    #[test]
    fn audio_accepts_strings() {
        assert_eq!(parse("audio = \"system\"").audio, AudioBell::System);
        assert_eq!(parse("audio = \"beep\"").audio, AudioBell::Beep);
        assert_eq!(parse("audio = \"off\"").audio, AudioBell::Off);
    }

    #[test]
    fn audio_rejects_unknown_strings() {
        assert!(toml::from_str::<Bell>("audio = \"loud\"").is_err());
    }

    #[test]
    fn toggles_accept_bools_and_strings() {
        assert!(parse("urgency = true").urgency.is_enabled());
        assert!(!parse("urgency = false").urgency.is_enabled());
        assert!(parse("notification = \"on\"").notification.is_enabled());
        assert!(!parse("notification = \"off\"").notification.is_enabled());
    }

    #[test]
    fn defaults_match_platform() {
        let bell = Bell::default();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            assert_eq!(bell.audio, AudioBell::System);
            assert!(bell.urgency.is_enabled());
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            assert_eq!(bell.audio, AudioBell::Beep);
            assert!(!bell.urgency.is_enabled());
        }
        assert!(!bell.notification.is_enabled());
    }

    #[test]
    fn missing_fields_use_defaults() {
        let bell = parse("");
        assert_eq!(bell, Bell::default());
        assert_eq!(bell.min_interval, 3_000);
    }

    #[test]
    fn min_interval_is_configurable() {
        assert_eq!(parse("min-interval = 500").min_interval, 500);
    }

    #[test]
    fn round_trips_through_serialize() {
        let bell = Bell {
            audio: AudioBell::System,
            urgency: UrgencyHint::Enabled,
            notification: BellNotification::Disabled,
            min_interval: 3_000,
        };
        let serialized = toml::to_string(&bell).unwrap();
        assert_eq!(toml::from_str::<Bell>(&serialized).unwrap(), bell);
    }
}
