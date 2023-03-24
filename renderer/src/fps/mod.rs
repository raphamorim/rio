use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Measures Frames Per Second (FPS).
#[derive(Debug)]
pub struct FramesCounter {
    /// The last registered frames.
    last_second_frames: VecDeque<Instant>,
}

impl Default for FramesCounter {
    fn default() -> Self {
        FramesCounter::new()
    }
}

impl FramesCounter {
    /// Creates a new FramesCounter.
    pub fn new() -> FramesCounter {
        FramesCounter {
            last_second_frames: VecDeque::with_capacity(240),
        }
    }

    /// Updates the FramesCounter and returns number of frames.
    pub fn tick(&mut self) -> usize {
        let now = Instant::now();
        let a_second_ago = now - Duration::from_secs(1);

        while self
            .last_second_frames
            .front()
            .map_or(false, |t| *t < a_second_ago)
        {
            self.last_second_frames.pop_front();
        }

        self.last_second_frames.push_back(now);
        self.last_second_frames.len()
    }
}
