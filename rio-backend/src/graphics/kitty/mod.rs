// Kitty Graphics Protocol Tests
// Combined test suite for Kitty graphics functionality

use crate::ansi::kitty_graphics_protocol::{self, DeleteRequest, PlacementRequest};
use crate::crosswords::grid::Dimensions;
use crate::crosswords::Crosswords;
use crate::event::{EventListener, RioEvent, WindowId};
use crate::performer::handler::Handler;
use sugarloaf::{ColorType, GraphicData, GraphicId, ResizeCommand, ResizeParameter};

// Common test utilities

/// Test handler that captures graphics operations
#[derive(Default)]
struct TestHandler {
    graphics: Vec<GraphicData>,
    placements: Vec<PlacementRequest>,
    deletions: Vec<DeleteRequest>,
    responses: Vec<String>,
}

impl Handler for TestHandler {
    fn insert_graphic(
        &mut self,
        data: GraphicData,
        _palette: Option<Vec<crate::config::colors::ColorRgb>>,
        _cursor_movement: Option<u8>,
    ) {
        self.graphics.push(data);
    }

    fn place_graphic(&mut self, placement: PlacementRequest) {
        self.placements.push(placement);
    }

    fn delete_graphics(&mut self, delete: DeleteRequest) {
        self.deletions.push(delete);
    }

    fn kitty_graphics_response(&mut self, response: String) {
        self.responses.push(response);
    }
}

/// Test event listener
#[derive(Clone)]
struct TestEventListener;

impl EventListener for TestEventListener {
    fn event(&self) -> (Option<RioEvent>, bool) {
        (None, false)
    }
}

// Integration Tests

#[test]
fn test_direct_parse_transmit() {
    let mut handler = TestHandler::default();

    // Parse kitty graphics directly through the protocol parser
    // 1x1 RGBA pixel (4 bytes) - base64 encoded [255, 0, 0, 255] (red pixel)
    let params = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,i=1".as_ref(),
        b"/wAA/w==".as_ref(),
    ];

    if let Some(response) = kitty_graphics_protocol::parse(&params) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0));
        }
    }

    // Verify graphic was captured
    assert_eq!(handler.graphics.len(), 1, "Should capture one graphic");

    let graphic = &handler.graphics[0];
    assert_eq!(graphic.width, 1);
    assert_eq!(graphic.height, 1);
    assert_eq!(graphic.pixels.len(), 4); // 1x1x4 bytes (RGBA)
    assert_eq!(graphic.id.0, 1);
}

#[test]
fn test_placement_request() {
    let mut handler = TestHandler::default();

    // Parse placement request (a=p is Put action, x and y are source coordinates)
    let params = vec![b"G".as_ref(), b"a=p,i=1,x=5,y=10,c=3,r=2".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params) {
        if let Some(placement) = response.placement_request {
            handler.place_graphic(placement);
        }
    }

    // Verify placement was captured
    assert_eq!(handler.placements.len(), 1, "Should capture one placement");

    let placement = &handler.placements[0];
    assert_eq!(placement.image_id, 1);
    assert_eq!(placement.x, 5);
    assert_eq!(placement.y, 10);
    assert_eq!(placement.columns, 3);
    assert_eq!(placement.rows, 2);
}

#[test]
fn test_delete_request() {
    let mut handler = TestHandler::default();

    // Parse delete request (a=d is Delete action, d=a means delete all)
    let params = vec![b"G".as_ref(), b"a=d,d=a".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params) {
        if let Some(delete) = response.delete_request {
            handler.delete_graphics(delete);
        }
    }

    // Verify deletion was captured
    assert_eq!(handler.deletions.len(), 1, "Should capture one deletion");
    assert_eq!(handler.deletions[0].action, b'a');
}

