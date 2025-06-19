// Implementation made by ayosec
// https://github.com/ayosec/alacritty/commit/661a64c2b35283c97bac71d29535393e909c7d19
// This module implements support for the [iTerm2 images protocol](https://iterm2.com/documentation-images.html).
//
// iTerm2 uses the OSC 1337 for a many non-standard commands, but we only support
// adding inline graphics.
//
// This implementation also supports `width` and `height` parameters to resize the image.

use sugarloaf::{GraphicData, GraphicId, ResizeCommand, ResizeParameter};

use rustc_hash::FxHashMap;
use std::str;

use base64::engine::general_purpose::STANDARD as Base64;
use base64::Engine;

use crate::simd_utf8;

/// Parse the OSC 1337 parameters to add a graphic to the grid.
pub fn parse(params: &[&[u8]]) -> Option<GraphicData> {
    let (params, contents) = param_values(params)?;

    if params.get("inline") != Some(&"1") {
        return None;
    }

    let buffer = match Base64.decode(contents) {
        Ok(buffer) => buffer,
        Err(err) => {
            tracing::warn!("Can't decode base64 data: {}", err);
            return None;
        }
    };

    let image = match image_rs::load_from_memory(&buffer) {
        Ok(image) => image,
        Err(err) => {
            tracing::warn!("Can't load image: {}", err);
            return None;
        }
    };

    let mut graphics = GraphicData::from_dynamic_image(GraphicId(0), image);
    graphics.resize = resize_param(&params);
    Some(graphics)
}

/// Extract parameter values.
///
/// The format defined by iTerm2 starts with a `File=` string, and the file
/// contents are specified after a `:`.
///
/// ```notrust
/// ESC ] 1337 ; File = [arguments] : base-64 encoded file contents ^G
/// ```
///
/// This format is not expected by the parser in the `vte` crate.
///
/// The `File=` string is found in the first parameter, and the file contents are
/// appended in the last one. We have to split these parameter to get the expected
/// data.
fn param_values<'a>(
    params: &[&'a [u8]],
) -> Option<(FxHashMap<&'a str, &'a str>, &'a [u8])> {
    let mut map = FxHashMap::default();
    let mut contents = None;

    for (index, mut param) in params.iter().skip(1).copied().enumerate() {
        // First parameter should start with "File="
        if index == 0 {
            if !param.starts_with(&b"File="[..]) {
                return None;
            }

            param = &param[5..];
        }

        if let Some(separator) = param.iter().position(|&b| b == b'=') {
            let (key, mut value) = param.split_at(separator);
            value = &value[1..];

            // Last parameter has the file contents after the first ':'.
            // Add 2 because we are skipping the first param.
            if index + 2 == params.len() {
                if let Some(separator) = value.iter().position(|&b| b == b':') {
                    let (a, b) = value.split_at(separator);
                    value = a;
                    contents = Some(&b[1..]);
                }
            }

            if let (Ok(key), Ok(value)) = (
                simd_utf8::from_utf8_fast(key),
                simd_utf8::from_utf8_fast(value),
            ) {
                map.insert(key, value);
            }
        }
    }

    contents.map(|c| (map, c))
}

/// Compute the resize operation from the OSC parameters.
///
/// Accepted formats:
///
/// - N: N character cells.
/// - Npx: N pixels.
/// - N%: N percent of the window's width or height.
/// - auto: Computed from the original graphic size.
fn resize_param(params: &FxHashMap<&str, &str>) -> Option<ResizeCommand> {
    fn parse(value: Option<&str>) -> Option<ResizeParameter> {
        let value = match value {
            None | Some("auto") => return Some(ResizeParameter::Auto),
            Some(value) => value,
        };

        // Split the value after the first non-digit byte.
        // If there is no unit, parse as number of cells.
        let first_nondigit = value
            .as_bytes()
            .iter()
            .position(|b: &u8| !b.is_ascii_digit());
        // .position(|b| !(b'0'..=b'9').contains(&b));
        let (number, unit) = match first_nondigit {
            Some(position) => value.split_at(position),
            None => return Some(ResizeParameter::Cells(str::parse(value).ok()?)),
        };

        match (str::parse(number), unit) {
            (Ok(number), "%") => Some(ResizeParameter::WindowPercent(number)),
            (Ok(number), "px") => Some(ResizeParameter::Pixels(number)),
            _ => None,
        }
    }

    let width = parse(params.get(&"width").copied())?;
    let height = parse(params.get(&"height").copied())?;

    let preserve_aspect_ratio = params.get(&"preserveAspectRatio") != Some(&"0");

    Some(ResizeCommand {
        width,
        height,
        preserve_aspect_ratio,
    })
}

#[test]
fn parse_osc1337_parameters() {
    let params = [
        b"1337".as_ref(),
        b"File=name=ABCD".as_ref(),
        b"size=3".as_ref(),
        b"inline=1:AAAA".as_ref(),
    ];

    let (params, contents) = param_values(&params).unwrap();

    assert_eq!(params["name"], "ABCD");
    assert_eq!(params["size"], "3");
    assert_eq!(params["inline"], "1");

    assert_eq!(contents, b"AAAA".as_ref())
}

#[test]
fn parse_osc1337_single_parameter() {
    let params = [b"1337".as_ref(), b"File=inline=1:AAAA".as_ref()];

    let (params, contents) = param_values(&params).unwrap();

    assert_eq!(params["inline"], "1");
    assert_eq!(contents, b"AAAA".as_ref())
}

#[test]
fn resize_params() {
    use ResizeParameter::{Auto, Cells, Pixels, WindowPercent};

    macro_rules! assert_resize {
        ($param_width:expr, $param_height:expr, $width:expr, $height:expr) => {
            let mut params = FxHashMap::default();
            params.insert("width", $param_width);
            params.insert("height", $param_height);

            let resize = resize_param(&params).unwrap();
            assert_eq!(resize.width, $width);
            assert_eq!(resize.height, $height);
        };
    }

    assert_resize!("auto", "50%", Auto, WindowPercent(50));
    assert_resize!("10", "20", Cells(10), Cells(20));
    assert_resize!("10%", "50px", WindowPercent(10), Pixels(50));
}
