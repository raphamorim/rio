// Part of this file was originally taken from Alacritty
// https://github.com/alacritty/alacritty/blob/34b5dbacd28cd1abaedf1d81cc0ebe57aa44a086/alacritty/src/input.rs
// which is licensed under Apache 2.0 license.

use std::collections::hash_map::RandomState;
use std::time::Instant;
use std::{collections::HashSet, mem};

use rio_window::event::{ElementState, MouseButton, Touch, TouchPhase};

use crate::bindings::FontSizeAction;
use crate::event::ClickState;
use crate::router::Route;

#[derive(Debug)]
pub enum TouchPurpose {
    None,
    Select(Touch),
    Scroll(Touch),
    Zoom(TouchZoom),
    Tap(Touch),
    Invalid(HashSet<u64, RandomState>),
}

impl Default for TouchPurpose {
    fn default() -> Self {
        Self::None
    }
}

const FONT_SIZE_STEP: f32 = 1.00;

/// Touch zoom speed.
const TOUCH_ZOOM_FACTOR: f32 = 1.0;

/// Distance before a touch input is considered a drag.
pub const MAX_TAP_DISTANCE: f64 = 5.;

/// Touch zooming state.
#[derive(Debug)]
pub struct TouchZoom {
    slots: (Touch, Touch),
    fractions: f32,
}

impl TouchZoom {
    pub fn new(slots: (Touch, Touch)) -> Self {
        Self {
            slots,
            fractions: Default::default(),
        }
    }

    /// Get slot distance change since last update.
    pub fn font_delta(&mut self, slot: Touch) -> f32 {
        let old_distance = self.distance();

        // Update touch slots.
        if slot.id == self.slots.0.id {
            self.slots.0 = slot;
        } else {
            self.slots.1 = slot;
        }

        // Calculate font change in `FONT_SIZE_STEP` increments.
        let delta = (self.distance() - old_distance) * TOUCH_ZOOM_FACTOR + self.fractions;
        let font_delta =
            (delta.abs() / FONT_SIZE_STEP).floor() * FONT_SIZE_STEP * delta.signum();
        self.fractions = delta - font_delta;

        font_delta
    }

    /// Get active touch slots.
    pub fn slots(&self) -> HashSet<u64, RandomState> {
        let mut set = HashSet::default();
        set.insert(self.slots.0.id);
        set.insert(self.slots.1.id);
        set
    }

    /// Calculate distance between slots.
    fn distance(&self) -> f32 {
        let delta_x = self.slots.0.location.x - self.slots.1.location.x;
        let delta_y = self.slots.0.location.y - self.slots.1.location.y;
        delta_x.hypot(delta_y) as f32
    }
}

#[inline]
pub fn on_touch(route: &mut Route, touch: Touch) {
    match touch.phase {
        TouchPhase::Started => {
            on_touch_start(route, touch);
        }
        TouchPhase::Moved => on_touch_motion(route, touch),
        TouchPhase::Ended | TouchPhase::Cancelled => on_touch_end(route, touch),
    }
}

#[inline]
fn on_touch_start(route: &mut Route, touch: Touch) {
    let touch_purpose = route.window.screen.touch_purpose();
    *touch_purpose = match mem::take(touch_purpose) {
        TouchPurpose::None => TouchPurpose::Tap(touch),
        TouchPurpose::Tap(start) => TouchPurpose::Zoom(TouchZoom::new((start, touch))),
        TouchPurpose::Zoom(zoom) => TouchPurpose::Invalid(zoom.slots()),
        TouchPurpose::Scroll(event) | TouchPurpose::Select(event) => {
            let mut set = HashSet::default();
            set.insert(event.id);
            TouchPurpose::Invalid(set)
        }
        TouchPurpose::Invalid(mut slots) => {
            slots.insert(touch.id);
            TouchPurpose::Invalid(slots)
        }
    };
}

