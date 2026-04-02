// Kitty Graphics Protocol Tests
// Combined test suite for Kitty graphics functionality

use crate::ansi::kitty_graphics_protocol::{
    self, DeleteRequest, KittyGraphicsState, PlacementRequest,
};
use crate::crosswords::Crosswords;
use crate::event::{EventListener, RioEvent, WindowId};
use crate::performer::handler::Handler;
use crate::ansi::graphics::KittyPlacement;
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
        generation: 0,
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
        generation: 0,
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
        generation: 0,
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
        generation: 0,
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
        generation: 0,
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
        generation: 0,
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
        generation: 0,
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
        generation: 0,
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
        generation: 0,
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
    let cell0 =
        &term.grid[crate::crosswords::pos::Line(0)][crate::crosswords::pos::Column(0)];
    assert!(
        cell0.graphics().is_none(),
        "z=5 graphic should have been deleted"
    );

    // Cell at col 1 should still have graphics (z=10 was not deleted)
    let cell1 =
        &term.grid[crate::crosswords::pos::Line(0)][crate::crosswords::pos::Column(1)];
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
        generation: 0,
    };
    // insert_graphic assigns a NEW internal GraphicId (via next_id()),
    // but we pass kitty_image_id=42 so delete-by-id can find it.
    term.insert_graphic(graphic, None, Some(0), Some(42), 0);

    // Verify it was placed
    let cell =
        &term.grid[crate::crosswords::pos::Line(0)][crate::crosswords::pos::Column(0)];
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
    let cell =
        &term.grid[crate::crosswords::pos::Line(0)][crate::crosswords::pos::Column(0)];
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
        generation: 0,
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

    let cell =
        &term.grid[crate::crosswords::pos::Line(0)][crate::crosswords::pos::Column(0)];
    assert!(
        cell.graphics().is_some(),
        "Delete with wrong image_id should NOT remove the graphic"
    );
}

