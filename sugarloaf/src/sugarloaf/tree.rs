// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! # Render Tree
//! 
//! The render tree provides a more efficient and flexible API for managing rendering objects
//! compared to the previous `set_objects` approach. Instead of replacing the entire object list
//! each frame, you can add, remove, and update individual objects incrementally.
//! 
//! ## Basic Usage
//! 
//! ```rust
//! use sugarloaf::{RenderTree, QuadItem, RichText, RichTextLinesRange};
//! 
//! let mut tree = RenderTree::new();
//! 
//! // Add objects and get handles for later manipulation
//! let red_quad = tree.add_quad_with_params(10.0, 10.0, 100.0, 50.0, 1.0, [1.0, 0.0, 0.0, 1.0]);
//! let text = tree.add_rich_text(RichText {
//!     id: 1,
//!     position: [10.0, 70.0],
//!     lines: Some(RichTextLinesRange { start: 0, end: 5 }),
//! });
//! 
//! // Remove objects using handles
//! red_quad.remove(&mut tree);
//! 
//! // Update objects
//! let new_quad = QuadItem::new(15.0, 15.0, 110.0, 60.0, 2.0, [0.0, 1.0, 0.0, 1.0]);
//! text.update(&mut tree, Object::Quad(new_quad));
//! 
//! // Clear all objects
//! tree.clear();
//! ```
//! 
//! ## Integration with Sugarloaf
//! 
//! ```rust
//! use sugarloaf::{Sugarloaf, QuadItem, RichText};
//! 
//! // Instead of the old way:
//! // sugarloaf.set_objects(vec![Object::Quad(quad), Object::RichText(text)]);
//! 
//! // Use the new render tree API:
//! let quad_handle = sugarloaf.add_quad(10.0, 10.0, 100.0, 50.0, 1.0, [1.0, 0.0, 0.0, 1.0]);
//! let text_handle = sugarloaf.add_rich_text(RichText { id: 1, position: [10.0, 70.0], lines: None });
//! 
//! // Later, remove or update specific objects:
//! quad_handle.remove(sugarloaf.render_tree_mut());
//! ```
//! 
//! ## Benefits
//! 
//! - **Incremental updates**: Only change what's needed instead of rebuilding everything
//! - **Better performance**: Avoid unnecessary allocations and copies
//! - **Easier state management**: The render tree handles object lifecycle
//! - **More intuitive API**: Add/remove operations instead of managing vectors
//! - **Type safety**: Handles prevent use-after-free errors

use crate::sugarloaf::primitives::{Object, QuadItem, RichText};
use std::collections::HashMap;

/// Unique identifier for render tree nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u64);

impl NodeId {
    fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Handle returned when adding objects to the render tree
/// Can be used to remove or update the object later
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectHandle {
    id: NodeId,
}

impl ObjectHandle {
    /// Remove this object from the render tree
    pub fn remove(self, tree: &mut RenderTree) {
        tree.remove_object(self.id);
    }

    /// Update this object in the render tree
    pub fn update(self, tree: &mut RenderTree, object: Object) {
        tree.update_object(self.id, object);
    }

    /// Get the node ID for this handle
    pub fn id(&self) -> NodeId {
        self.id
    }
}

/// A render tree that manages objects for rendering
/// Provides incremental updates instead of replacing the entire object list
#[derive(Debug)]
pub struct RenderTree {
    objects: HashMap<NodeId, Object>,
    next_id: u64,
    dirty: bool,
}

impl RenderTree {
    /// Create a new empty render tree
    pub fn new() -> Self {
        Self {
            objects: HashMap::new(),
            next_id: 1,
            dirty: false,
        }
    }

    /// Add a rich text object to the render tree
    /// Returns a handle that can be used to remove or update the object
    pub fn add_rich_text(&mut self, rich_text: RichText) -> ObjectHandle {
        let id = NodeId::new(self.next_id);
        self.next_id += 1;
        
        self.objects.insert(id, Object::RichText(rich_text));
        self.dirty = true;
        
        ObjectHandle { id }
    }

    /// Add a quad object to the render tree
    /// Returns a handle that can be used to remove or update the object
    pub fn add_quad(&mut self, quad: QuadItem) -> ObjectHandle {
        let id = NodeId::new(self.next_id);
        self.next_id += 1;
        
        self.objects.insert(id, Object::Quad(quad));
        self.dirty = true;
        
        ObjectHandle { id }
    }

    /// Add a quad with individual parameters (convenience method)
    pub fn add_quad_with_params(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        depth: f32,
        color: [f32; 4],
    ) -> ObjectHandle {
        let quad = QuadItem::new(x, y, width, height, depth, color);
        self.add_quad(quad)
    }

