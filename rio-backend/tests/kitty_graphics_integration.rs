// Integration tests for Kitty Graphics Protocol

use rio_backend::ansi::kitty_graphics_protocol::{self, DeleteRequest, PlacementRequest};
use rio_backend::performer::handler::Handler;
use sugarloaf::GraphicData;

/// Test handler that captures graphics operations
#[derive(Default)]
struct TestHandler {
    graphics: Vec<GraphicData>,
    placements: Vec<PlacementRequest>,
    deletions: Vec<DeleteRequest>,
    responses: Vec<String>,
}

impl rio_backend::performer::handler::Handler for TestHandler {
    fn insert_graphic(&mut self, data: GraphicData, _palette: Option<Vec<rio_backend::config::colors::ColorRgb>>) {
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

#[test]
fn test_direct_parse_transmit() {
    let mut handler = TestHandler::default();

    // Parse kitty graphics directly through the protocol parser
    // 1x1 RGBA pixel (4 bytes) - base64 encoded [255, 0, 0, 255] (red pixel)
    let params = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,i=1".as_ref(), b"/wAA/w==".as_ref()];

    if let Some(response) = kitty_graphics_protocol::parse(&params) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None);
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
    let params1 = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,m=1,i=100".as_ref(), b"/wA".as_ref()];
    let result1 = kitty_graphics_protocol::parse(&params1);
    assert!(result1.is_none());

    // Send second chunk
    let params2 = vec![b"G".as_ref(), b"a=t,m=1,i=100".as_ref(), b"A/".as_ref()];
    let result2 = kitty_graphics_protocol::parse(&params2);
    assert!(result2.is_none());

    // Send final chunk with complete image info (m=0 means last chunk)
    let params3 = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,m=0,i=100".as_ref(), b"w==".as_ref()];
    if let Some(response) = kitty_graphics_protocol::parse(&params3) {
        if let Some(graphic_data) = response.graphic_data {
            handler.insert_graphic(graphic_data, None);
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
        (vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,i=1".as_ref(), b"/wAA/w==".as_ref()], 1u64),
        (vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,i=2".as_ref(), b"/wAA/w==".as_ref()], 2u64),
        (vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,i=3".as_ref(), b"/wAA/w==".as_ref()], 3u64),
    ];

    for (params, _) in &graphics_params {
        if let Some(response) = kitty_graphics_protocol::parse(params) {
            if let Some(graphic_data) = response.graphic_data {
                handler.insert_graphic(graphic_data, None);
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
