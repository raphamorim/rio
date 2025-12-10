// Taffy-based layout management for split panels
//
// This module provides a flexible layout system using the Taffy layout engine.
// It replaces manual width/height calculations with a proper flexbox-based layout.
//
// Layout Structure:
// - Root container: Window/tab area with outer `padding` config
//   - Applies gap between split panels using `row-gap` and `column-gap`
// - Panel nodes: Individual terminal contexts
//   - Each panel has inner padding from `panel.padding` config
//   - Panel padding creates space INSIDE each panel (around terminal content)
//
// Configuration:
// - `padding`: Outer spacing around the entire layout [top, right, bottom, left]
// - `panel.padding`: Inner spacing inside each panel (around terminal content)
// - `panel.row-gap`: Vertical spacing between panels (when split vertically)
// - `panel.column-gap`: Horizontal spacing between panels (when split horizontally)
//
// Split Directions:
// - Horizontal (right): Flex row direction, uses column-gap
// - Vertical (down): Flex column direction, uses row-gap

use rustc_hash::FxHashMap;
use taffy::{AvailableSpace, Display, style_helpers::length, Style, NodeId, TaffyTree, TaffyError, Dimension, geometry};

/// Border configuration for split panels
#[derive(Debug, Clone, Copy)]
pub struct BorderConfig {
    pub width: f32,
    pub color: [f32; 4], // RGBA
    pub radius: f32,     // Corner radius (0.0 = sharp)
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            width: 2.0,
            color: [0.8, 0.8, 0.8, 1.0], // Light gray
            radius: 0.0,
        }
    }
}

/// Maps Taffy layout nodes to terminal context IDs
pub struct TaffyLayoutManager {
    /// The Taffy layout tree
    taffy: TaffyTree<()>,
    /// Root container node (has padding from config)
    root_node: NodeId,
    /// Maps Taffy NodeId to context route_id
    node_to_context: FxHashMap<NodeId, usize>,
    /// Maps context route_id to Taffy NodeId
    context_to_node: FxHashMap<usize, NodeId>,
    /// Current active context
    current_context: usize,
    /// Column gap (horizontal spacing between panels)
    column_gap: f32,
    /// Row gap (vertical spacing between panels)
    row_gap: f32,
    /// Border configuration
    border_config: BorderConfig,
}

impl TaffyLayoutManager {
    /// Create a new layout manager with initial context
    ///
    /// # Arguments
    /// * `initial_context_id` - The route_id of the initial terminal context
    /// * `width` - Initial window width
    /// * `height` - Initial window height
    /// * `padding` - Outer padding (top, right, bottom, left)
    /// * `column_gap` - Horizontal gap between panels
    /// * `row_gap` - Vertical gap between panels
    /// * `border_config` - Border configuration for split panels
    pub fn new(
        initial_context_id: usize,
        width: f32,
        height: f32,
        padding: [f32; 4], // [top, right, bottom, left]
        column_gap: f32,
        row_gap: f32,
        border_config: BorderConfig,
    ) -> Result<Self, TaffyError> {
        let mut taffy = TaffyTree::new();

        // Create root container with padding and gaps between children
        let root_style = Style {
            display: Display::Flex,
            padding: geometry::Rect {
                left: length(padding[3]),
                right: length(padding[1]),
                top: length(padding[0]),
                bottom: length(padding[2]),
            },
            gap: geometry::Size {
                width: length(column_gap),  // Horizontal gap
                height: length(row_gap),    // Vertical gap
            },
            size: geometry::Size {
                width: length(width),
                height: length(height),
            },
            ..Default::default()
        };

        let root_node = taffy.new_leaf(root_style)?;

        // Create initial panel node (fills container)
        let panel_style = Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Default::default()
        };

        let panel_node = taffy.new_leaf(panel_style)?;
        taffy.add_child(root_node, panel_node)?;

        let mut node_to_context = FxHashMap::default();
        let mut context_to_node = FxHashMap::default();
        node_to_context.insert(panel_node, initial_context_id);
        context_to_node.insert(initial_context_id, panel_node);

