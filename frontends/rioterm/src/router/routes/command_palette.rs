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
    pub scroll_offset: usize, // Track scroll position for long lists
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
            scroll_offset: 0,
        }
    }

    pub fn update_query(&mut self, query: String) {
        self.query = query;
        self.filter_items();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn add_char(&mut self, ch: char) {
        self.query.push(ch);
        self.filter_items();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn remove_char(&mut self) {
        self.query.pop();
        self.filter_items();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.filter_items();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn move_selection_up(&mut self) {
        if !self.filtered_items.is_empty() {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            } else {
                // Wrap to the last item
                self.selected_index = self.filtered_items.len() - 1;
            }
            self.update_scroll_for_selection();
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
            self.update_scroll_for_selection();
        }
    }

    // Update scroll offset to keep selected item visible
    fn update_scroll_for_selection(&mut self) {
        const MAX_VISIBLE_ITEMS: usize = 7; // Show 7 items at once like Arc
        
        if self.filtered_items.len() <= MAX_VISIBLE_ITEMS {
            self.scroll_offset = 0;
            return;
        }

        // When scrollbar is visible, show one fewer item
        let actual_visible_items = MAX_VISIBLE_ITEMS - 1;

        // If selected item is above visible area, scroll up
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
        // If selected item is below visible area, scroll down
        else if self.selected_index >= self.scroll_offset + actual_visible_items {
            self.scroll_offset = self.selected_index - actual_visible_items + 1;
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
    // Arc-inspired color palette (matching updated example.html)
    let text_primary = [0.75, 0.75, 0.75, 1.0]; // #c0c0c0 - Arc's muted text color
    let text_secondary = [0.53, 0.53, 0.53, 1.0]; // #888888 - muted gray for placeholders
    let query_text_color = [0.88, 0.88, 0.88, 1.0]; // #e0e0e0 - Arc's input text color
    let selection_text_color = [1.0, 1.0, 1.0, 1.0]; // Pure white for selected items
    let icon_background_color = [0.39, 0.39, 1.0, 0.2]; // rgba(100, 100, 255, 0.2) - Arc's icon background
    let icon_text_color = [0.63, 0.63, 1.0, 1.0]; // #a0a0ff - Arc's icon color

    let layout = sugarloaf.window_size();
    let panel_width = 650.0; // Wider like Arc (max-width: 650px)
    let panel_height = 450.0; // Taller for more content
    
    // Center the panel
    let scaled_width = layout.width / context_dimension.dimension.scale;
    let panel_x = (scaled_width - panel_width) / 2.0;
    let panel_y = context_dimension.margin.top_y + 60.0; // Higher up like Arc

    let mut objects = existing_objects;

    // No backdrop overlay - terminal content remains fully visible

    // Arc-style shadow layers (deeper, softer shadows)
    let shadow_layers = [
        (15.0, 0.5),  // Close, strong shadow
        (25.0, 0.3),  // Medium shadow  
        (40.0, 0.15), // Far shadow
    ];

    for (blur_radius, alpha) in shadow_layers.iter() {
        objects.push(Object::Quad(Quad::blur(
            [panel_x + 2.0, panel_y + 6.0], // Slight offset for natural shadow
            [panel_width, panel_height],
            [0.0, 0.0, 0.0, *alpha], // Black shadow with varying alpha
            *blur_radius
        )));
    }

    // Main panel with Arc-style frosted glass effect
    // background-color: rgba(30, 30, 40, 0.9) + backdrop-filter: blur(30px) saturate(180%)
    objects.push(Object::Quad(Quad::blur(
        [panel_x, panel_y],
        [panel_width, panel_height],
        [0.12, 0.12, 0.16, 0.9], // rgba(30, 30, 40, 0.9) from Arc style
        30.0 // Strong frosted glass effect matching CSS backdrop-filter: blur(30px)
    ).with_border(
        [1.0, 1.0, 1.0, 0.05], // Subtle white border like Arc (0 0 0 1px rgba(255, 255, 255, 0.05))
        [20.0; 4], // More pronounced rounded corners (border-radius: 20px)
        1.0
    )));

    // Remove the inner highlight - keep it clean like example.html

    // Create all rich text objects first
    let query_text = sugarloaf.create_temp_rich_text();
    let instructions_text = sugarloaf.create_temp_rich_text();

    // Create rich text objects for items (show 7 items like Arc with scrolling)
    let max_visible_items = 7; // Show fewer items to match Arc's compact design
    let total_items = command_palette.filtered_items.len();
    let has_scrollbar = total_items > max_visible_items;
    
    // When scrollbar is visible, show one fewer item to make room
    let actual_visible_items = if has_scrollbar { max_visible_items - 1 } else { max_visible_items };
    
    // Calculate visible items based on scroll offset
    let visible_items: Vec<(usize, &CommandPaletteItem)> = command_palette.filtered_items
        .iter()
        .enumerate()
        .skip(command_palette.scroll_offset)
        .take(actual_visible_items)
        .collect();
    
    let mut item_title_texts = Vec::new();
    let mut icon_texts = Vec::new();
    
    for _ in 0..visible_items.len() {
        item_title_texts.push(sugarloaf.create_temp_rich_text());
        icon_texts.push(sugarloaf.create_temp_rich_text());
    }

    // Set font sizes (matching Arc's typography)
    sugarloaf.set_rich_text_font_size(&query_text, 19.0); // font-size: 1.2rem from Arc
    sugarloaf.set_rich_text_font_size(&instructions_text, 12.0); // Small footer text
    
    let item_font_size = 17.0; // font-size: 1.05rem from Arc
    for title_text_id in &item_title_texts {
        sugarloaf.set_rich_text_font_size(title_text_id, item_font_size);
    }
    
    for icon_text_id in &icon_texts {
        sugarloaf.set_rich_text_font_size(icon_text_id, 18.0); // font-size: 1.1rem for icons
    }

    // Get text dimensions to calculate proper spacing (keeping for compatibility)
    let sample_text_id = sugarloaf.create_temp_rich_text();
    sugarloaf.set_rich_text_font_size(&sample_text_id, item_font_size);
    let content_temp = sugarloaf.content();
    let sample_line = content_temp.sel(sample_text_id);
    sample_line
        .clear()
        .add_text("Sample", FragmentStyle::default())
        .build();
    let _text_dimensions = sugarloaf.get_rich_text_dimensions(&sample_text_id);

    let content = sugarloaf.content();

    // Input area styling (matching Arc's .arc-input)
    let input_y = panel_y + 12.0; // padding: 12px from Arc container
    let input_height = 64.0; // Larger padding for spacious feel (padding: 16px 20px)
    
    // Input background (matching Arc's rgba(40, 40, 55, 0.8))
    objects.push(Object::Quad(Quad::solid(
        [panel_x + 12.0, input_y],
        [panel_width - 24.0, input_height],
        [0.16, 0.16, 0.22, 0.8], // rgba(40, 40, 55, 0.8) from Arc style
    ).with_border(
        [0.0, 0.0, 0.0, 0.2], // inset shadow effect
        [15.0; 4], // border-radius: 15px
        0.0
    )));

    // Remove command symbol - Arc style doesn't use it
    
    // Query input text
    let query_display = if command_palette.query.is_empty() {
        "Search commands, apps, files..."
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
        position: [panel_x + 32.0, input_y + 22.0], // Centered in larger input area
        lines: None,
    }));

    // Command items (matching Arc's .arc-result-item styling)
    let item_height = 54.0; // Larger items (padding: 12px 16px + margin-bottom: 6px)
    let items_start_y = input_y + input_height + 10.0; // margin-top: 10px from Arc style
    let items_area_width = if has_scrollbar { panel_width - 48.0 } else { panel_width - 32.0 }; // Leave space for scrollbar

    for (display_index, (actual_index, item)) in visible_items.iter().enumerate() {
        let item_y = items_start_y + (display_index as f32 * item_height);
        
        // Selection highlight (matching Arc's hover state)
        if *actual_index == command_palette.selected_index {
            objects.push(Object::Quad(Quad::solid(
                [panel_x + 16.0, item_y],
                [items_area_width, item_height - 6.0], // Account for margin-bottom: 6px
                [0.20, 0.20, 0.27, 0.7], // rgba(50, 50, 70, 0.7) from Arc hover
            ).with_border(
                [0.0; 4],
                [12.0; 4], // border-radius: 12px from Arc
                0.0
            )));
        }

        // Icon background (matching Arc's .arc-result-item-icon)
        let icon_x = panel_x + 28.0;
        let icon_y = item_y + 12.0; // Centered in item
        objects.push(Object::Quad(Quad::solid(
            [icon_x, icon_y],
            [30.0, 30.0], // width: 30px; height: 30px from Arc
            icon_background_color, // rgba(100, 100, 255, 0.2)
        ).with_border(
            [0.0; 4],
            [8.0; 4], // border-radius: 8px from Arc
            0.0
        )));

        // Add emoji/icon for each command (matching Arc style)
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

        // Icon text
        let icon_line = content.sel(icon_texts[display_index]);
        icon_line
            .clear()
            .add_text(icon, FragmentStyle {
                color: icon_text_color, // #a0a0ff from Arc
                ..FragmentStyle::default()
            })
            .build();

        objects.push(Object::RichText(RichText {
            id: icon_texts[display_index],
            position: [icon_x + 8.0, icon_y + 6.0], // Centered in icon background
            lines: None,
        }));

        // Item title
        let title_color = if *actual_index == command_palette.selected_index {
            selection_text_color
        } else {
            text_primary
        };

        let item_title_line = content.sel(item_title_texts[display_index]);
        item_title_line
            .clear()
            .add_text(&item.title, FragmentStyle {
                color: title_color,
                ..FragmentStyle::default()
            })
            .build();

        objects.push(Object::RichText(RichText {
            id: item_title_texts[display_index],
            position: [icon_x + 45.0, item_y + 18.0], // margin-right: 15px from Arc
            lines: None,
        }));
    }

    // Add scrollbar if needed (matching Arc's scrollbar styling)
    if has_scrollbar {
        let scrollbar_x = panel_x + panel_width - 20.0; // 8px width + 12px margin
        let scrollbar_y = items_start_y;
        let scrollbar_height = actual_visible_items as f32 * item_height - 6.0; // Total visible area height
        
        // Scrollbar track (transparent background)
        objects.push(Object::Quad(Quad::solid(
            [scrollbar_x, scrollbar_y],
            [8.0, scrollbar_height],
            [0.0, 0.0, 0.0, 0.0], // Transparent track like Arc
        )));
        
        // Calculate scrollbar thumb position and size
        let max_scroll = total_items - actual_visible_items;
        let scroll_ratio = if max_scroll > 0 {
            command_palette.scroll_offset as f32 / max_scroll as f32
        } else {
            0.0
        };
        let thumb_height = (actual_visible_items as f32 / total_items as f32) * scrollbar_height;
        let thumb_y = scrollbar_y + (scroll_ratio * (scrollbar_height - thumb_height));
        
        // Scrollbar thumb (matching Arc's rgba(255, 255, 255, 0.15))
        objects.push(Object::Quad(Quad::solid(
            [scrollbar_x, thumb_y],
            [8.0, thumb_height],
            [1.0, 1.0, 1.0, 0.15], // rgba(255, 255, 255, 0.15) from Arc
        ).with_border(
            [0.0; 4],
            [4.0; 4], // border-radius: 10px equivalent
            0.0
        )));
    }

    // Instructions footer (matching example.html footer styling)
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
        position: [panel_x + 20.0, panel_y + panel_height - 32.0], // More space from bottom
        lines: None,
    }));

    sugarloaf.set_objects(objects);
}