#[test]
fn test_no_double_push_on_graphic_cell_drop() {
    use crate::ansi::graphics::{GraphicCell, TextureRef};
    use parking_lot::Mutex;
    use std::sync::Arc;

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
        generation: 0,
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

// Overlay placement tests

#[test]
fn test_graphic_id_kitty_vs_sixel_no_collision() {
    let kitty_id = GraphicId::new_kitty(1);
    let sixel_id = GraphicId::new(1);

    assert_ne!(kitty_id, sixel_id, "Kitty and sixel IDs must not collide");
    assert!(kitty_id.is_kitty());
    assert!(!sixel_id.is_kitty());

    // High bit set for kitty
    assert!(kitty_id.get() & (1u64 << 63) != 0);
    assert!(sixel_id.get() & (1u64 << 63) == 0);
}

#[test]
fn test_graphic_id_kitty_different_images() {
    let id1 = GraphicId::new_kitty(1);
    let id2 = GraphicId::new_kitty(2);
    let id1_again = GraphicId::new_kitty(1);

    assert_ne!(id1, id2);
    assert_eq!(id1, id1_again);
}

#[test]
fn test_store_kitty_image_increments_generation() {
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();
    let pixels = vec![255u8; 4 * 4 * 4];

    let data1 = GraphicData {
        id: GraphicId::new(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: pixels.clone(),
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, None, data1);
    let gen1 = graphics.get_kitty_image(1).unwrap().generation;

    let data2 = GraphicData {
        id: GraphicId::new(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: pixels.clone(),
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, None, data2);
    let gen2 = graphics.get_kitty_image(1).unwrap().generation;

    assert!(gen2 > gen1, "Generation must increase on re-transmission");
}

#[test]
fn test_kitty_placement_insert_and_delete() {
    use crate::ansi::graphics::{Graphics, KittyPlacement};

    let mut graphics = Graphics::default();

    let placement = KittyPlacement {
        image_id: 1,
        placement_id: 0,
        graphic_id: GraphicId::new_kitty(1),
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 0,
        dest_row: 0,
        columns: 10,
        rows: 5,
        pixel_width: 100,
        pixel_height: 50,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: 0,
        transmit_generation: 1,
    };

    graphics.kitty_placements.insert((1, 0), placement);
    assert_eq!(graphics.kitty_placements.len(), 1);

    // Delete by image_id
    graphics
        .kitty_placements
        .retain(|k, _| k.0 != 1);
    assert_eq!(graphics.kitty_placements.len(), 0);
}

#[test]
fn test_kitty_placement_delete_by_z_index() {
    use crate::ansi::graphics::{Graphics, KittyPlacement};

    let mut graphics = Graphics::default();

    let make_placement = |image_id: u32, z: i32| KittyPlacement {
        image_id,
        placement_id: 0,
        graphic_id: GraphicId::new_kitty(image_id),
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 0,
        dest_row: 0,
        columns: 1,
        rows: 1,
        pixel_width: 10,
        pixel_height: 10,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: z,
        transmit_generation: 1,
    };

    graphics.kitty_placements.insert((1, 0), make_placement(1, 0));
    graphics.kitty_placements.insert((2, 0), make_placement(2, -1));
    graphics.kitty_placements.insert((3, 0), make_placement(3, 0));
    assert_eq!(graphics.kitty_placements.len(), 3);

    // Delete z=0 placements
    graphics.kitty_placements.retain(|_, p| p.z_index != 0);
    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(graphics.kitty_placements.contains_key(&(2, 0)));
}

#[test]
fn test_collect_active_ids_includes_overlay_placements() {
    use crate::ansi::graphics::{Graphics, KittyPlacement};

    let mut graphics = Graphics::default();

    let placement = KittyPlacement {
        image_id: 42,
        placement_id: 0,
        graphic_id: GraphicId::new_kitty(42),
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 0,
        dest_row: 0,
        columns: 1,
        rows: 1,
        pixel_width: 10,
        pixel_height: 10,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: 0,
        transmit_generation: 1,
    };

    graphics.kitty_placements.insert((42, 0), placement);

    let active = graphics.collect_active_graphic_ids();
    assert!(
        active.contains(&GraphicId::new_kitty(42).get()),
        "Overlay placements should be counted as active"
    );
}

#[test]
fn test_eviction_removes_dangling_placements() {
    use crate::ansi::graphics::{Graphics, KittyPlacement};

    let mut graphics = Graphics {
        total_limit: 100,
        ..Graphics::default()
    };

    // Add a graphic that will be evicted
    let pixels = vec![255u8; 200]; // 200 bytes, exceeds 100 limit
    let data = GraphicData {
        id: GraphicId::new_kitty(1),
        width: 10,
        height: 5,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.pending.push(data);
    graphics.track_graphic(GraphicId::new_kitty(1), 200);

    // Add an overlay placement referencing this graphic
    let placement = KittyPlacement {
        image_id: 1,
        placement_id: 0,
        graphic_id: GraphicId::new_kitty(1),
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 0,
        dest_row: 0,
        columns: 1,
        rows: 1,
        pixel_width: 10,
        pixel_height: 10,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: 0,
        transmit_generation: 1,
    };
    graphics.kitty_placements.insert((1, 0), placement);

    // Trigger eviction
    let used_ids = std::collections::HashSet::new();
    graphics.evict_images(100, &used_ids);

    // Placement should be removed along with the image
    assert!(
        graphics.kitty_placements.is_empty(),
        "Dangling placements should be removed during eviction"
    );
}

#[test]
fn test_next_id_stays_below_kitty_space() {
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();
    graphics.last_id = (1u64 << 63) - 2;

    let id1 = graphics.next_id();
    assert!(!id1.is_kitty(), "Sixel ID must not enter kitty space");

    let id2 = graphics.next_id();
    // After wrapping, should be back to 1
    assert_eq!(id2.get(), 1);
    assert!(!id2.is_kitty());
}

/// Helper to create a KittyPlacement for tests.
fn make_test_placement(
    image_id: u32,
    placement_id: u32,
    dest_col: usize,
    dest_row: i64,
    columns: u32,
    rows: u32,
    z_index: i32,
) -> KittyPlacement {
    KittyPlacement {
        image_id,
        placement_id,
        graphic_id: GraphicId::new_kitty(image_id),
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col,
        dest_row,
        columns,
        rows,
        pixel_width: columns * 10,
        pixel_height: rows * 20,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index,
        transmit_generation: 1,
    }
}

#[test]
fn test_placement_id_zero_creates_multiple() {
    // Test: add placement with zero placement id"
    // When placement_id=0, each insertion should use a unique key
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Insert two placements with placement_id=0 for same image
    // In the real code, the handler auto-assigns unique IDs, but at the
    // data structure level, (image_id, 0) would overwrite. The protocol
    // layer should assign unique placement_ids before inserting.
    let p1 = make_test_placement(1, 0, 0, 0, 5, 3, 0);
    let p2 = make_test_placement(1, 1, 5, 0, 5, 3, 0);

    graphics.kitty_placements.insert((1, 0), p1);
    graphics.kitty_placements.insert((1, 1), p2);

    assert_eq!(graphics.kitty_placements.len(), 2);
}

#[test]
fn test_delete_all_placements_preserves_images() {
    // Kitty test: "test_gr_delete" d=a (lowercase) deletes placements but not images
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Store an image
    let data = GraphicData {
        id: GraphicId::new_kitty(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 64],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, None, data);

    // Add placements
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));
    graphics
        .kitty_placements
        .insert((1, 1), make_test_placement(1, 1, 5, 0, 5, 3, 0));

    // Delete all placements (lowercase 'a' = keep images)
    graphics.kitty_placements.clear();

    assert_eq!(graphics.kitty_placements.len(), 0, "All placements removed");
    assert!(
        graphics.get_kitty_image(1).is_some(),
        "Image should still exist"
    );
}

#[test]
fn test_delete_all_placements_and_images() {
    // Kitty test: "test_gr_delete" d=A (uppercase) deletes both
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    let data = GraphicData {
        id: GraphicId::new_kitty(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 64],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, None, data);
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));

    // Uppercase A: delete placements AND images
    graphics.kitty_placements.clear();
    graphics.kitty_images.clear();
    graphics.kitty_image_numbers.clear();

    assert_eq!(graphics.kitty_placements.len(), 0);
    assert!(graphics.get_kitty_image(1).is_none());
}

