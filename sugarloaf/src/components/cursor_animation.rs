// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Cursor animation system inspired by Neovide's smooth cursor movement.
//! Uses critically damped spring animations for natural-feeling cursor motion.

use std::time::Instant;

/// A critically damped spring animation for smooth value transitions.
/// This provides natural-looking animations without oscillation.
#[derive(Clone, Debug)]
pub struct CriticallyDampedSpringAnimation {
    pub position: f32,
    velocity: f32,
}

impl CriticallyDampedSpringAnimation {
    pub fn new() -> Self {
        Self {
            position: 0.0,
            velocity: 0.0,
        }
    }

    /// Update the animation with the given delta time and animation length.
    /// Returns true if the animation is still in progress, false if it has settled.
    pub fn update(&mut self, dt: f32, animation_length: f32) -> bool {
        if animation_length <= dt {
            // Animation should complete immediately
            self.position = 1.0;
            self.velocity = 0.0;
            return false;
        }

        // Critically damped spring parameters
        let omega = 2.0 / animation_length; // Natural frequency
        let zeta = 1.0; // Damping ratio (1.0 = critically damped)

        // Calculate spring forces
        let target = 1.0;
        let displacement = target - self.position;
        let spring_force = omega * omega * displacement;
        let damping_force = 2.0 * zeta * omega * self.velocity;

        // Update velocity and position
        let acceleration = spring_force - damping_force;
        self.velocity += acceleration * dt;
        self.position += self.velocity * dt;

        // Check if animation has settled (close enough to target with low velocity)
        let position_threshold = 0.001;
        let velocity_threshold = 0.01;
        
        if (self.position - target).abs() < position_threshold && self.velocity.abs() < velocity_threshold {
            self.position = target;
            self.velocity = 0.0;
            false
        } else {
            true
        }
    }

    /// Reset the animation to start position
    pub fn reset(&mut self) {
        self.position = 0.0;
        self.velocity = 0.0;
    }

    /// Set the animation to a specific position (useful for jumps)
    pub fn set_position(&mut self, position: f32) {
        self.position = position;
        self.velocity = 0.0;
    }
}

impl Default for CriticallyDampedSpringAnimation {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents one corner of the animated cursor.
/// Each corner animates independently for smooth deformation effects.
#[derive(Clone, Debug)]
pub struct CursorCorner {
    current_position: [f32; 2],
    relative_position: [f32; 2], // Position relative to cursor grid cell
    previous_destination: [f32; 2],
    animation_x: CriticallyDampedSpringAnimation,
    animation_y: CriticallyDampedSpringAnimation,
    animation_length: f32,
}

impl CursorCorner {
    pub fn new() -> Self {
        Self {
            current_position: [0.0, 0.0],
            relative_position: [0.0, 0.0],
            previous_destination: [-1000.0, -1000.0], // Far away to trigger initial animation
            animation_x: CriticallyDampedSpringAnimation::new(),
            animation_y: CriticallyDampedSpringAnimation::new(),
            animation_length: 0.0,
        }
    }

    /// Update the corner animation with the given parameters
    pub fn update(
        &mut self,
        cursor_dimensions: [f32; 2], // [width, height] of cursor cell
        destination: [f32; 2],       // Target position in pixels
        dt: f32,                     // Delta time in seconds
        animation_length: f32,       // Animation duration
    ) -> bool {
        let relative_scaled_position = [
            self.relative_position[0] * cursor_dimensions[0],
            self.relative_position[1] * cursor_dimensions[1],
        ];
        let corner_destination = [
            destination[0] + relative_scaled_position[0],
            destination[1] + relative_scaled_position[1],
        ];

        // Check if we need to start a new animation (destination changed significantly)
        let destination_changed = (corner_destination[0] - self.previous_destination[0]).abs() > 0.1
            || (corner_destination[1] - self.previous_destination[1]).abs() > 0.1;

        if destination_changed {
            self.previous_destination = corner_destination;
            self.animation_x.reset();
            self.animation_y.reset();
            self.animation_length = animation_length;
        }

        // Update animations
        let x_animating = self.animation_x.update(dt, self.animation_length);
        let y_animating = self.animation_y.update(dt, self.animation_length);

        // Interpolate position
        let start_pos = [
            self.current_position[0] - relative_scaled_position[0],
            self.current_position[1] - relative_scaled_position[1],
        ];
        
        self.current_position = [
            start_pos[0] + (corner_destination[0] - start_pos[0]) * self.animation_x.position,
            start_pos[1] + (corner_destination[1] - start_pos[1]) * self.animation_y.position,
        ];

        x_animating || y_animating
    }

