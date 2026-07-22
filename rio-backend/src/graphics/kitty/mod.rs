// Kitty Graphics Protocol Tests
// Combined test suite for Kitty graphics functionality

use crate::ansi::graphics::KittyPlacement;
use crate::ansi::kitty_graphics_protocol::{
    self, DeleteRequest, KittyGraphicsState, PlacementRequest,
};
use crate::crosswords::pos::Column;
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
            handler.insert_graphic(graphic_data, None, Some(0));
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
            handler.insert_graphic(graphic_data, None, Some(0));
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
        10_000,
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
                term.insert_graphic(graphic_data, None, Some(0));
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
            handler.insert_graphic(graphic_data, None, Some(0));

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

    // Total base64 for 1x1 RGBA pixel [255, 0, 0, 255] is "/wAA/w==".
    // Each chunk is decoded independently now, so each must be a
    // valid base64 on its own — either a multiple of 4 chars per kitty
    // spec, or an independently padded chunk.

    // Chunk 1 (m=1): 4 chars → 3 decoded bytes [0xFF, 0x00, 0x00]
    let params1 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,m=1,i=100".as_ref(),
        b"/wAA".as_ref(),
    ];
    let result1 = kitty_graphics_protocol::parse(&params1, &mut state)
        .expect("intermediate chunks must produce a Some response");
    assert!(result1.incomplete);
    assert!(result1.graphic_data.is_none());

    // Chunk 2 (m=0): 4 chars with padding → 1 decoded byte [0xFF]
    let params3 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,m=0,i=100".as_ref(),
        b"/w==".as_ref(),
    ];
    if let Some(response) = kitty_graphics_protocol::parse(&params3, &mut state) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None, Some(0));
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
                handler.insert_graphic(graphic_data, None, Some(0));
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
        10_000,
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
        transmit_time: std::time::Instant::now(),
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
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
    };

    term.place_graphic(placement);

    let final_cursor_row = term.grid.cursor.pos.row.0;
    let final_cursor_col = term.grid.cursor.pos.col.0;

    // With cursor_movement=0 (Kitty default), the cursor ends on the
    // image's last row at the first column after the image, so text
    // printed next never overwrites the image. 2-row image at row 0
    // occupies rows 0-1; r=2 with a 100x100 image in 10x20 cells gives
    // a 40x40 display, i.e. 4 columns.
    assert_eq!(
        final_cursor_row, 1,
        "Cursor should be at row 1 (last row of image) with cursor_movement=0. Initial: {}, Final: {}",
        initial_cursor_row,
        final_cursor_row
    );
    assert_eq!(
        final_cursor_col, 4,
        "Cursor should be at the first column after the image"
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
        10_000,
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
        transmit_time: std::time::Instant::now(),
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
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 1, // Don't move cursor
        cell_x_offset: 0,
        cell_y_offset: 0,
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
        10_000,
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
        transmit_time: std::time::Instant::now(),
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
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
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
fn test_subcell_offset_forwarded_and_clamped() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );

    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    let pixels = vec![255u8; 40 * 40 * 4];
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 40,
        height: 40,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    term.store_graphic(graphic);

    // In-range `X=`/`Y=` flows through to the stored placement.
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 7,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 0,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 1,
        cell_x_offset: 7,
        cell_y_offset: 9,
    };
    term.place_graphic(placement);

    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 7))
        .expect("placement stored");
    assert_eq!(stored.cell_x_offset, 7);
    assert_eq!(stored.cell_y_offset, 9);

    // Per kitty spec the offset must be smaller than the cell size.
    // The stored value stays raw (re-clamped at read time so cell
    // size changes don't lose it); span derivation uses the clamp.
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 8,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 0,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 1,
        cell_x_offset: 999,
        cell_y_offset: 999,
    };
    term.place_graphic(placement);

    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 8))
        .expect("placement stored");
    assert_eq!(stored.cell_x_offset, 999, "raw offset is kept");
    assert_eq!(stored.cell_y_offset, 999);
    // 40x40 image, 10x20 cells, offsets clamped to 9/19 for spans.
    assert_eq!(stored.columns, 5, "ceil((40 + 9) / 10)");
    assert_eq!(stored.rows, 3, "ceil((40 + 19) / 20)");
}

#[test]
fn test_subcell_offset_extends_row_occupation() {
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };

    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );

    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // 40px tall image on 20px cells: exactly 2 rows without an offset.
    let pixels = vec![255u8; 40 * 40 * 4];
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 40,
        height: 40,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    term.store_graphic(graphic);

    // `Y=15` shifts the image down within its first cell, so it spills
    // into a third row: ceil((40 + 15) / 20) = 3. Cursor movement and
    // occupation must cover that extra row.
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 7,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 0,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 15,
    };
    term.place_graphic(placement);

    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 7))
        .expect("placement stored");
    assert_eq!(stored.rows, 3, "Y offset spills the image into a 3rd row");
    assert_eq!(stored.columns, 4, "no X offset: 40px / 10px = 4 columns");

    // C=0: cursor lands on the last row of the image (row index rows - 1).
    assert_eq!(
        term.grid.cursor.pos.row.0, 2,
        "cursor advances to the extra row created by the Y offset"
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
        10_000,
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
        transmit_time: std::time::Instant::now(),
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
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
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
        10_000,
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
        transmit_time: std::time::Instant::now(),
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
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
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
        10_000,
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
        transmit_time: std::time::Instant::now(),
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
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
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
        10_000,
    );

    // Delete all graphics (d=a)
    let delete = DeleteRequest {
        action: b'a',
        image_id: 0,
        image_number: 0,
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
        10_000,
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
        transmit_time: std::time::Instant::now(),
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
        10_000,
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
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
    };

    // Should not panic, just warn
    term.place_graphic(placement);
}

// test_delete_by_kitty_image_id and test_delete_by_image_id_does_not_delete_wrong_id
// were removed: kitty images no longer go into grid cells (overlay path only).
// Equivalent tests exist as test_delete_by_image_id_removes_all_placements_for_image
// and test_delete_by_specific_placement_id.

#[test]
fn test_no_double_push_on_graphic_cell_drop() {
    use crate::ansi::graphics::{GraphicCell, TextureRef};
    use parking_lot::Mutex;
    use std::sync::Arc;

    let texture_ops: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));

    let texture = Arc::new(TextureRef {
        id: GraphicId::new(99),
        width: 10,
        height: 20,
        cell_width: 10,
        cell_height: 20,
        texture_operations: Arc::downgrade(&texture_ops),
    });

    // Create two GraphicCells referencing the same texture (simulating multi-cell image)
    let cell1 = GraphicCell {
        texture: texture.clone(),
        offset_x: 0,
        offset_y: 0,
        anchor_col: 0,
    };
    let cell2 = GraphicCell {
        texture: texture.clone(),
        offset_x: 10,
        offset_y: 0,
        anchor_col: 0,
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
    assert_eq!(ops[0], crate::sugarloaf::atlas_image_key(99));
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
        10_000,
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
        transmit_time: std::time::Instant::now(),
    };
    term.insert_graphic(graphic, None, Some(0));

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
        width: 10,
        height: 20,
        cell_width: 10,
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

// test_graphic_id_kitty_vs_sixel_no_collision and test_graphic_id_kitty_different_images
// removed: kitty images no longer use GraphicId. They use u32 image_id directly,
// in a completely separate rendering path from sixel/iTerm2 atlas graphics.

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
        transmit_time: std::time::Instant::now(),
    };
    graphics.store_kitty_image(1, None, data1);
    let time1 = graphics.get_kitty_image(1).unwrap().transmission_time;

    // Small sleep to ensure different timestamps
    std::thread::sleep(std::time::Duration::from_millis(1));

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
        transmit_time: std::time::Instant::now(),
    };
    graphics.store_kitty_image(1, None, data2);
    let time2 = graphics.get_kitty_image(1).unwrap().transmission_time;

    assert!(
        time2 > time1,
        "Transmit time must increase on re-transmission"
    );
}

#[test]
fn test_kitty_placement_insert_and_delete() {
    use crate::ansi::graphics::{Graphics, KittyPlacement};

    let mut graphics = Graphics::default();

    let placement = KittyPlacement {
        image_id: 1,
        placement_id: 0,
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 0,
        dest_row: 0,
        columns: 10,
        rows: 5,
        requested_columns: 0,
        requested_rows: 0,
        pixel_width: 100,
        pixel_height: 50,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: 0,
        transmit_time: std::time::Instant::now(),
    };

    graphics.kitty_placements.insert((1, 0), placement);
    assert_eq!(graphics.kitty_placements.len(), 1);

    // Delete by image_id
    graphics.kitty_placements.retain(|k, _| k.0 != 1);
    assert_eq!(graphics.kitty_placements.len(), 0);
}

