//! Tests for Poll functionality
//!
//! These tests verify the core Poll event notification mechanism.

use corcovado::*;
use std::time::Duration;

/// Tests that Poll correctly closes file descriptors.
///
/// This test creates and drops Poll instances repeatedly to ensure
/// that file descriptors are properly released and no resource leaks occur.
#[test]
fn test_poll_closes_fd() {
    for _ in 0..2000 {
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(4);
        let (registration, set_readiness) = Registration::new2();

        poll.register(&registration, Token(0), Ready::readable(), PollOpt::edge())
            .unwrap();
        poll.poll(&mut events, Some(Duration::from_millis(0)))
            .unwrap();

        drop(poll);
        drop(set_readiness);
        drop(registration);
    }
}
