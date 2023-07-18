#[cfg(not(any(target_os = "macos")))]
pub const PADDING_Y: f32 = 2.0;

#[cfg(target_os = "macos")]
pub const PADDING_Y: f32 = 15.;

#[cfg(not(any(target_os = "macos")))]
pub const INACTIVE_TAB_WIDTH_SIZE: f32 = 4.;

#[cfg(target_os = "macos")]
pub const INACTIVE_TAB_WIDTH_SIZE: f32 = 16.;

#[cfg(not(any(target_os = "macos")))]
pub const ACTIVE_TAB_WIDTH_SIZE: f32 = 8.;

#[cfg(target_os = "macos")]
pub const ACTIVE_TAB_WIDTH_SIZE: f32 = 26.;