    /// Remove an object from the render tree
    pub fn remove_object(&mut self, id: NodeId) -> bool {
        let removed = self.objects.remove(&id).is_some();
        if removed {
            self.dirty = true;
        }
        removed
    }

    /// Update an existing object in the render tree
    pub fn update_object(&mut self, id: NodeId, object: Object) -> bool {
        if self.objects.contains_key(&id) {
            self.objects.insert(id, object);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Get an object by its ID
    pub fn get_object(&self, id: NodeId) -> Option<&Object> {
        self.objects.get(&id)
    }

    /// Get a mutable reference to an object by its ID
    pub fn get_object_mut(&mut self, id: NodeId) -> Option<&mut Object> {
        if self.objects.contains_key(&id) {
            self.dirty = true;
        }
        self.objects.get_mut(&id)
    }

    /// Check if the tree contains an object with the given ID
    pub fn contains(&self, id: NodeId) -> bool {
        self.objects.contains_key(&id)
    }

    /// Get the number of objects in the tree
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Check if the tree is empty
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Clear all objects from the tree
    pub fn clear(&mut self) {
        if !self.objects.is_empty() {
            self.objects.clear();
            self.dirty = true;
        }
    }

    /// Check if the tree has been modified since the last call to mark_clean()
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the tree as clean (typically called after rendering)
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Get all objects as a vector (for compatibility with existing code)
    /// This creates a new vector each time, so use sparingly
    pub fn get_objects(&self) -> Vec<Object> {
        self.objects.values().cloned().collect()
    }

    /// Get all rich text objects
    pub fn get_rich_texts(&self) -> Vec<RichText> {
        self.objects
            .values()
            .filter_map(|obj| match obj {
                Object::RichText(rt) => Some(*rt),
                _ => None,
            })
            .collect()
    }

    /// Get all quad objects
    pub fn get_quads(&self) -> Vec<QuadItem> {
        self.objects
            .values()
            .filter_map(|obj| match obj {
                Object::Quad(quad) => Some(*quad),
                _ => None,
            })
            .collect()
    }

    /// Iterate over all objects
    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &Object)> {
        self.objects.iter()
    }

    /// Iterate over all objects mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&NodeId, &mut Object)> {
        self.dirty = true;
        self.objects.iter_mut()
    }

    /// Get objects by type
    pub fn get_objects_by_type<F, T>(&self, filter: F) -> Vec<T>
    where
        F: Fn(&Object) -> Option<T>,
    {
        self.objects.values().filter_map(filter).collect()
    }

    /// Remove objects by predicate
    pub fn remove_objects_where<F>(&mut self, predicate: F) -> usize
    where
        F: Fn(&Object) -> bool,
    {
        let to_remove: Vec<NodeId> = self
            .objects
            .iter()
            .filter(|(_, obj)| predicate(obj))
            .map(|(id, _)| *id)
            .collect();

        let count = to_remove.len();
        for id in to_remove {
            self.objects.remove(&id);
        }

        if count > 0 {
            self.dirty = true;
        }

        count
    }

    /// Update objects by predicate
    pub fn update_objects_where<F>(&mut self, mut updater: F) -> usize
    where
        F: FnMut(&mut Object) -> bool,
    {
        let mut count = 0;
        for obj in self.objects.values_mut() {
            if updater(obj) {
                count += 1;
            }
        }

        if count > 0 {
            self.dirty = true;
        }

        count
    }
}

impl Default for RenderTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sugarloaf::primitives::RichTextLinesRange;

    #[test]
    fn test_render_tree_creation() {
        let tree = RenderTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert!(!tree.is_dirty());
    }

    #[test]
    fn test_add_quad() {
        let mut tree = RenderTree::new();
        let quad = QuadItem::new(10.0, 20.0, 100.0, 50.0, 1.0, [1.0, 0.0, 0.0, 1.0]);
        
        let handle = tree.add_quad(quad);
        
        assert_eq!(tree.len(), 1);
        assert!(tree.is_dirty());
        assert!(tree.contains(handle.id()));
        
        let retrieved = tree.get_object(handle.id()).unwrap();
        match retrieved {
            Object::Quad(q) => assert_eq!(*q, quad),
            _ => panic!("Expected quad object"),
        }
    }

    #[test]
    fn test_add_quad_with_params() {
        let mut tree = RenderTree::new();
        
        let handle = tree.add_quad_with_params(5.0, 10.0, 200.0, 100.0, 2.0, [0.0, 1.0, 0.0, 1.0]);
        
        assert_eq!(tree.len(), 1);
        assert!(tree.is_dirty());
        
        let retrieved = tree.get_object(handle.id()).unwrap();
        match retrieved {
            Object::Quad(q) => {
                assert_eq!(q.x, 5.0);
                assert_eq!(q.y, 10.0);
                assert_eq!(q.width, 200.0);
                assert_eq!(q.height, 100.0);
                assert_eq!(q.depth, 2.0);
                assert_eq!(q.color, [0.0, 1.0, 0.0, 1.0]);
            }
            _ => panic!("Expected quad object"),
        }
    }