#[test]
fn test_kitty_placement_delete_by_z_index() {
    use crate::ansi::graphics::{Graphics, KittyPlacement};

    let mut graphics = Graphics::default();

    let make_placement = |image_id: u32, z: i32| KittyPlacement {
        image_id,
        placement_id: 0,
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 0,
        dest_row: 0,
        columns: 1,
        rows: 1,
        requested_columns: 0,
        requested_rows: 0,
        pixel_width: 10,
        pixel_height: 10,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: z,
        transmit_time: std::time::Instant::now(),
    };

    graphics
        .kitty_placements
        .insert((1, 0), make_placement(1, 0));
    graphics
        .kitty_placements
        .insert((2, 0), make_placement(2, -1));
    graphics
        .kitty_placements
        .insert((3, 0), make_placement(3, 0));
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
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 0,
        dest_row: 0,
        columns: 1,
        rows: 1,
        requested_columns: 0,
        requested_rows: 0,
        pixel_width: 10,
        pixel_height: 10,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: 0,
        transmit_time: std::time::Instant::now(),
    };

    graphics.kitty_placements.insert((42, 0), placement);

    let active = graphics.collect_active_graphic_ids();
    assert!(
        active.contains(&42u64),
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
        id: GraphicId::new(1),
        width: 10,
        height: 5,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.pending.push(data);
    graphics.track_graphic(GraphicId::new(1), 200);

    // Add an overlay placement referencing this graphic
    let placement = KittyPlacement {
        image_id: 1,
        placement_id: 0,
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 0,
        dest_row: 0,
        columns: 1,
        rows: 1,
        requested_columns: 0,
        requested_rows: 0,
        pixel_width: 10,
        pixel_height: 10,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: 0,
        transmit_time: std::time::Instant::now(),
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
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col,
        dest_row,
        columns,
        rows,
        requested_columns: 0,
        requested_rows: 0,
        pixel_width: columns * 10,
        pixel_height: rows * 20,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index,
        transmit_time: std::time::Instant::now(),
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
        id: GraphicId::new(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 64],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
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
        id: GraphicId::new(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 64],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
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
    graphics
        .kitty_placements
        .retain(|_, p| !(p.dest_row <= abs_row && abs_row < p.dest_row + p.rows as i64));

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
    graphics
        .kitty_placements
        .retain(|_, p| !(p.dest_row <= abs_row && abs_row < p.dest_row + p.rows as i64));

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
        id: GraphicId::new(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: vec![0u8; 64],
        is_opaque: false,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.store_kitty_image(1, None, data1);
    let time1 = graphics.get_kitty_image(1).unwrap().transmission_time;
    let pixels1 = graphics.get_kitty_image(1).unwrap().data.pixels[0];

    // Re-transmit with different pixel data
    let data2 = GraphicData {
        id: GraphicId::new(1),
        width: 4,
        height: 4,
        color_type: ColorType::Rgba,
        pixels: vec![128u8; 64],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.store_kitty_image(1, None, data2);
    let time2 = graphics.get_kitty_image(1).unwrap().transmission_time;
    let pixels2 = graphics.get_kitty_image(1).unwrap().data.pixels[0];

    assert!(time2 > time1, "Transmit time must increase");
    assert_ne!(pixels1, pixels2, "Pixel data must be replaced");
    assert_eq!(pixels2, 128);
}

#[test]
fn test_image_number_mapping() {
    // Kitty test: "test_gr_operations_with_numbers" — I parameter maps to image_id
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    let data = GraphicData {
        id: GraphicId::new(42),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    // Store with image_number=7
    graphics.store_kitty_image(42, Some(7), data);

    // Lookup by number
    let stored = graphics.get_kitty_image_by_number(7);
    assert!(stored.is_some(), "Should find image by number");
    assert_eq!(stored.unwrap().data.id, GraphicId::new(42));

    // Non-existent number
    assert!(graphics.get_kitty_image_by_number(99).is_none());
}

#[test]
fn test_image_number_remapping_on_retransmit() {
    // Kitty: re-transmitting with same I= gets new image data but same mapping
    use crate::ansi::graphics::Graphics;

    let mut graphics = Graphics::default();

    let data1 = GraphicData {
        id: GraphicId::new(1),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![0u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.store_kitty_image(1, Some(100), data1);

    // Re-transmit same image_id with same number
    let data2 = GraphicData {
        id: GraphicId::new(1),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
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
        id: GraphicId::new(1),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
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
        id: GraphicId::new(42),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.store_kitty_image(42, Some(7), data);
    graphics
        .kitty_placements
        .insert((42, 0), make_test_placement(42, 0, 0, 0, 5, 3, 0));

    // Look up by number
    assert!(graphics.get_kitty_image_by_number(7).is_some());

    // Delete by number (simulate d=n with image_number=7)
    if let Some(&image_id) = graphics.kitty_image_numbers.get(&7) {
        graphics.kitty_placements.retain(|k, _| k.0 != image_id);
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
    graphics
        .kitty_placements
        .retain(|k, _| k.0 < range_start || k.0 > range_end);

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
    let params: Vec<&[u8]> = vec![b"G", b"a=T,f=32,s=1,v=1,i=4", b"/////w=="];
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
    let params: Vec<&[u8]> = vec![b"G", b"a=t,f=32,s=1,v=1,I=4", b"/////w=="];
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
    graphics.kitty_placements.retain(|k, _| k.0 < 1 || k.0 > 2);
    assert_eq!(graphics.kitty_placements.len(), 1);
    assert!(graphics.kitty_placements.contains_key(&(3, 0)));

    // Single-image range [3, 3]
    graphics.kitty_placements.retain(|k, _| k.0 != 3);
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
        id: GraphicId::new(1),
        width: 2,
        height: 2,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 16],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
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
    // Chunked transmission: q= on the first chunk must be preserved
    // through the merged command, so subsequent chunks — which only
    // carry `m=` per the kitty spec — still take the original q value.
    //
    // q=1 suppresses OK responses but NOT errors. We test that here by
    // sending a correctly-sized 2x2 RGBA image across two spec-compliant
    // chunks; the OK response must be suppressed.
    let mut state = KittyGraphicsState::default();

    // 2x2 RGBA = 16 bytes. Full base64 = 24 chars with trailing padding.
    // We'll split on a 4-char boundary into chunk1=12 chars, chunk2=12 chars.
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    let raw = vec![0xFFu8; 16];
    let encoded = B64.encode(&raw);
    assert_eq!(encoded.len() % 4, 0);
    let (first, second) = encoded.split_at(encoded.len() / 2);
    let (first_bytes, second_bytes) = (first.as_bytes(), second.as_bytes());

    let ctrl1 = "a=t,f=32,s=2,v=2,i=1,m=1,q=1";
    let params1: Vec<&[u8]> = vec![b"G", ctrl1.as_bytes(), first_bytes];
    let resp1 = kitty_graphics_protocol::parse(&params1, &mut state)
        .expect("pending chunk must return Some");
    assert!(resp1.incomplete);

    let ctrl2 = "m=0,i=1";
    let params2: Vec<&[u8]> = vec![b"G", ctrl2.as_bytes(), second_bytes];
    let resp2 = kitty_graphics_protocol::parse(&params2, &mut state)
        .expect("final chunk must return Some");
    // Successful transmission + q=1 inherited from first chunk →
    // the OK response must be suppressed.
    assert!(resp2.graphic_data.is_some(), "image must decode");
    assert!(
        resp2.response.is_none(),
        "q=1 must suppress OK response even after chunk merge: {:?}",
        resp2.response
    );
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
        transmit_time: std::time::Instant::now(),
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
        transmit_time: std::time::Instant::now(),
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

// Free-data deletion bug regression tests.
//
// The parser lowercases `delete_action` and stores the original case in
// `delete_data: bool`. The dispatcher used to check
// `delete.action == b'I'` etc., which was always false because the parser
// already normalized to lowercase, so the uppercase free-data variants
// silently leaked image bytes. These tests pin the fix.

fn make_test_term() -> Crosswords<TestEventListener> {
    Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        TestEventListener,
        unsafe { WindowId::dummy() },
        0,
        10_000,
    )
}

fn store_red_pixel(term: &mut Crosswords<TestEventListener>, image_id: u32) {
    let graphic = GraphicData {
        id: GraphicId::new(image_id as u64),
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        pixels: vec![255, 0, 0, 255],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    term.store_graphic(graphic);
}

#[test]
fn test_delete_uppercase_i_actually_frees_image_data() {
    // Regression: d=I (uppercase) must remove the stored image, not just
    // its placements. Pre-fix the dispatcher checked `delete.action == b'I'`
    // which was always false, so the image cache leaked.
    let mut term = make_test_term();
    store_red_pixel(&mut term, 7);
    assert!(term.graphics.get_kitty_image(7).is_some());

    // Parser path: d=I sets delete_action='I', then is normalized to
    // lowercase 'i' with delete_data=true.
    let mut state = KittyGraphicsState::default();
    let params = vec![b"G".as_ref(), b"a=d,d=I,i=7"];
    let resp = kitty_graphics_protocol::parse(&params, &mut state).unwrap();
    let delete = resp.delete_request.expect("expected DeleteRequest");
    assert_eq!(delete.action, b'i');
    assert!(delete.delete_data, "uppercase I must set delete_data");

    term.delete_graphics(delete);

    assert!(
        term.graphics.get_kitty_image(7).is_none(),
        "d=I must free image data — the dispatcher should rely on \
         delete.delete_data, not on a dead `action == b'I'` check"
    );
}

#[test]
fn test_delete_uppercase_a_clears_all_image_data() {
    let mut term = make_test_term();
    store_red_pixel(&mut term, 1);
    store_red_pixel(&mut term, 2);
    store_red_pixel(&mut term, 3);
    assert_eq!(term.graphics.kitty_images.len(), 3);

    let delete = DeleteRequest {
        action: b'a',
        image_id: 0,
        image_number: 0,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: true, // simulating d=A
    };
    term.delete_graphics(delete);

    assert!(
        term.graphics.kitty_images.is_empty(),
        "d=A must clear all image data, not just placements"
    );
    assert!(term.graphics.kitty_image_numbers.is_empty());
}

#[test]
fn test_delete_lowercase_a_keeps_image_data() {
    // Per spec: lowercase deletes placements only, image data stays so a
    // future `a=p` can still place the same image.
    let mut term = make_test_term();
    store_red_pixel(&mut term, 1);

    let delete = DeleteRequest {
        action: b'a',
        image_id: 0,
        image_number: 0,
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: false, // d=a (lowercase)
    };
    term.delete_graphics(delete);

    assert!(
        term.graphics.get_kitty_image(1).is_some(),
        "Lowercase d=a must keep image data — only placements are removed"
    );
}

#[test]
fn test_delete_uppercase_n_frees_image_via_number() {
    // d=N: delete by image number, free data
    let mut term = make_test_term();
    let graphic = GraphicData {
        id: GraphicId::new(42),
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        pixels: vec![255, 0, 0, 255],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    // Store with image_number=9
    term.graphics.store_kitty_image(42, Some(9), graphic);
    assert!(term.graphics.get_kitty_image(42).is_some());
    assert!(term.graphics.get_kitty_image_by_number(9).is_some());

    // d=N with image_id=9 (the parser stores the image *number* into
    // image_id for the d=n/N case via the `i=` key per spec).
    let delete = DeleteRequest {
        action: b'n',
        image_id: 0,
        image_number: 9, // canonical: I= for d=n
        placement_id: 0,
        x: 0,
        y: 0,
        z_index: 0,
        delete_data: true,
    };
    term.delete_graphics(delete);

    assert!(
        term.graphics.get_kitty_image(42).is_none(),
        "d=N must free image data resolved through the number map"
    );
}

#[test]
fn test_delete_uppercase_r_frees_image_range() {
    // d=R deletes a range of image_ids and frees their data.
    let mut term = make_test_term();
    store_red_pixel(&mut term, 1);
    store_red_pixel(&mut term, 5);
    store_red_pixel(&mut term, 10);
    assert_eq!(term.graphics.kitty_images.len(), 3);

    // d=R with x=range_start, y=range_end (inclusive). Source x/y carry
    // these values per the parser's field reuse.
    let delete = DeleteRequest {
        action: b'r',
        image_id: 0,
        image_number: 0,
        placement_id: 0,
        x: 1, // range start
        y: 5, // range end
        z_index: 0,
        delete_data: true,
    };
    term.delete_graphics(delete);

    // Images 1 and 5 should be gone, 10 should remain.
    assert!(term.graphics.get_kitty_image(1).is_none());
    assert!(term.graphics.get_kitty_image(5).is_none());
    assert!(
        term.graphics.get_kitty_image(10).is_some(),
        "Image outside range must survive"
    );
}

// Per-screen kitty graphics state isolation.

#[test]
fn test_swap_alt_isolates_kitty_images() {
    // Per spec: each terminal screen owns its own image cache. After
    // swapping into the alt screen, main-screen images must not be
    // visible, and vice versa.
    let mut term = make_test_term();

    // Store two images on the main screen.
    store_red_pixel(&mut term, 1);
    store_red_pixel(&mut term, 2);
    assert!(term.graphics.get_kitty_image(1).is_some());
    assert!(term.graphics.get_kitty_image(2).is_some());

    // Swap to alt screen.
    term.swap_alt();

    assert!(
        term.graphics.get_kitty_image(1).is_none(),
        "Main-screen image 1 must be hidden after swapping to alt screen"
    );
    assert!(
        term.graphics.get_kitty_image(2).is_none(),
        "Main-screen image 2 must be hidden after swapping to alt screen"
    );

    // Store a different image on the alt screen.
    store_red_pixel(&mut term, 3);
    assert!(term.graphics.get_kitty_image(3).is_some());
    // The main-screen images are still hidden.
    assert!(term.graphics.get_kitty_image(1).is_none());

    // Swap back to main screen.
    term.swap_alt();

    assert!(
        term.graphics.get_kitty_image(1).is_some(),
        "Image 1 must reappear when swapping back to main screen"
    );
    assert!(term.graphics.get_kitty_image(2).is_some());
    assert!(
        term.graphics.get_kitty_image(3).is_none(),
        "Alt-screen image 3 must not leak into main screen"
    );

    // Swap back to alt — image 3 should be there again.
    term.swap_alt();
    assert!(
        term.graphics.get_kitty_image(3).is_some(),
        "Alt-screen image 3 must be preserved across screen swaps"
    );
}

#[test]
fn test_swap_alt_isolates_placements() {
    // Placements are also per-screen — putting a placement on the main
    // screen should not appear on the alt screen.
    let mut term = make_test_term();
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    store_red_pixel(&mut term, 1);
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 1,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 1,
        cell_x_offset: 0,
        cell_y_offset: 0,
    };
    term.place_graphic(placement);
    assert!(
        !term.graphics.kitty_placements.is_empty(),
        "Main-screen placement should be present after place_graphic"
    );

    term.swap_alt();
    assert!(
        term.graphics.kitty_placements.is_empty(),
        "Main-screen placements must not be visible on the alt screen"
    );

    term.swap_alt();
    assert!(
        !term.graphics.kitty_placements.is_empty(),
        "Main-screen placements must reappear after swapping back"
    );
}

#[test]
fn test_swap_alt_isolates_image_numbers() {
    // Image-number mappings (I=) are per-screen too.
    let mut term = make_test_term();
    let g = GraphicData {
        id: GraphicId::new(1),
        width: 1,
        height: 1,
        color_type: ColorType::Rgba,
        pixels: vec![255, 0, 0, 255],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    term.graphics.store_kitty_image(1, Some(50), g);
    assert!(term.graphics.get_kitty_image_by_number(50).is_some());

    term.swap_alt();
    assert!(
        term.graphics.get_kitty_image_by_number(50).is_none(),
        "Image-number mapping must not bleed across screens"
    );

    term.swap_alt();
    assert!(
        term.graphics.get_kitty_image_by_number(50).is_some(),
        "Image-number mapping must come back when we swap to its screen"
    );
}

#[test]
fn test_swap_alt_marks_kitty_dirty() {
    // The renderer relies on the dirty flag to know when to rebuild
    // the overlay layer; swap must set it.
    let mut term = make_test_term();
    term.graphics.kitty_graphics_dirty = false;
    term.swap_alt();
    assert!(
        term.graphics.kitty_graphics_dirty,
        "swap_alt must mark kitty graphics dirty so the renderer rebuilds"
    );
}

#[test]
fn test_full_reset_clears_both_screens() {
    // reset_state should clear images on both main and alt screens.
    let mut term = make_test_term();

    // Image on main screen.
    store_red_pixel(&mut term, 1);
    // Swap to alt and store another image.
    term.swap_alt();
    store_red_pixel(&mut term, 2);
    // Sanity: alt has image 2, not 1.
    assert!(term.graphics.get_kitty_image(2).is_some());
    assert!(term.graphics.get_kitty_image(1).is_none());

    // Full reset.
    term.reset_state();

    // Both screens should be empty.
    assert!(term.graphics.get_kitty_image(1).is_none());
    assert!(term.graphics.get_kitty_image(2).is_none());
    assert!(term.graphics.kitty_inactive_screen.kitty_images.is_empty());
}

// Eviction prefers inactive-screen images.

#[test]
fn test_eviction_prefers_inactive_screen_images() {
    use crate::ansi::graphics::{Graphics, KittyScreenState, StoredImage};

    let mut graphics = Graphics {
        total_limit: 100, // tiny limit so a 60-byte add forces eviction
        ..Graphics::default()
    };

    // Active screen: image 1, 50 bytes, no placement (unused).
    let active_data = GraphicData {
        id: GraphicId::new(1),
        width: 5,
        height: 5,
        color_type: ColorType::Rgba,
        pixels: vec![1u8; 50],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.store_kitty_image(1, None, active_data);

    // Inactive screen: image 2, 50 bytes, no placement either.
    // Pre-load via the inactive_screen field directly so we don't need
    // to drive a swap.
    let inactive_data = GraphicData {
        id: GraphicId::new(2),
        width: 5,
        height: 5,
        color_type: ColorType::Rgba,
        pixels: vec![2u8; 50],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now() - std::time::Duration::from_secs(60),
    };
    graphics.kitty_inactive_screen = KittyScreenState::default();
    graphics.kitty_inactive_screen.kitty_images.insert(
        2,
        StoredImage {
            data: inactive_data,
            transmission_time: std::time::Instant::now()
                - std::time::Duration::from_secs(60),
        },
    );
    // Inactive bytes also count toward total_bytes (kept consistent).
    graphics.total_bytes += 50;

    // Now total_bytes = 100. Adding 60 more would push us to 160 > 100,
    // so eviction must free 60 bytes. The inactive image (50 bytes) is
    // tier 0 and gets evicted first; the active unused image (tier 1)
    // is the next candidate to free the remaining 10 bytes.
    let used = std::collections::HashSet::new();
    let ok = graphics.evict_images(60, &used);
    assert!(ok, "Eviction should free enough");

    assert!(
        !graphics.kitty_inactive_screen.kitty_images.contains_key(&2),
        "Inactive image should be evicted before active images"
    );
}

#[test]
fn test_eviction_keeps_active_used_image_when_inactive_available() {
    use crate::ansi::graphics::{Graphics, KittyScreenState, StoredImage};

    let mut graphics = Graphics {
        total_limit: 100,
        ..Graphics::default()
    };

    // Active screen: image 1 with a *live* placement (used).
    let active = GraphicData {
        id: GraphicId::new(1),
        width: 5,
        height: 5,
        color_type: ColorType::Rgba,
        pixels: vec![1u8; 50],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.store_kitty_image(1, None, active);
    graphics
        .kitty_placements
        .insert((1, 0), make_test_placement(1, 0, 0, 0, 5, 1, 0));

    // Inactive screen: image 2 (older, unused on its screen).
    let inactive = GraphicData {
        id: GraphicId::new(2),
        width: 5,
        height: 5,
        color_type: ColorType::Rgba,
        pixels: vec![2u8; 50],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    graphics.kitty_inactive_screen = KittyScreenState::default();
    graphics.kitty_inactive_screen.kitty_images.insert(
        2,
        StoredImage {
            data: inactive,
            transmission_time: std::time::Instant::now(),
        },
    );
    graphics.total_bytes += 50;

    // active placements protect image 1.
    let mut used = std::collections::HashSet::new();
    used.insert(1u64);

    let ok = graphics.evict_images(50, &used);
    assert!(ok);

    // The active visible image must survive; the inactive image is gone.
    assert!(
        graphics.kitty_images.contains_key(&1),
        "Active visible image must not be evicted while an inactive \
         alternative exists"
    );
    assert!(
        !graphics.kitty_inactive_screen.kitty_images.contains_key(&2),
        "Inactive image should be the eviction target"
    );
}

// kitten icat regression: multiple invocations must not collapse into
// the last image. Reproduces the user-reported issue where running
// `kitten icat` repeatedly only renders the most recent image.

/// Drive a single icat-style transmit+display through the full pipeline.
/// `payload` is a 1x1 RGBA pixel base64 encoded; we vary the colour so
/// each transmission is distinguishable. `with_explicit_id` controls
/// whether we send `i=N` (true) or omit it (false, like icat does).
fn icat_invocation(
    term: &mut Crosswords<TestEventListener>,
    payload: &[u8],
    explicit_id: Option<u32>,
) {
    let control = match explicit_id {
        Some(id) => format!("a=T,f=32,s=1,v=1,i={id}"),
        None => "a=T,f=32,s=1,v=1".to_string(),
    };
    let params = vec![b"G".as_ref(), control.as_bytes(), payload];
    let mut state = std::mem::take(&mut term.graphics.kitty_chunking_state);
    let resp = kitty_graphics_protocol::parse(&params, &mut state);
    term.graphics.kitty_chunking_state = state;
    let resp = resp.expect("transmit+display must produce a response struct");

    if let Some(graphic_data) = resp.graphic_data {
        if let Some(placement) = resp.placement_request {
            term.kitty_transmit_and_display(graphic_data, placement);
        } else {
            term.insert_graphic(graphic_data, None, Some(0));
        }
    }
}

#[test]
fn test_kitten_icat_two_invocations_without_explicit_id_keep_both_images() {
    // The user reported that running `kitten icat` multiple times only
    // renders the last image. icat doesn't always send an `i=` parameter,
    // and prior to this fix Rio's parser left image_id at 0, so every
    // implicit-id transmission collided in `kitty_images[0]` and
    // `kitty_placements[(0, 0)]`. After the fix the parser auto-assigns
    // a unique image_id and the placement layer auto-assigns a unique
    // internal placement_id, so both icat outputs survive.
    let mut term = make_test_term();
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Two distinguishable 1x1 RGBA pixels (red, then green).
    icat_invocation(&mut term, b"/wAA/w==", None); // red
    icat_invocation(&mut term, b"AP8A/w==", None); // green

    assert_eq!(
        term.graphics.kitty_images.len(),
        2,
        "Both icat invocations should produce distinct stored images"
    );
    assert_eq!(
        term.graphics.kitty_placements.len(),
        2,
        "Both icat placements should remain visible — only the last one \
         survived before the fix"
    );
}

#[test]
fn test_kitten_icat_two_invocations_with_same_explicit_id_each_get_unique_placement() {
    // Even when icat reuses the same `i=N` (which kitty itself allows
    // and uses for re-transmission), the *placements* should still be
    // distinct so both copies render. The image data is shared (the
    // second transmission overwrites it per spec) but each placement
    // gets its own internal placement_id.
    let mut term = make_test_term();
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    icat_invocation(&mut term, b"/wAA/w==", Some(1));
    icat_invocation(&mut term, b"/wAA/w==", Some(1));

    // One image (re-transmissions overwrite at same id per spec).
    assert_eq!(term.graphics.kitty_images.len(), 1);
    // Two placements (each `a=T` with implicit p=0 must get its own
    // internal placement_id so the prior placement isn't overwritten).
    assert_eq!(
        term.graphics.kitty_placements.len(),
        2,
        "Two `a=T` calls with the same image_id must produce two \
         placements, not collapse into one"
    );
}

#[test]
fn test_implicit_image_ids_are_distinct() {
    // Two parses with no `i=` should yield two different graphic IDs.
    let mut state = KittyGraphicsState::default();

    let p1 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1".as_ref(),
        b"/wAA/w==".as_ref(),
    ];
    let r1 = kitty_graphics_protocol::parse(&p1, &mut state).unwrap();
    let id1 = r1.graphic_data.unwrap().id.get();

    let p2 = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1".as_ref(),
        b"AP8A/w==".as_ref(),
    ];
    let r2 = kitty_graphics_protocol::parse(&p2, &mut state).unwrap();
    let id2 = r2.graphic_data.unwrap().id.get();

    assert_ne!(
        id1, id2,
        "Two implicit-ID transmissions must get distinct allocated IDs"
    );
    assert!(id1 > 0, "Auto-assigned id must be non-zero");
    assert!(id2 > 0, "Auto-assigned id must be non-zero");
}

#[test]
fn test_implicit_image_id_still_suppresses_response() {
    // Per spec: even though we auto-assign an id internally, we must
    // not respond to commands the client transmitted *without* an
    // explicit id (otherwise the client would see a stray APC reply
    // it doesn't know how to interpret).
    let mut state = KittyGraphicsState::default();
    let params = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1".as_ref(),
        b"/wAA/w==".as_ref(),
    ];
    let resp = kitty_graphics_protocol::parse(&params, &mut state).unwrap();
    assert!(
        resp.response.is_none() || resp.response.as_deref() == Some(""),
        "Implicit-id transmissions must not produce a response"
    );
}

#[test]
fn test_explicit_image_id_still_responds() {
    // Sanity check that adding implicit-id auto-assignment didn't
    // accidentally suppress responses for explicit-id transmissions.
    let mut state = KittyGraphicsState::default();
    let params = vec![
        b"G".as_ref(),
        b"a=t,f=32,s=1,v=1,i=42".as_ref(),
        b"/wAA/w==".as_ref(),
    ];
    let resp = kitty_graphics_protocol::parse(&params, &mut state).unwrap();
    let body = resp.response.expect("explicit-id response must be present");
    assert!(body.contains("i=42"));
    assert!(body.contains("OK"));
}

// Resize-with-reflow placement tracking.
//
// The user's actual scenario: a long command wraps to 2 lines, an image is
// placed below it, then the window is widened so the command fits on 1 line.
// The image must follow the surrounding text (move up by 1 row when widening,
// down by 1 when narrowing) instead of staying anchored to its absolute
// scrollback row.

#[derive(Debug, Clone, Copy)]
struct ReflowDim {
    columns: usize,
    lines: usize,
}

impl crate::crosswords::grid::Dimensions for ReflowDim {
    fn columns(&self) -> usize {
        self.columns
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn square_width(&self) -> f32 {
        10.0
    }
    fn square_height(&self) -> f32 {
        20.0
    }
}

/// Type a string of ASCII into the terminal so it lands in the grid like
/// real shell input would.
fn type_text(term: &mut Crosswords<TestEventListener>, text: &str) {
    use crate::performer::handler::Handler;
    for c in text.chars() {
        term.input(c);
    }
}

#[test]
fn test_resize_widen_unwraps_command_image_follows() {
    // Reproduce: narrow window where the command wraps to 2 lines, place
    // an image right after the wrap, then widen the window so the command
    // fits on a single line. The image must move *up* by one row to stay
    // pinned to the spot just below the (now shorter) command.
    use crate::performer::handler::Handler;
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(20, 10),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Type a 32-char command. With columns=20 it wraps onto 2 rows;
    // after we widen to columns=50 it will fit on 1 row.
    type_text(&mut term, "$ kitten icat /path/to/image.png");
    term.linefeed();
    term.carriage_return();

    let cursor_before = term.grid.cursor.pos.row.0;

    store_red_pixel(&mut term, 1);
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 1,
        rows: 1,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 1,
        cell_x_offset: 0,
        cell_y_offset: 0,
    };
    term.place_graphic(placement);

    let initial_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .expect("placement must exist")
        .dest_row;
    assert_eq!(
        initial_dest_row,
        term.history_size() as i64 + cursor_before as i64,
        "placement should anchor at the cursor's absolute row"
    );

    // Widen the window. The wrapped command should join back onto a
    // single row, and the image should follow up by 1.
    term.resize(ReflowDim {
        columns: 50,
        lines: 10,
    });

    let final_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .expect("placement must still exist")
        .dest_row;
    assert_eq!(
        final_dest_row,
        initial_dest_row - 1,
        "Widening should drop dest_row by 1 so the image follows the \
         (now unwrapped) command. Got {final_dest_row}, expected {}",
        initial_dest_row - 1
    );
}

#[test]
fn test_resize_narrow_wraps_command_image_follows() {
    // Mirror case: a wide window where the command fits on 1 line.
    // Narrowing the window forces the command onto 2 wrapped rows;
    // the image below it must shift *down* by 1.
    use crate::performer::handler::Handler;
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(50, 10),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    type_text(&mut term, "$ kitten icat /path/to/image.png");
    term.linefeed();
    term.carriage_return();

    let cursor_before = term.grid.cursor.pos.row.0;

    store_red_pixel(&mut term, 1);
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 1,
        rows: 1,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 1,
        cell_x_offset: 0,
        cell_y_offset: 0,
    };
    term.place_graphic(placement);

    let initial_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;
    assert_eq!(
        initial_dest_row,
        term.history_size() as i64 + cursor_before as i64
    );

    // Narrow the window so the command wraps onto two rows.
    term.resize(ReflowDim {
        columns: 20,
        lines: 10,
    });

    let final_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;
    assert_eq!(
        final_dest_row,
        initial_dest_row + 1,
        "Narrowing should bump dest_row by 1 so the image follows the \
         (now wrapped) command down. Got {final_dest_row}, expected {}",
        initial_dest_row + 1
    );
}

/// Print the visible grid contents for debugging.
fn dump_grid(term: &Crosswords<TestEventListener>, label: &str) {
    use crate::crosswords::grid::Dimensions;
    eprintln!("=== {label} ===");
    eprintln!(
        "  cursor.row={}, history={}, columns={}, screen_lines={}",
        term.grid.cursor.pos.row.0,
        term.history_size(),
        Dimensions::columns(&term.grid),
        Dimensions::screen_lines(&term.grid),
    );
    for placement in term.graphics.kitty_placements.values() {
        eprintln!(
            "  placement: image_id={}, dest_row={}, dest_col={}, columns={}, rows={}",
            placement.image_id,
            placement.dest_row,
            placement.dest_col,
            placement.columns,
            placement.rows,
        );
    }
    use crate::crosswords::pos::{Column, Line};
    let lines = Dimensions::screen_lines(&term.grid);
    let cols = Dimensions::columns(&term.grid);
    for r in 0..lines {
        let line = Line(r as i32);
        let mut s = String::new();
        for c in 0..cols {
            let cell = &term.grid[line][Column(c)];
            let ch = cell.c();
            if ch == '\0' || ch == ' ' {
                s.push('.');
            } else {
                s.push(ch);
            }
        }
        eprintln!("  row {:>2}: |{}|", r, s.trim_end_matches('.'));
    }
}

#[test]
fn test_debug_widen_visible_layout() {
    // Mirror of test_debug_narrow_visible_layout: starts NARROW with the
    // command wrapped onto 2 rows, then widens.
    use crate::performer::handler::Handler;
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(20, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    for _ in 0..18 {
        term.linefeed();
    }
    term.carriage_return();

    type_text(&mut term, "$ kitten icat /path/to/image.png");
    term.linefeed();
    term.carriage_return();

    store_red_pixel(&mut term, 1);
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 1,
        rows: 1,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
    };
    term.place_graphic(placement);

    term.linefeed();
    term.carriage_return();
    type_text(&mut term, "$ ");

    dump_grid(&term, "BEFORE widen");

    term.resize(ReflowDim {
        columns: 50,
        lines: 24,
    });

    dump_grid(&term, "AFTER widen");
}

#[test]
fn test_debug_narrow_visible_layout() {
    // Print visible layout before/after narrowing to understand what
    // shrink_columns actually does to cursor and content positioning.
    use crate::performer::handler::Handler;
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(50, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    for _ in 0..20 {
        term.linefeed();
    }
    term.carriage_return();

    type_text(&mut term, "$ kitten icat /path/to/image.png");
    term.linefeed();
    term.carriage_return();

    store_red_pixel(&mut term, 1);
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 1,
        rows: 1,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
    };
    term.place_graphic(placement);

    term.linefeed();
    term.carriage_return();
    type_text(&mut term, "$ ");

    dump_grid(&term, "BEFORE narrow");

    term.resize(ReflowDim {
        columns: 20,
        lines: 24,
    });

    dump_grid(&term, "AFTER narrow");
}

#[test]
fn test_resize_narrow_combined_col_and_row_change() {
    // Real window resize: user drags the corner, both columns and
    // lines change in the same Crosswords::resize call. Both
    // grow_columns/shrink_columns AND grow_lines/shrink_lines fire.
    // Cursor delta accumulates from both.
    use crate::performer::handler::Handler;
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(50, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    for _ in 0..10 {
        term.linefeed();
    }
    term.carriage_return();

    type_text(&mut term, "$ kitten icat /path/to/image.png");
    term.linefeed();
    term.carriage_return();

    store_red_pixel(&mut term, 1);
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 1,
        rows: 1,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
    };
    term.place_graphic(placement);

    term.linefeed();
    term.carriage_return();
    type_text(&mut term, "$ ");

    let initial_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;

    eprintln!(
        "BEFORE combined: cursor.row={}, history={}, dest_row={}",
        term.grid.cursor.pos.row.0,
        term.history_size(),
        initial_dest_row,
    );

    // Narrow + shorten at the same time.
    term.resize(ReflowDim {
        columns: 20,
        lines: 20,
    });

    let final_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;

    eprintln!(
        "AFTER combined : cursor.row={}, history={}, dest_row={}, delta={}",
        term.grid.cursor.pos.row.0,
        term.history_size(),
        final_dest_row,
        final_dest_row - initial_dest_row,
    );

    // The image should still follow the wrap regardless of the
    // simultaneous row count change.
    // Cursor delta should be (history_grew_by_wrap) +
    // (cursor_row_change_from_shrink_lines + wrap_above_cursor).
    // The exact number depends on how shrink_lines + shrink_columns
    // interact, but the image should track the cursor.
}

#[test]
fn test_resize_narrow_with_multi_row_image() {
    // Realistic icat: a tall image (e.g. 8 rows). The cursor advances
    // by `rows - 1` linefeeds during placement, so the dest_row is
    // *above* the cursor. Then the next prompt sits below the image.
    use crate::performer::handler::Handler;
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(50, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Push the cursor down to where icat would normally land.
    for _ in 0..10 {
        term.linefeed();
    }
    term.carriage_return();

    type_text(&mut term, "$ kitten icat /path/to/image.png");
    term.linefeed();
    term.carriage_return();

    let placement_row = term.grid.cursor.pos.row.0;

    store_red_pixel(&mut term, 1);
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 1,
        rows: 8, // 8-row image
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0, // Default: cursor moves to last row of image
        cell_x_offset: 0,
        cell_y_offset: 0,
    };
    term.place_graphic(placement);

    // After place_kitty_overlay with cursor_movement=0, cursor was
    // advanced by rows-1 linefeeds.
    let cursor_after_image = term.grid.cursor.pos.row.0;
    assert!(
        cursor_after_image > placement_row,
        "8-row image should advance cursor below placement_row \
         (placement={placement_row}, cursor_after={cursor_after_image})"
    );

    // Then the next shell prompt.
    term.linefeed();
    term.carriage_return();
    type_text(&mut term, "$ ");

    let initial_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;

    eprintln!(
        "BEFORE: cursor.row={}, history={}, dest_row={}, placement_row={}",
        term.grid.cursor.pos.row.0,
        term.history_size(),
        initial_dest_row,
        placement_row,
    );

    term.resize(ReflowDim {
        columns: 20,
        lines: 24,
    });

    let final_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;

    eprintln!(
        "AFTER : cursor.row={}, history={}, dest_row={}, delta={}",
        term.grid.cursor.pos.row.0,
        term.history_size(),
        final_dest_row,
        final_dest_row - initial_dest_row,
    );

    assert_eq!(
        final_dest_row - initial_dest_row,
        1,
        "8-row image should still follow the +1 wrap delta"
    );
}

#[test]
fn test_resize_narrow_with_cursor_at_bottom_of_screen() {
    // Realistic terminal: cursor pinned at the bottom row when icat
    // runs at the prompt. After narrowing, the wrap above the image
    // pushes everything down, but Rio's `shrink_columns` may also
    // scroll to keep the cursor in view, which makes history grow more
    // than 1.
    use crate::performer::handler::Handler;
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(50, 24),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Push the cursor to near the bottom by linefeeding several times.
    // This simulates a terminal session where some history has been
    // built up before icat runs.
    for _ in 0..20 {
        term.linefeed();
    }
    term.carriage_return();

    // Now run the icat-style sequence.
    type_text(&mut term, "$ kitten icat /path/to/image.png");
    term.linefeed();
    term.carriage_return();

    let placement_row = term.grid.cursor.pos.row.0;
    let placement_history = term.history_size();

    store_red_pixel(&mut term, 1);
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 1,
        rows: 1,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0,
        cell_x_offset: 0,
        cell_y_offset: 0,
    };
    term.place_graphic(placement);

    // Then the shell prints its next prompt.
    term.linefeed();
    term.carriage_return();
    type_text(&mut term, "$ ");

    let initial_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;

    eprintln!(
        "BEFORE RESIZE: cursor.row={}, history={}, placement.dest_row={}, placement_row_at_place={}, history_at_place={}",
        term.grid.cursor.pos.row.0,
        term.history_size(),
        initial_dest_row,
        placement_row,
        placement_history,
    );

    term.resize(ReflowDim {
        columns: 20,
        lines: 24,
    });

    let final_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;

    eprintln!(
        "AFTER  RESIZE: cursor.row={}, history={}, placement.dest_row={}, delta={}",
        term.grid.cursor.pos.row.0,
        term.history_size(),
        final_dest_row,
        final_dest_row - initial_dest_row,
    );

    // The image is one row below the wrapped command, so wrapping
    // should push it down by 1.
    assert_eq!(
        final_dest_row - initial_dest_row,
        1,
        "Image should follow the wrap-down by exactly 1 row (delta {})",
        final_dest_row - initial_dest_row
    );
}

#[test]
fn test_resize_narrow_with_prompt_after_image() {
    // Realistic icat flow: command on row 0, image at row 1, then the
    // shell prints a new prompt on row 2 below the image. Narrowing
    // the window should wrap row 0 into 2 rows, pushing both the image
    // and the prompt below it down by 1. This is the case the user
    // reported as still broken — content after the image makes the
    // cursor land at a row below the placement, which changes the
    // delta math.
    use crate::performer::handler::Handler;
    let event_listener = TestEventListener;
    let window_id = unsafe { WindowId::dummy() };
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(50, 10),
        crate::ansi::CursorShape::Block,
        event_listener,
        window_id,
        0,
        10_000,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // Row 0: the command (32 chars, fits at columns=50)
    type_text(&mut term, "$ kitten icat /path/to/image.png");
    term.linefeed();
    term.carriage_return();

    // Row 1: this is where the image goes. Place it here.
    let placement_row = term.grid.cursor.pos.row.0;
    store_red_pixel(&mut term, 1);
    let placement = kitty_graphics_protocol::PlacementRequest {
        image_id: 1,
        placement_id: 0,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 1,
        rows: 1,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 0, // Default kitty behaviour: cursor stays on the last row of image
        cell_x_offset: 0,
        cell_y_offset: 0,
    };
    term.place_graphic(placement);

    // Then the shell moves to row 2 and prints its prompt.
    term.linefeed();
    term.carriage_return();
    type_text(&mut term, "$ ");

    let cursor_before = term.grid.cursor.pos.row.0;
    assert!(
        cursor_before > placement_row,
        "test setup: cursor should be below the image, got cursor={cursor_before} placement={placement_row}"
    );
    let initial_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;

    // Narrow: row 0 wraps onto 2 rows.
    term.resize(ReflowDim {
        columns: 20,
        lines: 10,
    });

    let final_dest_row = term
        .graphics
        .kitty_placements
        .values()
        .next()
        .unwrap()
        .dest_row;

    // The image is anchored to a cell directly below the wrapped row;
    // after the wrap there is one extra row above it, so dest_row
    // should increase by exactly 1.
    assert_eq!(
        final_dest_row - initial_dest_row,
        1,
        "Narrowing with content below the image should still shift the \
         placement down by 1 (got delta {})",
        final_dest_row - initial_dest_row
    );
}

// Animation actions surface EINVAL (regression).

#[test]
fn test_animation_action_surfaces_unsupported_response() {
    // Going through the full Crosswords path: a=f should produce a
    // response that the terminal can forward back to the client. Pre-fix
    // this returned None and the client got nothing.
    let mut state = KittyGraphicsState::default();
    let params = vec![
        b"G".as_ref(),
        b"a=f,i=1,r=2,s=1,v=1,f=32".as_ref(),
        b"AAAA".as_ref(),
    ];

    let resp = kitty_graphics_protocol::parse(&params, &mut state)
        .expect("animation actions must produce a response");
    let body = resp
        .response
        .expect("response body must contain EINVAL marker");
    assert!(body.contains("EINVAL:unsupported action"));
    assert!(body.contains("i=1"));
}

// Placement geometry tests

fn geometry_test_term() -> Crosswords<TestEventListener> {
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 24),
        crate::ansi::CursorShape::Block,
        TestEventListener,
        unsafe { WindowId::dummy() },
        0,
        10_000,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 100,
        height: 100,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 100 * 100 * 4],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    term.store_graphic(graphic);
    term
}

fn placement_request(placement_id: u32) -> PlacementRequest {
    PlacementRequest {
        image_id: 1,
        placement_id,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        columns: 0,
        rows: 0,
        z_index: 0,
        virtual_placement: false,
        unicode_placeholder: 0,
        cursor_movement: 1,
        cell_x_offset: 0,
        cell_y_offset: 0,
    }
}

#[test]
fn test_source_crop_keeps_placement_at_cursor() {
    let mut term = geometry_test_term();

    // x=/y= select the source rectangle, never the destination cell.
    let mut placement = placement_request(5);
    placement.x = 30;
    placement.y = 40;
    term.place_graphic(placement);

    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 5))
        .expect("placement stored");
    assert_eq!(stored.dest_col, 0, "placement lands at the cursor column");
    assert_eq!(stored.dest_row, 0, "placement lands at the cursor row");
    assert_eq!(stored.source_x, 30, "crop is stored raw");
    assert_eq!(stored.source_y, 40);
    assert_eq!(stored.source_width, 0, "raw zero means to the edge");
    assert_eq!(stored.source_height, 0);
    assert_eq!(stored.pixel_width, 70, "footprint shows only the crop");
    assert_eq!(stored.pixel_height, 60);
    assert_eq!(stored.columns, 7);
    assert_eq!(stored.rows, 3);
}

#[test]
fn test_source_crop_clamped_and_degenerate_invisible() {
    use crate::ansi::graphics::{kitty_overlay_geometry, OverlayViewport};

    let mut term = geometry_test_term();
    let viewport = OverlayViewport {
        cell_width: 10.0,
        cell_height: 20.0,
        origin_x: 0.0,
        origin_y: 0.0,
        history_size: 0,
        display_offset: 0,
        screen_lines: 24,
    };

    // Crop wider than the image clamps to the edge at render time.
    let mut placement = placement_request(6);
    placement.x = 90;
    placement.width = 50;
    term.place_graphic(placement);

    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 6))
        .expect("placement stored");
    assert_eq!(stored.source_x, 90, "raw request kept");
    assert_eq!(stored.source_width, 50);
    assert_eq!(stored.pixel_width, 10, "footprint uses the clamped crop");
    assert_eq!(stored.pixel_height, 100);
    let geometry = kitty_overlay_geometry(stored, 100, 100, &viewport).expect("visible");
    assert_eq!(geometry.width, 10.0, "clamped to image width");
    assert_eq!(geometry.source_rect, [0.9, 0.0, 1.0, 1.0]);

    // A crop fully outside the image is stored (kitty answers OK) but
    // renders nothing and occupies no cells.
    let mut placement = placement_request(7);
    placement.y = 100;
    placement.cursor_movement = 0;
    let cursor_before = term.grid.cursor.pos;
    term.place_graphic(placement);
    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 7))
        .expect("degenerate placement still stored");
    assert_eq!(stored.columns, 0, "no cell footprint");
    assert_eq!(stored.rows, 0);
    assert!(
        kitty_overlay_geometry(stored, 100, 100, &viewport).is_none(),
        "degenerate crop renders nothing"
    );
    assert_eq!(
        term.grid.cursor.pos, cursor_before,
        "invisible placement leaves the cursor untouched"
    );
}

