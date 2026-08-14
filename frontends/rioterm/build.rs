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
}
