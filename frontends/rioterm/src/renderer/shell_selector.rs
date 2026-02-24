//! Shell selector UI renderer
//!
//! This module renders the shell profile selector overlay that allows users
//! to quickly switch between different shell configurations.

use crate::shell_selector::ShellSelector;
use rio_backend::config::colors::Colors;
use rio_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText};

/// Width of the selector panel in pixels
const PANEL_WIDTH: f32 = 300.0;

/// Height of each item in the list in pixels
const ITEM_HEIGHT: f32 = 28.0;

/// Padding inside the panel in pixels
const PANEL_PADDING: f32 = 12.0;

/// Font size for the selector text
const FONT_SIZE: f32 = 14.0;

/// Alpha value for the backdrop overlay
const BACKDROP_ALPHA: f32 = 0.5;

/// Border radius for all corners (uniform)
const BORDER_RADIUS: [f32; 4] = [8.0, 8.0, 8.0, 8.0];

/// Draw the shell selector overlay
///
/// This renders:
/// - A semi-transparent backdrop covering the terminal
/// - A centered panel with the list of shell profiles
/// - Each profile with a shortcut key, name, and selection highlight
#[inline]
pub fn draw_shell_selector(
    objects: &mut Vec<Object>,
    sugarloaf: &mut rio_backend::sugarloaf::Sugarloaf,
    shell_selector: &ShellSelector,
    colors: &Colors,
    dimensions: (f32, f32, f32),
) {
    let (width, height, scale) = dimensions;

    // Scale dimensions
    let scaled_width = width / scale;
    let scaled_height = height / scale;

    let profiles = shell_selector.profiles_for_display();

    // Calculate panel dimensions
    let panel_height =
        (profiles.len() as f32 * ITEM_HEIGHT) + (PANEL_PADDING * 2.0) + ITEM_HEIGHT;
    let panel_x = (scaled_width - PANEL_WIDTH) / 2.0;
    let panel_y = (scaled_height - panel_height) / 2.0;

    // Draw semi-transparent backdrop
    let backdrop_color = [
        colors.background.0[0],
        colors.background.0[1],
        colors.background.0[2],
        BACKDROP_ALPHA,
    ];
    objects.push(Object::Quad(Quad {
        position: [0.0, 0.0],
        size: [scaled_width, scaled_height],
        color: backdrop_color,
        ..Quad::default()
    }));

    // Draw panel background
    objects.push(Object::Quad(Quad {
        position: [panel_x, panel_y],
        size: [PANEL_WIDTH, panel_height],
        color: colors.bar,
        border_radius: BORDER_RADIUS,
        ..Quad::default()
    }));

    // Draw panel border (using a second quad with slightly larger size for border effect)
    let border_color = colors.tabs;
    let border_width = 1.0;
    objects.push(Object::Quad(Quad {
        position: [panel_x - border_width, panel_y - border_width],
        size: [
            PANEL_WIDTH + (border_width * 2.0),
            panel_height + (border_width * 2.0),
        ],
        color: border_color,
        border_radius: [
            BORDER_RADIUS[0] + border_width,
            BORDER_RADIUS[1] + border_width,
            BORDER_RADIUS[2] + border_width,
            BORDER_RADIUS[3] + border_width,
        ],
        ..Quad::default()
    }));

    // Redraw panel background on top of border
    objects.push(Object::Quad(Quad {
        position: [panel_x, panel_y],
        size: [PANEL_WIDTH, panel_height],
        color: colors.bar,
        border_radius: BORDER_RADIUS,
        ..Quad::default()
    }));

    // Draw title
    let title_rich_text = sugarloaf.create_temp_rich_text();
    sugarloaf.set_rich_text_font_size(&title_rich_text, FONT_SIZE);

    let content = sugarloaf.content();
    let title_line = content.sel(title_rich_text);
    title_line
        .clear()
        .new_line()
        .add_text(
            "Select Shell Profile",
            FragmentStyle {
                color: colors.foreground,
                ..FragmentStyle::default()
            },
        )
        .build();

    objects.push(Object::RichText(RichText {
        id: title_rich_text,
        position: [panel_x + PANEL_PADDING, panel_y + PANEL_PADDING - 4.0],
        lines: None,
    }));

    // Draw each profile item
    let items_start_y = panel_y + PANEL_PADDING + ITEM_HEIGHT;
    for (index, shell, is_selected) in &profiles {
        let item_y = items_start_y + (index as f32 * ITEM_HEIGHT);

        // Draw selection highlight background
        if *is_selected {
            objects.push(Object::Quad(Quad {
                position: [panel_x + 4.0, item_y],
                size: [PANEL_WIDTH - 8.0, ITEM_HEIGHT - 4.0],
                color: colors.tabs_active,
                border_radius: [4.0, 4.0, 4.0, 4.0],
                ..Quad::default()
            }));
        }

        // Create the display text with shortcut key
        let shortcut = get_shortcut_key(index);
        let display_name = ShellSelector::display_name(shell);

        // Create rich text for this item
        let item_rich_text = sugarloaf.create_temp_rich_text();
        sugarloaf.set_rich_text_font_size(&item_rich_text, FONT_SIZE);

        let content = sugarloaf.content();
        let item_line = content.sel(item_rich_text);

        // Use different colors for selected item
        let text_color = if *is_selected {
            colors.tabs_active_foreground
        } else {
            colors.tabs_foreground
        };

        let shortcut_color = if *is_selected {
            colors.tabs_active_highlight
        } else {
            colors.tabs
        };

        // Draw shortcut key with accent color
        item_line
            .clear()
            .new_line()
            .add_text(
                &format!("{} ", shortcut),
                FragmentStyle {
                    color: shortcut_color,
                    ..FragmentStyle::default()
                },
            )
            .add_text(
                display_name,
                FragmentStyle {
                    color: text_color,
                    ..FragmentStyle::default()
                },
            )
            .build();

        objects.push(Object::RichText(RichText {
            id: item_rich_text,
            position: [panel_x + PANEL_PADDING, item_y + 4.0],
            lines: None,
        }));
    }

    // Draw help text at the bottom
    let help_rich_text = sugarloaf.create_temp_rich_text();
    sugarloaf.set_rich_text_font_size(&help_rich_text, 12.0);

    let content = sugarloaf.content();
    let help_line = content.sel(help_rich_text);
    help_line
        .clear()
        .new_line()
        .add_text(
            "Enter to select, Esc to cancel",
            FragmentStyle {
                color: [
                    colors.foreground[0],
                    colors.foreground[1],
                    colors.foreground[2],
                    0.6, // Slightly dimmed
                ],
                ..FragmentStyle::default()
            },
        )
        .build();

    objects.push(Object::RichText(RichText {
        id: help_rich_text,
        position: [
            panel_x + PANEL_PADDING,
            panel_y + panel_height - PANEL_PADDING - 8.0,
        ],
        lines: None,
    }));
}

/// Get the shortcut key for a given index
///
/// Uses 1-9 for the first 9 items, then a-z for items 10+
fn get_shortcut_key(index: usize) -> String {
    if index < 9 {
        // 1-9 for first 9 items (1-indexed for user friendliness)
        format!("{}.", index + 1)
    } else {
        // a-z for items 10-35
        let letter_index = index - 9;
        if letter_index < 26 {
            let c = (b'a' + letter_index as u8) as char;
            format!("{}.", c)
        } else {
            // Fall back to number for items beyond 35
            format!("{}.", index + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_shortcut_key() {
        // Test first 9 items (1-9)
        assert_eq!(get_shortcut_key(0), "1.");
        assert_eq!(get_shortcut_key(1), "2.");
        assert_eq!(get_shortcut_key(8), "9.");

        // Test items 10-35 (a-z)
        assert_eq!(get_shortcut_key(9), "a.");
        assert_eq!(get_shortcut_key(10), "b.");
        assert_eq!(get_shortcut_key(34), "y.");
        assert_eq!(get_shortcut_key(35), "z.");

        // Test items beyond 35
        assert_eq!(get_shortcut_key(36), "37.");
        assert_eq!(get_shortcut_key(37), "38.");
    }
}
