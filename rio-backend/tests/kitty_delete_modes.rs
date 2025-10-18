// Tests for Kitty Graphics Protocol Delete Modes

use rio_backend::ansi::kitty_graphics_protocol::DeleteRequest;
use rio_backend::crosswords::Crosswords;
use rio_backend::event::{EventListener, RioEvent, WindowId};
use rio_backend::performer::handler::Handler;

#[derive(Clone)]
struct TestEventListener;

impl EventListener for TestEventListener {
    fn event(&self) -> (Option<RioEvent>, bool) {
        (None, false)
    }
}

#[test]
fn test_delete_all() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Delete all graphics (d=a)
    let delete = DeleteRequest {
        action: b'a',
        image_id: 0,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: false,
    };

    // Should not panic
    term.delete_graphics(delete);
}

#[test]
fn test_delete_all_with_data() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Delete all graphics and image data (d=A, delete_data=true)
    let delete = DeleteRequest {
        action: b'A',
        image_id: 0,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: true,
    };

    term.delete_graphics(delete);
}

#[test]
fn test_delete_by_image_id() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Delete by image ID (d=i, i=100)
    let delete = DeleteRequest {
        action: b'i',
        image_id: 100,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: false,
    };

    term.delete_graphics(delete);
}

#[test]
fn test_delete_at_cursor() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Delete at cursor position (d=c)
    let delete = DeleteRequest {
        action: b'c',
        image_id: 0,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: false,
    };

    term.delete_graphics(delete);
}

#[test]
fn test_delete_at_position() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Delete at specific position (d=p, x=10, y=5)
    let delete = DeleteRequest {
        action: b'p',
        image_id: 0,
        placement_id: 0,
        x: 10,
        y: 5,
        z_index: 0,
        delete_data: false,
    };

    term.delete_graphics(delete);
}

#[test]
fn test_delete_by_column() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Delete by column (d=x, x=10)
    let delete = DeleteRequest {
        action: b'x',
        image_id: 0,
        placement_id: 0,
        x: 10,
        y: 0,
        z_index: 0,
        delete_data: false,
    };

    term.delete_graphics(delete);
}

#[test]
fn test_delete_by_row() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Delete by row (d=y, y=5)
    let delete = DeleteRequest {
        action: b'y',
        image_id: 0,
        placement_id: 0,
        x: 0,
        y: 5,
        z_index: 0,
        delete_data: false,
    };

    term.delete_graphics(delete);
}

#[test]
fn test_delete_by_z_index() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Delete by z-index (d=z, z_index=10)
    let delete = DeleteRequest {
        action: b'z',
        image_id: 0,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 10,
        delete_data: false,
    };

    term.delete_graphics(delete);
}
