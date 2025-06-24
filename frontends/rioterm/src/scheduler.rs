// scheduler.rs was retired originally from https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty/src/scheduler.rs
// which is licensed under Apache 2.0 license.

use crate::event::EventPayload;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use rio_window::event_loop::EventLoopProxy;

/// ID uniquely identifying a timer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TimerId {
    topic: Topic,
    id: usize,
}

impl TimerId {
    pub fn new(topic: Topic, id: usize) -> Self {
        Self { topic, id }
    }
}

/// Available timer topics.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Topic {
    Render,
    RenderRoute,
    UpdateConfig,
    CursorBlinking,
    UpdateTitles,
}

/// Event scheduled to be emitted at a specific time.
#[derive(Debug)]
pub struct Timer {
    pub deadline: Instant,
    pub event: EventPayload,
    pub id: TimerId,

    interval: Option<Duration>,
}

/// Scheduler tracking all pending timers.
pub struct Scheduler {
    timers: VecDeque<Timer>,
    event_proxy: EventLoopProxy<EventPayload>,
}

impl Scheduler {
    pub fn new(event_proxy: EventLoopProxy<EventPayload>) -> Self {
        Self {
            timers: VecDeque::new(),
            event_proxy,
        }
    }

    /// Process all pending timers.
    ///
    /// If there are still timers pending after all ready events have been processed, the closest
    /// pending deadline will be returned.
    pub fn update(&mut self) -> Option<Instant> {
        let now = Instant::now();

        while !self.timers.is_empty() && self.timers[0].deadline <= now {
            if let Some(timer) = self.timers.pop_front() {
                // Automatically repeat the event.
                if let Some(interval) = timer.interval {
                    self.schedule(timer.event.clone(), interval, true, timer.id);
                }
                let _ = self.event_proxy.send_event(timer.event);
            }
        }

        self.timers.front().map(|timer| timer.deadline)
    }

    /// Schedule a new event.
    pub fn schedule(
        &mut self,
        event: EventPayload,
        interval: Duration,
        repeat: bool,
        timer_id: TimerId,
    ) {
        let deadline = Instant::now() + interval;

        // Get insert position in the schedule.
        let index = self
            .timers
            .iter()
            .position(|timer| timer.deadline > deadline)
            .unwrap_or(self.timers.len());

        // Set the automatic event repeat rate.
        let interval = if repeat { Some(interval) } else { None };

        self.timers.insert(
            index,
            Timer {
                interval,
                deadline,
                event,
                id: timer_id,
            },
        );
    }

    /// Cancel a scheduled event.
    pub fn unschedule(&mut self, id: TimerId) -> Option<Timer> {
        let index = self.timers.iter().position(|timer| timer.id == id)?;
        self.timers.remove(index)
    }

    /// Check if a timer is already scheduled.
    pub fn scheduled(&mut self, id: TimerId) -> bool {
        self.timers.iter().any(|timer| timer.id == id)
    }

    /// Remove all timers scheduled for a tab.
    ///
    /// This must be called when a tab is removed to ensure that timers on intervals do not
    /// stick around forever and cause a memory leak.
    pub fn unschedule_window(&mut self, id: usize) {
        self.timers.retain(|timer| timer.id.id != id);
    }
}