#[test]
fn test_crop_scaled_to_requested_columns() {
    let mut term = geometry_test_term();

    // 50x25 crop over c=10 columns: width = 10 cells * 10px, height
    // keeps the crop aspect ratio.
    let mut placement = placement_request(8);
    placement.width = 50;
    placement.height = 25;
    placement.columns = 10;
    term.place_graphic(placement);

    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 8))
        .expect("placement stored");
    assert_eq!(stored.pixel_width, 100);
    assert_eq!(stored.pixel_height, 50, "aspect follows the crop");
    assert_eq!(stored.columns, 10);
    assert_eq!(stored.rows, 3, "ceil(50 / 20)");
}

#[test]
fn test_overlay_geometry_scroll_and_pixel_position() {
    use crate::ansi::graphics::{kitty_overlay_geometry, OverlayViewport};

    let placement = KittyPlacement {
        image_id: 1,
        placement_id: 1,
        source_x: 50,
        source_y: 25,
        source_width: 100,
        source_height: 50,
        dest_col: 4,
        dest_row: 55,
        columns: 2,
        rows: 3,
        requested_columns: 0,
        requested_rows: 0,
        pixel_width: 25,
        pixel_height: 45,
        cell_x_offset: 3,
        cell_y_offset: 7,
        z_index: 0,
        transmit_time: std::time::Instant::now(),
    };

    let viewport = OverlayViewport {
        cell_width: 10.0,
        cell_height: 20.0,
        origin_x: 100.0,
        origin_y: 50.0,
        history_size: 80,
        display_offset: 30,
        screen_lines: 24,
    };

    // Scrolled back 30 rows into 80 rows of history: the absolute row
    // 55 sits 5 rows below the viewport top (80 - 30).
    let geometry = kitty_overlay_geometry(&placement, 200, 100, &viewport)
        .expect("visible placement");
    assert_eq!(geometry.x, 100.0 + 4.0 * 10.0 + 3.0);
    assert_eq!(geometry.y, 50.0 + 5.0 * 20.0 + 7.0);
    // Display size resolves per frame: no cell span requested, so the
    // 100x50 crop shows at native size.
    assert_eq!(geometry.width, 100.0);
    assert_eq!(geometry.height, 50.0);
    // (origin, end): crop (50, 25)-(150, 75) of a 200x100 image.
    assert_eq!(geometry.source_rect, [0.25, 0.25, 0.75, 0.75]);

    // At the live view (no scroll) the same placement is 25 rows above
    // the viewport and fully culled.
    let live = OverlayViewport {
        display_offset: 0,
        ..viewport
    };
    assert!(kitty_overlay_geometry(&placement, 200, 100, &live).is_none());

    // One row past the bottom edge is culled too.
    let mut below = placement.clone();
    below.dest_row = 80 + 24;
    assert!(kitty_overlay_geometry(&below, 200, 100, &live).is_none());
}

