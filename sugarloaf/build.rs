// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Compile sugarloaf's GLSL shaders to SPIR-V at build time.
//!
//! The Vulkan backend is Linux-only — on other targets this script is
//! a no-op. On Linux we walk a hard-coded list of `.glsl` files
//! (kept in lock-step with the `include_bytes!` call sites) and shell
//! out to `glslc` (preferred) or `glslangValidator` (Debian fallback)
//! to compile each into `$OUT_DIR/<name>.spv`. Compiled bytes get
//! pulled in via `include_bytes!(concat!(env!("OUT_DIR"), "/..."))`
//! at the call sites, so the `.spv` files never live in the source
//! tree (and never need to be committed).
//!
//! ## Required tooling
//!
//! - **Debian**: `apt install glslang-tools` (provides
//!   `glslangValidator`) or `apt install glslc` (provides Google's
//!   `glslc`, available since Bookworm).
//! - **Arch**: `pacman -S shaderc` (provides `glslc`).
//! - **macOS / Windows**: not required — the Vulkan backend isn't
//!   built on those targets, so this script returns early.
//!
//! Override the compiler with `GLSLC=/path/to/binary` if your
//! compiler isn't on PATH or you want a specific version.
//!
//! ## TODO: drop the system-binary requirement
//!
//! `librust-naga-dev` (the wgpu shader translator, pure Rust) has a
//! GLSL frontend + SPIR-V backend and would let us compile shaders
//! in-process with zero system tooling. As of 2026-04 it's only in
//! Debian forky/sid (24.0.0-3); not yet in trixie/stable. Once it
//! lands in Debian stable, switch to a `naga` build-dependency and
//! delete the `glslc`/`glslangValidator` subprocess plumbing.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Directories we scan for `*.{vert,frag}.glsl` sources. New shader
/// files dropped into either are picked up automatically — no
/// build.rs edit needed. File stems must be unique across all
/// scanned directories (we flatten output into a single `OUT_DIR`);
/// `cargo build` panics with a clear message if two sources end up
/// with the same `.spv` name.
const SHADER_DIRS: &[&str] = &[
    // renderer (rich-text quad / non-quad / image / bootstrap)
    "src/renderer",
    // grid (per-panel terminal cell + text + UI text overlay)
    "src/grid/shaders",
];

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" {
        // Vulkan backend is Linux-only; nothing to compile. We still
        // emit `rerun-if-env-changed` so a future port (e.g.
        // `khr::win32_surface`) flipping target conditions takes
        // effect on the next cargo invocation.
        println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
        return;
    }

    println!("cargo:rerun-if-env-changed=GLSLC");
    println!("cargo:rerun-if-env-changed=GLSLANG_VALIDATOR");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir =
        PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR must be set by cargo"));

    let compiler = locate_compiler();
    eprintln!("sugarloaf build.rs: GLSL compiler = {:?}", compiler);

    let sources = discover_shaders(SHADER_DIRS);
    if sources.is_empty() {
        // Survives a partial crate checkout (no source dir present)
        // without breaking the build — but warn so the missing dir
        // is visible in cargo output.
        println!(
            "cargo:warning=sugarloaf build.rs: no GLSL shaders found in {SHADER_DIRS:?}"
        );
        return;
    }

    for src in &sources {
        println!("cargo:rerun-if-changed={}", src.display());
        compile(&compiler, src, &out_dir);
    }
}

