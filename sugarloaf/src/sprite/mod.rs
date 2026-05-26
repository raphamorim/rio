// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Built-in "sprite" glyphs: box-drawing, block elements, braille and
//! other characters that terminals render procedurally so they tile
//! seamlessly and stay crisp regardless of the active font.
//!
//! Each sprite is CPU-rasterized once into an R8 (alpha) bitmap sized to
//! the exact cell, then the caller uploads it to the grid's glyph atlas
//! and samples it like any other glyph. Drawing is split per character
//! family.

mod block;
mod box_drawing;
mod braille;
mod branch;
mod canvas;
mod geometric;
mod legacy;
mod powerline;

use canvas::Canvas;

/// A rasterized sprite: an R8 coverage bitmap exactly `width * height`
/// bytes, sized to fill one terminal cell (so the caller places it at
/// `bearing_x = 0`, `bearing_y = cell_h`).
pub struct Sprite {
    pub width: u16,
    pub height: u16,
    pub bytes: Vec<u8>,
}

/// Cheap codepoint membership test: is `cp` a sprite Rio draws itself?
/// Used to short-circuit the per-cell hot loop before touching the atlas.
/// Keep in sync with the families handled by [`rasterize`].
#[inline]
pub fn is_drawable(cp: u32) -> bool {
    // Fast path: nothing below U+2500 is drawable, which covers ASCII and
    // most text — one comparison instead of testing every range.
    if cp < 0x2500 {
        return false;
    }
    matches!(
        cp,
        0x2500..=0x259F
            | 0x2800..=0x28FF
            | 0x1FB00..=0x1FB3B
            | 0x1CD00..=0x1CDE5
            | 0x1CEA0
            | 0x1CEA3
            | 0x1CEA8
            | 0x1CEAB
            | 0x1FBE6
            | 0x1FBE7
            | 0xE0B0..=0xE0BF
            | 0xE0D2
            | 0xE0D4
            | 0x25E2..=0x25E5
            | 0x25F8..=0x25FA
            | 0x25FF
            | 0xF5D0..=0xF60D
    )
}

