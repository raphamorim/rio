// Functional tests for Kitty Graphics Protocol
// Tests the protocol handler methods

use rio_backend::ansi::kitty_graphics_protocol;
use rio_backend::crosswords::Crosswords;
use rio_backend::event::{EventListener, RioEvent, WindowId};
use rio_backend::performer::handler::Handler;

/// Test event listener
#[derive(Clone)]
struct TestEventListener;

impl EventListener for TestEventListener {
    fn event(&self) -> (Option<RioEvent>, bool) {
        (None, false)
    }
}

#[test]
fn test_place_graphic_handler() {
    // Test that place_graphic handler method exists and can be called
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 2,
        rows: 2,
        z_index: 0,
        unicode_placeholder: 0,
    };

    // Should not panic
    term.place_graphic(placement);
}

#[test]
fn test_delete_graphics_handler() {
    // Test that delete_graphics handler method exists and can be called
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    let delete_request = kitty_graphics_protocol::DeleteRequest {
        action: b'a',
        image_id: 0,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: false,
    };

    // Should not panic
    term.delete_graphics(delete_request);
}

#[test]
fn test_kitty_graphics_response_handler() {
    // Test that kitty_graphics_response handler method exists and can be called
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Should not panic - sends PtyWrite event
    term.kitty_graphics_response("Gi=1;OK".to_string());
}