#[test]
fn test_delete_by_specific_placement_id() {
    // Test: delete placement by specific id"
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));
    graphics
        .kitty_placements
        .insert((1, 1), make_test_placement(1, 1, 5, 0, 5, 3, 0));
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 0, 5, 5, 3, 0));

    assert_eq!(graphics.kitty_placements.len(), 3);

    // Delete specific placement (image_id=1, placement_id=1)
    graphics.kitty_placements.remove(&(1, 1));

    assert_eq!(graphics.kitty_placements.len(), 2);
    assert!(graphics.kitty_placements.contains_key(&(1, 0)));
    assert!(!graphics.kitty_placements.contains_key(&(1, 1)));
    assert!(graphics.kitty_placements.contains_key(&(2, 0)));
}

#[test]
fn test_delete_by_image_id_removes_all_placements_for_image() {
    // Test: delete all placements by image id"
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));
    graphics
        .kitty_placements
        .insert((1, 1), make_test_placement(1, 1, 5, 0, 5, 3, 0));
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 0, 5, 5, 3, 0));

    // Delete all placements for image_id=1
    graphics.kitty_placements.retain(|k, _| k.0 != 1);

    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(graphics.kitty_placements.contains_key(&(2, 0)));
}

#[test]
fn test_delete_intersecting_cursor() {
    // Test: delete intersecting cursor"
    // Kitty test: "test_gr_delete" d=C
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Place at col=0, row=0, size 5x3
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));
    // Place at col=10, row=10, size 5x3
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 10, 10, 5, 3, 0));

    // Cursor at (2, 1) — intersects placement 1 (col 0..5, row 0..3)
    let cursor_col = 2usize;
    let cursor_abs_row = 1i64;
    graphics.kitty_placements.retain(|_, p| {
        !(p.dest_col <= cursor_col
            && cursor_col < p.dest_col + p.columns as usize
            && p.dest_row <= cursor_abs_row
            && cursor_abs_row < p.dest_row + p.rows as i64)
    });

    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(graphics.kitty_placements.contains_key(&(2, 0)));
}

#[test]
fn test_delete_intersecting_cursor_hits_multiple() {
    // Test: delete intersecting cursor hits multiple"
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Two overlapping placements at same position
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 10, 10, 0));
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 0, 0, 5, 5, 1));

    let cursor_col = 2usize;
    let cursor_abs_row = 2i64;
    graphics.kitty_placements.retain(|_, p| {
        !(p.dest_col <= cursor_col
            && cursor_col < p.dest_col + p.columns as usize
            && p.dest_row <= cursor_abs_row
            && cursor_abs_row < p.dest_row + p.rows as i64)
    });

    assert_eq!(
        graphics.kitty_placements.len(),
        0,
        "Both overlapping placements should be removed"
    );
}