/// Walk `dirs` looking for `*.vert.glsl` and `*.frag.glsl` files.
/// Returns absolute-relative-to-CARGO_MANIFEST_DIR paths. Also
/// emits `cargo:rerun-if-changed` for each *directory* so cargo
/// triggers a rebuild when a new file is added (without that, a new
/// `.glsl` would only be picked up after a `cargo clean`).
fn discover_shaders(dirs: &[&str]) -> Vec<PathBuf> {
    use std::collections::HashSet;

    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen_stems: HashSet<String> = HashSet::new();

    for dir in dirs {
        // Tell cargo to rebuild when files appear/disappear in this
        // directory. (cargo also implicitly tracks individual files
        // we list via rerun-if-changed below; this catches NEW files.)
        println!("cargo:rerun-if-changed={dir}");

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_owned(),
                None => continue,
            };
            if !(name.ends_with(".vert.glsl") || name.ends_with(".frag.glsl")) {
                continue;
            }
            let stem = name
                .strip_suffix(".glsl")
                .expect("just checked the suffix")
                .to_owned();
            if !seen_stems.insert(stem.clone()) {
                panic!(
                    "sugarloaf build.rs: duplicate shader stem `{stem}` across \
                     scanned dirs — `OUT_DIR` would collide. Rename one of them."
                );
            }
            out.push(path);
        }
    }

    // Stable order so build logs and rerun-if-changed lines diff
    // cleanly across runs.
    out.sort();
    out
}

#[derive(Debug)]
enum Compiler {
    /// Google's `glslc` from shaderc — better diagnostics, what
    /// almost every Vulkan tutorial uses.
    Glslc(PathBuf),
    /// Khronos reference compiler. The `-V` (Vulkan) flag is
    /// load-bearing: without it we'd get GL SPIR-V which Vulkan
    /// drivers reject.
    Glslang(PathBuf),
}

/// PATH-like lookup with `GLSLC` / `GLSLANG_VALIDATOR` env-var
/// override. We don't pull in the `which` crate (Debian doesn't
/// universally package it) — the lookup is a few lines.
fn locate_compiler() -> Compiler {
    if let Some(p) = env_path("GLSLC") {
        return Compiler::Glslc(p);
    }
    if let Some(p) = env_path("GLSLANG_VALIDATOR") {
        return Compiler::Glslang(p);
    }
    if let Some(p) = which("glslc") {
        return Compiler::Glslc(p);
    }
    if let Some(p) = which("glslangValidator") {
        return Compiler::Glslang(p);
    }
    panic!(
        "\nsugarloaf: no GLSL → SPIR-V compiler found.\n\
         Install one of:\n  \
         * Debian: `apt install glslang-tools` (provides glslangValidator)\n  \
         *         or `apt install glslc` (Google's glslc, Bookworm+)\n  \
         * Arch:   `pacman -S shaderc` (provides glslc)\n  \
         * Or set GLSLC=/path/to/binary or GLSLANG_VALIDATOR=/path/to/binary.\n"
    );
}

fn env_path(name: &str) -> Option<PathBuf> {
    let v = std::env::var_os(name)?;
    if v.is_empty() {
        return None;
    }
    let p = PathBuf::from(v);
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

fn which(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn compile(compiler: &Compiler, src: &Path, out_dir: &Path) {
    let src_str = src.to_string_lossy();
    let stage = if src_str.ends_with(".vert.glsl") {
        "vertex"
    } else if src_str.ends_with(".frag.glsl") {
        "fragment"
    } else {
        panic!("unrecognised GLSL stage suffix in {src_str}");
    };

    let stem = src
        .file_name()
        .expect("source path has a file name")
        .to_string_lossy();
    // foo.vert.glsl → foo.vert.spv
    let spv_name = stem
        .strip_suffix(".glsl")
        .expect("source ends in .glsl")
        .to_string()
        + ".spv";
    let dst = out_dir.join(&spv_name);

    let output = match compiler {
        Compiler::Glslc(bin) => Command::new(bin)
            .arg(format!("-fshader-stage={stage}"))
            .arg(src)
            .arg("-o")
            .arg(&dst)
            .output(),
        Compiler::Glslang(bin) => Command::new(bin)
            .arg("-V")
            .arg("-S")
            .arg(stage)
            .arg(src)
            .arg("-o")
            .arg(&dst)
            .output(),
    }
    .unwrap_or_else(|e| panic!("failed to invoke {compiler:?} on {src_str}: {e}"));

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "GLSL compile failed for {src_str}:\n--stdout--\n{stdout}\n--stderr--\n{stderr}"
        );
    }
}
