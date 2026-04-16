use cfg_aliases::cfg_aliases;

#[cfg(windows)]
extern crate winres;

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

    #[cfg(target_os = "macos")]
    generate_dispatch_bindings();

    #[cfg(target_os = "windows")]
    load_app_icon();
}

#[cfg(target_os = "macos")]
fn generate_dispatch_bindings() {
    use std::{env, path::PathBuf};

    println!("cargo:rustc-link-lib=framework=System");
    println!("cargo:rerun-if-changed=src/platform_impl/macos/dispatch.h");

    let bindings = bindgen::Builder::default()
        .header("src/platform_impl/macos/dispatch.h")
        .allowlist_var("_dispatch_main_q")
        .allowlist_var("_dispatch_source_type_data_add")
        .allowlist_function("dispatch_source_create")
        .allowlist_function("dispatch_source_merge_data")
        .allowlist_function("dispatch_source_set_event_handler_f")
        .allowlist_function("dispatch_set_context")
        .allowlist_function("dispatch_resume")
        .allowlist_function("dispatch_suspend")
        .allowlist_function("dispatch_source_cancel")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .layout_tests(false)
        .generate()
        .expect("unable to generate dispatch bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("dispatch_sys.rs"))
        .expect("couldn't write dispatch bindings");
}

#[cfg(target_os = "windows")]
fn load_app_icon() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("../misc/windows/rio-2024.ico");
    res.compile()
        .expect("Failed to compile Window icon resource");
}