        Ok(Self {
            taffy,
            root_node,
            node_to_context,
            context_to_node,
            current_context: initial_context_id,
            column_gap,
            row_gap,
            border_config,
        })
    }

    /// Get the number of panels in the layout
    pub fn panel_count(&self) -> usize {
        self.node_to_context.len()
    }

    /// Check if borders should be drawn (only when 2+ panels)
    pub fn should_draw_borders(&self) -> bool {
        self.panel_count() > 1
    }

    /// Update the root container size (e.g., on window resize)
    pub fn update_size(&mut self, width: f32, height: f32) -> Result<(), TaffyError> {
        let mut style = self.taffy.style(self.root_node)?.clone();
        style.size = geometry::Size {
            width: length(width),
            height: length(height),
        };
        self.taffy.set_style(self.root_node, style)?;
        Ok(())
    }

    /// Compute the layout and return positions/sizes for all contexts
    ///
    /// Returns a map of context_id -> (x, y, width, height)
    pub fn compute_layout(&mut self) -> Result<FxHashMap<usize, (f32, f32, f32, f32)>, TaffyError> {
        // Compute layout - the root node size is already set, so we use MAX for available space
        let available = geometry::Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        };
        self.taffy.compute_layout(self.root_node, available)?;

        // Extract layout for all panel nodes
        let mut layouts = FxHashMap::default();
        for (&node, &context_id) in &self.node_to_context {
            let layout = self.taffy.layout(node)?;
            layouts.insert(
                context_id,
                (
                    layout.location.x,
                    layout.location.y,
                    layout.size.width,
                    layout.size.height,
                ),
            );
        }

        Ok(layouts)
    }

    /// Get the current active context
    pub fn current_context(&self) -> usize {
        self.current_context
    }

    /// Set the current active context
    pub fn set_current_context(&mut self, context_id: usize) {
        if self.context_to_node.contains_key(&context_id) {
            self.current_context = context_id;
        }
    }

    /// Get the layout for a specific context
    pub fn get_context_layout(&self, context_id: usize) -> Option<(f32, f32, f32, f32)> {
        let node = self.context_to_node.get(&context_id)?;
        let layout = self.taffy.layout(*node).ok()?;
        Some((
            layout.location.x,
            layout.location.y,
            layout.size.width,
            layout.size.height,
        ))
    }

    /// Find which context contains the given point (for mouse interaction)
    pub fn find_context_at_position(&self, x: f32, y: f32) -> Option<usize> {
        for (&node, &context_id) in &self.node_to_context {
            if let Ok(layout) = self.taffy.layout(node) {
                let left = layout.location.x;
                let top = layout.location.y;
                let right = left + layout.size.width;
                let bottom = top + layout.size.height;

                if x >= left && x < right && y >= top && y < bottom {
                    return Some(context_id);
                }
            }
        }
        None
    }

    /// Create border rectangles for all panels (only if 2+ panels exist)
    /// Returns renderable border objects for sugarloaf
    pub fn create_panel_borders(&self) -> Vec<rio_backend::sugarloaf::Object> {
        use rio_backend::sugarloaf::{Object, Rect};
        
        if !self.should_draw_borders() {
            return vec![];
        }

        let mut borders = Vec::new();
        let border_width = self.border_config.width;
        let color = self.border_config.color;

        for &node in self.node_to_context.keys() {
            if let Ok(layout) = self.taffy.layout(node) {
                let x = layout.location.x;
                let y = layout.location.y;
                let width = layout.size.width;
                let height = layout.size.height;

                // Top border
                borders.push(Object::Rect(Rect::new(x, y, width, border_width, color)));
                
                // Right border
                borders.push(Object::Rect(Rect::new(
                    x + width - border_width,
                    y,
                    border_width,
                    height,
                    color,
                )));
                
                // Bottom border
                borders.push(Object::Rect(Rect::new(
                    x,
                    y + height - border_width,
                    width,
                    border_width,
                    color,
                )));
                
                // Left border
                borders.push(Object::Rect(Rect::new(x, y, border_width, height, color)));
            }
        }

        borders
    }

    /// Split the current panel horizontally (right)
    /// Sets root to flex row direction
    pub fn split_right(&mut self, new_context_id: usize) -> Result<(), TaffyError> {
        // Set root to row direction (horizontal split)
        let mut root_style = self.taffy.style(self.root_node)?.clone();
        root_style.flex_direction = taffy::FlexDirection::Row;
        self.taffy.set_style(self.root_node, root_style)?;
        
        // Create new panel node
        let new_panel_style = Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Default::default()
        };
        let new_node = self.taffy.new_leaf(new_panel_style)?;
        
        // Add to root container (gap will handle spacing)
        self.taffy.add_child(self.root_node, new_node)?;
        
        // Update mappings
        self.node_to_context.insert(new_node, new_context_id);
        self.context_to_node.insert(new_context_id, new_node);
        self.current_context = new_context_id;
        
        Ok(())
    }

    /// Split the current panel vertically (down)
    /// Sets root to flex column direction
    pub fn split_down(&mut self, new_context_id: usize) -> Result<(), TaffyError> {
        // Set root to column direction (vertical split)
        let mut root_style = self.taffy.style(self.root_node)?.clone();
        root_style.flex_direction = taffy::FlexDirection::Column;
        self.taffy.set_style(self.root_node, root_style)?;
        
        // Create new panel node
        let new_panel_style = Style {
            display: Display::Flex,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Default::default()
        };
        let new_node = self.taffy.new_leaf(new_panel_style)?;
        
        // Add to root container (gap will handle spacing)
        self.taffy.add_child(self.root_node, new_node)?;
        
        // Update mappings
        self.node_to_context.insert(new_node, new_context_id);
        self.context_to_node.insert(new_context_id, new_node);
        self.current_context = new_context_id;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_layout_manager() {
        let manager = TaffyLayoutManager::new(
            1,
            800.0,
            600.0,
            [10.0, 10.0, 10.0, 10.0],
            5.0,
            BorderConfig::default(),
        );
        assert!(manager.is_ok());
        let manager = manager.unwrap();
        assert_eq!(manager.current_context(), 1);
    }

    #[test]
    fn test_compute_initial_layout() {
        let mut manager = TaffyLayoutManager::new(
            1,
            800.0,
            600.0,
            [10.0, 10.0, 10.0, 10.0],
            5.0,
            BorderConfig::default(),
        )
        .unwrap();
        let layouts = manager.compute_layout().unwrap();
        assert_eq!(layouts.len(), 1);
        
        let (x, y, width, height) = layouts[&1];
        // Should account for padding (10px on each side)
        assert_eq!(x, 10.0);
        assert_eq!(y, 10.0);
        assert_eq!(width, 780.0); // 800 - 10 - 10
        assert_eq!(height, 580.0); // 600 - 10 - 10
    }

    #[test]
    fn test_update_size() {
        let mut manager = TaffyLayoutManager::new(
            1,
            800.0,
            600.0,
            [10.0, 10.0, 10.0, 10.0],
            5.0,
            BorderConfig::default(),
        )
        .unwrap();
        assert!(manager.update_size(1024.0, 768.0).is_ok());
        
        let layouts = manager.compute_layout().unwrap();
        let (_, _, width, height) = layouts[&1];
        assert_eq!(width, 1004.0); // 1024 - 10 - 10
        assert_eq!(height, 748.0); // 768 - 10 - 10
    }

    #[test]
    fn test_find_context_at_position() {
        let mut manager = TaffyLayoutManager::new(
            1,
            800.0,
            600.0,
            [10.0, 10.0, 10.0, 10.0],
            5.0,
            BorderConfig::default(),
        )
        .unwrap();
        manager.compute_layout().unwrap();

        // Inside the panel (accounting for padding)
        assert_eq!(manager.find_context_at_position(100.0, 100.0), Some(1));
        
        // Outside the panel (in padding area)
        assert_eq!(manager.find_context_at_position(5.0, 5.0), None);
        
        // Outside the window
        assert_eq!(manager.find_context_at_position(900.0, 700.0), None);
    }
}
