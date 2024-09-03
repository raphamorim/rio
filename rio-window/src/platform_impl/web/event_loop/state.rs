use super::backend;

use web_time::Instant;

#[derive(Debug)]
pub enum State {
    Init,
    WaitUntil {
        _timeout: backend::Schedule,
        start: Instant,
        end: Instant,
    },
    Wait {
        start: Instant,
    },
    Poll {
        _request: backend::Schedule,
    },
    Exit,
}

impl State {
    pub fn exiting(&self) -> bool {
        matches!(self, State::Exit)
    }
}
