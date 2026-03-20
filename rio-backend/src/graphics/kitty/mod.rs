// Kitty Graphics Protocol Tests
// Combined test suite for Kitty graphics functionality

use crate::ansi::kitty_graphics_protocol::{
    self, DeleteRequest, KittyGraphicsState, PlacementRequest,
};
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
        _kitty_image_id: Option<u32>,
        _z_index: i32,
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
    let mut state = KittyGraphicsState::default();

    // Parse kitty graphics directly through the protocol parser
    // 1x1 RGBA pixel (4 bytes) - base64 encoded [255, 0, 0, 255] (red pixel)
    let params = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,i=1".as_ref(),
        b"/wAA/w==".as_ref(),
    ];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0), None, 0);
        }
    }

    // Verify graphic was captured
    assert_eq!(handler.graphics.len(), 1, "Should capture one graphic");

    let graphic = &handler.graphics[0];
    assert_eq!(graphic.width, 1);
    assert_eq!(graphic.height, 1);
    assert_eq!(graphic.pixels.len(), 4); // 1x1x4 bytes (RGBA)
    assert_eq!(graphic.id.get(), 1);
}

#[test]
fn test_parse_png_format() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // 1x1 red PNG image, base64 encoded
    // This is a complete, valid PNG file
    let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

    // Parse with f=100 (PNG format)
    let params = vec![
        b"G".as_ref(),
        b"a=t,f=100,i=2".as_ref(),
        png_base64.as_bytes(),
    ];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0), None, 0);
        }
    }

    // Verify PNG was decoded and captured
    assert_eq!(handler.graphics.len(), 1, "Should capture one PNG graphic");

    let graphic = &handler.graphics[0];
    assert_eq!(graphic.width, 1, "PNG should be decoded to 1x1");
    assert_eq!(graphic.height, 1, "PNG should be decoded to 1x1");
    assert_eq!(graphic.id.get(), 2);
    // PNG should be decoded to RGBA pixels
    assert!(
        graphic.pixels.len() >= 4,
        "PNG should decode to at least 4 bytes (RGBA)"
    );
}

#[test]
fn test_png_transmit_and_display() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    // Set proper cell dimensions
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // 1x1 red PNG image
    let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

    // Test a=T (transmit and display) with PNG format
    let params = vec![
        b"G".as_ref(),
        b"a=T,f=100,r=1,C=0,i=10".as_ref(),
        png_base64.as_bytes(),
    ];

    let mut state = KittyGraphicsState::default();
    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            if let Some(placement) = response.placement_request {
                // Store and place the graphic
                term.store_graphic(graphic_data.clone());
                term.place_graphic(placement);
            } else {
                // Direct display without placement request
                term.insert_graphic(graphic_data, None, Some(0), None, 0);
            }
        }
    }

    let final_row = term.grid.cursor.pos.row.0;

    // For 1-row PNG, cursor should stay on row 0 (last row of image)
    assert_eq!(
        final_row, 0,
        "PNG with r=1 should place cursor on row 0, got row {}",
        final_row
    );
}

#[test]
fn test_png_format_support() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // Test f=100 (PNG format) with a 1x1 PNG
    let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

    let params = vec![
        b"G".as_ref(),
        b"a=t,f=100,i=100".as_ref(),
        png_base64.as_bytes(),
    ];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0), None, 0);

            let graphic = &handler.graphics[0];
            assert_eq!(graphic.width, 1, "PNG should decode to 1x1");
            assert_eq!(graphic.height, 1, "PNG should decode to 1x1");
            assert_eq!(graphic.id.get(), 100);
        } else {
            panic!("PNG failed to decode");
        }
    } else {
        panic!("PNG failed to parse");
    }
}