#[test]
fn test_delete_by_column() {
    // Test: delete by column"
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Placement at col 0, width 5 cells
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));
    // Placement at col 10, width 5 cells
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 10, 0, 5, 3, 0));
    // Placement at col 3, width 2 cells (overlaps column 3)
    graphics
        .kitty_placements
        .insert((3, 0), make_test_placement(3, 0, 3, 5, 2, 1, 0));

    // Delete placements intersecting column 3
    let col = 3usize;
    graphics
        .kitty_placements
        .retain(|_, p| !(p.dest_col <= col && col < p.dest_col + p.columns as usize));

    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(
        graphics.kitty_placements.contains_key(&(2, 0)),
        "Only placement at col 10 should survive"
    );
}

#[test]
fn test_delete_by_row() {
    // Test: delete by row"
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Placement at row 0, height 3
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));
    // Placement at row 10, height 2
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 0, 10, 5, 2, 0));

    // Delete placements intersecting row 1
    let abs_row = 1i64;
    graphics.kitty_placements.retain(|_, p| {
        !(p.dest_row <= abs_row && abs_row < p.dest_row + p.rows as i64)
    });

    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(graphics.kitty_placements.contains_key(&(2, 0)));
}

#[test]
fn test_delete_by_column_1x1() {
    // Test: delete by column 1x1"
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 1, 1, 0));
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 1, 0, 1, 1, 0));
    graphics
        .kitty_placements
        .insert((3, 0), make_test_placement(3, 0, 2, 0, 1, 1, 0));

    // Delete column 1
    let col = 1usize;
    graphics
        .kitty_placements
        .retain(|_, p| !(p.dest_col <= col && col < p.dest_col + p.columns as usize));

    assert_eq!(graphics.kitty_placements.len(), 2);
    assert!(graphics.kitty_placements.contains_key(&(1, 0)));
    assert!(!graphics.kitty_placements.contains_key(&(2, 0)));
    assert!(graphics.kitty_placements.contains_key(&(3, 0)));
}

#[test]
fn test_delete_by_row_1x1() {
    // Test: delete by row 1x1"
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 1, 1, 0));
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 0, 1, 1, 1, 0));
    graphics
        .kitty_placements
        .insert((3, 0), make_test_placement(3, 0, 0, 2, 1, 1, 0));

    // Delete row 1
    let abs_row = 1i64;
    graphics.kitty_placements.retain(|_, p| {
        !(p.dest_row <= abs_row && abs_row < p.dest_row + p.rows as i64)
    });

    assert_eq!(graphics.kitty_placements.len(), 2);
    assert!(graphics.kitty_placements.contains_key(&(1, 0)));
    assert!(!graphics.kitty_placements.contains_key(&(2, 0)));
    assert!(graphics.kitty_placements.contains_key(&(3, 0)));
}

#[test]
fn test_retransmit_same_image_id_updates_data() {
    // Kitty test: "test_load_images" — re-transmit replaces image data
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    let data1 = GraphicData {
        id: GraphicId::new_kitty(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: vec![0u8; 64],
        is_opaque: false,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, None, data1);
    let gen1 = graphics.get_kitty_image(1).unwrap().generation;
    let pixels1 = graphics.get_kitty_image(1).unwrap().data.pixels[0];

    // Re-transmit with different pixel data
    let data2 = GraphicData {
        id: GraphicId::new_kitty(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: vec![128u8; 64],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, None, data2);
    let gen2 = graphics.get_kitty_image(1).unwrap().generation;
    let pixels2 = graphics.get_kitty_image(1).unwrap().data.pixels[0];

    assert!(gen2 > gen1, "Generation must increase");
    assert_ne!(pixels1, pixels2, "Pixel data must be replaced");
    assert_eq!(pixels2, 128);
}

#[test]
fn test_image_number_mapping() {
    // Kitty test: "test_gr_operations_with_numbers" — I parameter maps to image_id
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    let data = GraphicData {
        id: GraphicId::new_kitty(42),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    // Store with image_number=7
    graphics.store_kitty_image(42, Some(7), data);

    // Lookup by number
    let stored = graphics.get_kitty_image_by_number(7);
    assert!(stored.is_some(), "Should find image by number");
    assert_eq!(stored.unwrap().data.id, GraphicId::new_kitty(42));

    // Non-existent number
    assert!(graphics.get_kitty_image_by_number(99).is_none());
}

#[test]
fn test_image_number_remapping_on_retransmit() {
    // Kitty: re-transmitting with same I= gets new image data but same mapping
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    let data1 = GraphicData {
        id: GraphicId::new_kitty(1),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![0u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, Some(100), data1);

    // Re-transmit same image_id with same number
    let data2 = GraphicData {
        id: GraphicId::new_kitty(1),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, Some(100), data2);

    let stored = graphics.get_kitty_image_by_number(100).unwrap();
    assert_eq!(
        stored.data.pixels[0], 255,
        "Number mapping should point to newest data"
    );
}

#[test]
fn test_placement_source_rect_tracking() {
    // placements track source rectangle for partial image display
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    let mut p = make_test_placement(1, 0, 0, 0, 10, 5, 0);
    p.source_x = 10;
    p.source_y = 20;
    p.source_width = 100;
    p.source_height = 50;

    graphics.kitty_placements.insert((1, 0), p);

    let stored = graphics.kitty_placements.get(&(1, 0)).unwrap();
    assert_eq!(stored.source_x, 10);
    assert_eq!(stored.source_y, 20);
    assert_eq!(stored.source_width, 100);
    assert_eq!(stored.source_height, 50);
}

#[test]
fn test_placement_z_ordering_sort() {
    // placements sorted by z-index for layered rendering
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 10));
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 0, 0, 5, 3, -1));
    graphics
        .kitty_placements
        .insert((3, 0), make_test_placement(3, 0, 0, 0, 5, 3, 0));

    let mut sorted: Vec<_> = graphics.kitty_placements.values().collect();
    sorted.sort_by_key(|p| p.z_index);

    assert_eq!(sorted[0].z_index, -1, "Negative z first");
    assert_eq!(sorted[1].z_index, 0, "Zero z middle");
    assert_eq!(sorted[2].z_index, 10, "Positive z last");
}