#[inline]
fn on_touch_motion(route: &mut Route, touch: Touch) {
    let touch_purpose = route.window.screen.touch_purpose();
    match touch_purpose {
        TouchPurpose::None => (),
        // Handle transition from tap to scroll/select.
        TouchPurpose::Tap(start) => {
            let delta_x = touch.location.x - start.location.x;
            let delta_y = touch.location.y - start.location.y;
            if delta_x.abs() > MAX_TAP_DISTANCE {
                tracing::info!("tap to select");
                // Update gesture state.
                let start_location = start.location;
                *touch_purpose = TouchPurpose::Select(*start);

                let layout = route.window.screen.sugarloaf.window_size();

                // Start simulated mouse input.
                let x = start_location.x.clamp(0.0, layout.width.into()) as usize;
                let y = start_location.y.clamp(0.0, layout.height.into()) as usize;

                route.window.screen.mouse.x = x;
                route.window.screen.mouse.y = y;

                let now = Instant::now();
                route.window.screen.mouse.last_click_timestamp = now;
                route.window.screen.mouse.last_click_button = MouseButton::Left;

                route.window.screen.mouse.click_state = ClickState::Click;
                route.window.screen.mouse.left_button_state = ElementState::Pressed;
                route
                    .window
                    .screen
                    .on_left_click(route.window.screen.mouse_position(0));

                // Apply motion since touch start.
                on_touch_motion(route, touch);
            } else if delta_y.abs() > MAX_TAP_DISTANCE {
                tracing::info!("tap to scroll");
                // Update gesture state.
                *touch_purpose = TouchPurpose::Scroll(*start);
                // Apply motion since touch start.
                on_touch_motion(route, touch);
            } else {
                tracing::info!("tap normal");
            }
        }
        TouchPurpose::Zoom(zoom) => {
            let font_delta = zoom.font_delta(touch);
            if font_delta >= 0. {
                route
                    .window
                    .screen
                    .change_font_size(FontSizeAction::Increase);
            } else {
                route
                    .window
                    .screen
                    .change_font_size(FontSizeAction::Decrease);
            }
            tracing::info!("zoom motion: {}", font_delta);
        }
        TouchPurpose::Scroll(last_touch) => {
            // Calculate delta and update last touch position.
            let delta_y = touch.location.y - last_touch.location.y;
            *touch_purpose = TouchPurpose::Scroll(touch);
            route.window.screen.scroll(0., delta_y);
            tracing::info!("scroll motion: {}", delta_y);
        }
        TouchPurpose::Select(_) => {
            let layout = route.window.screen.sugarloaf.window_size();
            let x = touch.location.x.clamp(0.0, layout.width.into()) as usize;
            let y = touch.location.y.clamp(0.0, layout.height.into()) as usize;
            route.window.screen.mouse.x = x;
            route.window.screen.mouse.y = y;
            tracing::info!("select motion");
        }
        TouchPurpose::Invalid(_) => (),
    }
}

#[inline]
fn on_touch_end(route: &mut Route, touch: Touch) {
    on_touch_motion(route, touch);

    let touch_purpose = route.window.screen.touch_purpose();
    match touch_purpose {
        // Simulate LMB clicks.
        TouchPurpose::Tap(start) => {
            let start_location = start.location;
            *touch_purpose = Default::default();

            let layout = route.window.screen.sugarloaf.window_size();

            let x = start_location.x.clamp(0.0, layout.width.into()) as usize;
            let y = start_location.y.clamp(0.0, layout.height.into()) as usize;

            route.window.screen.mouse.x = x;
            route.window.screen.mouse.y = y;

            let now = Instant::now();
            route.window.screen.mouse.last_click_timestamp = now;
            route.window.screen.mouse.last_click_button = MouseButton::Left;

            route.window.screen.mouse.click_state = ClickState::Click;
            route.window.screen.mouse.left_button_state = ElementState::Pressed;
            route
                .window
                .screen
                .on_left_click(route.window.screen.mouse_position(0));
            route.window.screen.mouse.click_state = ClickState::None;
            route.window.screen.mouse.left_button_state = ElementState::Released;
            tracing::info!("tap end");
        }
        // Invalidate zoom once a finger was released.
        TouchPurpose::Zoom(zoom) => {
            let mut slots = zoom.slots();
            slots.remove(&touch.id);
            *touch_purpose = TouchPurpose::Invalid(slots);
            tracing::info!("zoom end");
        }
        // Reset touch state once all slots were released.
        TouchPurpose::Invalid(slots) => {
            slots.remove(&touch.id);
            if slots.is_empty() {
                *touch_purpose = Default::default();
            }
        }
        // Release simulated LMB.
        TouchPurpose::Select(_) => {
            *touch_purpose = Default::default();
            route.window.screen.mouse.click_state = ClickState::None;
            route.window.screen.mouse.left_button_state = ElementState::Released;
            tracing::info!("select end");
        }
        // Reset touch state on scroll finish.
        TouchPurpose::Scroll(_) => {
            *touch_purpose = Default::default();
            tracing::info!("scroll end");
        }
        TouchPurpose::None => (),
    }
}