#[test]
fn test_placement_request() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

    // Parse placement request (a=p is Put action, x and y are source coordinates)
    let params = vec![b"G".as_ref(), b"a=p,i=1,x=5,y=10,c=3,r=2".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
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
    let mut state = KittyGraphicsState::default();

    // Parse delete request (a=d is Delete action, d=a means delete all)
    let params = vec![b"G".as_ref(), b"a=d,d=a".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
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
    let mut state = KittyGraphicsState::default();

    // Parse query request
    let params = vec![b"G".as_ref(), b"a=q,i=1".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params, &mut state) {
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
    let mut state = KittyGraphicsState::default();

    // Total base64 for 1x1 RGBA pixel [255, 0, 0, 255] is "/wAA/w=="
    // Split into 3 chunks: "/wA", "A/", "w=="

    // Send first chunk (m=1 means more chunks coming)
    let params1 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,m=1,i=100".as_ref(),
        b"/wA".as_ref(),
    ];
    let result1 = kitty_graphics_protocol::parse(&params1, &mut state);
    assert!(result1.is_none());

    // Send second chunk
    let params2 = vec![b"G".as_ref(), b"a=t,m=1,i=100".as_ref(), b"A/".as_ref()];
    let result2 = kitty_graphics_protocol::parse(&params2, &mut state);
    assert!(result2.is_none());

    // Send final chunk with complete image info (m=0 means last chunk)
    let params3 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,m=0,i=100".as_ref(),
        b"w==".as_ref(),
    ];
    if let Some(response) = kitty_graphics_protocol::parse(&params3, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0), None, 0);
        }
    }

    // Now graphic should be created
    assert_eq!(handler.graphics.len(), 1);
    assert_eq!(handler.graphics[0].id.get(), 100);
    assert_eq!(handler.graphics[0].width, 1);
    assert_eq!(handler.graphics[0].height, 1);
}

#[test]
fn test_multiple_graphics_in_sequence() {
    let mut handler = TestHandler::default();
    let mut state = KittyGraphicsState::default();

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
        if let Some(response) = kitty_graphics_protocol::parse(params, &mut state) {
            if let Some(graphic_data) = response.graphic_data {
                handler.insert_graphic(graphic_data, None, Some(0), None, 0);
            }
        }
    }

    // Should have 3 graphics
    assert_eq!(handler.graphics.len(), 3);

    // Verify IDs
    assert_eq!(handler.graphics[0].id.get(), 1);
    assert_eq!(handler.graphics[1].id.get(), 2);
    assert_eq!(handler.graphics[2].id.get(), 3);
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
        id: GraphicId::new(1),
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
        display_width: None,
        display_height: None,
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

    // Set proper cell dimensions for testing
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Start at a specific position
    term.grid.cursor.pos.row.0 = 5;
    term.grid.cursor.pos.col.0 = 10;

    // Create a 100x100 pixel image
    let pixels = vec![255u8; 100 * 100 * 4];
    let graphic = GraphicData {
        id: GraphicId::new(2),
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
        display_width: None,
        display_height: None,
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
    let mut state = KittyGraphicsState::default();

    // Test that C=0 is parsed
    let result = kitty_graphics_protocol::parse(&[b"G", b"a=p,i=1,C=0", b""], &mut state);
    assert!(result.is_some());
    let response = result.unwrap();
    assert!(response.placement_request.is_some());
    let placement = response.placement_request.unwrap();
    assert_eq!(
        placement.cursor_movement, 0,
        "C=0 should parse as cursor_movement=0"
    );

    // Test that C=1 is parsed
    let result = kitty_graphics_protocol::parse(&[b"G", b"a=p,i=1,C=1", b""], &mut state);
    assert!(result.is_some());
    let response = result.unwrap();
    assert!(response.placement_request.is_some());
    let placement = response.placement_request.unwrap();
    assert_eq!(
        placement.cursor_movement, 1,
        "C=1 should parse as cursor_movement=1"
    );

    // Test default (no C key)
    let result = kitty_graphics_protocol::parse(&[b"G", b"a=p,i=1", b""], &mut state);
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
        id: GraphicId::new(1),
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
        display_width: None,
        display_height: None,
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

    // Set proper cell dimensions for testing
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    let _initial_cursor_row = term.grid.cursor.pos.row.0;

    // Create a small image that fits in 1 row
    let pixels = vec![255u8; 50 * 20 * 4];
    let graphic = GraphicData {
        id: GraphicId::new(2),
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
        display_width: None,
        display_height: None,
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
        id: GraphicId::new(3),
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
        display_width: None,
        display_height: None,
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
        id: GraphicId::new(4),
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
        display_width: None,
        display_height: None,
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
        id: GraphicId::new(100),
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
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

#[test]
fn test_delete_by_z_index_only_deletes_matching() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Insert a graphic with z_index=5
    let pixels = vec![255u8; 10 * 20 * 4]; // 1 cell
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 10,
        height: 20,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
    };
    term.insert_graphic(graphic, None, Some(0), Some(1), 5);

    // Insert another graphic with z_index=10
    term.grid.cursor.pos.col = crate::crosswords::pos::Column(1);
    term.grid.cursor.pos.row = crate::crosswords::pos::Line(0);
    let pixels2 = vec![255u8; 10 * 20 * 4];
    let graphic2 = GraphicData {
        id: GraphicId::new(2),
        width: 10,
        height: 20,
        color_type: ColorType::Rgba,
        pixels: pixels2,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
    };
    term.insert_graphic(graphic2, None, Some(0), Some(2), 10);

    // Delete by z_index=5 — should only remove the first graphic
    let delete = DeleteRequest {
        action: b'z',
        image_id: 0,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 5,
        delete_data: false,
    };
    term.delete_graphics(delete);

    // Cell at col 0 should have no graphics (z=5 was deleted)
    let cell0 = &term.grid[crate::crosswords::pos::Line(0)]
        [crate::crosswords::pos::Column(0)];
    assert!(
        cell0.graphics().is_none(),
        "z=5 graphic should have been deleted"
    );

    // Cell at col 1 should still have graphics (z=10 was not deleted)
    let cell1 = &term.grid[crate::crosswords::pos::Line(0)]
        [crate::crosswords::pos::Column(1)];
    assert!(
        cell1.graphics().is_some(),
        "z=10 graphic should NOT have been deleted"
    );
}

