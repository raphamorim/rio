use cfg_aliases::cfg_aliases;

fn main() {
    // The script doesn't depend on our code
    println!("cargo:rerun-if-changed=build.rs");

    // Setup cfg aliases
    cfg_aliases! {
        // Systems.
        android_platform: { target_os = "android" },
        web_platform: { all(target_family = "wasm", target_os = "unknown") },
        macos_platform: { target_os = "macos" },
        ios_platform: { target_os = "ios" },
        windows_platform: { target_os = "windows" },
        apple: { any(target_os = "ios", target_os = "macos") },
        free_unix: { all(unix, not(apple), not(android_platform), not(target_os = "emscripten")) },
        redox: { target_os = "redox" },

        // Native displays.
        x11_platform: { all(feature = "x11", free_unix, not(redox)) },
        wayland_platform: { all(feature = "wayland", free_unix, not(redox)) },
        orbital_platform: { redox },
    }

    println!("cargo:rustc-check-cfg=cfg(android_platform)");
    println!("cargo:rustc-check-cfg=cfg(web_platform)");
    println!("cargo:rustc-check-cfg=cfg(macos_platform)");
    println!("cargo:rustc-check-cfg=cfg(ios_platform)");
    println!("cargo:rustc-check-cfg=cfg(windows_platform)");
    println!("cargo:rustc-check-cfg=cfg(apple)");
    println!("cargo:rustc-check-cfg=cfg(free_unix)");
    println!("cargo:rustc-check-cfg=cfg(redox)");

    println!("cargo:rustc-check-cfg=cfg(x11_platform)");
    println!("cargo:rustc-check-cfg=cfg(wayland_platform)");
    println!("cargo:rustc-check-cfg=cfg(orbital_platform)");

    println!("cargo:rustc-check-cfg=cfg(unreleased_changelogs)");
}
