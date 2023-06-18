#[cfg(not(any(target_os = "macos", windows)))]
pub const PADDING_Y: f32 = 10.;

#[cfg(any(target_os = "macos", windows))]
pub const PADDING_Y: f32 = 30.;

#[cfg(not(any(target_os = "macos", windows)))]
pub const INACTIVE_TAB_WIDTH_SIZE: f32 = 4.;

#[cfg(any(target_os = "macos", windows))]
pub const INACTIVE_TAB_WIDTH_SIZE: f32 = 16.;

#[cfg(not(any(target_os = "macos", windows)))]
pub const ACTIVE_TAB_WIDTH_SIZE: f32 = 8.;

#[cfg(any(target_os = "macos", windows))]
pub const ACTIVE_TAB_WIDTH_SIZE: f32 = 26.;