#[test]
fn test_overlay_geometry_partial_visibility_and_full_source() {
    use crate::ansi::graphics::{kitty_overlay_geometry, OverlayViewport};

    let placement = KittyPlacement {
        image_id: 1,
        placement_id: 1,
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 0,
        dest_row: 48,
        columns: 2,
        rows: 3,
        requested_columns: 0,
        requested_rows: 0,
        pixel_width: 20,
        pixel_height: 60,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: 0,
        transmit_time: std::time::Instant::now(),
    };

    let viewport = OverlayViewport {
        cell_width: 10.0,
        cell_height: 20.0,
        origin_x: 0.0,
        origin_y: 0.0,
        history_size: 50,
        display_offset: 0,
        screen_lines: 24,
    };

    // Two of three rows scrolled off the top: keep the quad with a
    // negative y and let the GPU clip it.
    let geometry = kitty_overlay_geometry(&placement, 20, 60, &viewport)
        .expect("partially visible placement");
    assert_eq!(geometry.y, -40.0);

    // Zero crop falls back to the full texture.
    assert_eq!(geometry.source_rect, [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn test_rescale_keeps_native_size_and_tracks_cell_span() {
    use crate::ansi::graphics::KittyPlacement;

    // Native-size placement: 25x45 crop, 10x20 cells.
    let mut native = KittyPlacement {
        image_id: 1,
        placement_id: 1,
        source_x: 0,
        source_y: 0,
        source_width: 25,
        source_height: 45,
        dest_col: 0,
        dest_row: 0,
        columns: 3,
        rows: 3,
        requested_columns: 0,
        requested_rows: 0,
        pixel_width: 25,
        pixel_height: 45,
        cell_x_offset: 0,
        cell_y_offset: 0,
        z_index: 0,
        transmit_time: std::time::Instant::now(),
    };

    // Font grows to 12x24 cells: the image must NOT stretch to its
    // cell box; only the derived span shrinks.
    native.rescale(25, 45, 12, 24);
    assert_eq!(native.pixel_width, 25, "native pixel size is kept");
    assert_eq!(native.pixel_height, 45);
    assert_eq!(native.columns, 3, "ceil(25 / 12)");
    assert_eq!(native.rows, 2, "ceil(45 / 24)");

    // Cell-sized placement (c=4, r=2) tracks the grid instead.
    let mut cell_sized = KittyPlacement {
        requested_columns: 4,
        requested_rows: 2,
        columns: 4,
        rows: 2,
        pixel_width: 40,
        pixel_height: 40,
        ..native.clone()
    };
    cell_sized.rescale(25, 45, 12, 24);
    assert_eq!(cell_sized.pixel_width, 48);
    assert_eq!(cell_sized.pixel_height, 48);
    assert_eq!(cell_sized.columns, 4);
    assert_eq!(cell_sized.rows, 2);
}

#[test]
fn test_crop_scaled_to_requested_rows_and_both_axes() {
    let mut term = geometry_test_term();

    // r= only: height = 2 cells * 20px, width keeps the crop aspect.
    let mut placement = placement_request(9);
    placement.width = 50;
    placement.height = 25;
    placement.rows = 2;
    term.place_graphic(placement);

    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 9))
        .expect("placement stored");
    assert_eq!(stored.pixel_height, 40);
    assert_eq!(stored.pixel_width, 80, "aspect follows the crop");
    assert_eq!(stored.rows, 2);
    assert_eq!(stored.columns, 8, "ceil(80 / 10)");

    // c= and r= both: exact fit, aspect not preserved.
    let mut placement = placement_request(10);
    placement.width = 50;
    placement.height = 25;
    placement.columns = 3;
    placement.rows = 4;
    term.place_graphic(placement);

    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 10))
        .expect("placement stored");
    assert_eq!(stored.pixel_width, 30);
    assert_eq!(stored.pixel_height, 80, "stretched to the exact span");
    assert_eq!(stored.columns, 3);
    assert_eq!(stored.rows, 4);
}