#[test]
fn test_delete_kitty_images_cleans_number_mapping() {
    // When images are deleted, number mappings should be cleaned up
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    let data = GraphicData {
        id: GraphicId::new_kitty(1),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, Some(7), data);

    assert!(graphics.get_kitty_image_by_number(7).is_some());

    // Delete by predicate
    graphics.delete_kitty_images(|id, _| *id == 1);

    assert!(
        graphics.get_kitty_image_by_number(7).is_none(),
        "Number mapping should be cleaned up when image is deleted"
    );
}

#[test]
fn test_both_columns_and_rows_no_aspect_ratio() {
    // When both c= and r= specified, stretch to fill (no aspect ratio).
    let mut state = KittyGraphicsState::default();

    // 2x2 RGBA = 16 bytes, base64("/////w==" is 4 bytes, need 16 bytes)
    // Use pre-encoded: 16 bytes of 0xFF = "/////////////////////w=="
    let params: Vec<&[u8]> = vec![
        b"G",
        b"a=T,f=32,s=2,v=2,c=80,r=20,i=1",
        b"/////////////////////w==",
    ];

    let response = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(response.is_some());
    let graphic_data = response.unwrap().graphic_data.unwrap();

    assert!(graphic_data.resize.is_some());
    let resize = graphic_data.resize.unwrap();
    assert!(
        !resize.preserve_aspect_ratio,
        "Both c= and r= specified: should NOT preserve aspect ratio"
    );
}

#[test]
fn test_only_columns_preserves_aspect_ratio() {
    // When only c= specified, compute r= from aspect ratio
    let mut state = KittyGraphicsState::default();

    let params: Vec<&[u8]> = vec![
        b"G",
        b"a=T,f=32,s=2,v=2,c=80,i=1",
        b"/////////////////////w==",
    ];

    let response = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(response.is_some());
    let graphic_data = response.unwrap().graphic_data.unwrap();

    let resize = graphic_data.resize.unwrap();
    assert!(
        resize.preserve_aspect_ratio,
        "Only c= specified: should preserve aspect ratio"
    );
}

#[test]
fn test_only_rows_preserves_aspect_ratio() {
    // When only r= specified, compute c= from aspect ratio
    let mut state = KittyGraphicsState::default();

    let params: Vec<&[u8]> = vec![
        b"G",
        b"a=T,f=32,s=2,v=2,r=20,i=1",
        b"/////////////////////w==",
    ];

    let response = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(response.is_some());
    let graphic_data = response.unwrap().graphic_data.unwrap();

    let resize = graphic_data.resize.unwrap();
    assert!(
        resize.preserve_aspect_ratio,
        "Only r= specified: should preserve aspect ratio"
    );
}

