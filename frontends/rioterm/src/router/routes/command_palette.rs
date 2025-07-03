use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText, Sugarloaf};

#[derive(Debug, Clone)]
pub struct CommandPaletteItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub action: CommandAction,
}

#[derive(Debug, Clone)]
pub enum CommandAction {
    ConfigEditor,
    CreateWindow,
    CreateTab,
    CloseTab,
    ToggleFullscreen,
    SearchForward,
    SearchBackward,
    ClearHistory,
    ToggleViMode,
    SplitRight,
    SplitDown,
    SelectNextTab,
    SelectPrevTab,
    Copy,
    Paste,
    Quit,
}

pub struct CommandPalette {
    pub query: String,
    pub items: Vec<CommandPaletteItem>,
    pub filtered_items: Vec<CommandPaletteItem>,
    pub selected_index: usize,
}

impl CommandPalette {
    pub fn new() -> Self {
        let items = vec![
            CommandPaletteItem {
                id: "config".to_string(),
                title: "Open Settings".to_string(),
                description: "Open the configuration editor".to_string(),
                action: CommandAction::ConfigEditor,
            },
            CommandPaletteItem {
                id: "new_window".to_string(),
                title: "New Window".to_string(),
                description: "Create a new terminal window".to_string(),
                action: CommandAction::CreateWindow,
            },
            CommandPaletteItem {
                id: "new_tab".to_string(),
                title: "New Tab".to_string(),
                description: "Create a new terminal tab".to_string(),
                action: CommandAction::CreateTab,
            },
            CommandPaletteItem {
                id: "close_tab".to_string(),
                title: "Close Tab".to_string(),
                description: "Close the current tab".to_string(),
                action: CommandAction::CloseTab,
            },
            CommandPaletteItem {
                id: "fullscreen".to_string(),
                title: "Toggle Fullscreen".to_string(),
                description: "Toggle fullscreen mode".to_string(),
                action: CommandAction::ToggleFullscreen,
            },
            CommandPaletteItem {
                id: "search_forward".to_string(),
                title: "Search Forward".to_string(),
                description: "Start searching forward in the terminal".to_string(),
                action: CommandAction::SearchForward,
            },
            CommandPaletteItem {
                id: "search_backward".to_string(),
                title: "Search Backward".to_string(),
                description: "Start searching backward in the terminal".to_string(),
                action: CommandAction::SearchBackward,
            },
            CommandPaletteItem {
                id: "clear_history".to_string(),
                title: "Clear History".to_string(),
                description: "Clear the terminal history".to_string(),
                action: CommandAction::ClearHistory,
            },
            CommandPaletteItem {
                id: "vi_mode".to_string(),
                title: "Toggle Vi Mode".to_string(),
                description: "Toggle Vi mode for navigation".to_string(),
                action: CommandAction::ToggleViMode,
            },
            CommandPaletteItem {
                id: "split_right".to_string(),
                title: "Split Right".to_string(),
                description: "Split the terminal vertically".to_string(),
                action: CommandAction::SplitRight,
            },
            CommandPaletteItem {
                id: "split_down".to_string(),
                title: "Split Down".to_string(),
                description: "Split the terminal horizontally".to_string(),
                action: CommandAction::SplitDown,
            },
            CommandPaletteItem {
                id: "next_tab".to_string(),
                title: "Next Tab".to_string(),
                description: "Switch to the next tab".to_string(),
                action: CommandAction::SelectNextTab,
            },
            CommandPaletteItem {
                id: "prev_tab".to_string(),
                title: "Previous Tab".to_string(),
                description: "Switch to the previous tab".to_string(),
                action: CommandAction::SelectPrevTab,
            },
            CommandPaletteItem {
                id: "copy".to_string(),
                title: "Copy".to_string(),
                description: "Copy selected text to clipboard".to_string(),
                action: CommandAction::Copy,
            },
            CommandPaletteItem {
                id: "paste".to_string(),
                title: "Paste".to_string(),
                description: "Paste from clipboard".to_string(),
                action: CommandAction::Paste,
            },
            CommandPaletteItem {
                id: "quit".to_string(),
                title: "Quit Rio".to_string(),
                description: "Exit the terminal application".to_string(),
                action: CommandAction::Quit,
            },
        ];

        let filtered_items = items.clone();

        Self {
            query: String::new(),
            items,
            filtered_items,
            selected_index: 0,
        }
    }

    pub fn update_query(&mut self, query: String) {
        self.query = query;
        self.filter_items();
        self.selected_index = 0;
    }

    pub fn add_char(&mut self, ch: char) {
        self.query.push(ch);
        self.filter_items();
        self.selected_index = 0;
    }

    pub fn remove_char(&mut self) {
        self.query.pop();
        self.filter_items();
        self.selected_index = 0;
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.filter_items();
        self.selected_index = 0;
    }