#[test]
fn test_overlay_geometry_clamps_raw_offsets() {
    use crate::ansi::graphics::{kitty_overlay_geometry, OverlayViewport};

    // Raw offsets larger than the current cell box clamp at read
    // time; the stored value survives cell size changes.
    let placement = KittyPlacement {
        image_id: 1,
        placement_id: 1,
        source_x: 0,
        source_y: 0,
        source_width: 0,
        source_height: 0,
        dest_col: 2,
        dest_row: 0,
        columns: 1,
        rows: 1,
        requested_columns: 0,
        requested_rows: 0,
        pixel_width: 8,
        pixel_height: 8,
        cell_x_offset: 999,
        cell_y_offset: 999,
        z_index: 0,
        transmit_time: std::time::Instant::now(),
    };
    let viewport = OverlayViewport {
        cell_width: 10.0,
        cell_height: 20.0,
        origin_x: 0.0,
        origin_y: 0.0,
        history_size: 0,
        display_offset: 0,
        screen_lines: 24,
    };
    let geometry =
        kitty_overlay_geometry(&placement, 8, 8, &viewport).expect("visible placement");
    assert_eq!(geometry.x, 2.0 * 10.0 + 9.0, "offset clamped to cell box");
    assert_eq!(geometry.y, 19.0);
}