#[test]
fn test_query_response() {
    let mut handler = TestHandler::default();

    // Parse query request
    let params = vec![b"G".as_ref(), b"a=q,i=1".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params) {
        if let Some(response_str) = response.response {
            handler.kitty_graphics_response(response_str);
        }
    }

    // Verify response was generated
    assert_eq!(handler.responses.len(), 1, "Should generate one response");
    assert!(handler.responses[0].contains("Gi=1;OK"));
}

#[test]
fn test_chunked_transfer() {
    let mut handler = TestHandler::default();

    // Total base64 for 1x1 RGBA pixel [255, 0, 0, 255] is "/wAA/w=="
    // Split into 3 chunks: "/wA", "A/", "w=="

    // Send first chunk (m=1 means more chunks coming)
    let params1 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,m=1,i=100".as_ref(),
        b"/wA".as_ref(),
    ];
    let result1 = kitty_graphics_protocol::parse(&params1);
    assert!(result1.is_none());

    // Send second chunk
    let params2 = vec![b"G".as_ref(), b"a=t,m=1,i=100".as_ref(), b"A/".as_ref()];
    let result2 = kitty_graphics_protocol::parse(&params2);
    assert!(result2.is_none());

    // Send final chunk with complete image info (m=0 means last chunk)
    let params3 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,m=0,i=100".as_ref(),
        b"w==".as_ref(),
    ];
    if let Some(response) = kitty_graphics_protocol::parse(&params3) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0));
        }
    }

    // Now graphic should be created
    assert_eq!(handler.graphics.len(), 1);
    assert_eq!(handler.graphics[0].id.0, 100);
    assert_eq!(handler.graphics[0].width, 1);
    assert_eq!(handler.graphics[0].height, 1);
}

#[test]
fn test_multiple_graphics_in_sequence() {
    let mut handler = TestHandler::default();

    // Send multiple graphics (1x1 RGBA pixels with different IDs)
    // Base64 for [255, 0, 0, 255] = "/wAA/w=="
    let graphics_params = [
        (
            vec![
                b"G".as_ref(),
                b"a=t,f=32,s=1,v=1,i=1".as_ref(),
                b"/wAA/w==".as_ref(),
            ],
            1u64,
        ),
        (
            vec![
                b"G".as_ref(),
                b"a=t,f=32,s=1,v=1,i=2".as_ref(),
                b"/wAA/w==".as_ref(),
            ],
            2u64,
        ),
        (
            vec![
                b"G".as_ref(),
                b"a=t,f=32,s=1,v=1,i=3".as_ref(),
                b"/wAA/w==".as_ref(),
            ],
            3u64,
        ),
    ];

    for (params, _) in &graphics_params {
        if let Some(response) = kitty_graphics_protocol::parse(params) {
            if let Some(graphic_data) = response.graphic_data {
                handler.insert_graphic(graphic_data, None, Some(0));
            }
        }
    }

    // Should have 3 graphics
    assert_eq!(handler.graphics.len(), 3);

    // Verify IDs
    assert_eq!(handler.graphics[0].id.0, 1);
    assert_eq!(handler.graphics[1].id.0, 2);
    assert_eq!(handler.graphics[2].id.0, 3);
}

// Cursor Movement Tests

#[test]
fn test_cursor_movement_default() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    let initial_cursor_row = term.grid.cursor.pos.row.0;

    // Set proper cell dimensions for testing
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Create a 100x100 pixel image (will be resized to fit 2 rows)
    let pixels = vec![255u8; 100 * 100 * 4];
    let graphic = GraphicData {
        id: GraphicId(1),
        width: 100,
        height: 100,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: Some(ResizeCommand {
            width: ResizeParameter::Auto,
            height: ResizeParameter::Cells(2),
            preserve_aspect_ratio: true,
        }),
    };

    term.store_graphic(graphic);

    // Place with cursor_movement=0 (move cursor to after image)
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 2,
        z_index: 0,
        unicode_placeholder: 0,
        cursor_movement: 0,
    };

    term.place_graphic(placement);

    let final_cursor_row = term.grid.cursor.pos.row.0;
    let final_cursor_col = term.grid.cursor.pos.col.0;

    // With cursor_movement=0 (Kitty default), cursor stays ON last row of image
    // For a 2-row image starting at row 0 (occupies rows 0-1), cursor should be at row 1, col 0
    assert_eq!(
        final_cursor_row, 1,
        "Cursor should be at row 1 (last row of image) with cursor_movement=0. Initial: {}, Final: {}",
        initial_cursor_row,
        final_cursor_row
    );
    assert_eq!(
        final_cursor_col, 0,
        "Cursor should be at column 0 after carriage return"
    );
}