/// Rasterize the sprite for `cp` at the given cell size. Returns `None`
/// when `cp` isn't a sprite we handle (caller falls back to the font) or
/// the cell size is degenerate.
pub fn rasterize(cp: u32, cell_w: u32, cell_h: u32) -> Option<Sprite> {
    if cell_w == 0 || cell_h == 0 {
        return None;
    }

    let mut canvas = Canvas::new(cell_w, cell_h);
    let drawn = match cp {
        0x2500..=0x257F => box_drawing::draw(cp, &mut canvas),
        0x2580..=0x259F => block::draw(cp, &mut canvas),
        0x2800..=0x28FF => braille::draw(cp, &mut canvas),
        0x1FB00..=0x1FB3B
        | 0x1CD00..=0x1CDE5
        | 0x1CEA0
        | 0x1CEA3
        | 0x1CEA8
        | 0x1CEAB
        | 0x1FBE6
        | 0x1FBE7 => legacy::draw(cp, &mut canvas),
        0xE0B0..=0xE0BF | 0xE0D2 | 0xE0D4 => powerline::draw(cp, &mut canvas),
        0x25E2..=0x25E5 | 0x25F8..=0x25FA | 0x25FF => geometric::draw(cp, &mut canvas),
        0xF5D0..=0xF60D => branch::draw(cp, &mut canvas),
        _ => false,
    };
    if !drawn {
        return None;
    }

    Some(Sprite {
        width: cell_w.min(u16::MAX as u32) as u16,
        height: cell_h.min(u16::MAX as u32) as u16,
        bytes: canvas.into_bytes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_drawing_range_is_drawable_and_rasterizes() {
        for cp in 0x2500u32..=0x257F {
            assert!(is_drawable(cp), "U+{cp:04X} should be drawable");
            let s = rasterize(cp, 10, 20).expect("box-drawing cp must rasterize");
            assert_eq!(s.bytes.len(), 10 * 20);
            assert_eq!(s.width, 10);
            assert_eq!(s.height, 20);
            // Every box-drawing glyph paints at least one pixel.
            assert!(
                s.bytes.iter().any(|&b| b > 0),
                "U+{cp:04X} produced an empty sprite"
            );
        }
    }

    #[test]
    fn non_drawable_returns_none() {
        assert!(!is_drawable('A' as u32));
        assert!(rasterize('A' as u32, 10, 20).is_none());
    }

    #[test]
    fn degenerate_cell_returns_none() {
        assert!(rasterize(0x2500, 0, 20).is_none());
        assert!(rasterize(0x2500, 10, 0).is_none());
    }

    #[test]
    fn horizontal_paints_the_middle_row_full_width() {
        // U+2500 '─' should span the full cell width at the vertical center.
        let s = rasterize(0x2500, 12, 24).unwrap();
        let mid = (s.height as usize / 2) * s.width as usize;
        let row = &s.bytes[mid..mid + s.width as usize];
        assert!(row.iter().all(|&b| b == 0xFF), "middle row should be solid");
    }

    /// Visual dump harness: lay out every codepoint in `range` on a grid
    /// and write a PNG for manual inspection. The basis for blessing
    /// golden references later.
    fn dump_range_png(
        name: &str,
        range: std::ops::RangeInclusive<u32>,
        cw: u32,
        ch: u32,
    ) {
        use tiny_skia::{Pixmap, PremultipliedColorU8};

        let cols = 16u32;
        let gap = 1u32;
        let count = range.clone().count() as u32;
        let rows = count.div_ceil(cols);
        let img_w = cols * (cw + gap) + gap;
        let img_h = rows * (ch + gap) + gap;
        let mut pm = Pixmap::new(img_w, img_h).unwrap();

        // Dark slate background; white anti-aliased glyphs blended over it.
        let (bg_r, bg_g, bg_b) = (20u32, 20u32, 30u32);
        let blend = |bg: u32, a: u8| -> u8 { (bg + (255 - bg) * a as u32 / 255) as u8 };
        for p in pm.pixels_mut() {
            *p = PremultipliedColorU8::from_rgba(bg_r as u8, bg_g as u8, bg_b as u8, 255)
                .unwrap();
        }

        {
            let pixels = pm.pixels_mut();
            for (idx, cp) in range.clone().enumerate() {
                let Some(sprite) = rasterize(cp, cw, ch) else {
                    continue;
                };
                let col = idx as u32 % cols;
                let row = idx as u32 / cols;
                let ox = gap + col * (cw + gap);
                let oy = gap + row * (ch + gap);
                for y in 0..ch {
                    for x in 0..cw {
                        let a = sprite.bytes[(y * cw + x) as usize];
                        if a == 0 {
                            continue;
                        }
                        let dst = ((oy + y) * img_w + (ox + x)) as usize;
                        pixels[dst] = PremultipliedColorU8::from_rgba(
                            blend(bg_r, a),
                            blend(bg_g, a),
                            blend(bg_b, a),
                            255,
                        )
                        .unwrap();
                    }
                }
            }
        }

        let path = std::env::temp_dir().join(name);
        pm.save_png(&path).unwrap();
        println!("wrote {} ({}x{})", path.display(), img_w, img_h);
    }

    #[test]
    #[ignore = "visual dump; run: cargo test -p sugarloaf --lib sprite::tests::dump -- --ignored --nocapture"]
    fn dump_box_drawing_png() {
        // Even and odd cell sizes — odd sizes stress line centering.
        dump_range_png("rio_sprite_box_20x40.png", 0x2500..=0x257F, 20, 40);
        dump_range_png("rio_sprite_box_9x19.png", 0x2500..=0x257F, 9, 19);
    }

    #[test]
    fn block_range_rasterizes() {
        for cp in 0x2580u32..=0x259F {
            assert!(is_drawable(cp), "U+{cp:04X} should be drawable");
            let s = rasterize(cp, 12, 24).expect("block cp must rasterize");
            assert_eq!(s.bytes.len(), 12 * 24);
            assert!(
                s.bytes.iter().any(|&b| b > 0),
                "U+{cp:04X} produced an empty sprite"
            );
        }
        // Full block is fully opaque; light shade is partial everywhere.
        let full = rasterize(0x2588, 12, 24).unwrap();
        assert!(
            full.bytes.iter().all(|&b| b == 0xFF),
            "full block must be solid"
        );
        let light = rasterize(0x2591, 12, 24).unwrap();
        assert!(
            light.bytes.iter().all(|&b| b == 0x40),
            "light shade must be uniform 0x40"
        );
    }

    #[test]
    #[ignore = "visual dump; see dump_box_drawing_png"]
    fn dump_block_png() {
        dump_range_png("rio_sprite_block_20x40.png", 0x2580..=0x259F, 20, 40);
    }

    #[test]
    fn braille_range_rasterizes() {
        for cp in 0x2800u32..=0x28FF {
            assert!(is_drawable(cp), "U+{cp:04X} should be drawable");
            let s = rasterize(cp, 12, 24).expect("braille cp must rasterize");
            assert_eq!(s.bytes.len(), 12 * 24);
        }
        // U+2800 is the empty pattern (no dots); U+28FF has all 8 dots.
        let empty = rasterize(0x2800, 12, 24).unwrap();
        assert!(empty.bytes.iter().all(|&b| b == 0), "U+2800 must be empty");
        let full = rasterize(0x28FF, 12, 24).unwrap();
        assert!(full.bytes.iter().any(|&b| b > 0), "U+28FF must paint dots");
    }

    #[test]
    #[ignore = "visual dump; see dump_box_drawing_png"]
    fn dump_braille_png() {
        dump_range_png("rio_sprite_braille_20x40.png", 0x2800..=0x28FF, 20, 40);
    }

    #[test]
    fn legacy_sextant_octant_rasterizes() {
        for cp in 0x1FB00u32..=0x1FB3B {
            let s = rasterize(cp, 12, 24).expect("sextant must rasterize");
            assert_eq!(s.bytes.len(), 12 * 24);
            assert!(s.bytes.iter().any(|&b| b > 0), "U+{cp:04X} empty");
        }
        for cp in 0x1CD00u32..=0x1CDE5 {
            let s = rasterize(cp, 12, 24).expect("octant must rasterize");
            assert!(s.bytes.iter().any(|&b| b > 0), "U+{cp:04X} empty");
        }
        for cp in [0x1CEA0u32, 0x1CEA3, 0x1CEA8, 0x1CEAB, 0x1FBE6, 0x1FBE7] {
            assert!(is_drawable(cp), "U+{cp:04X} should be drawable");
            let s = rasterize(cp, 12, 24).expect("octant singleton must rasterize");
            assert!(s.bytes.iter().any(|&b| b > 0), "U+{cp:04X} empty");
        }
    }

    #[test]
    #[ignore = "visual dump; see dump_box_drawing_png"]
    fn dump_legacy_png() {
        dump_range_png("rio_sprite_sextant_20x40.png", 0x1FB00..=0x1FB3B, 20, 40);
        dump_range_png("rio_sprite_octant_20x40.png", 0x1CD00..=0x1CDE5, 20, 40);
    }

    #[test]
    fn powerline_rasterizes() {
        for cp in (0xE0B0u32..=0xE0BF).chain([0xE0D2, 0xE0D4]) {
            assert!(is_drawable(cp), "U+{cp:04X} should be drawable");
            let s = rasterize(cp, 16, 32).expect("powerline cp must rasterize");
            assert!(s.bytes.iter().any(|&b| b > 0), "U+{cp:04X} empty");
        }
    }

    #[test]
    #[ignore = "visual dump; see dump_box_drawing_png"]
    fn dump_powerline_png() {
        dump_range_png("rio_sprite_powerline_24x48.png", 0xE0B0..=0xE0D4, 24, 48);
    }

    #[test]
    fn geometric_rasterizes() {
        for cp in (0x25E2u32..=0x25E5).chain(0x25F8..=0x25FA).chain([0x25FF]) {
            assert!(is_drawable(cp), "U+{cp:04X} should be drawable");
            let s = rasterize(cp, 16, 32).expect("geometric cp must rasterize");
            assert!(s.bytes.iter().any(|&b| b > 0), "U+{cp:04X} empty");
        }
    }

    #[test]
    #[ignore = "visual dump; see dump_box_drawing_png"]
    fn dump_geometric_png() {
        dump_range_png("rio_sprite_geometric_24x48.png", 0x25E0..=0x25FF, 24, 48);
    }

    #[test]
    fn branch_rasterizes() {
        for cp in 0xF5D0u32..=0xF60D {
            assert!(is_drawable(cp), "U+{cp:04X} should be drawable");
            let s = rasterize(cp, 16, 32).expect("branch cp must rasterize");
            assert!(s.bytes.iter().any(|&b| b > 0), "U+{cp:04X} empty");
        }
    }

    #[test]
    #[ignore = "visual dump; see dump_box_drawing_png"]
    fn dump_branch_png() {
        dump_range_png("rio_sprite_branch_24x48.png", 0xF5D0..=0xF60D, 24, 48);
    }
}