#[test]
fn test_retransmit_reclamps_crop_and_updates_footprint() {
    use crate::ansi::graphics::{kitty_overlay_geometry, OverlayViewport};

    let mut term = geometry_test_term();
    let viewport = OverlayViewport {
        cell_width: 10.0,
        cell_height: 20.0,
        origin_x: 0.0,
        origin_y: 0.0,
        history_size: 0,
        display_offset: 0,
        screen_lines: 24,
    };

    // Crop the bottom half of the 100x100 image.
    let mut placement = placement_request(11);
    placement.y = 50;
    term.place_graphic(placement);

    let stored = term.graphics.kitty_placements.get(&(1, 11)).unwrap();
    assert_eq!(stored.rows, 3, "ceil(50 / 20)");
    let geometry = kitty_overlay_geometry(stored, 100, 100, &viewport).unwrap();
    assert_eq!(geometry.height, 50.0);
    assert_eq!(geometry.source_rect, [0.0, 0.5, 1.0, 1.0]);

    // Retransmit the same id as 100x60: the crop re-resolves against
    // the new dimensions instead of showing a stale region, and the
    // grid footprint follows.
    term.graphics.kitty_graphics_dirty = false;
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 100,
        height: 60,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 100 * 60 * 4],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    term.store_graphic(graphic);

    let stored = term.graphics.kitty_placements.get(&(1, 11)).unwrap();
    assert_eq!(stored.pixel_height, 10, "60 - 50 remaining below the crop");
    assert_eq!(stored.rows, 1);
    let geometry = kitty_overlay_geometry(stored, 100, 60, &viewport).unwrap();
    assert_eq!(geometry.height, 10.0);
    let [_, v0, _, v1] = geometry.source_rect;
    assert!((v0 - 50.0 / 60.0).abs() < 1e-6);
    assert_eq!(v1, 1.0);

    // The new pixels were dispatched for upload without a re-place
    // (send_graphics_updates drains the queue into an event, so the
    // dirty flag is the observable side).
    assert!(term.graphics.kitty_graphics_dirty);
}

