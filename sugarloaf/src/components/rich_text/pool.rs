// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Advanced object pooling system for rich text rendering performance

use crate::components::rich_text::compositor::BatchOperation;
use crate::components::rich_text::text::Glyph;
use std::collections::VecDeque;

/// Object pool for reusing vectors to reduce allocations
pub struct VectorPool<T> {
    pool: VecDeque<Vec<T>>,
    max_pool_size: usize,
    initial_capacity: usize,
}

impl<T> VectorPool<T> {
    pub fn new(max_pool_size: usize, initial_capacity: usize) -> Self {
        let mut pool = VecDeque::with_capacity(max_pool_size);
        
        // Pre-populate pool with some vectors
        for _ in 0..(max_pool_size / 2) {
            pool.push_back(Vec::with_capacity(initial_capacity));
        }
        
        Self {
            pool,
            max_pool_size,
            initial_capacity,
        }
    }

    /// Get a vector from the pool, or create a new one if pool is empty
    pub fn get(&mut self) -> Vec<T> {
        self.pool.pop_front().unwrap_or_else(|| Vec::with_capacity(self.initial_capacity))
    }

    /// Return a vector to the pool for reuse
    pub fn return_vec(&mut self, mut vec: Vec<T>) {
        if self.pool.len() < self.max_pool_size {
            vec.clear(); // Clear but keep capacity
            self.pool.push_back(vec);
        }
        // If pool is full, just drop the vector
    }

    /// Get current pool size (for monitoring)
    pub fn pool_size(&self) -> usize {
        self.pool.len()
    }
}

/// Specialized pools for different vector types used in rich text rendering
pub struct RichTextPools {
    pub glyph_pool: VectorPool<Glyph>,
    pub operation_pool: VectorPool<BatchOperation>,
    pub vertex_indices_pool: VectorPool<u32>,
}

impl RichTextPools {
    pub fn new() -> Self {
        Self {
            glyph_pool: VectorPool::new(16, 256),        // Pool for glyph vectors
            operation_pool: VectorPool::new(16, 64),     // Pool for operation vectors  
            vertex_indices_pool: VectorPool::new(8, 128), // Pool for vertex index vectors
        }
    }

    /// Clear all pools (useful for memory cleanup)
    pub fn clear_all(&mut self) {
        self.glyph_pool.pool.clear();
        self.operation_pool.pool.clear();
        self.vertex_indices_pool.pool.clear();
    }

    /// Get total memory usage estimate (for monitoring)
    pub fn estimated_memory_usage(&self) -> usize {
        let glyph_mem = self.glyph_pool.pool.len() * self.glyph_pool.initial_capacity * std::mem::size_of::<Glyph>();
        let op_mem = self.operation_pool.pool.len() * self.operation_pool.initial_capacity * std::mem::size_of::<BatchOperation>();
        let idx_mem = self.vertex_indices_pool.pool.len() * self.vertex_indices_pool.initial_capacity * std::mem::size_of::<u32>();
        glyph_mem + op_mem + idx_mem
    }
}

impl Default for RichTextPools {
    fn default() -> Self {
        Self::new()
    }
}

/// Fast memory arena for temporary allocations during rendering
pub struct RenderArena {
    buffer: Vec<u8>,
    offset: usize,
}

impl RenderArena {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            offset: 0,
        }
    }

    /// Allocate space for T and return a mutable reference
    pub fn alloc<T>(&mut self) -> Option<&mut T> {
        let size = std::mem::size_of::<T>();
        let align = std::mem::align_of::<T>();
        
        // Align offset
        let aligned_offset = (self.offset + align - 1) & !(align - 1);
        let end_offset = aligned_offset + size;
        
        if end_offset > self.buffer.capacity() {
            return None; // Arena is full
        }
        
        // Ensure buffer is large enough
        if end_offset > self.buffer.len() {
            self.buffer.resize(end_offset, 0);
        }
        
        self.offset = end_offset;
        
        // Safety: We've ensured proper alignment and bounds
        unsafe {
            let ptr = self.buffer.as_mut_ptr().add(aligned_offset) as *mut T;
            Some(&mut *ptr)
        }
    }

    /// Reset arena for reuse (doesn't deallocate)
    pub fn reset(&mut self) {
        self.offset = 0;
        // Keep the buffer allocated for reuse
    }

    /// Get current usage
    pub fn used_bytes(&self) -> usize {
        self.offset
    }

    /// Get total capacity
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_pool() {
        let mut pool: VectorPool<i32> = VectorPool::new(4, 10);
        
        // Get a vector from pool
        let mut vec1 = pool.get();
        assert_eq!(vec1.capacity(), 10);
        
        // Use the vector
        vec1.push(1);
        vec1.push(2);
        assert_eq!(vec1.len(), 2);
        
        // Return to pool
        pool.return_vec(vec1);
        assert_eq!(pool.pool_size(), 3); // Started with 2, added 1
        
        // Get it back - should be cleared but keep capacity
        let vec2 = pool.get();
        assert_eq!(vec2.len(), 0);
        assert_eq!(vec2.capacity(), 10);
    }

    #[test]
    fn test_render_arena() {
        let mut arena = RenderArena::new(1024);
        
        // Allocate some integers
        let int1 = arena.alloc::<i32>().unwrap();
        *int1 = 42;
        
        let int2 = arena.alloc::<i32>().unwrap();
        *int2 = 84;
        
        assert_eq!(*int1, 42);
        assert_eq!(*int2, 84);
        
        // Reset and reuse
        arena.reset();
        let int3 = arena.alloc::<i32>().unwrap();
        *int3 = 168;
        assert_eq!(*int3, 168);
    }
}