#[test]
fn test_delete_by_kitty_image_id() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Insert graphic with kitty_image_id=42
    let pixels = vec![255u8; 10 * 20 * 4];
    let graphic = GraphicData {
        id: GraphicId::new(42),
        width: 10,
        height: 20,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
    };
    // insert_graphic assigns a NEW internal GraphicId (via next_id()),
    // but we pass kitty_image_id=42 so delete-by-id can find it.
    term.insert_graphic(graphic, None, Some(0), Some(42), 0);

    // Verify it was placed
    let cell = &term.grid[crate::crosswords::pos::Line(0)]
        [crate::crosswords::pos::Column(0)];
    assert!(cell.graphics().is_some(), "Graphic should be placed");

    // The internal GraphicId should NOT be 42 (it's assigned by next_id)
    let internal_id = cell.graphics().unwrap()[0].texture.id;
    assert_ne!(
        internal_id.get(),
        42,
        "Internal ID should differ from kitty image_id"
    );

    // But kitty_image_id should be 42
    assert_eq!(
        cell.graphics().unwrap()[0].texture.kitty_image_id,
        Some(42),
        "kitty_image_id should be stored in TextureRef"
    );

    // Delete by image_id=42 (d=i)
    let delete = DeleteRequest {
        action: b'i',
        image_id: 42,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: false,
    };
    term.delete_graphics(delete);

    // Graphic should be gone
    let cell = &term.grid[crate::crosswords::pos::Line(0)]
        [crate::crosswords::pos::Column(0)];
    assert!(
        cell.graphics().is_none(),
        "Delete by image_id should remove the placed graphic"
    );
}