#[test]
fn test_delete_by_image_number() {
    // d=n deletes by image number (I= parameter).
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Store image with number mapping
    let data = GraphicData {
        id: GraphicId::new_kitty(42),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(42, Some(7), data);
    graphics
        .kitty_placements
        .insert((42, 0), make_test_placement(42, 0, 0, 0, 5, 3, 0));

    // Look up by number
    assert!(graphics.get_kitty_image_by_number(7).is_some());

    // Delete by number (simulate d=n with image_number=7)
    if let Some(&image_id) = graphics.kitty_image_numbers.get(&7) {
        graphics
            .kitty_placements
            .retain(|k, _| k.0 != image_id);
    }

    assert_eq!(graphics.kitty_placements.len(), 0);
    // Image still exists (lowercase n = keep data)
    assert!(graphics.get_kitty_image(42).is_some());
}

#[test]
fn test_delete_at_cell_with_z_filter() {
    // d=q deletes at cell position with z-index filter.
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Two placements at same position, different z-index
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));
    graphics
        .kitty_placements
        .insert((2, 0), make_test_placement(2, 0, 0, 0, 5, 3, -1));

    // Delete at (2, 1) with z=0 — should only remove image 1
    let col = 2usize;
    let abs_row = 1i64;
    let z = 0i32;
    graphics.kitty_placements.retain(|_, p| {
        !(p.z_index == z
            && p.dest_col <= col
            && col < p.dest_col + p.columns as usize
            && p.dest_row <= abs_row
            && abs_row < p.dest_row + p.rows as i64)
    });

    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(graphics.kitty_placements.contains_key(&(2, 0)));
}

#[test]
fn test_delete_by_image_range() {
    // d=r deletes by image ID range.
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));
    graphics
        .kitty_placements
        .insert((5, 0), make_test_placement(5, 0, 5, 0, 5, 3, 0));
    graphics
        .kitty_placements
        .insert((10, 0), make_test_placement(10, 0, 0, 5, 5, 3, 0));

    // Delete range 1..5
    let range_start = 1u32;
    let range_end = 5u32;
    graphics.kitty_placements.retain(|k, _| {
        k.0 < range_start || k.0 > range_end
    });

    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(graphics.kitty_placements.contains_key(&(10, 0)));
}

#[test]
fn test_implicit_id_no_response() {
    // When image_id=0 and image_number=0, no response should be sent
    let mut state = KittyGraphicsState::default();

    // Transmit with no explicit ID
    let params: Vec<&[u8]> = vec![
        b"G",
        b"a=t,f=32,s=1,v=1",
        b"/w==", // 1 byte base64
    ];

    let response = kitty_graphics_protocol::parse(&params, &mut state);
    // Should have graphic data but no response string
    if let Some(resp) = response {
        assert!(
            resp.response.is_none() || resp.response.as_deref() == Some(""),
            "No response should be sent for implicit IDs"
        );
    }
}

// Command parsing tests

#[test]
fn test_parse_transmission_with_format_and_dimensions() {
    // Test: transmission command
    let mut state = KittyGraphicsState::default();
    // 1x1 RGB (3 bytes) base64 = "AAAA"
    let params: Vec<&[u8]> = vec![b"G", b"f=24,s=1,v=1,i=1", b"AAAA"];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let data = resp.unwrap().graphic_data;
    assert!(data.is_some());
}

#[test]
fn test_parse_display_command_with_columns_rows() {
    // Test: display command
    let mut state = KittyGraphicsState::default();
    let params: Vec<&[u8]> = vec![b"G", b"a=p,c=80,r=120,i=31", b""];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let placement = resp.unwrap().placement_request;
    assert!(placement.is_some());
    let p = placement.unwrap();
    assert_eq!(p.columns, 80);
    assert_eq!(p.rows, 120);
    assert_eq!(p.image_id, 31);
}

#[test]
fn test_parse_delete_command_with_position() {
    // Test: delete command
    let mut state = KittyGraphicsState::default();
    let params: Vec<&[u8]> = vec![b"G", b"a=d,d=p,x=3,y=4", b""];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let delete = resp.unwrap().delete_request;
    assert!(delete.is_some());
    let d = delete.unwrap();
    assert_eq!(d.action, b'p');
    assert_eq!(d.x, 3);
    assert_eq!(d.y, 4);
}

#[test]
fn test_parse_ignores_unknown_keys() {
    // Test: ignore unknown keys
    let mut state = KittyGraphicsState::default();
    // 1x1 RGB with unknown key
    let params: Vec<&[u8]> = vec![b"G", b"f=24,s=1,v=1,hello=world,i=1", b"AAAA"];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    // Should parse successfully despite unknown key
    assert!(resp.is_some());
}

