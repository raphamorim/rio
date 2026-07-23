// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use naga::valid::{Capabilities, ValidationFlags, Validator};

const SHADERS: &[(&str, &str)] = &[
    (
        "renderer/renderer.wgsl",
        include_str!("../src/renderer/renderer.wgsl"),
    ),
    (
        "renderer/image.wgsl",
        include_str!("../src/renderer/image.wgsl"),
    ),
    ("text_shader.wgsl", include_str!("../src/text_shader.wgsl")),
    (
        "grid/shaders/grid.wgsl",
        include_str!("../src/grid/shaders/grid.wgsl"),
    ),
    (
        "components/filters/shader/triangle.wgsl",
        include_str!("../src/components/filters/shader/triangle.wgsl"),
    ),
    (
        "components/filters/shader/blit.wgsl",
        include_str!("../src/components/filters/shader/blit.wgsl"),
    ),
];

#[test]
fn bundled_wgsl_shaders_pass_naga_validation() {
    let mut failures = Vec::new();
    for (name, source) in SHADERS {
        match naga::front::wgsl::parse_str(source) {
            Err(err) => {
                failures.push(format!(
                    "{name}: parse error\n{}",
                    err.emit_to_string(source)
                ));
            }
            Ok(module) => {
                let mut validator =
                    Validator::new(ValidationFlags::all(), Capabilities::default());
                if let Err(err) = validator.validate(&module) {
                    failures.push(format!(
                        "{name}: validation error\n{}",
                        err.emit_to_string(source)
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "WGSL shaders rejected by naga:\n\n{}",
        failures.join("\n\n")
    );
}