    /// Get the current position of this corner
    pub fn position(&self) -> [f32; 2] {
        self.current_position
    }

    /// Set the relative position of this corner within the cursor cell
    pub fn set_relative_position(&mut self, relative_pos: [f32; 2]) {
        self.relative_position = relative_pos;
    }

    /// Instantly move the corner to a position (for teleporting cursor)
    pub fn teleport_to(&mut self, position: [f32; 2]) {
        self.current_position = position;
        self.previous_destination = position;
        self.animation_x.set_position(1.0);
        self.animation_y.set_position(1.0);
    }
}

impl Default for CursorCorner {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for cursor animation behavior
#[derive(Clone, Debug)]
pub struct CursorAnimationConfig {
    /// Base animation duration in seconds
    pub animation_length: f32,
    /// Shorter animation duration for rapid movements
    pub short_animation_length: f32,
    /// Whether to animate in insert mode
    pub animate_in_insert_mode: bool,
    /// Whether to animate in command line
    pub animate_command_line: bool,
    /// Minimum distance to trigger animation (in pixels)
    pub animation_threshold: f32,
}

impl Default for CursorAnimationConfig {
    fn default() -> Self {
        Self {
            animation_length: 0.15,
            short_animation_length: 0.04,
            animate_in_insert_mode: true,
            animate_command_line: true,
            animation_threshold: 1.0,
        }
    }
}

/// Main cursor animation state manager
#[derive(Clone, Debug)]
pub struct CursorAnimator {
    corners: Vec<CursorCorner>,
    destination: [f32; 2],
    previous_cursor_position: Option<[u32; 2]>, // Grid position
    last_update: Option<Instant>,
    config: CursorAnimationConfig,
    is_animating: bool,
}

impl CursorAnimator {
    pub fn new(config: CursorAnimationConfig) -> Self {
        let mut animator = Self {
            corners: Vec::new(),
            destination: [0.0, 0.0],
            previous_cursor_position: None,
            last_update: None,
            config,
            is_animating: false,
        };

        // Initialize corners for a block cursor (4 corners)
        animator.set_cursor_shape_block();
        animator
    }

    /// Set up corners for a block cursor shape
    pub fn set_cursor_shape_block(&mut self) {
        self.corners = vec![
            CursorCorner::new(), // Top-left
            CursorCorner::new(), // Top-right
            CursorCorner::new(), // Bottom-right
            CursorCorner::new(), // Bottom-left
        ];

        // Set relative positions for each corner
        self.corners[0].set_relative_position([0.0, 0.0]);     // Top-left
        self.corners[1].set_relative_position([1.0, 0.0]);     // Top-right
        self.corners[2].set_relative_position([1.0, 1.0]);     // Bottom-right
        self.corners[3].set_relative_position([0.0, 1.0]);     // Bottom-left
    }

    /// Set up corners for a beam cursor shape
    pub fn set_cursor_shape_beam(&mut self) {
        self.corners = vec![
            CursorCorner::new(), // Top-left
            CursorCorner::new(), // Top-right
            CursorCorner::new(), // Bottom-right
            CursorCorner::new(), // Bottom-left
        ];

        // Beam cursor is thin vertical line
        let beam_width = 0.1; // 10% of cell width
        self.corners[0].set_relative_position([0.0, 0.0]);           // Top-left
        self.corners[1].set_relative_position([beam_width, 0.0]);    // Top-right
        self.corners[2].set_relative_position([beam_width, 1.0]);    // Bottom-right
        self.corners[3].set_relative_position([0.0, 1.0]);           // Bottom-left
    }

