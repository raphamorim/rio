#[cfg(not(any(target_os = "macos")))]
pub const PADDING_Y: f32 = 2.0;

#[cfg(not(any(target_os = "macos")))]
pub const PADDING_Y_WITH_TAB_ON_TOP: f32 = 15.0;

#[cfg(target_os = "macos")]
pub const PADDING_Y: f32 = 26.;

#[cfg(target_os = "macos")]
pub const ADDITIONAL_PADDING_Y_ON_UNIFIED_TITLEBAR: f32 = 2.;

#[cfg(target_os = "macos")]
pub const TRAFFIC_LIGHT_PADDING: f64 = 9.;

#[cfg(all(
    feature = "audio",
    not(target_os = "macos"),
    not(target_os = "windows")
))]
pub const BELL_DURATION: std::time::Duration = std::time::Duration::from_millis(200);
