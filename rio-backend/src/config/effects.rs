use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Effects {
    #[serde(default = "bool::default", rename = "custom-mouse-cursor")]
    pub custom_mouse_cursor: bool,
    #[serde(default = "bool::default", rename = "trail-cursor")]
    pub trail_cursor: bool,
    /// Trail color as a hex string ("#F07178"). Unset means the trail
    /// takes the cursor color.
    #[serde(default, rename = "trail-cursor-color")]
    pub trail_cursor_color: Option<String>,
    /// Trail opacity, 0.0 to 1.0.
    #[serde(default = "default_trail_opacity", rename = "trail-cursor-opacity")]
    pub trail_cursor_opacity: f32,
    /// Decay pair in milliseconds: how long the fastest and the
    /// slowest corner of the trail take to catch up with the cursor.
    /// The gap between the two is what stretches the trail.
    #[serde(default = "default_trail_decay", rename = "trail-cursor-decay")]
    pub trail_cursor_decay: [u32; 2],
    /// Cursor jumps at or under this many cells (on both axes) do not
    /// start a trail. 0 animates every movement.
    #[serde(
        default = "default_trail_start_threshold",
        rename = "trail-cursor-start-threshold"
    )]
    pub trail_cursor_start_threshold: u32,
}

fn default_trail_opacity() -> f32 {
    1.0
}

fn default_trail_decay() -> [u32; 2] {
    [100, 400]
}

fn default_trail_start_threshold() -> u32 {
    2
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            custom_mouse_cursor: false,
            trail_cursor: false,
            trail_cursor_color: None,
            trail_cursor_opacity: default_trail_opacity(),
            trail_cursor_decay: default_trail_decay(),
            trail_cursor_start_threshold: default_trail_start_threshold(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_bare_table() {
        let effects: Effects = toml::from_str("").unwrap();
        assert_eq!(effects, Effects::default());
        assert_eq!(effects.trail_cursor_opacity, 1.0);
        assert_eq!(effects.trail_cursor_decay, [100, 400]);
        assert_eq!(effects.trail_cursor_start_threshold, 2);
        assert!(effects.trail_cursor_color.is_none());
    }

    #[test]
    fn full_table_parses() {
        let effects: Effects = toml::from_str(
            r##"
            trail-cursor = true
            trail-cursor-color = "#F07178"
            trail-cursor-opacity = 0.4
            trail-cursor-decay = [50, 250]
            trail-cursor-start-threshold = 0
            "##,
        )
        .unwrap();
        assert!(effects.trail_cursor);
        assert_eq!(effects.trail_cursor_color.as_deref(), Some("#F07178"));
        assert_eq!(effects.trail_cursor_opacity, 0.4);
        assert_eq!(effects.trail_cursor_decay, [50, 250]);
        assert_eq!(effects.trail_cursor_start_threshold, 0);
    }
}
