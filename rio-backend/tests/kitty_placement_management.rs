// Tests for Kitty Graphics Protocol Placement Management

use rio_backend::crosswords::Crosswords;
use rio_backend::event::{EventListener, RioEvent, WindowId};
use rio_backend::performer::handler::Handler;
use sugarloaf::{ColorType, GraphicData, GraphicId};

#[derive(Clone)]
struct TestEventListener;

impl EventListener for TestEventListener {
    fn event(&self) -> (Option<RioEvent>, bool) {
        (None, false)
    }
}

#[test]
fn test_store_graphic() {
    // Test a=t (transmit-only)
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    let pixels = vec![255u8, 0, 0, 255]; // 1x1 red pixel
    let graphic = GraphicData {
        id: GraphicId(100),
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
    };

    // Store without displaying
    term.store_graphic(graphic);

    // Verify image is in cache
    let stored = term.graphics.get_kitty_image(100);
    assert!(stored.is_some(), "Image should be stored in cache");
    assert_eq!(stored.unwrap().data.width, 1);
}

#[test]
fn test_place_stored_graphic() {
    // Test a=p (place previously stored image)
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // First store an image
    let pixels = vec![255u8, 0, 0, 255]; // 1x1 red pixel
    let graphic = GraphicData {
        id: GraphicId(100),
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
    };

    term.store_graphic(graphic);

    // Now place it
    let placement = rio_backend::ansi::kitty_graphics_protocol::PlacementRequest {
        image_id: 100,
        placement_id: 0,
        x: 5,
        y: 3,
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
fn test_place_nonexistent_graphic() {
    // Test placing a graphic that doesn't exist
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    let placement = rio_backend::ansi::kitty_graphics_protocol::PlacementRequest {
        image_id: 999, // Doesn't exist
        placement_id: 0,
        x: 5,
        y: 3,
        width: 0,
        height: 0,
        columns: 2,
        rows: 2,
        z_index: 0,
        unicode_placeholder: 0,
    };

    // Should not panic, just warn
    term.place_graphic(placement);
}

#[test]
fn test_multiple_placements_same_image() {
    // Test placing the same image multiple times
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Store an image
    let pixels = vec![255u8, 0, 0, 255]; // 1x1 red pixel
    let graphic = GraphicData {
        id: GraphicId(100),
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
    };

    term.store_graphic(graphic);

    // Place it at position 1
    let placement1 = rio_backend::ansi::kitty_graphics_protocol::PlacementRequest {
        image_id: 100,
        placement_id: 1,
        x: 5,
        y: 3,
        width: 0,
        height: 0,
        columns: 2,
        rows: 2,
        z_index: 0,
        unicode_placeholder: 0,
    };

    term.place_graphic(placement1);

    // Place it again at position 2
    let placement2 = rio_backend::ansi::kitty_graphics_protocol::PlacementRequest {
        image_id: 100,
        placement_id: 2,
        x: 10,
        y: 5,
        width: 0,
        height: 0,
        columns: 2,
        rows: 2,
        z_index: 0,
        unicode_placeholder: 0,
    };

    term.place_graphic(placement2);

    // Both placements should succeed without panic
}

#[test]
fn test_delete_stored_images() {
    // Test that delete with uppercase (I) removes from cache
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        rio_backend::crosswords::CrosswordsSize::new(80, 24),
        rio_backend::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Store an image
    let pixels = vec![255u8, 0, 0, 255]; // 1x1 red pixel
    let graphic = GraphicData {
        id: GraphicId(100),
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
    };

    term.store_graphic(graphic);

    // Verify it's in cache
    assert!(term.graphics.get_kitty_image(100).is_some());

    // Delete with uppercase I and delete_data=true
    let delete = rio_backend::ansi::kitty_graphics_protocol::DeleteRequest {
        action: b'I',
        image_id: 100,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: true,
    };

    term.delete_graphics(delete);

    // Verify it's removed from cache
    assert!(term.graphics.get_kitty_image(100).is_none());
}