#[test]
fn test_cursor_movement_no_move() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Start at a specific position
    term.grid.cursor.pos.row.0 = 5;
    term.grid.cursor.pos.col.0 = 10;

    // Create a 100x100 pixel image
    let pixels = vec![255u8; 100 * 100 * 4];
    let graphic = GraphicData {
        id: GraphicId(2),
        width: 100,
        height: 100,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: Some(ResizeCommand {
            width: ResizeParameter::Auto,
            height: ResizeParameter::Cells(2),
            preserve_aspect_ratio: true,
        }),
    };

    term.store_graphic(graphic);

    // Place with cursor_movement=1 (don't move cursor)
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 2,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 2,
        z_index: 0,
        unicode_placeholder: 0,
        cursor_movement: 1, // Don't move cursor
    };

    term.place_graphic(placement);

    // With cursor_movement=1, cursor behavior depends on placement x,y
    // This test verifies the no-move code path executes without panic
}

#[test]
fn test_protocol_parses_cursor_movement() {
    // Test that C=0 is parsed
    let result = kitty_graphics_protocol::parse(&[b"G", b"a=p,i=1,C=0", b""]);
    assert!(result.is_some());
    let response = result.unwrap();
    assert!(response.placement_request.is_some());
    let placement = response.placement_request.unwrap();
    assert_eq!(
        placement.cursor_movement, 0,
        "C=0 should parse as cursor_movement=0"
    );

    // Test that C=1 is parsed
    let result = kitty_graphics_protocol::parse(&[b"G", b"a=p,i=1,C=1", b""]);
    assert!(result.is_some());
    let response = result.unwrap();
    assert!(response.placement_request.is_some());
    let placement = response.placement_request.unwrap();
    assert_eq!(
        placement.cursor_movement, 1,
        "C=1 should parse as cursor_movement=1"
    );

    // Test default (no C key)
    let result = kitty_graphics_protocol::parse(&[b"G", b"a=p,i=1", b""]);
    assert!(result.is_some());
    let response = result.unwrap();
    assert!(response.placement_request.is_some());
    let placement = response.placement_request.unwrap();
    assert_eq!(
        placement.cursor_movement, 0,
        "Default should be cursor_movement=0"
    );
}

// Row Calculation Tests

#[test]
fn test_image_row_occupation_exact_fit() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Start at row 0
    let initial_cursor_row = term.grid.cursor.pos.row.0;
    assert_eq!(initial_cursor_row, 0, "Cursor should start at row 0");

    // Set proper cell dimensions for testing
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Create a 100x100 pixel image (will be resized to fit 2 rows)
    let pixels = vec![255u8; 100 * 100 * 4];
    let graphic = GraphicData {
        id: GraphicId(1),
        width: 100,
        height: 100,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: Some(ResizeCommand {
            width: ResizeParameter::Auto,
            height: ResizeParameter::Cells(2),
            preserve_aspect_ratio: true,
        }),
    };

    term.store_graphic(graphic);

    // Place it with rows=2 (should occupy exactly 2 rows)
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 2,
        z_index: 0,
        unicode_placeholder: 0,
        cursor_movement: 0,
    };

    term.place_graphic(placement);

    let final_cursor_row = term.grid.cursor.pos.row.0;

    // With fix: cursor stays ON last row of image (row 1)
    assert_eq!(
        final_cursor_row, 1,
        "Cursor should be at row 1 (last row of image) after placing a 2-row image, but got row {}",
        final_cursor_row
    );
}