    /// Set up corners for an underline cursor shape
    pub fn set_cursor_shape_underline(&mut self) {
        self.corners = vec![
            CursorCorner::new(), // Top-left
            CursorCorner::new(), // Top-right
            CursorCorner::new(), // Bottom-right
            CursorCorner::new(), // Bottom-left
        ];

        // Underline cursor is thin horizontal line at bottom
        let underline_height = 0.15; // 15% of cell height
        self.corners[0].set_relative_position([0.0, 1.0 - underline_height]); // Top-left
        self.corners[1].set_relative_position([1.0, 1.0 - underline_height]); // Top-right
        self.corners[2].set_relative_position([1.0, 1.0]);                    // Bottom-right
        self.corners[3].set_relative_position([0.0, 1.0]);                    // Bottom-left
    }

    /// Update cursor destination and trigger animation if needed
    pub fn update_cursor_destination(
        &mut self,
        grid_position: [u32; 2],
        pixel_position: [f32; 2],
        cell_dimensions: [f32; 2],
    ) {
        let now = Instant::now();
        
        // Check if cursor position changed significantly
        let position_changed = self.previous_cursor_position
            .map(|prev| prev != grid_position)
            .unwrap_or(true);

        if position_changed {
            // Calculate distance moved for animation length selection
            let distance = if let Some(prev_pos) = self.previous_cursor_position {
                let dx = (grid_position[0] as f32 - prev_pos[0] as f32) * cell_dimensions[0];
                let dy = (grid_position[1] as f32 - prev_pos[1] as f32) * cell_dimensions[1];
                (dx * dx + dy * dy).sqrt()
            } else {
                0.0
            };

            // Use shorter animation for rapid movements
            let animation_length = if distance > cell_dimensions[0] * 3.0 {
                self.config.short_animation_length
            } else {
                self.config.animation_length
            };

            self.destination = pixel_position;
            self.previous_cursor_position = Some(grid_position);
            self.is_animating = true;

            // Update all corners with new destination
            for corner in &mut self.corners {
                corner.animation_length = animation_length;
            }
        }

        self.last_update = Some(now);
    }

    /// Update animation state and return whether animation is still in progress
    pub fn update_animation(&mut self, cell_dimensions: [f32; 2]) -> bool {
        let now = Instant::now();
        let dt = self.last_update
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(0.0);

        if dt > 0.0 {
            let mut any_animating = false;
            
            for corner in &mut self.corners {
                let still_animating = corner.update(
                    cell_dimensions,
                    self.destination,
                    dt,
                    corner.animation_length,
                );
                any_animating = any_animating || still_animating;
            }

            self.is_animating = any_animating;
            self.last_update = Some(now);
        }

        self.is_animating
    }

    /// Get the current animated corner positions
    pub fn get_corner_positions(&self) -> Vec<[f32; 2]> {
        self.corners.iter().map(|corner| corner.position()).collect()
    }

    /// Check if cursor is currently animating
    pub fn is_animating(&self) -> bool {
        self.is_animating
    }

    /// Instantly teleport cursor to position (no animation)
    pub fn teleport_to(&mut self, grid_position: [u32; 2], pixel_position: [f32; 2]) {
        self.destination = pixel_position;
        self.previous_cursor_position = Some(grid_position);
        self.is_animating = false;

        for corner in &mut self.corners {
            let relative_pos = [
                pixel_position[0] + corner.relative_position[0],
                pixel_position[1] + corner.relative_position[1],
            ];
            corner.teleport_to(relative_pos);
        }
    }

    /// Update configuration
    pub fn set_config(&mut self, config: CursorAnimationConfig) {
        self.config = config;
    }

    /// Get current configuration
    pub fn config(&self) -> &CursorAnimationConfig {
        &self.config
    }
}

impl Default for CursorAnimator {
    fn default() -> Self {
        Self::new(CursorAnimationConfig::default())
    }
}