#[cfg(not(any(target_os = "macos")))]
pub const PADDING_Y: f32 = 2.0;

#[cfg(target_os = "macos")]
pub const PADDING_Y: f32 = 26.;

#[cfg(target_os = "macos")]
pub const ADDITIONAL_PADDING_Y_ON_UNIFIED_TITLEBAR: f32 = 2.;

// Vertically centers the ~14px buttons in the 38px island strip
// ((38 - 14) / 2 = 12).
#[cfg(target_os = "macos")]
pub const TRAFFIC_LIGHT_PADDING: f64 = 12.;

pub const MULTI_CLICK_THRESHOLD: std::time::Duration =
    std::time::Duration::from_millis(300);

#[cfg(all(
    feature = "audio",
    not(target_os = "macos"),
    not(target_os = "windows")
))]
pub const BELL_DURATION: std::time::Duration = std::time::Duration::from_millis(200);