    pub fn move_selection_up(&mut self) {
        if !self.filtered_items.is_empty() {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            } else {
                // Wrap to the last item
                self.selected_index = self.filtered_items.len() - 1;
            }
        }
    }

    pub fn move_selection_down(&mut self) {
        if !self.filtered_items.is_empty() {
            if self.selected_index < self.filtered_items.len() - 1 {
                self.selected_index += 1;
            } else {
                // Wrap to the first item
                self.selected_index = 0;
            }
        }
    }

    pub fn get_selected_item(&self) -> Option<&CommandPaletteItem> {
        self.filtered_items.get(self.selected_index)
    }

    fn filter_items(&mut self) {
        if self.query.is_empty() {
            self.filtered_items = self.items.clone();
        } else {
            let query_lower = self.query.to_lowercase();
            self.filtered_items = self.items
                .iter()
                .filter(|item| {
                    item.title.to_lowercase().contains(&query_lower)
                        || item.description.to_lowercase().contains(&query_lower)
                })
                .cloned()
                .collect();
        }
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    command_palette: &CommandPalette,
) {
    screen_with_objects(sugarloaf, context_dimension, command_palette, Vec::new());
}

#[inline]
pub fn screen_with_objects(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    command_palette: &CommandPalette,
    existing_objects: Vec<Object>,
) {
    // Raycast-inspired color palette
    let _backdrop_color = [0.0, 0.0, 0.0, 0.4]; // Dark backdrop overlay (unused)
    let text_primary = [0.95, 0.95, 0.95, 1.0]; // High contrast white
    let text_secondary = [0.6, 0.6, 0.6, 1.0]; // Muted gray for descriptions
    let query_text_color = [0.98, 0.98, 0.98, 1.0]; // Bright white for input
    let selection_text_color = [1.0, 1.0, 1.0, 1.0]; // Pure white for selected items

    let layout = sugarloaf.window_size();
    let panel_width = 600.0; // Raycast-like width
    let panel_height = 400.0; // Compact height
    
    // Center the panel
    let scaled_width = layout.width / context_dimension.dimension.scale;
    let panel_x = (scaled_width - panel_width) / 2.0;
    let panel_y = context_dimension.margin.top_y + 100.0; // Space from top

    let mut objects = existing_objects;

    // No backdrop overlay - terminal content remains fully visible

    // Multi-layer shadow for the main panel (like macOS/Raycast)
    let shadow_layers = [
        (12.0, 0.25), // Close, strong shadow
        (24.0, 0.15), // Medium shadow
        (48.0, 0.08), // Far, soft shadow
        (96.0, 0.04), // Very far, subtle shadow
    ];

    for (blur_radius, alpha) in shadow_layers.iter() {
        objects.push(Object::Quad(Quad::blur(
            [panel_x + 4.0, panel_y + 8.0], // Slight offset for natural shadow
            [panel_width, panel_height], // Same size as main panel
            [0.0, 0.0, 0.0, *alpha], // Black shadow with varying alpha
            *blur_radius // Different blur amounts for layered effect
        )));
    }

    // Main panel with frosted glass effect (like example.html)
    // Using the new blur quad with Raycast-style colors
    objects.push(Object::Quad(Quad::blur(
        [panel_x, panel_y],
        [panel_width, panel_height],
        [0.12, 0.16, 0.21, 0.6], // Semi-transparent dark background (matches example.html)
        20.0 // Strong frosted glass effect
    ).with_border(
        [0.4, 0.4, 0.45, 0.4], // More visible border for definition
        [16.0; 4], // Rounded corners like Raycast (larger radius)
        1.5 // Slightly thicker border
    )));

    // Inner highlight for glassmorphism effect (subtle light reflection)
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

    // Create all rich text objects first
    let query_text = sugarloaf.create_temp_rich_text();
    let instructions_text = sugarloaf.create_temp_rich_text();
    let symbol_text = sugarloaf.create_temp_rich_text();

    // Create rich text objects for items
    let max_visible_items = 8;
    let mut item_title_texts = Vec::new();
    let mut icon_texts = Vec::new();
    
    for _ in 0..max_visible_items.min(command_palette.filtered_items.len()) {
        item_title_texts.push(sugarloaf.create_temp_rich_text());
        icon_texts.push(sugarloaf.create_temp_rich_text());
    }

    // Set font sizes
    sugarloaf.set_rich_text_font_size(&query_text, 18.0); // Input text
    sugarloaf.set_rich_text_font_size(&instructions_text, 11.0);
    sugarloaf.set_rich_text_font_size(&symbol_text, 20.0);
    
    let item_font_size = 16.0;
    for title_text_id in &item_title_texts {
        sugarloaf.set_rich_text_font_size(title_text_id, item_font_size); // Item text
    }
    
    for icon_text_id in &icon_texts {
        sugarloaf.set_rich_text_font_size(icon_text_id, 18.0); // Icon text
    }

    // Get text dimensions to calculate proper spacing
    let sample_text_id = sugarloaf.create_temp_rich_text();
    sugarloaf.set_rich_text_font_size(&sample_text_id, item_font_size);
    let content_temp = sugarloaf.content();
    let sample_line = content_temp.sel(sample_text_id);
    sample_line
        .clear()
        .add_text("Sample", FragmentStyle::default())
        .build();
    let text_dimensions = sugarloaf.get_rich_text_dimensions(&sample_text_id);

    let content = sugarloaf.content();

    // Input area with command symbol (like Raycast)
    let input_y = panel_y + 20.0;
    let input_height = 60.0;
    
    // Input background with subtle styling and rounded corners
    objects.push(Object::Quad(Quad::solid(
        [panel_x + 16.0, input_y],
        [panel_width - 32.0, input_height],
        [0.08, 0.08, 0.12, 0.8], // Slightly darker background for input
    ).with_border(
        [0.25, 0.25, 0.3, 0.4], // Subtle border
        [12.0; 4], // Rounded input area
        1.0
    )));

    // Command symbol (⌘) like in example.html
    let symbol_line = content.sel(symbol_text);
    symbol_line
        .clear()
        .add_text("⌘", FragmentStyle {
            color: text_secondary,
            ..FragmentStyle::default()
        })
        .build();

    objects.push(Object::RichText(RichText {
        id: symbol_text,
        position: [panel_x + 32.0, input_y + 20.0],
        lines: None,
    }));

    // Query input text
    let query_display = if command_palette.query.is_empty() {
        "Type a command or search..."
    } else {
        &command_palette.query
    };

    let query_line = content.sel(query_text);
    query_line
        .clear()
        .add_text(query_display, FragmentStyle {
            color: if command_palette.query.is_empty() { 
                text_secondary 
            } else { 
                query_text_color 
            },
            ..FragmentStyle::default()
        })
        .build();

    objects.push(Object::RichText(RichText {
        id: query_text,
        position: [panel_x + 70.0, input_y + 20.0], // After the command symbol
        lines: None,
    }));

    // Separator line (like in example.html) - with rounded ends
    objects.push(Object::Quad(Quad::solid(
        [panel_x + 24.0, input_y + input_height + 8.0],
        [panel_width - 48.0, 1.0],
        [0.3, 0.3, 0.35, 0.6], // Slightly more visible separator
    ).with_border(
        [0.0; 4],
        [0.5, 0.5, 0.5, 0.5], // Tiny rounding for smooth line
        0.0
    )));

    // Command items (like example.html list items)
    let item_height = text_dimensions.height + 2.0; // Text height + minimal padding
    let items_start_y = input_y + input_height + 16.0; // More space after separator

    for (index, item) in command_palette.filtered_items.iter().enumerate().take(max_visible_items) {
        let item_y = items_start_y + (index as f32 * item_height);
        
        // Selection highlight (like example.html selected item) with rounded corners
        if index == command_palette.selected_index {
            // Selection background with subtle blur for smooth appearance
            objects.push(Object::Quad(Quad::blur(
                [panel_x + 12.0, item_y],
                [panel_width - 24.0, item_height],
                [0.2, 0.4, 0.8, 0.7], // Blue selection (like example.html)
                2.0 // Very subtle blur for smooth selection
            ).with_border(
                [0.3, 0.5, 0.9, 0.5], // Bright blue border
                [10.0; 4], // Rounded selection
                1.0
            )));
        }

        // Add emoji/icon for each command (like example.html)
        let icon = match item.action {
            CommandAction::ConfigEditor => "⚙️",
            CommandAction::CreateWindow => "🪟",
            CommandAction::CreateTab => "📄",
            CommandAction::CloseTab => "❌",
            CommandAction::ToggleFullscreen => "⛶",
            CommandAction::SearchForward => "🔍",
            CommandAction::SearchBackward => "🔍",
            CommandAction::ClearHistory => "🗑️",
            CommandAction::ToggleViMode => "📝",
            CommandAction::SplitRight => "⫸",
            CommandAction::SplitDown => "⫷",
            CommandAction::SelectNextTab => "→",
            CommandAction::SelectPrevTab => "←",
            CommandAction::Copy => "📋",
            CommandAction::Paste => "📄",
            CommandAction::Quit => "🚪",
        };

        // Icon
        let icon_line = content.sel(icon_texts[index]);
        icon_line
            .clear()
            .add_text(icon, FragmentStyle {
                color: text_primary,
                ..FragmentStyle::default()
            })
            .build();

        objects.push(Object::RichText(RichText {
            id: icon_texts[index],
            position: [panel_x + 24.0, item_y + 1.0], // Minimal top padding
            lines: None,
        }));

        // Item title
        let title_color = if index == command_palette.selected_index {
            selection_text_color
        } else {
            text_primary
        };

        let item_title_line = content.sel(item_title_texts[index]);
        item_title_line
            .clear()
            .add_text(&item.title, FragmentStyle {
                color: title_color,
                ..FragmentStyle::default()
            })
            .build();

        objects.push(Object::RichText(RichText {
            id: item_title_texts[index],
            position: [panel_x + 60.0, item_y + 1.0], // Minimal top padding, after icon
            lines: None,
        }));
    }

    // Instructions footer (like example.html)
    let instructions_line = content.sel(instructions_text);
    instructions_line
        .clear()
        .add_text("Press ↑↓ to navigate • ↩ to select", FragmentStyle {
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