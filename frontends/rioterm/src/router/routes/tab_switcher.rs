use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText, Sugarloaf};

#[derive(Debug, Clone)]
pub struct TabSwitcherItem {
    pub index: usize,
    pub title: String,
    pub is_current: bool,
}

pub struct TabSwitcher {
    pub items: Vec<TabSwitcherItem>,
    pub selected_index: usize,
}

impl TabSwitcher {
    pub fn new(tabs: Vec<TabSwitcherItem>, current_tab: usize) -> Self {
        let selected_index = tabs.iter().position(|tab| tab.index == current_tab).unwrap_or(0);
        
        Self {
            items: tabs,
            selected_index,
        }
    }

    pub fn move_selection_up(&mut self) {
        if !self.items.is_empty() {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            } else {
                // Wrap to the last item
                self.selected_index = self.items.len() - 1;
            }
        }
    }

    pub fn move_selection_down(&mut self) {
        if !self.items.is_empty() {
            if self.selected_index < self.items.len() - 1 {
                self.selected_index += 1;
            } else {
                // Wrap to the first item
                self.selected_index = 0;
            }
        }
    }

    pub fn get_selected_tab(&self) -> Option<&TabSwitcherItem> {
        self.items.get(self.selected_index)
    }
}

impl Default for TabSwitcher {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected_index: 0,
        }
    }
}

#[inline]
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    tab_switcher: &TabSwitcher,
) {
    screen_with_objects(sugarloaf, context_dimension, tab_switcher, Vec::new());
}