#[test]
fn test_image_row_occupation_single_row() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    let initial_cursor_row = term.grid.cursor.pos.row.0;

    // Create a small image that fits in 1 row
    let pixels = vec![255u8; 50 * 20 * 4];
    let graphic = GraphicData {
        id: GraphicId(2),
        width: 50,
        height: 20,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: Some(ResizeCommand {
            width: ResizeParameter::Auto,
            height: ResizeParameter::Cells(1),
            preserve_aspect_ratio: true,
        }),
    };

    term.store_graphic(graphic);

    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 2,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 1,
        z_index: 0,
        unicode_placeholder: 0,
        cursor_movement: 0,
    };

    term.place_graphic(placement);

    let final_cursor_row = term.grid.cursor.pos.row.0;

    // With fix: cursor stays ON last row of image (row 0)
    assert_eq!(
        final_cursor_row, 0,
        "Cursor should be at row 0 (last row of image) after placing a 1-row image, but got row {}",
        final_cursor_row
    );
}

#[test]
fn test_image_row_occupation_three_rows() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    let initial_cursor_row = term.grid.cursor.pos.row.0;

    // Set proper cell dimensions for testing
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    let pixels = vec![255u8; 100 * 150 * 4];
    let graphic = GraphicData {
        id: GraphicId(3),
        width: 100,
        height: 150,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: Some(ResizeCommand {
            width: ResizeParameter::Auto,
            height: ResizeParameter::Cells(3),
            preserve_aspect_ratio: true,
        }),
    };

    term.store_graphic(graphic);

    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 3,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 3,
        z_index: 0,
        unicode_placeholder: 0,
        cursor_movement: 0,
    };

    term.place_graphic(placement);

    let final_cursor_row = term.grid.cursor.pos.row.0;

    // With fix: cursor stays ON last row of image (row 2)
    assert_eq!(
        final_cursor_row, 2,
        "Cursor should be at row 2 (last row of image) after placing a 3-row image, but got row {}. \
         Delta from start: {} (expected: 2)",
        final_cursor_row,
        final_cursor_row - initial_cursor_row
    );
}

#[test]
fn test_image_row_occupation_from_middle() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Move cursor to row 5
    term.grid.cursor.pos.row.0 = 5;
    let initial_cursor_row = term.grid.cursor.pos.row.0;
    assert_eq!(initial_cursor_row, 5);

    // Set proper cell dimensions for testing
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    let pixels = vec![255u8; 100 * 100 * 4];
    let graphic = GraphicData {
        id: GraphicId(4),
        width: 100,
        height: 100,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: Some(ResizeCommand {
            width: ResizeParameter::Auto,
            height: ResizeParameter::Cells(2),
            preserve_aspect_ratio: true,
        }),
    };

    term.store_graphic(graphic);

    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 4,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 2,
        z_index: 0,
        unicode_placeholder: 0,
        cursor_movement: 0,
    };

    term.place_graphic(placement);

    let final_cursor_row = term.grid.cursor.pos.row.0;

    // With fix: cursor stays ON last row of image (row 6)
    assert_eq!(
        final_cursor_row, 6,
        "Cursor should be at row 6 (last row of image) after placing a 2-row image from row 5, but got row {}",
        final_cursor_row
    );
}

// Delete Tests

#[test]
fn test_delete_all() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
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

// Placement Management Tests

#[test]
fn test_store_graphic() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
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
fn test_place_nonexistent_graphic() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    let placement = kitty_graphics_protocol::PlacementRequest {
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
        cursor_movement: 0,
    };

    // Should not panic, just warn
    term.place_graphic(placement);
}
