// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! High-performance vertex memory pool for reducing allocation overhead
//!
//! This pool manages reusable vertex buffers to minimize GC pressure and improve
//! rendering performance by avoiding frequent allocations/deallocations.

use crate::components::rich_text::batch::Vertex;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tracing::debug;

/// Size categories for vertex buffers to minimize waste
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum BufferSizeCategory {
    Small,  // 1-64 vertices
    Medium, // 65-256 vertices
    Large,  // 257-1024 vertices
    XLarge, // 1025+ vertices
}

#[allow(dead_code)]
impl BufferSizeCategory {
    /// Get the appropriate category for a given vertex count
    fn for_vertex_count(count: usize) -> Self {
        match count {
            0..=64 => Self::Small,
            65..=256 => Self::Medium,
            257..=1024 => Self::Large,
            _ => Self::XLarge,
        }
    }

    /// Get the buffer capacity for this category
    fn capacity(self) -> usize {
        match self {
            Self::Small => 64,
            Self::Medium => 256,
            Self::Large => 1024,
            Self::XLarge => 4096,
        }
    }
}

/// A pooled vertex buffer that can be reused
#[allow(dead_code)]
pub struct PooledVertexBuffer {
    vertices: Vec<Vertex>,
    category: BufferSizeCategory,
    pool: Arc<Mutex<VertexPoolInner>>,
}

#[allow(dead_code)]
impl PooledVertexBuffer {
    /// Get a mutable reference to the vertex data
    pub fn vertices_mut(&mut self) -> &mut Vec<Vertex> {
        &mut self.vertices
    }

    /// Get an immutable reference to the vertex data
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// Get the capacity of the underlying vector
    pub fn capacity(&self) -> usize {
        self.vertices.capacity()
    }

    /// Clear the buffer and prepare for reuse
    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    /// Reserve capacity for additional vertices
    pub fn reserve(&mut self, additional: usize) {
        self.vertices.reserve(additional);
    }

    /// Get the current length of the buffer
    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Extend the buffer with vertices from a slice
    pub fn extend_from_slice(&mut self, vertices: &[Vertex]) {
        self.vertices.extend_from_slice(vertices);
    }

    /// Push a single vertex to the buffer
    pub fn push(&mut self, vertex: Vertex) {
        self.vertices.push(vertex);
    }
}

impl Drop for PooledVertexBuffer {
    fn drop(&mut self) {
        // Return the buffer to the pool when dropped
        if let Ok(mut pool) = self.pool.lock() {
            pool.return_buffer(std::mem::take(&mut self.vertices), self.category);
        }
    }
}

/// Internal vertex pool implementation
struct VertexPoolInner {
    /// Pools for different buffer sizes
    small_buffers: VecDeque<Vec<Vertex>>,
    medium_buffers: VecDeque<Vec<Vertex>>,
    large_buffers: VecDeque<Vec<Vertex>>,
    xlarge_buffers: VecDeque<Vec<Vertex>>,

    /// Pool statistics
    total_allocations: u64,
    pool_hits: u64,
    pool_misses: u64,

    /// Pool size limits to prevent unbounded growth
    max_small_buffers: usize,
    max_medium_buffers: usize,
    max_large_buffers: usize,
    max_xlarge_buffers: usize,
}

#[allow(dead_code)]
impl VertexPoolInner {
    fn new() -> Self {
        Self {
            small_buffers: VecDeque::new(),
            medium_buffers: VecDeque::new(),
            large_buffers: VecDeque::new(),
            xlarge_buffers: VecDeque::new(),
            total_allocations: 0,
            pool_hits: 0,
            pool_misses: 0,
            max_small_buffers: 32,  // Keep up to 32 small buffers
            max_medium_buffers: 16, // Keep up to 16 medium buffers
            max_large_buffers: 8,   // Keep up to 8 large buffers
            max_xlarge_buffers: 4,  // Keep up to 4 xlarge buffers
        }
    }

    /// Get a buffer from the appropriate pool
    fn get_buffer(&mut self, category: BufferSizeCategory) -> Vec<Vertex> {
        self.total_allocations += 1;

        let pool = match category {
            BufferSizeCategory::Small => &mut self.small_buffers,
            BufferSizeCategory::Medium => &mut self.medium_buffers,
            BufferSizeCategory::Large => &mut self.large_buffers,
            BufferSizeCategory::XLarge => &mut self.xlarge_buffers,
        };

        if let Some(mut buffer) = pool.pop_front() {
            buffer.clear(); // Clear but keep capacity
            self.pool_hits += 1;
            buffer
        } else {
            // Create new buffer with appropriate capacity
            self.pool_misses += 1;
            Vec::with_capacity(category.capacity())
        }
    }