#[inline]
pub fn screen_with_objects(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    tab_switcher: &TabSwitcher,
    existing_objects: Vec<Object>,
) {
    // Tab switcher color palette
    let text_primary = [0.95, 0.95, 0.95, 1.0]; // High contrast white
    let text_secondary = [0.6, 0.6, 0.6, 1.0]; // Muted gray for tab numbers
    let selection_text_color = [1.0, 1.0, 1.0, 1.0]; // Pure white for selected items
    let current_tab_color = [0.2, 0.8, 0.2, 1.0]; // Green for current tab indicator

    let layout = sugarloaf.window_size();
    let panel_width = 400.0; // Narrower than command palette
    let item_count = tab_switcher.items.len().min(10); // Max 10 visible items
    let panel_height = 80.0 + (item_count as f32 * 32.0); // Header + items
    
    // Center the panel
    let scaled_width = layout.width / context_dimension.dimension.scale;
    let panel_x = (scaled_width - panel_width) / 2.0;
    let panel_y = context_dimension.margin.top_y + 100.0; // Space from top

    let mut objects = existing_objects;

    // Multi-layer shadow for the main panel
    let shadow_layers = [
        (12.0, 0.25), // Close, strong shadow
        (24.0, 0.15), // Medium shadow
        (48.0, 0.08), // Far, soft shadow
    ];

    for (blur_radius, alpha) in shadow_layers.iter() {
        objects.push(Object::Quad(Quad::blur(
            [panel_x + 4.0, panel_y + 8.0], // Slight offset for natural shadow
            [panel_width, panel_height], // Same size as main panel
            [0.0, 0.0, 0.0, *alpha], // Black shadow with varying alpha
            *blur_radius // Different blur amounts for layered effect
        )));
    }

    // Main panel with frosted glass effect
    objects.push(Object::Quad(Quad::blur(
        [panel_x, panel_y],
        [panel_width, panel_height],
        [0.12, 0.16, 0.21, 0.6], // Semi-transparent dark background
        20.0 // Strong frosted glass effect
    ).with_border(
        [0.4, 0.4, 0.45, 0.4], // More visible border for definition
        [16.0; 4], // Rounded corners
        1.5 // Slightly thicker border
    )));

    // Inner highlight for glassmorphism effect
    objects.push(Object::Quad(Quad::blur(
        [panel_x + 2.0, panel_y + 2.0],
        [panel_width - 4.0, 40.0], // Just at the top
        [0.6, 0.6, 0.7, 0.1], // Very subtle white highlight
        4.0 // Small blur for soft highlight
    ).with_border(
        [0.0; 4],
        [14.0, 14.0, 0.0, 0.0], // Only top corners rounded
        0.0
    )));

    // Create rich text objects
    let header_text = sugarloaf.create_temp_rich_text();
    let instructions_text = sugarloaf.create_temp_rich_text();

    // Create rich text objects for items
    let max_visible_items = 10;
    let mut item_texts = Vec::new();
    let mut number_texts = Vec::new();
    let mut indicator_texts = Vec::new();
    
    for _ in 0..max_visible_items.min(tab_switcher.items.len()) {
        item_texts.push(sugarloaf.create_temp_rich_text());
        number_texts.push(sugarloaf.create_temp_rich_text());
        indicator_texts.push(sugarloaf.create_temp_rich_text());
    }

    // Get text dimensions to calculate proper spacing
    let sample_text_id = sugarloaf.create_temp_rich_text();
    let item_font_size = 16.0;
    sugarloaf.set_rich_text_font_size(&sample_text_id, item_font_size);
    let content_temp = sugarloaf.content();
    let sample_line = content_temp.sel(sample_text_id);
    sample_line
        .clear()
        .add_text("Sample", FragmentStyle::default())
        .build();
    let text_dimensions = sugarloaf.get_rich_text_dimensions(&sample_text_id);

    // Set font sizes
    sugarloaf.set_rich_text_font_size(&header_text, 18.0); // Header text
    sugarloaf.set_rich_text_font_size(&instructions_text, 11.0);
    
    for text_id in &item_texts {
        sugarloaf.set_rich_text_font_size(text_id, item_font_size); // Item text
    }
    
    for text_id in &number_texts {
        sugarloaf.set_rich_text_font_size(text_id, 14.0); // Number text
    }

    for text_id in &indicator_texts {
        sugarloaf.set_rich_text_font_size(text_id, 16.0); // Indicator text
    }

    let content = sugarloaf.content();

    // Header
    let header_y = panel_y + 20.0;
    
    let header_line = content.sel(header_text);
    header_line
        .clear()
        .add_text("Switch Tab", FragmentStyle {
            color: text_primary,
            ..FragmentStyle::default()
        })
        .build();

    objects.push(Object::RichText(RichText {
        id: header_text,
        position: [panel_x + 24.0, header_y],
        lines: None,
    }));

    // Separator line
    objects.push(Object::Quad(Quad::solid(
        [panel_x + 24.0, header_y + 30.0],
        [panel_width - 48.0, 1.0],
        [0.3, 0.3, 0.35, 0.6], // Slightly more visible separator
    ).with_border(
        [0.0; 4],
        [0.5, 0.5, 0.5, 0.5], // Tiny rounding for smooth line
        0.0
    )));

    // Tab items
    let item_height = text_dimensions.height + 4.0; // Text height + padding
    let items_start_y = header_y + 45.0; // Space after separator

    for (index, item) in tab_switcher.items.iter().enumerate().take(max_visible_items) {
        let item_y = items_start_y + (index as f32 * item_height);
        
        // Selection highlight
        if index == tab_switcher.selected_index {
            objects.push(Object::Quad(Quad::blur(
                [panel_x + 12.0, item_y - 2.0],
                [panel_width - 24.0, item_height],
                [0.2, 0.4, 0.8, 0.7], // Blue selection
                2.0 // Very subtle blur for smooth selection
            ).with_border(
                [0.3, 0.5, 0.9, 0.5], // Bright blue border
                [8.0; 4], // Rounded selection
                1.0
            )));
        }

        // Tab number
        let number_line = content.sel(number_texts[index]);
        number_line
            .clear()
            .add_text(&format!("{}", item.index + 1), FragmentStyle {
                color: text_secondary,
                ..FragmentStyle::default()
            })
            .build();

        objects.push(Object::RichText(RichText {
            id: number_texts[index],
            position: [panel_x + 24.0, item_y],
            lines: None,
        }));

        // Current tab indicator
        if item.is_current {
            let indicator_line = content.sel(indicator_texts[index]);
            indicator_line
                .clear()
                .add_text("●", FragmentStyle {
                    color: current_tab_color,
                    ..FragmentStyle::default()
                })
                .build();

            objects.push(Object::RichText(RichText {
                id: indicator_texts[index],
                position: [panel_x + 50.0, item_y],
                lines: None,
            }));
        }

        // Tab title
        let title_color = if index == tab_switcher.selected_index {
            selection_text_color
        } else {
            text_primary
        };

        let item_line = content.sel(item_texts[index]);
        item_line
            .clear()
            .add_text(&item.title, FragmentStyle {
                color: title_color,
                ..FragmentStyle::default()
            })
            .build();

        objects.push(Object::RichText(RichText {
            id: item_texts[index],
            position: [panel_x + 70.0, item_y], // After number and indicator
            lines: None,
        }));
    }

    // Instructions footer
    let instructions_line = content.sel(instructions_text);
    instructions_line
        .clear()
        .add_text("Press ↑↓ to navigate • ↩ to switch", FragmentStyle {
            color: text_secondary,
            ..FragmentStyle::default()
        })
        .build();

    objects.push(Object::RichText(RichText {
        id: instructions_text,
        position: [panel_x + 24.0, panel_y + panel_height - 24.0],
        lines: None,
    }));

    sugarloaf.set_objects(objects);
}