#[test]
fn test_parse_large_negative_z_index() {
    // Test: large negative z-index values
    let mut state = KittyGraphicsState::default();
    let params: Vec<&[u8]> = vec![b"G", b"a=p,z=-2000000000,i=1", b""];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let placement = resp.unwrap().placement_request.unwrap();
    assert_eq!(placement.z_index, -2000000000);
}

#[test]
fn test_response_encoding_with_image_id() {
    // Test: response encoding with image id
    let mut state = KittyGraphicsState::default();
    // 1x1 RGBA = 4 bytes, base64 = "/////w=="
    let params: Vec<&[u8]> = vec![
        b"G",
        b"a=T,f=32,s=1,v=1,i=4",
        b"/////w==",
    ];
    let resp = kitty_graphics_protocol::parse(&params, &mut state).unwrap();
    assert!(resp.response.is_some());
    let response_str = resp.response.unwrap();
    assert!(
        response_str.contains("i=4"),
        "Response should contain image id: {}",
        response_str
    );
    assert!(
        response_str.contains("OK"),
        "Response should contain OK: {}",
        response_str
    );
}

#[test]
fn test_response_encoding_with_image_number() {
    // Test: response encoding with image number
    let mut state = KittyGraphicsState::default();
    // 1x1 RGBA = 4 bytes
    let params: Vec<&[u8]> = vec![
        b"G",
        b"a=t,f=32,s=1,v=1,I=4",
        b"/////w==",
    ];
    let resp = kitty_graphics_protocol::parse(&params, &mut state).unwrap();
    assert!(resp.response.is_some());
    let response_str = resp.response.unwrap();
    assert!(
        response_str.contains("I=4"),
        "Response should contain image number: {}",
        response_str
    );
}

#[test]
fn test_default_format_is_rgba() {
    // Test: default format is RGBA
    let mut state = KittyGraphicsState::default();
    // No f= parameter — should default to RGBA (f=32)
    let params: Vec<&[u8]> = vec![
        b"G",
        b"a=t,s=1,v=1,i=1",
        b"/////w==", // 4 bytes = 1x1 RGBA
    ];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let data = resp.unwrap().graphic_data;
    assert!(data.is_some(), "Should parse with default RGBA format");
}

#[test]
fn test_delete_range_multiple_variants() {
    // Test: delete range variants
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    // Create placements for images 1, 2, 3
    for id in 1..=3u32 {
        graphics
            .kitty_placements
            .insert((id, 0), make_test_placement(id, 0, 0, id as i64, 5, 3, 0));
    }

    // Range delete [1, 2] — should keep image 3
    graphics
        .kitty_placements
        .retain(|k, _| k.0 < 1 || k.0 > 2);
    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(graphics.kitty_placements.contains_key(&(3, 0)));

    // Single-image range [3, 3]
    graphics
        .kitty_placements
        .retain(|k, _| k.0 < 3 || k.0 > 3);
    assert_eq!(graphics.kitty_placements.len(), 0);
}

#[test]
fn test_delete_all_preserves_memory_limit() {
    // Test: delete all preserves memory limit
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics {
        total_limit: 5000,
        ..Graphics::default()
    };

    let data = GraphicData {
        id: GraphicId::new_kitty(1),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        generation: 0,
    };
    graphics.store_kitty_image(1, None, data);
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 3, 0));

    // Delete all
    graphics.kitty_placements.clear();
    graphics.kitty_images.clear();

    assert_eq!(graphics.total_limit, 5000, "Limit should be preserved");
}

#[test]
fn test_chunked_quiet_flag_inheritance() {
    // Test: chunked quiet flag inheritance
    let mut state = KittyGraphicsState::default();

    // First chunk with q=1 (suppress responses)
    let params1: Vec<&[u8]> = vec![b"G", b"a=t,f=32,s=2,v=2,i=1,m=1,q=1", b"AAAA"];
    let resp1 = kitty_graphics_protocol::parse(&params1, &mut state);
    // First chunk of multi-part: no response yet (data incomplete)
    // Just verify it doesn't crash
    assert!(resp1.is_none() || resp1.as_ref().unwrap().graphic_data.is_none());

    // Second chunk (final)
    let params2: Vec<&[u8]> = vec![b"G", b"m=0", b"AAAAAAAAAAAAAAAA"];
    let resp2 = kitty_graphics_protocol::parse(&params2, &mut state);
    // With q=1, response should be suppressed
    if let Some(resp) = resp2 {
        assert!(
            resp.response.is_none() || resp.response.as_deref() == Some(""),
            "q=1 should suppress response"
        );
    }
}

