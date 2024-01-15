//! Support for detecting OS specific font paths and selecting appropriate
//! fallbacks.

#[allow(dead_code)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Os {
    MacOs,
    Ios,
    Windows,
    Unix,
    Android,
    Other,
}

#[cfg(target_os = "macos")]
pub const OS: Os = Os::MacOs;

#[cfg(target_os = "ios")]
pub const OS: Os = Os::Ios;

#[cfg(target_os = "windows")]
pub const OS: Os = Os::Windows;

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
pub const OS: Os = Os::Unix;

#[cfg(target_os = "android")]
pub const OS: Os = Os::Android;

#[cfg(not(any(unix, windows)))]
pub const OS: Os = Os::Other;
