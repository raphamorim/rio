fn main() {
    // wgpu-hal references HDR colorspace constants (ExtendedDisplayP3,
    // the ITUR_2100 pair) that older macOS does not export. Their use
    // is runtime-gated inside wgpu, but the references link as strong
    // imports, so dyld refuses to load the binary on macOS 10.15 even
    // though those paths can never run there (the bundle advertises
    // LSMinimumSystemVersion 10.15.7). Weak-linking CoreGraphics turns
    // the missing constants into NULLs instead of a launch abort; rio
    // never requests an HDR surface colorspace, so they stay untouched,
    // and the constants rio does use (DisplayP3) exist since 10.11.
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("apple-darwin") {
        println!("cargo:rustc-link-arg-bins=-weak_framework");
        println!("cargo:rustc-link-arg-bins=CoreGraphics");
    }

    #[cfg(windows)]
    load_app_icon();
}

// The exe resource Explorer and the taskbar read; the same image the
// runtime window icon uses (router::window::LOGO_ICON). cfg(windows) in
// a build script is the host, so cross-compiles from a windows host to
// another os must still skip via the target check.
#[cfg(windows)]
fn load_app_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon_path = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("src")
        .join("router")
        .join("resources")
        .join("images")
        .join("rio-logo.ico");
    println!("cargo:rerun-if-changed={}", icon_path.display());
    let mut res = winres::WindowsResource::new();
    res.set_icon(icon_path.to_str().expect("manifest dir must be utf-8"));
    res.compile()
        .expect("failed to compile the windows icon resource");
}