    #[test]
    fn test_add_rich_text() {
        let mut tree = RenderTree::new();
        let rich_text = RichText {
            id: 42,
            position: [100.0, 200.0],
            lines: Some(RichTextLinesRange { start: 0, end: 10 }),
        };
        
        let handle = tree.add_rich_text(rich_text);
        
        assert_eq!(tree.len(), 1);
        assert!(tree.is_dirty());
        assert!(tree.contains(handle.id()));
        
        let retrieved = tree.get_object(handle.id()).unwrap();
        match retrieved {
            Object::RichText(rt) => assert_eq!(*rt, rich_text),
            _ => panic!("Expected rich text object"),
        }
    }

    #[test]
    fn test_remove_object() {
        let mut tree = RenderTree::new();
        let quad = QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 1.0, 1.0, 1.0]);
        let handle = tree.add_quad(quad);
        
        tree.mark_clean();
        assert!(!tree.is_dirty());
        
        let removed = tree.remove_object(handle.id());
        assert!(removed);
        assert!(tree.is_empty());
        assert!(tree.is_dirty());
        
        // Try to remove again
        let removed_again = tree.remove_object(handle.id());
        assert!(!removed_again);
    }

    #[test]
    fn test_handle_remove() {
        let mut tree = RenderTree::new();
        let quad = QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 1.0, 1.0, 1.0]);
        let handle = tree.add_quad(quad);
        
        assert_eq!(tree.len(), 1);
        
        handle.remove(&mut tree);
        
        assert!(tree.is_empty());
        assert!(!tree.contains(handle.id()));
    }

    #[test]
    fn test_update_object() {
        let mut tree = RenderTree::new();
        let quad1 = QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 0.0, 0.0, 1.0]);
        let handle = tree.add_quad(quad1);
        
        tree.mark_clean();
        
        let quad2 = QuadItem::new(5.0, 5.0, 20.0, 20.0, 1.0, [0.0, 1.0, 0.0, 1.0]);
        let updated = tree.update_object(handle.id(), Object::Quad(quad2));
        
        assert!(updated);
        assert!(tree.is_dirty());
        
        let retrieved = tree.get_object(handle.id()).unwrap();
        match retrieved {
            Object::Quad(q) => assert_eq!(*q, quad2),
            _ => panic!("Expected quad object"),
        }
    }

    #[test]
    fn test_handle_update() {
        let mut tree = RenderTree::new();
        let quad1 = QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 0.0, 0.0, 1.0]);
        let handle = tree.add_quad(quad1);
        
        let quad2 = QuadItem::new(5.0, 5.0, 20.0, 20.0, 1.0, [0.0, 1.0, 0.0, 1.0]);
        handle.update(&mut tree, Object::Quad(quad2));
        
        let retrieved = tree.get_object(handle.id()).unwrap();
        match retrieved {
            Object::Quad(q) => assert_eq!(*q, quad2),
            _ => panic!("Expected quad object"),
        }
    }

    #[test]
    fn test_clear() {
        let mut tree = RenderTree::new();
        
        // Add multiple objects
        tree.add_quad(QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 0.0, 0.0, 1.0]));
        tree.add_quad(QuadItem::new(10.0, 10.0, 20.0, 20.0, 1.0, [0.0, 1.0, 0.0, 1.0]));
        tree.add_rich_text(RichText {
            id: 1,
            position: [0.0, 0.0],
            lines: None,
        });
        
        assert_eq!(tree.len(), 3);
        
        tree.mark_clean();
        tree.clear();
        
        assert!(tree.is_empty());
        assert!(tree.is_dirty());
    }

    #[test]
    fn test_get_objects_by_type() {
        let mut tree = RenderTree::new();
        
        // Add mixed objects
        tree.add_quad(QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 0.0, 0.0, 1.0]));
        tree.add_rich_text(RichText {
            id: 1,
            position: [0.0, 0.0],
            lines: None,
        });
        tree.add_quad(QuadItem::new(10.0, 10.0, 20.0, 20.0, 1.0, [0.0, 1.0, 0.0, 1.0]));
        
        let quads = tree.get_quads();
        let rich_texts = tree.get_rich_texts();
        
        assert_eq!(quads.len(), 2);
        assert_eq!(rich_texts.len(), 1);
    }

    #[test]
    fn test_remove_objects_where() {
        let mut tree = RenderTree::new();
        
        // Add objects with different colors
        tree.add_quad(QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 0.0, 0.0, 1.0])); // Red
        tree.add_quad(QuadItem::new(10.0, 10.0, 20.0, 20.0, 1.0, [0.0, 1.0, 0.0, 1.0])); // Green
        tree.add_quad(QuadItem::new(20.0, 20.0, 30.0, 30.0, 2.0, [1.0, 0.0, 0.0, 1.0])); // Red
        
        assert_eq!(tree.len(), 3);
        
        // Remove all red quads
        let removed = tree.remove_objects_where(|obj| {
            match obj {
                Object::Quad(quad) => quad.color == [1.0, 0.0, 0.0, 1.0],
                _ => false,
            }
        });
        
        assert_eq!(removed, 2);
        assert_eq!(tree.len(), 1);
        assert!(tree.is_dirty());
    }

    #[test]
    fn test_update_objects_where() {
        let mut tree = RenderTree::new();
        
        // Add objects
        tree.add_quad(QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 0.0, 0.0, 1.0]));
        tree.add_quad(QuadItem::new(10.0, 10.0, 20.0, 20.0, 1.0, [0.0, 1.0, 0.0, 1.0]));
        
        tree.mark_clean();
        
        // Update all quads to have depth 5.0
        let updated = tree.update_objects_where(|obj| {
            match obj {
                Object::Quad(quad) => {
                    quad.depth = 5.0;
                    true
                }
                _ => false,
            }
        });
        
        assert_eq!(updated, 2);
        assert!(tree.is_dirty());
        
        // Verify all quads have depth 5.0
        let quads = tree.get_quads();
        for quad in quads {
            assert_eq!(quad.depth, 5.0);
        }
    }

    #[test]
    fn test_render_tree_api_example() {
        let mut tree = RenderTree::new();
        
        // Add some objects using the new API
        let red_quad = tree.add_quad_with_params(10.0, 10.0, 100.0, 50.0, 1.0, [1.0, 0.0, 0.0, 1.0]);
        let green_quad = tree.add_quad_with_params(120.0, 10.0, 100.0, 50.0, 1.0, [0.0, 1.0, 0.0, 1.0]);
        
        let _rich_text = tree.add_rich_text(RichText {
            id: 1,
            position: [10.0, 70.0],
            lines: Some(RichTextLinesRange { start: 0, end: 5 }),
        });
        
        // Verify objects were added
        assert_eq!(tree.len(), 3);
        assert!(tree.is_dirty());
        
        // Get all quads
        let quads = tree.get_quads();
        assert_eq!(quads.len(), 2);
        
        // Get all rich texts
        let rich_texts = tree.get_rich_texts();
        assert_eq!(rich_texts.len(), 1);
        
        // Remove the green quad
        green_quad.remove(&mut tree);
        assert_eq!(tree.len(), 2);
        
        // Update the red quad
        let new_quad = QuadItem::new(15.0, 15.0, 110.0, 60.0, 2.0, [0.5, 0.0, 0.0, 1.0]);
        red_quad.update(&mut tree, Object::Quad(new_quad));
        
        // Verify the update
        let updated_quad = tree.get_object(red_quad.id()).unwrap();
        match updated_quad {
            Object::Quad(q) => {
                assert_eq!(q.x, 15.0);
                assert_eq!(q.y, 15.0);
                assert_eq!(q.color, [0.5, 0.0, 0.0, 1.0]);
            }
            _ => panic!("Expected quad object"),
        }
        
        // Clear all objects
        tree.clear();
        assert!(tree.is_empty());
        assert!(tree.is_dirty());
    }

    #[test]
    fn test_dirty_flag() {
        let mut tree = RenderTree::new();
        
        // Initially not dirty
        assert!(!tree.is_dirty());
        
        // Adding makes it dirty
        tree.add_quad(QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 0.0, 0.0, 1.0]));
        assert!(tree.is_dirty());
        
        // Mark clean
        tree.mark_clean();
        assert!(!tree.is_dirty());
        
        // Getting mutable reference makes it dirty
        let handle = tree.add_quad(QuadItem::new(10.0, 10.0, 20.0, 20.0, 1.0, [0.0, 1.0, 0.0, 1.0]));
        tree.mark_clean();
        
        tree.get_object_mut(handle.id());
        assert!(tree.is_dirty());
    }

    #[test]
    fn test_unique_ids() {
        let mut tree = RenderTree::new();
        
        let handle1 = tree.add_quad(QuadItem::new(0.0, 0.0, 10.0, 10.0, 0.0, [1.0, 0.0, 0.0, 1.0]));
        let handle2 = tree.add_quad(QuadItem::new(10.0, 10.0, 20.0, 20.0, 1.0, [0.0, 1.0, 0.0, 1.0]));
        
        assert_ne!(handle1.id(), handle2.id());
    }
}