    /// Return a buffer to the appropriate pool
    fn return_buffer(&mut self, mut buffer: Vec<Vertex>, category: BufferSizeCategory) {
        // Only return buffers that aren't too large (prevent memory bloat)
        if buffer.capacity() > category.capacity() * 2 {
            return; // Let it be deallocated
        }

        buffer.clear(); // Clear but keep capacity

        let (pool, max_size) = match category {
            BufferSizeCategory::Small => {
                (&mut self.small_buffers, self.max_small_buffers)
            }
            BufferSizeCategory::Medium => {
                (&mut self.medium_buffers, self.max_medium_buffers)
            }
            BufferSizeCategory::Large => {
                (&mut self.large_buffers, self.max_large_buffers)
            }
            BufferSizeCategory::XLarge => {
                (&mut self.xlarge_buffers, self.max_xlarge_buffers)
            }
        };

        if pool.len() < max_size {
            pool.push_back(buffer);
        }
        // If pool is full, let the buffer be deallocated
    }

    /// Get pool statistics
    fn stats(&self) -> VertexPoolStats {
        let hit_rate = if self.total_allocations > 0 {
            (self.pool_hits as f32 / self.total_allocations as f32) * 100.0
        } else {
            0.0
        };

        VertexPoolStats {
            total_allocations: self.total_allocations,
            pool_hits: self.pool_hits,
            pool_misses: self.pool_misses,
            hit_rate,
            small_buffers_available: self.small_buffers.len(),
            medium_buffers_available: self.medium_buffers.len(),
            large_buffers_available: self.large_buffers.len(),
            xlarge_buffers_available: self.xlarge_buffers.len(),
        }
    }

    /// Clear all pools (for cleanup)
    fn clear_all(&mut self) {
        self.small_buffers.clear();
        self.medium_buffers.clear();
        self.large_buffers.clear();
        self.xlarge_buffers.clear();
    }
}

/// High-performance vertex buffer pool
pub struct VertexPool {
    inner: Arc<Mutex<VertexPoolInner>>,
}

#[allow(dead_code)]
impl VertexPool {
    /// Create a new vertex pool
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VertexPoolInner::new())),
        }
    }

    /// Get a pooled vertex buffer for the given vertex count
    pub fn get_buffer(&self, expected_vertex_count: usize) -> PooledVertexBuffer {
        let category = BufferSizeCategory::for_vertex_count(expected_vertex_count);

        let vertices = if let Ok(mut pool) = self.inner.lock() {
            pool.get_buffer(category)
        } else {
            // Fallback if lock fails
            Vec::with_capacity(category.capacity())
        };

        PooledVertexBuffer {
            vertices,
            category,
            pool: Arc::clone(&self.inner),
        }
    }

    /// Get pool statistics
    pub fn stats(&self) -> Option<VertexPoolStats> {
        self.inner.lock().ok().map(|pool| pool.stats())
    }

    /// Clear all pooled buffers (for cleanup)
    pub fn clear(&self) {
        if let Ok(mut pool) = self.inner.lock() {
            pool.clear_all();
        }
    }

    /// Log statistics if enough allocations have occurred
    pub fn maybe_log_stats(&self) {
        if let Some(stats) = self.stats() {
            if stats.total_allocations % 1000 == 0 && stats.total_allocations > 0 {
                debug!(
                    "VertexPool stats: {:.1}% hit rate ({} allocs), Available: S:{} M:{} L:{} XL:{}",
                    stats.hit_rate,
                    stats.total_allocations,
                    stats.small_buffers_available,
                    stats.medium_buffers_available,
                    stats.large_buffers_available,
                    stats.xlarge_buffers_available
                );
            }
        }
    }
}