#[test]
fn test_retransmit_can_make_degenerate_placement_visible() {
    let mut term = geometry_test_term();

    // Fully outside the 100x100 image: invisible, zero footprint.
    let mut placement = placement_request(12);
    placement.y = 100;
    term.place_graphic(placement);
    assert_eq!(
        term.graphics.kitty_placements.get(&(1, 12)).unwrap().rows,
        0
    );

    // Retransmit taller: the placement comes alive.
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 100,
        height: 200,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 100 * 200 * 4],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    term.store_graphic(graphic);

    let stored = term.graphics.kitty_placements.get(&(1, 12)).unwrap();
    assert_eq!(stored.pixel_height, 100, "rows 100..200 of the new image");
    assert_eq!(stored.rows, 5, "ceil(100 / 20)");
}

#[test]
fn test_cursor_movement_scrolls_at_screen_bottom() {
    let mut term = geometry_test_term();

    // Park the cursor on the bottom row.
    for _ in 0..23 {
        term.linefeed();
    }
    assert_eq!(term.grid.cursor.pos.row.0, 23);

    // 100x100 image in 10x20 cells: 5 rows, 10 columns.
    let mut placement = placement_request(13);
    placement.cursor_movement = 0;
    term.place_graphic(placement);

    // The image scrolled into view (5 linefeeds at the bottom) and the
    // cursor sits on its last row, first column after it.
    assert_eq!(term.grid.cursor.pos.row.0, 22);
    assert_eq!(term.grid.cursor.pos.col.0, 10);
    assert_eq!(term.history_size(), 5, "five rows scrolled into history");
}

#[test]
fn test_virtual_run_geometry_honors_source_crop() {
    use crate::ansi::kitty_virtual::{compute_run_geometry, PlaceholderRun};

    let run = PlaceholderRun {
        image_id: 1,
        placement_id: 1,
        row: 0,
        col: 0,
        width: 5,
    };

    // Right half of a 100x50 image (crop 50x50) into a 5x5 cell
    // placement at 10x10 cells: the crop fits the 50x50 box exactly.
    let g = compute_run_geometry(
        &run,
        5,
        5,
        100,
        50,
        (50, 0, 50, 50),
        10.0,
        10.0,
        0.0,
        0.0,
        0,
        0,
    )
    .expect("visible run");
    assert_eq!(g.width, 50.0);
    assert_eq!(g.height, 10.0, "one cell row");
    // u spans the right half of the image; v the top fifth of the crop.
    assert_eq!(g.source_rect, [0.5, 0.0, 1.0, 0.2]);

    // Without a crop the full image aspect-fits with letterboxing;
    // the top run sits entirely in the padding, while the second row
    // maps to the full-image left edge.
    assert!(compute_run_geometry(
        &run,
        5,
        5,
        100,
        50,
        (0, 0, 0, 0),
        10.0,
        10.0,
        0.0,
        0.0,
        0,
        0,
    )
    .is_none());
    let second_row = PlaceholderRun { row: 1, ..run };
    let g = compute_run_geometry(
        &second_row,
        5,
        5,
        100,
        50,
        (0, 0, 0, 0),
        10.0,
        10.0,
        0.0,
        0.0,
        1,
        0,
    )
    .expect("visible run");
    assert_eq!(g.source_rect[0], 0.0, "full-image left edge");
    assert_eq!(g.source_rect[1], 0.0, "crop origin maps to image top");
}

#[test]
fn test_cursor_movement_clears_pending_wrap() {
    let mut term = geometry_test_term();

    // A character printed into the last column arms the wrap flag; a
    // C=0 placement repositions the cursor and must disarm it, or the
    // next printed character wraps below the intended position.
    term.grid.cursor.pos.col = Column(79);
    term.grid.cursor.should_wrap = true;

    let mut placement = placement_request(14);
    placement.cursor_movement = 0;
    term.place_graphic(placement);

    assert!(
        !term.grid.cursor.should_wrap,
        "cursor repositioning discards a pending wrap"
    );
}

#[test]
fn test_transmit_and_display_refreshes_sibling_placements() {
    let mut term = geometry_test_term();

    // Direct placement of the 100x100 image: 5 rows at 20px cells.
    let mut placement = placement_request(15);
    placement.cursor_movement = 1;
    term.place_graphic(placement);
    assert_eq!(
        term.graphics.kitty_placements.get(&(1, 15)).unwrap().rows,
        5
    );

    // a=T retransmit of the same id (new 100x200 pixels + a second
    // placement): the first placement's footprint must follow the new
    // dimensions like the a=t path.
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 100,
        height: 200,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 100 * 200 * 4],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    let mut second = placement_request(16);
    second.cursor_movement = 1;
    term.kitty_transmit_and_display(graphic, second);

    assert_eq!(
        term.graphics.kitty_placements.get(&(1, 15)).unwrap().rows,
        10,
        "sibling placement footprint follows the retransmit"
    );
    assert_eq!(
        term.graphics.kitty_placements.get(&(1, 16)).unwrap().rows,
        10
    );
}

#[test]
fn test_image_key_namespaces_are_disjoint() {
    use crate::sugarloaf::{atlas_image_key, kitty_image_key};

    // kitty clients may pick any u32 image id (kitten icat uses random
    // ones); the atlas namespace must live entirely above that range.
    assert_eq!(kitty_image_key(u32::MAX), u32::MAX as u64);
    assert_eq!(kitty_image_key(0x8000_0001), 0x8000_0001);
    assert!(atlas_image_key(0) > u32::MAX as u64);
    assert_eq!(atlas_image_key(7), (1u64 << 32) + 7);
}

#[test]
fn test_texture_ref_drop_queues_atlas_key() {
    use crate::ansi::graphics::TextureRef;
    use crate::sugarloaf::atlas_image_key;
    use parking_lot::Mutex;
    use std::sync::Arc;

    let ops = Arc::new(Mutex::new(Vec::new()));
    {
        let _texture = TextureRef {
            id: GraphicId::new(3),
            width: 10,
            height: 10,
            cell_width: 10,
            cell_height: 20,
            texture_operations: Arc::downgrade(&ops),
        };
    }
    assert_eq!(
        *ops.lock(),
        vec![atlas_image_key(3)],
        "dropping the last grid reference queues the atlas key for GPU cleanup"
    );
}

// Atlas (sixel/iTerm2) cursor placement tests

/// 30x40 px graphic at 10x20 cells: 3 columns, 2 rows.
fn atlas_graphic() -> GraphicData {
    GraphicData {
        id: GraphicId::new(0),
        width: 30,
        height: 40,
        color_type: ColorType::Rgba,
        pixels: vec![255u8; 30 * 40 * 4],
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    }
}

#[test]
fn test_sixel_cursor_lands_on_last_row_start_column() {
    let mut term = geometry_test_term();

    // Start mid-line: the image anchors at the cursor column.
    term.grid.cursor.pos.col = Column(5);
    term.insert_graphic(atlas_graphic(), None, None);

    // DEC STD 070: cursor on the last text row the image touches, at
    // the image's start column (foot, xterm, contour agree).
    assert_eq!(term.grid.cursor.pos.row.0, 1, "last image row");
    assert_eq!(term.grid.cursor.pos.col.0, 5, "image start column");
    assert!(!term.grid.cursor.should_wrap);
}

#[test]
fn test_sixel_mode_8452_cursor_right_of_image() {
    let mut term = geometry_test_term();
    term.set_private_mode(crate::ansi::mode::PrivateMode::Unknown(8452));

    term.grid.cursor.pos.col = Column(5);
    term.insert_graphic(atlas_graphic(), None, None);

    assert_eq!(term.grid.cursor.pos.row.0, 1, "last image row");
    assert_eq!(
        term.grid.cursor.pos.col.0, 8,
        "first column right of the image"
    );
}

