use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText, Sugarloaf, vertical_gradient};

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
    let background_color = [0.0, 0.0, 0.0, 0.75]; // Semi-transparent black overlay
    let border_color = [0.3, 0.3, 0.3, 0.9]; // Border color with slight opacity
    let text_color = [1.0, 1.0, 1.0, 0.95]; // White text with slight transparency
    let description_color = [0.7, 0.7, 0.7, 0.8]; // Gray description text with opacity
    let query_color = [0.9, 0.9, 0.9, 0.9]; // Light gray for query with opacity

    let layout = sugarloaf.window_size();
    let panel_width = 600.0;
    let panel_height = 400.0;
    
    // Account for scaling factor like other dialogs do
    let scaled_width = layout.width / context_dimension.dimension.scale;
    let panel_x = (scaled_width - panel_width) / 2.0;
    let panel_y = context_dimension.margin.top_y + 50.0;

    let mut objects = Vec::new();

    // Background overlay
    objects.push(Object::Quad(Quad::solid(
        [0.0, 0.0],
        [scaled_width, layout.height],
        background_color,
    )));

    // Panel border (drawn first, behind the main panel)
    objects.push(Object::Quad(Quad::solid(
        [panel_x - 2.0, panel_y - 2.0],
        [panel_width + 4.0, panel_height + 4.0],
        border_color,
    )));

    // Main panel with gradient background
    let gradient = vertical_gradient(
        panel_x, panel_y, panel_width, panel_height,
        [0.08, 0.08, 0.12, 0.95], // Darker blue-gray at top
        [0.05, 0.05, 0.08, 0.95], // Even darker at bottom
    );
    
    objects.push(Object::Quad(Quad::gradient(
        gradient,
        [panel_x, panel_y],
        [panel_width, panel_height],
    ).with_border(border_color, [8.0; 4], 1.0)));

    // Create all rich text objects first
    let title_text = sugarloaf.create_temp_rich_text();
    let query_text = sugarloaf.create_temp_rich_text();
    let instructions_text = sugarloaf.create_temp_rich_text();

    // Create rich text objects for items
    let max_visible_items = 8;
    let mut item_title_texts = Vec::new();
    let mut item_desc_texts = Vec::new();
    
    for _ in 0..max_visible_items.min(command_palette.filtered_items.len()) {
        item_title_texts.push(sugarloaf.create_temp_rich_text());
        item_desc_texts.push(sugarloaf.create_temp_rich_text());
    }

    // Set font sizes
    sugarloaf.set_rich_text_font_size(&title_text, 20.0);
    sugarloaf.set_rich_text_font_size(&query_text, 16.0);
    sugarloaf.set_rich_text_font_size(&instructions_text, 11.0);
    
    for title_text_id in &item_title_texts {
        sugarloaf.set_rich_text_font_size(title_text_id, 14.0);
    }
    
    for desc_text_id in &item_desc_texts {
        sugarloaf.set_rich_text_font_size(desc_text_id, 12.0);
    }

    // Now build the content
    let content = sugarloaf.content();

    // Title
    let title_line = content.sel(title_text);
    title_line
        .clear()
        .add_text("Command Palette", FragmentStyle {
            color: text_color,
            ..FragmentStyle::default()
        })
        .build();

    objects.push(Object::RichText(RichText {
        id: title_text,
        position: [panel_x + 20.0, panel_y + 30.0],
        lines: None,
    }));

    // Query input
    let query_line = content.sel(query_text);
    query_line
        .clear()
        .add_text(&format!("> {}", command_palette.query), FragmentStyle {
            color: query_color,
            ..FragmentStyle::default()
        })
        .build();

    objects.push(Object::RichText(RichText {
        id: query_text,
        position: [panel_x + 20.0, panel_y + 70.0],
        lines: None,
    }));

    // Command items
    let item_height = 35.0;
    let items_start_y = panel_y + 110.0;

    for (index, item) in command_palette.filtered_items.iter().enumerate().take(max_visible_items) {
        let item_y = items_start_y + (index as f32 * item_height);
        
        // Selection highlight with gradient
        if index == command_palette.selected_index {
            let selection_gradient = vertical_gradient(
                panel_x + 10.0, item_y - 5.0, panel_width - 20.0, item_height,
                [0.2, 0.4, 0.8, 0.5], // Brighter blue at top
                [0.1, 0.2, 0.6, 0.3], // Darker blue at bottom
            );
            
            objects.push(Object::Quad(Quad::gradient(
                selection_gradient,
                [panel_x + 10.0, item_y - 5.0],
                [panel_width - 20.0, item_height],
            ).with_border([0.3, 0.5, 0.9, 0.6], [4.0; 4], 0.0)));
        }

        // Item title
        let item_title_line = content.sel(item_title_texts[index]);
        item_title_line
            .clear()
            .add_text(&item.title, FragmentStyle {
                color: text_color,
                ..FragmentStyle::default()
            })
            .build();

        objects.push(Object::RichText(RichText {
            id: item_title_texts[index],
            position: [panel_x + 20.0, item_y],
            lines: None,
        }));

        // Item description
        let item_desc_line = content.sel(item_desc_texts[index]);
        item_desc_line
            .clear()
            .add_text(&item.description, FragmentStyle {
                color: description_color,
                ..FragmentStyle::default()
            })
            .build();

        objects.push(Object::RichText(RichText {
            id: item_desc_texts[index],
            position: [panel_x + 20.0, item_y + 18.0],
            lines: None,
        }));
    }

    // Instructions
    let instructions_line = content.sel(instructions_text);
    instructions_line
        .clear()
        .add_text("↑↓ navigate • Enter execute • Esc close", FragmentStyle {
            color: description_color,
            ..FragmentStyle::default()
        })
        .build();

    objects.push(Object::RichText(RichText {
        id: instructions_text,
        position: [panel_x + 20.0, panel_y + panel_height - 30.0],
        lines: None,
    }));

    sugarloaf.set_objects(objects);
}