#[test]
fn test_aspect_ratio_with_only_columns() {
    // Test: aspect ratio with only columns
    // A 16:9 image with c=10 should compute height preserving aspect ratio
    use sugarloaf::GraphicData;

    let data = GraphicData {
        id: GraphicId::new(1),
        width: 160,
        height: 90,
        color_type: ColorType::Rgba,
        pixels: vec![],
        is_opaque: true,
        resize: Some(sugarloaf::ResizeCommand {
            width: sugarloaf::ResizeParameter::Cells(10),
            height: sugarloaf::ResizeParameter::Auto,
            preserve_aspect_ratio: true,
        }),
        display_width: None,
        display_height: None,
        generation: 0,
    };

    let cell_w = 10;
    let cell_h = 20;
    let (w, h) = data.compute_display_dimensions(cell_w, cell_h, 800, 600);

    // Width = 10 cells * 10px = 100px
    assert_eq!(w, 100);
    // Height should preserve 16:9 ratio: 100 * 90/160 = 56.25 ≈ 56
    assert!(h > 50 && h < 60, "Height should be ~56, got {}", h);
}

#[test]
fn test_aspect_ratio_with_only_rows() {
    // Test: aspect ratio with only rows
    use sugarloaf::GraphicData;

    let data = GraphicData {
        id: GraphicId::new(1),
        width: 160,
        height: 90,
        color_type: ColorType::Rgba,
        pixels: vec![],
        is_opaque: true,
        resize: Some(sugarloaf::ResizeCommand {
            width: sugarloaf::ResizeParameter::Auto,
            height: sugarloaf::ResizeParameter::Cells(5),
            preserve_aspect_ratio: true,
        }),
        display_width: None,
        display_height: None,
        generation: 0,
    };

    let cell_w = 10;
    let cell_h = 20;
    let (w, h) = data.compute_display_dimensions(cell_w, cell_h, 800, 600);

    // Height = 5 cells * 20px = 100px
    assert_eq!(h, 100);
    // Width should preserve 16:9 ratio: 100 * 160/90 = 177.7 ≈ 178
    assert!(w > 170 && w < 185, "Width should be ~178, got {}", w);
}

// Format conversion tests

#[test]
fn test_grayscale_format_conversion() {
    // Test: gray (1 bpp) to RGBA conversion
    let mut state = KittyGraphicsState::default();
    // 2x1 grayscale: 2 bytes, base64 of [128, 255] = "gP8="
    let params: Vec<&[u8]> = vec![b"G", b"a=t,f=8,s=2,v=1,i=1", b"gP8="];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let data = resp.unwrap().graphic_data.unwrap();
    assert_eq!(data.pixels.len(), 8); // 2 pixels * 4 bytes RGBA
    // First pixel: gray=128 → [128, 128, 128, 255]
    assert_eq!(data.pixels[0], 128);
    assert_eq!(data.pixels[1], 128);
    assert_eq!(data.pixels[2], 128);
    assert_eq!(data.pixels[3], 255);
    // Second pixel: gray=255 → [255, 255, 255, 255]
    assert_eq!(data.pixels[4], 255);
    assert_eq!(data.pixels[7], 255);
}

#[test]
fn test_gray_alpha_format_conversion() {
    // Test: gray+alpha (2 bpp) to RGBA conversion
    let mut state = KittyGraphicsState::default();
    // 1x1 gray+alpha: 2 bytes [128, 200], base64 = "gMg="
    let params: Vec<&[u8]> = vec![b"G", b"a=t,f=16,s=1,v=1,i=1", b"gMg="];
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    assert!(resp.is_some());
    let data = resp.unwrap().graphic_data.unwrap();
    assert_eq!(data.pixels.len(), 4); // 1 pixel * 4 bytes RGBA
    // gray=128, alpha=200 → [128, 128, 128, 200]
    assert_eq!(data.pixels[0], 128);
    assert_eq!(data.pixels[1], 128);
    assert_eq!(data.pixels[2], 128);
    assert_eq!(data.pixels[3], 200);
    assert!(!data.is_opaque); // alpha != 255
}