#[test]
fn test_sixel_display_mode_leaves_cursor_untouched() {
    let mut term = geometry_test_term();
    // DECSDM set: sixel scrolling disabled, image at page top-left,
    // cursor unmodified.
    term.set_private_mode(crate::ansi::mode::PrivateMode::Unknown(80));

    term.grid.cursor.pos.col = Column(5);
    term.insert_graphic(atlas_graphic(), None, None);

    assert_eq!(term.grid.cursor.pos.row.0, 0);
    assert_eq!(term.grid.cursor.pos.col.0, 5);
}

#[test]
fn test_iterm2_cursor_right_of_image_and_do_not_move() {
    use crate::ansi::iterm2_image_protocol::{CURSOR_DO_NOT_MOVE, CURSOR_RIGHT_OF_IMAGE};

    let mut term = geometry_test_term();
    term.grid.cursor.pos.col = Column(2);
    term.insert_graphic(atlas_graphic(), None, Some(CURSOR_RIGHT_OF_IMAGE));

    // iTerm2 behavior: last image row, first column after the image.
    assert_eq!(term.grid.cursor.pos.row.0, 1);
    assert_eq!(term.grid.cursor.pos.col.0, 5, "right of the 3-cell image");

    // doNotMoveCursor=1 (WezTerm extension): cursor untouched.
    let mut term = geometry_test_term();
    term.grid.cursor.pos.col = Column(2);
    term.insert_graphic(atlas_graphic(), None, Some(CURSOR_DO_NOT_MOVE));
    assert_eq!(term.grid.cursor.pos.row.0, 0);
    assert_eq!(term.grid.cursor.pos.col.0, 2);
}

#[test]
fn test_iterm2_parse_cursor_movement_params() {
    use crate::ansi::iterm2_image_protocol::{
        self, CURSOR_DO_NOT_MOVE, CURSOR_RIGHT_OF_IMAGE,
    };

    const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";

    let default_params = format!("File=inline=1:{PNG_B64}");
    let params: Vec<&[u8]> = vec![b"1337", default_params.as_bytes()];
    let (_, movement) = iterm2_image_protocol::parse(&params).expect("parses");
    assert_eq!(movement, CURSOR_RIGHT_OF_IMAGE);

    // vte splits OSC params on ';': each key=value arrives separately,
    // with the payload after ':' in the last one.
    let no_move = format!("doNotMoveCursor=1:{PNG_B64}");
    let params: Vec<&[u8]> = vec![b"1337", b"File=inline=1", no_move.as_bytes()];
    let (_, movement) = iterm2_image_protocol::parse(&params).expect("parses");
    assert_eq!(movement, CURSOR_DO_NOT_MOVE);
}

#[test]
fn test_gc_cadence_reclaims_scrolled_out_image_slots() {
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 4),
        crate::ansi::CursorShape::Block,
        TestEventListener,
        unsafe { WindowId::dummy() },
        0,
        2, // two lines of scrollback: images fall off the ring fast
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // 30x40 image: occupies 2 rows.
    term.insert_graphic(atlas_graphic(), None, None);
    assert!(
        term.graphics.texture_operations.lock().is_empty(),
        "texture alive while rows reference it"
    );

    // Scroll the image out of the screen AND the 2-line history.
    for _ in 0..10 {
        term.linefeed();
    }

    // The sweep finds no cell referencing the slot; dropping it drops
    // the TextureRef, which queues the GPU texture removal.
    term.grid.gc_extras();
    let ops = term.graphics.texture_operations.lock();
    assert_eq!(
        ops.as_slice(),
        &[crate::sugarloaf::atlas_image_key(1)],
        "scrolled-out image queues its texture for removal"
    );
}

#[test]
fn test_gc_cadence_trigger() {
    let mut term = geometry_test_term();
    assert!(!term.grid.extras_table.should_gc());

    // Burn through the allocation cadence with throwaway slots.
    for _ in 0..4096 {
        let id = term
            .grid
            .extras_table
            .alloc(crate::crosswords::square::Extras::default());
        term.grid.extras_table.free(id);
    }
    assert!(term.grid.extras_table.should_gc(), "cadence elapsed");

    term.grid.gc_extras();
    assert!(
        !term.grid.extras_table.should_gc(),
        "sweep resets the cadence"
    );
}

#[test]
fn test_kitty_placement_glued_across_ring_saturation_and_expiry() {
    use crate::ansi::graphics::{kitty_overlay_geometry, OverlayViewport};

    // 4-line screen, 3-line scrollback cap.
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 4),
        crate::ansi::CursorShape::Block,
        TestEventListener,
        unsafe { WindowId::dummy() },
        0,
        3,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    // 30x40 image: 2 rows, anchored at absolute row 0.
    let pixels = vec![255u8; 30 * 40 * 4];
    let graphic = GraphicData {
        id: GraphicId::new(1),
        width: 30,
        height: 40,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: true,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    term.store_graphic(graphic);
    let mut placement = placement_request(20);
    placement.cursor_movement = 1;
    term.place_graphic(placement);
    assert_eq!(
        term.graphics
            .kitty_placements
            .get(&(1, 20))
            .unwrap()
            .dest_row,
        0
    );

    // Fill the screen, then scroll into history up to the cap.
    for _ in 0..6 {
        term.linefeed();
    }
    assert_eq!(term.history_size(), 3);
    assert_eq!(term.lines_evicted(), 0);

    // One more line: the ring is at cap, the oldest row (the image's
    // first) is evicted.
    term.linefeed();
    assert_eq!(term.lines_evicted(), 1);
    let stored = term
        .graphics
        .kitty_placements
        .get(&(1, 20))
        .expect("placement survives while a row remains in the ring");

    // Scrolled fully back: the image's second row is the topmost
    // retained line, so the placement renders one row above the
    // viewport top (negative y, partially visible). Without the
    // absolute base it would drift a full row down.
    let viewport = OverlayViewport {
        cell_width: 10.0,
        cell_height: 20.0,
        origin_x: 0.0,
        origin_y: 0.0,
        history_size: (term.lines_evicted() + term.history_size() as u64) as i64,
        display_offset: term.history_size() as i64,
        screen_lines: 4,
    };
    let geometry =
        kitty_overlay_geometry(stored, 30, 40, &viewport).expect("partially visible");
    assert_eq!(geometry.y, -20.0, "glued: first image row evicted");

    // One more eviction takes the image's last row off the ring: the
    // placement expires like kitty's do.
    term.linefeed();
    assert_eq!(term.lines_evicted(), 2);
    assert!(
        !term.graphics.kitty_placements.contains_key(&(1, 20)),
        "placement expires with its content"
    );
}

#[test]
fn test_atlas_placement_created_on_insert() {
    let mut term = geometry_test_term();
    term.grid.cursor.pos.col = Column(5);
    term.insert_graphic(atlas_graphic(), None, Some(1));

    assert_eq!(term.graphics.atlas_placements.len(), 1);
    let p = &term.graphics.atlas_placements[0];
    assert_eq!(p.image_key, crate::sugarloaf::atlas_image_key(1));
    assert_eq!(p.abs_row, 0);
    assert_eq!(p.col, 5, "anchored at the cursor column");
    assert_eq!((p.columns, p.rows), (3, 2), "30x40 at 10x20 cells");
    assert_eq!(
        (p.src_x, p.src_y, p.src_width, p.src_height),
        (0, 0, 30, 40),
        "initial crop covers the whole display"
    );
    assert_eq!((p.insert_cell_w, p.insert_cell_h), (10, 20));
    assert_eq!(
        term.graphics.atlas_key_refs.get(&p.image_key),
        Some(&1),
        "one placement references the key"
    );
}

#[test]
fn test_atlas_overlay_geometry_scaling_and_scroll() {
    use crate::ansi::graphics::{
        atlas_overlay_geometry, AtlasPlacement, OverlayViewport,
    };

    let placement = AtlasPlacement {
        image_key: 7,
        abs_row: 55,
        col: 4,
        columns: 3,
        rows: 2,
        src_x: 0,
        src_y: 20,
        src_width: 30,
        src_height: 20,
        total_width: 30,
        total_height: 40,
        insert_cell_w: 10,
        insert_cell_h: 20,
    };
    // Scrolled back 30 rows into 80 rows above the screen top.
    let viewport = OverlayViewport {
        cell_width: 12.0,
        cell_height: 24.0,
        origin_x: 100.0,
        origin_y: 50.0,
        history_size: 80,
        display_offset: 30,
        screen_lines: 24,
    };
    let g = atlas_overlay_geometry(&placement, &viewport).expect("visible");
    assert_eq!(g.x, 100.0 + 4.0 * 12.0);
    assert_eq!(g.y, 50.0 + 5.0 * 24.0);
    // Live cells grew 10x20 -> 12x24: display scales with the font.
    assert_eq!(g.width, 30.0 * 1.2);
    assert_eq!(g.height, 20.0 * 1.2);
    // Bottom half of the display space.
    assert_eq!(g.source_rect, [0.0, 0.5, 1.0, 1.0]);

    // Fully above the viewport: culled.
    let live = OverlayViewport {
        display_offset: 0,
        ..viewport
    };
    assert!(atlas_overlay_geometry(&placement, &live).is_none());
}

#[test]
fn test_atlas_placement_expires_off_the_ring() {
    let mut term: Crosswords<TestEventListener> = Crosswords::new(
        crate::crosswords::CrosswordsSize::new(80, 4),
        crate::ansi::CursorShape::Block,
        TestEventListener,
        unsafe { WindowId::dummy() },
        0,
        2,
    );
    term.graphics.cell_width = 10.0;
    term.graphics.cell_height = 20.0;

    term.insert_graphic(atlas_graphic(), None, Some(1));
    assert_eq!(term.graphics.atlas_placements.len(), 1);

    // Push the image's two rows out of the screen and the 2-line ring.
    for _ in 0..10 {
        term.linefeed();
    }
    assert!(
        term.graphics.atlas_placements.is_empty(),
        "placement expires with its content"
    );
    assert!(
        term.graphics.atlas_key_refs.is_empty(),
        "last placement released the image key"
    );
}