impl Default for VertexPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for VertexPool {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Statistics for vertex pool performance monitoring
#[derive(Debug, Clone)]
pub struct VertexPoolStats {
    pub total_allocations: u64,
    pub pool_hits: u64,
    pub pool_misses: u64,
    pub hit_rate: f32,
    pub small_buffers_available: usize,
    pub medium_buffers_available: usize,
    pub large_buffers_available: usize,
    pub xlarge_buffers_available: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_size_categories() {
        assert_eq!(
            BufferSizeCategory::for_vertex_count(10),
            BufferSizeCategory::Small
        );
        assert_eq!(
            BufferSizeCategory::for_vertex_count(64),
            BufferSizeCategory::Small
        );
        assert_eq!(
            BufferSizeCategory::for_vertex_count(65),
            BufferSizeCategory::Medium
        );
        assert_eq!(
            BufferSizeCategory::for_vertex_count(256),
            BufferSizeCategory::Medium
        );
        assert_eq!(
            BufferSizeCategory::for_vertex_count(257),
            BufferSizeCategory::Large
        );
        assert_eq!(
            BufferSizeCategory::for_vertex_count(1024),
            BufferSizeCategory::Large
        );
        assert_eq!(
            BufferSizeCategory::for_vertex_count(1025),
            BufferSizeCategory::XLarge
        );
        assert_eq!(
            BufferSizeCategory::for_vertex_count(5000),
            BufferSizeCategory::XLarge
        );
    }

    #[test]
    fn test_vertex_pool_basic_functionality() {
        let pool = VertexPool::new();

        // Get a buffer
        let mut buffer = pool.get_buffer(100);
        assert!(buffer.is_empty());
        assert!(buffer.capacity() >= 100);

        // Use the buffer
        let vertex = Vertex {
            pos: [1.0, 2.0, 3.0],
            color: [1.0, 1.0, 1.0, 1.0],
            uv: [0.0, 0.0],
            layers: [0, 0],
        };
        buffer.push(vertex);
        assert_eq!(buffer.len(), 1);

        // Buffer should be returned to pool when dropped
        drop(buffer);

        // Get another buffer - should reuse the previous one
        let buffer2 = pool.get_buffer(100);
        assert!(buffer2.is_empty()); // Should be cleared

        // Check stats
        if let Some(stats) = pool.stats() {
            assert_eq!(stats.total_allocations, 2);
            assert_eq!(stats.pool_hits, 1); // Second allocation was a hit
            assert_eq!(stats.pool_misses, 1); // First allocation was a miss
        }
    }

    #[test]
    fn test_vertex_pool_size_categories() {
        let pool = VertexPool::new();

        // Test different size categories
        let small_buffer = pool.get_buffer(10);
        let medium_buffer = pool.get_buffer(100);
        let large_buffer = pool.get_buffer(500);
        let xlarge_buffer = pool.get_buffer(2000);

        // Verify capacities are appropriate
        assert!(small_buffer.capacity() >= 10);
        assert!(medium_buffer.capacity() >= 100);
        assert!(large_buffer.capacity() >= 500);
        assert!(xlarge_buffer.capacity() >= 2000);

        // Different categories should have different capacities
        assert!(medium_buffer.capacity() > small_buffer.capacity());
        assert!(large_buffer.capacity() > medium_buffer.capacity());
        assert!(xlarge_buffer.capacity() > large_buffer.capacity());
    }

    #[test]
    fn test_vertex_pool_reuse() {
        let pool = VertexPool::new();

        // Get and return multiple buffers of the same size
        for i in 0..10 {
            let mut buffer = pool.get_buffer(50);

            // Add some vertices
            for j in 0..i {
                buffer.push(Vertex {
                    pos: [j as f32, 0.0, 0.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv: [0.0, 0.0],
                    layers: [0, 0],
                });
            }

            assert_eq!(buffer.len(), i);
            // Buffer is returned to pool when dropped
        }

        // Check that we got good reuse
        if let Some(stats) = pool.stats() {
            assert_eq!(stats.total_allocations, 10);
            assert!(stats.pool_hits > 0); // Should have some hits from reuse
            assert!(stats.hit_rate > 0.0);
        }
    }

    #[test]
    fn test_vertex_pool_operations() {
        let pool = VertexPool::new();
        let mut buffer = pool.get_buffer(10);

        // Test basic operations
        assert!(buffer.is_empty());

        let vertex = Vertex {
            pos: [1.0, 2.0, 3.0],
            color: [0.5, 0.5, 0.5, 1.0],
            uv: [0.1, 0.2],
            layers: [1, 2],
        };

        buffer.push(vertex);
        assert_eq!(buffer.len(), 1);
        assert!(!buffer.is_empty());

        // Test extend_from_slice
        let more_vertices = vec![vertex; 5];
        buffer.extend_from_slice(&more_vertices);
        assert_eq!(buffer.len(), 6);

        // Test clear
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);

        // Test reserve
        buffer.reserve(100);
        assert!(buffer.capacity() >= 100);
    }
}