#[test]
fn test_delete_by_image_id_does_not_delete_wrong_id() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Insert graphic with kitty_image_id=42
    let pixels = vec![255u8; 10 * 20 * 4];
    let graphic = GraphicData {
        id: GraphicId::new(42),
        width: 10,
        height: 20,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
    };
    term.insert_graphic(graphic, None, Some(0), Some(42), 0);

    // Try to delete image_id=99 — should NOT delete the image_id=42 graphic
    let delete = DeleteRequest {
        action: b'i',
        image_id: 99,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: false,
    };
    term.delete_graphics(delete);

    let cell = &term.grid[crate::crosswords::pos::Line(0)]
        [crate::crosswords::pos::Column(0)];
    assert!(
        cell.graphics().is_some(),
        "Delete with wrong image_id should NOT remove the graphic"
    );
}

#[test]
fn test_no_double_push_on_graphic_cell_drop() {
    use crate::ansi::graphics::{GraphicCell, TextureRef};
    use std::sync::{Arc, Weak};
    use parking_lot::Mutex;

    let texture_ops: Arc<Mutex<Vec<GraphicId>>> = Arc::new(Mutex::new(Vec::new()));

    let texture = Arc::new(TextureRef {
        id: GraphicId::new(99),
        kitty_image_id: None,
        z_index: 0,
        width: 10,
        height: 20,
        cell_height: 20,
        texture_operations: Arc::downgrade(&texture_ops),
    });

    // Create two GraphicCells referencing the same texture (simulating multi-cell image)
    let cell1 = GraphicCell {
        texture: texture.clone(),
        offset_x: 0,
        offset_y: 0,
    };
    let cell2 = GraphicCell {
        texture: texture.clone(),
        offset_x: 10,
        offset_y: 0,
    };

    // Drop both cells — should NOT push to texture_operations (GraphicCell has no Drop impl)
    drop(cell1);
    drop(cell2);
    assert!(
        texture_ops.lock().is_empty(),
        "GraphicCell drop should NOT push to texture_operations"
    );

    // Drop the last Arc<TextureRef> — should push exactly once
    drop(texture);
    let ops = texture_ops.lock();
    assert_eq!(
        ops.len(),
        1,
        "TextureRef drop should push exactly once, got {}",
        ops.len()
    );
    assert_eq!(ops[0], GraphicId::new(99));
}

#[test]
fn test_placed_textures_tracks_inserts() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
    );

    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    assert!(
        term.graphics.placed_textures.is_empty(),
        "Should start with no placed textures"
    );

    // Insert a graphic
    let pixels = vec![255u8; 10 * 20 * 4];
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 10,
        height: 20,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
    };
    term.insert_graphic(graphic, None, Some(0), None, 0);

    assert_eq!(
        term.graphics.placed_textures.len(),
        1,
        "Should track 1 placed texture after insert"
    );
}

#[test]
fn test_collect_active_ids_uses_weak_refs() {
    use crate::ansi::graphics::TextureRef;
    use std::sync::Arc;

    let mut graphics = crate::ansi::graphics::Graphics::default();

    // Simulate placing a texture
    let texture_ops = graphics.texture_operations.clone();
    let texture = Arc::new(TextureRef {
        id: GraphicId::new(1),
        kitty_image_id: None,
        z_index: 0,
        width: 10,
        height: 20,
        cell_height: 20,
        texture_operations: Arc::downgrade(&texture_ops),
    });
    graphics.register_placed_texture(GraphicId::new(1), Arc::downgrade(&texture));

    // While texture is alive, it should appear in active IDs
    let active = graphics.collect_active_graphic_ids();
    assert!(
        active.contains(&1),
        "Active texture should appear in collect_active_graphic_ids"
    );

    // Drop the texture — weak ref becomes dead
    drop(texture);

    // Now it should be cleaned up
    let active = graphics.collect_active_graphic_ids();
    assert!(
        !active.contains(&1),
        "Dropped texture should NOT appear in collect_active_graphic_ids"
    );
    assert!(
        graphics.placed_textures.is_empty(),
        "Stale entry should be cleaned up"
    );
}
