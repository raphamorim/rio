pub mod core;
// `filters` is the librashader integration — wgpu-only by upstream
// design (`librashader-runtime-wgpu`). Gated together with the rest
// of the wgpu code so the dep tree drops cleanly on Linux/macOS
// builds that don't enable the `wgpu` feature.
#[cfg(feature = "wgpu")]
pub mod filters;
