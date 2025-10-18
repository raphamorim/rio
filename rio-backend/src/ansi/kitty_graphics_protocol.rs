use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::collections::HashMap;
use std::sync::Mutex;
use sugarloaf::{ColorType, GraphicData, GraphicId, ResizeCommand, ResizeParameter};

// Global storage for incomplete image transfers
lazy_static::lazy_static! {
    static ref INCOMPLETE_IMAGES: Mutex<HashMap<u32, Vec<u8>>> = Mutex::new(HashMap::new());
}

#[derive(Debug)]
pub struct KittyGraphicsResponse {
    pub graphic_data: Option<GraphicData>,
    pub placement_request: Option<PlacementRequest>,
    pub delete_request: Option<DeleteRequest>,
    pub response: Option<String>,
}

#[derive(Debug)]
pub struct PlacementRequest {
    pub image_id: u32,
    pub placement_id: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub columns: u32,
    pub rows: u32,
    pub z_index: i32,
    pub unicode_placeholder: u32,
}

#[derive(Debug)]
pub struct DeleteRequest {
    pub action: u8,
    pub image_id: u32,
    pub placement_id: u32,
    pub x: u32,
    pub y: u32,
    pub z_index: i32,
    pub delete_data: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Transmit,
    TransmitAndDisplay,
    Query,
    Put,
    Delete,
    Frame,
    Animate,
    Compose,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Format {
    Rgb24,
    Rgba32,
    Png,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TransmissionMedium {
    Direct,
    File,
    TempFile,
    SharedMemory,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Compression {
    None,
    Zlib,
}

#[derive(Debug)]
pub struct KittyGraphicsCommand {
    // Action
    action: Action,
    quiet: u8,

    // Image transmission
    format: Format,
    medium: TransmissionMedium,
    width: u32,
    height: u32,
    size: u32,
    offset: u32,
    image_id: u32,
    image_number: u32,
    placement_id: u32,
    compression: Compression,
    more: bool,

    // Image display
    source_x: u32,
    source_y: u32,
    source_width: u32,
    source_height: u32,
    cell_x_offset: u32,
    cell_y_offset: u32,
    columns: u32,
    rows: u32,
    cursor_movement: u8,
    virtual_placement: bool,
    z_index: i32,
    parent_id: u32,
    parent_placement_id: u32,
    relative_x: i32,
    relative_y: i32,

    // Animation frame loading
    frame_number: u32,
    base_frame: u32,
    frame_gap: i32,
    composition_mode: u8,
    background_color: u32,

    // Animation control
    animation_state: u8,
    loop_count: u32,
    current_frame: u32,

    // Delete
    delete_action: u8,

    // Placeholder
    unicode_placeholder: u32,

    // Payload
    payload: Vec<u8>,
}

impl Default for KittyGraphicsCommand {
    fn default() -> Self {
        Self {
            action: Action::Transmit,
            quiet: 0,
            format: Format::Rgba32,
            medium: TransmissionMedium::Direct,
            width: 0,
            height: 0,
            size: 0,
            offset: 0,
            image_id: 0,
            image_number: 0,
            placement_id: 0,
            compression: Compression::None,
            more: false,
            source_x: 0,
            source_y: 0,
            source_width: 0,
            source_height: 0,
            cell_x_offset: 0,
            cell_y_offset: 0,
            columns: 0,
            rows: 0,
            cursor_movement: 0,
            virtual_placement: false,
            z_index: 0,
            parent_id: 0,
            parent_placement_id: 0,
            relative_x: 0,
            relative_y: 0,
            frame_number: 0,
            base_frame: 0,
            frame_gap: 0,
            composition_mode: 0,
            background_color: 0,
            animation_state: 0,
            loop_count: 0,
            current_frame: 0,
            delete_action: b'a',
            unicode_placeholder: 0,
            payload: Vec::new(),
        }
    }
}

pub fn parse(params: &[&[u8]]) -> Option<KittyGraphicsResponse> {
    let Some(b"G") = params.first() else {
        return None;
    }

    let mut cmd = KittyGraphicsCommand::default();

    // Parse control data if present
    if let Some(control) = params.get(1) && !control.is_empty() {
        let control_data = std::str::from_utf8(control).ok()?;
        parse_control_data(&mut cmd, control_data);
    }

    // Parse payload if present
    if let Some(payload) = params.get(2) && !payload.is_empty() {
        cmd.payload = payload.to_vec();
    }

    // Handle query action
    if cmd.action == Action::Query {
        let response = if cmd.quiet < 2 {
            format!("\x1b_Gi={};OK\x1b\\", cmd.image_id)
        } else {
            String::new()
        };
        return Some(KittyGraphicsResponse {
            graphic_data: None,
            placement_request: None,
            delete_request: None,
            response: Some(response),
        });
    }

    // Handle chunked data
    if cmd.more {
        // Store chunk for later
        let mut incomplete = INCOMPLETE_IMAGES.lock().unwrap();
        let image_key = if cmd.image_id > 0 {
            cmd.image_id
        } else {
            cmd.image_number
        };
        incomplete
            .entry(image_key)
            .or_default()
            .extend_from_slice(&cmd.payload);
        return None;
    } else if cmd.image_id > 0 || cmd.image_number > 0 {
        // Check if we have incomplete data
        let mut incomplete = INCOMPLETE_IMAGES.lock().unwrap();
        let image_key = if cmd.image_id > 0 {
            cmd.image_id
        } else {
            cmd.image_number
        };
        if let Some(mut stored_data) = incomplete.remove(&image_key) {
            // Combine stored data with final chunk
            stored_data.extend_from_slice(&cmd.payload);
            cmd.payload = stored_data;
        }
    }

    // Convert to GraphicData based on action
    match cmd.action {
        Action::Transmit | Action::TransmitAndDisplay => {
            let graphic_data = create_graphic_data(&cmd)?;
            let response = if cmd.quiet == 0 && (cmd.image_id > 0 || cmd.image_number > 0)
            {
                let id_part = if cmd.image_id > 0 {
                    format!("i={}", cmd.image_id)
                } else {
                    format!("i={},I={}", graphic_data.id.0, cmd.image_number)
                };
                Some(format!("\x1b_G{};OK\x1b\\", id_part))
            } else {
                None
            };

            let placement_request = if cmd.action == Action::TransmitAndDisplay {
                Some(PlacementRequest {
                    image_id: cmd.image_id,
                    placement_id: cmd.placement_id,
                    x: cmd.source_x,
                    y: cmd.source_y,
                    width: cmd.source_width,
                    height: cmd.source_height,
                    columns: cmd.columns,
                    rows: cmd.rows,
                    z_index: cmd.z_index,
                    unicode_placeholder: cmd.unicode_placeholder,
                })
            } else {
                None
            };

            Some(KittyGraphicsResponse {
                graphic_data: Some(graphic_data),
                placement_request,
                delete_request: None,
                response,
            })
        }
        Action::Put => {
            // Handle placement request
            let placement = PlacementRequest {
                image_id: cmd.image_id,
                placement_id: cmd.placement_id,
                x: cmd.source_x,
                y: cmd.source_y,
                width: cmd.source_width,
                height: cmd.source_height,
                columns: cmd.columns,
                rows: cmd.rows,
                z_index: cmd.z_index,
                unicode_placeholder: cmd.unicode_placeholder,
            };
            let response = if cmd.quiet == 0 && cmd.image_id > 0 {
                let id_part = if cmd.placement_id > 0 {
                    format!("i={},p={}", cmd.image_id, cmd.placement_id)
                } else {
                    format!("i={}", cmd.image_id)
                };
                Some(format!("\x1b_G{};OK\x1b\\", id_part))
            } else {
                None
            };
            Some(KittyGraphicsResponse {
                graphic_data: None,
                placement_request: Some(placement),
                delete_request: None,
                response,
            })
        }
        Action::Delete => {
            // Handle delete request
            let delete_data = cmd.delete_action.is_ascii_uppercase();
            let delete = DeleteRequest {
                action: cmd.delete_action.to_ascii_lowercase(),
                image_id: cmd.image_id,
                placement_id: cmd.placement_id,
                x: cmd.source_x,
                y: cmd.source_y,
                z_index: cmd.z_index,
                delete_data,
            };
            Some(KittyGraphicsResponse {
                graphic_data: None,
                placement_request: None,
                delete_request: Some(delete),
                response: None,
            })
        }
        _ => {
            // TODO: Handle other actions
            None
        }
    }
}

fn parse_control_data(cmd: &mut KittyGraphicsCommand, control_data: &str) {
    // First pass: parse action to determine context
    for pair in control_data.split(',') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == "a" {
                cmd.action = parse_action(value);
                break;
            }
        }
    }

    // Second pass: parse remaining keys based on action context
    for pair in control_data.split(',') {
        if let Some((key, value)) = pair.split_once('=') {
            match key {
                // Action (already parsed)
                "a" => {}
                "q" => cmd.quiet = value.parse().unwrap_or(0),

                // Image transmission
                "f" => cmd.format = parse_format(value),
                "t" => cmd.medium = parse_transmission_medium(value),
                "s" => match cmd.action {
                    Action::Animate => cmd.animation_state = value.parse().unwrap_or(0),
                    _ => cmd.width = value.parse().unwrap_or(0),
                },
                "v" => match cmd.action {
                    Action::Animate => cmd.loop_count = value.parse().unwrap_or(0),
                    _ => cmd.height = value.parse().unwrap_or(0),
                },
                "S" => cmd.size = value.parse().unwrap_or(0),
                "O" => cmd.offset = value.parse().unwrap_or(0),
                "i" => cmd.image_id = value.parse().unwrap_or(0),
                "I" => cmd.image_number = value.parse().unwrap_or(0),
                "p" => cmd.placement_id = value.parse().unwrap_or(0),
                "o" => cmd.compression = parse_compression(value),
                "m" => cmd.more = value == "1",

                // Context-dependent keys
                "x" => match cmd.action {
                    Action::Delete => cmd.source_x = value.parse().unwrap_or(0),
                    _ => cmd.source_x = value.parse().unwrap_or(0),
                },
                "y" => match cmd.action {
                    Action::Delete => cmd.source_y = value.parse().unwrap_or(0),
                    _ => cmd.source_y = value.parse().unwrap_or(0),
                },
                "w" => cmd.source_width = value.parse().unwrap_or(0),
                "h" => cmd.source_height = value.parse().unwrap_or(0),
                "X" => match cmd.action {
                    Action::Frame | Action::Compose => {
                        cmd.composition_mode = value.parse().unwrap_or(0)
                    }
                    _ => cmd.cell_x_offset = value.parse().unwrap_or(0),
                },
                "Y" => match cmd.action {
                    Action::Frame => cmd.background_color = value.parse().unwrap_or(0),
                    _ => cmd.cell_y_offset = value.parse().unwrap_or(0),
                },
                "c" => match cmd.action {
                    Action::Frame | Action::Compose => {
                        cmd.base_frame = value.parse().unwrap_or(0)
                    }
                    Action::Animate => cmd.current_frame = value.parse().unwrap_or(0),
                    _ => cmd.columns = value.parse().unwrap_or(0),
                },
                "r" => match cmd.action {
                    Action::Frame | Action::Compose | Action::Animate => {
                        cmd.frame_number = value.parse().unwrap_or(0)
                    }
                    _ => cmd.rows = value.parse().unwrap_or(0),
                },
                "z" => match cmd.action {
                    Action::Frame | Action::Animate => {
                        cmd.frame_gap = value.parse().unwrap_or(0)
                    }
                    _ => cmd.z_index = value.parse().unwrap_or(0),
                },

                // Other display keys
                "C" => cmd.cursor_movement = value.parse().unwrap_or(0),
                "U" => cmd.virtual_placement = value == "1",
                "P" => cmd.parent_id = value.parse().unwrap_or(0),
                "Q" => cmd.parent_placement_id = value.parse().unwrap_or(0),
                "H" => cmd.relative_x = value.parse().unwrap_or(0),
                "V" => cmd.relative_y = value.parse().unwrap_or(0),

                // Delete
                "d" => {
                    cmd.delete_action = value.as_bytes().get(0).copied().unwrap_or(b'a')
                }

                // Placeholder
                "u" => cmd.unicode_placeholder = value.parse().unwrap_or(0),

                _ => {} // Ignore unknown keys
            }
        }
    }
}

fn parse_action(value: &str) -> Action {
    match value {
        "t" => Action::Transmit,
        "T" => Action::TransmitAndDisplay,
        "q" => Action::Query,
        "p" => Action::Put,
        "d" => Action::Delete,
        "f" => Action::Frame,
        "a" => Action::Animate,
        "c" => Action::Compose,
        _ => Action::Transmit,
    }
}

fn parse_format(value: &str) -> Format {
    match value {
        "24" => Format::Rgb24,
        "32" => Format::Rgba32,
        "100" => Format::Png,
        _ => Format::Rgba32,
    }
}

fn parse_transmission_medium(value: &str) -> TransmissionMedium {
    match value {
        "d" => TransmissionMedium::Direct,
        "f" => TransmissionMedium::File,
        "t" => TransmissionMedium::TempFile,
        "s" => TransmissionMedium::SharedMemory,
        _ => TransmissionMedium::Direct,
    }
}

fn parse_compression(value: &str) -> Compression {
    match value {
        "z" => Compression::Zlib,
        _ => Compression::None,
    }
}

fn create_graphic_data(cmd: &KittyGraphicsCommand) -> Option<GraphicData> {
    // Get pixel data based on transmission medium
    let raw_data = match cmd.medium {
        TransmissionMedium::Direct => {
            // Decode base64 payload
            BASE64.decode(&cmd.payload).ok()?
        }
        TransmissionMedium::File | TransmissionMedium::TempFile => {
            // Read from file
            use std::fs::File;
            use std::io::Read;
            use std::path::Path;

            let path_str = std::str::from_utf8(&cmd.payload).ok()?;
            let path = Path::new(path_str);

            // Security checks
            if !path.is_file() {
                return None;
            }

            // Check for sensitive paths
            let path_str_lower = path_str.to_lowercase();
            if path_str_lower.contains("/proc/")
                || path_str_lower.contains("/sys/")
                || path_str_lower.contains("/dev/")
            {
                return None;
            }

            // For temp files, verify it contains "tty-graphics-protocol"
            if cmd.medium == TransmissionMedium::TempFile {
                if !path_str.contains("tty-graphics-protocol") {
                    return None;
                }
            }

            let mut file = File::open(path).ok()?;
            let mut data = Vec::new();

            if cmd.size > 0 {
                // Read specific size from offset
                if cmd.offset > 0 {
                    use std::io::Seek;
                    file.seek(std::io::SeekFrom::Start(cmd.offset as u64))
                        .ok()?;
                }
                data.resize(cmd.size as usize, 0);
                file.read_exact(&mut data).ok()?;
            } else {
                // Read entire file
                file.read_to_end(&mut data).ok()?;
            }

            // Delete temp file if requested
            if cmd.medium == TransmissionMedium::TempFile {
                let _ = std::fs::remove_file(path);
            }

            data
        }
        TransmissionMedium::SharedMemory => {
            // TODO: Implement shared memory support
            return None;
        }
    };

    // Decompress if needed
    let pixel_data = match cmd.compression {
        Compression::None => raw_data,
        Compression::Zlib => {
            use flate2::read::ZlibDecoder;
            use std::io::Read;

            let mut decoder = ZlibDecoder::new(&raw_data[..]);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed).ok()?;
            decompressed
        }
    };

    // Parse based on format
    match cmd.format {
        Format::Png => {
            // Decode PNG data
            use image_rs::ImageFormat;

            let img =
                image_rs::load_from_memory_with_format(&pixel_data, ImageFormat::Png)
                    .ok()?;
            let rgba_img = img.to_rgba8();
            let (width, height) = (rgba_img.width() as usize, rgba_img.height() as usize);
            let pixels = rgba_img.into_raw();

            // Check if image is opaque
            let is_opaque = pixels.chunks(4).all(|chunk| chunk[3] == 255);

            // Create resize command if columns/rows specified
            let resize = if cmd.columns > 0 || cmd.rows > 0 {
                Some(ResizeCommand {
                    width: if cmd.columns > 0 {
                        ResizeParameter::Cells(cmd.columns)
                    } else {
                        ResizeParameter::Auto
                    },
                    height: if cmd.rows > 0 {
                        ResizeParameter::Cells(cmd.rows)
                    } else {
                        ResizeParameter::Auto
                    },
                    preserve_aspect_ratio: true,
                })
            } else {
                None
            };

            Some(GraphicData {
                id: GraphicId(cmd.image_id as u64),
                width,
                height,
                color_type: ColorType::Rgba,
                pixels,
                is_opaque,
                resize,
            })
        }
        Format::Rgb24 | Format::Rgba32 => {
            let (color_type, bytes_per_pixel) = match cmd.format {
                Format::Rgb24 => (ColorType::Rgb, 3),
                Format::Rgba32 => (ColorType::Rgba, 4),
                _ => unreachable!(),
            };

            // Validate data size
            let expected_size =
                cmd.width as usize * cmd.height as usize * bytes_per_pixel;
            if pixel_data.len() != expected_size {
                return None;
            }

            // Check if image is opaque (for RGBA)
            let is_opaque = if color_type == ColorType::Rgba {
                pixel_data.chunks(4).all(|chunk| chunk[3] == 255)
            } else {
                true
            };

            // Create resize command if columns/rows specified
            let resize = if cmd.columns > 0 || cmd.rows > 0 {
                Some(ResizeCommand {
                    width: if cmd.columns > 0 {
                        ResizeParameter::Cells(cmd.columns)
                    } else {
                        ResizeParameter::Auto
                    },
                    height: if cmd.rows > 0 {
                        ResizeParameter::Cells(cmd.rows)
                    } else {
                        ResizeParameter::Auto
                    },
                    preserve_aspect_ratio: true,
                })
            } else {
                None
            };

            Some(GraphicData {
                id: GraphicId(cmd.image_id as u64),
                width: cmd.width as usize,
                height: cmd.height as usize,
                color_type,
                pixels: pixel_data,
                is_opaque,
                resize,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_kitty_graphics_protocol(
        keys: &str,
        payload: &str,
    ) -> Option<KittyGraphicsResponse> {
        // Convert keys and payload to the format expected by parse()
        let params = if keys.is_empty() && payload.is_empty() {
            vec![b"G".as_ref()]
        } else if payload.is_empty() {
            vec![b"G".as_ref(), keys.as_bytes()]
        } else {
            vec![b"G".as_ref(), keys.as_bytes(), payload.as_bytes()]
        };

        parse(&params)
    }

    #[test]
    fn test_parse_basic_transmit() {
        // 1x1 RGBA pixel (4 bytes) - base64 encoded [255, 0, 0, 255] (red pixel)
        let payload = "/wAA/w==";
        let result = parse_kitty_graphics_protocol("a=t,f=32,s=1,v=1", payload);
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.graphic_data.is_some());
        assert!(response.placement_request.is_none());
        assert!(response.delete_request.is_none());
    }

    #[test]
    fn test_parse_transmit_and_display() {
        // 1x1 RGBA pixel - base64 encoded [255, 0, 0, 255] (red pixel)
        let payload = "/wAA/w==";
        let result = parse_kitty_graphics_protocol("a=T,f=32,s=1,v=1,i=1", payload);
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.graphic_data.is_some());
        assert!(response.placement_request.is_some());

        let placement = response.placement_request.unwrap();
        assert_eq!(placement.image_id, 1);
    }

    #[test]
    fn test_parse_placement() {
        let result = parse_kitty_graphics_protocol("a=p,i=1,x=10,y=20,c=5,r=3,z=2", "");
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.graphic_data.is_none());
        assert!(response.placement_request.is_some());

        let placement = response.placement_request.unwrap();
        assert_eq!(placement.image_id, 1);
        assert_eq!(placement.x, 10);
        assert_eq!(placement.y, 20);
        assert_eq!(placement.columns, 5);
        assert_eq!(placement.rows, 3);
        assert_eq!(placement.z_index, 2);
    }

    #[test]
    fn test_parse_delete() {
        let result = parse_kitty_graphics_protocol("a=d,d=i,i=1", "");
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.delete_request.is_some());

        let delete = response.delete_request.unwrap();
        assert_eq!(delete.action, b'i');
        assert_eq!(delete.image_id, 1);
        assert!(!delete.delete_data);
    }

    #[test]
    fn test_parse_delete_uppercase() {
        let result = parse_kitty_graphics_protocol("a=d,d=I,i=1", "");
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.delete_request.is_some());

        let delete = response.delete_request.unwrap();
        assert_eq!(delete.action, b'i');
        assert_eq!(delete.image_id, 1);
        assert!(delete.delete_data);
    }

    #[test]
    fn test_parse_query() {
        let result = parse_kitty_graphics_protocol("a=q,i=1", "");
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.response.is_some());
        assert!(response.response.unwrap().contains("OK"));
    }

    #[test]
    fn test_parse_with_compression() {
        // zlib compressed single RGBA pixel [255, 0, 0, 255]
        let payload = "eJz7z8DwHwAE/wH/";
        let result = parse_kitty_graphics_protocol("a=t,f=32,s=1,v=1,o=z", payload);
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.graphic_data.is_some());
    }

    #[test]
    fn test_parse_with_unicode_placeholder() {
        let result = parse_kitty_graphics_protocol("a=p,i=1,u=128512", ""); // 😀
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.placement_request.is_some());

        let placement = response.placement_request.unwrap();
        assert_eq!(placement.unicode_placeholder, 128512);
    }

    #[test]
    fn test_parse_png_format() {
        // Small 1x1 red PNG
        let png_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";
        let result = parse_kitty_graphics_protocol("a=t,f=100,i=1", png_data);
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.graphic_data.is_some());
    }

    #[test]
    fn test_parse_multi_frame() {
        // 1x1 RGBA pixel
        let payload = "AAAA";
        let result = parse_kitty_graphics_protocol("a=f,i=1,r=2,s=1,v=1,f=32", payload);
        // Frame action returns None for now (not implemented)
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_invalid_action() {
        let result = parse_kitty_graphics_protocol("a=x", "");
        assert!(result.is_some()); // Falls back to Transmit
    }

    #[test]
    fn test_parse_empty_keys() {
        // Empty params should return None
        let result = parse(&[]);
        assert!(result.is_none());

        // Just "G" with no control data returns an empty graphic
        let result = parse(&[b"G"]);
        assert!(result.is_some());
        let response = result.unwrap();
        assert!(response.graphic_data.is_some());
        let graphic = response.graphic_data.unwrap();
        assert_eq!(graphic.width, 0);
        assert_eq!(graphic.height, 0);
        assert!(graphic.pixels.is_empty());

        // "G" with empty control data also returns an empty graphic
        let result = parse(&[b"G", b""]);
        assert!(result.is_some());
    }

    #[test]
    fn test_incomplete_image_accumulation() {
        // Clear any previous state
        INCOMPLETE_IMAGES.lock().unwrap().clear();

        // First chunk - 1x1 RGBA pixel split into chunks
        // Total base64 for [255, 0, 0, 255] is "/wAA/w=="
        let result1 = parse_kitty_graphics_protocol("a=t,f=32,s=1,v=1,m=1,i=100", "/wA");
        assert!(result1.is_none()); // Should accumulate

        // Second chunk - need to specify action and image id
        let result2 = parse_kitty_graphics_protocol("a=t,m=1,i=100", "A/");
        assert!(result2.is_none()); // Should accumulate

        // Final chunk - need to specify action, image id, and dimensions
        let result3 = parse_kitty_graphics_protocol("a=t,f=32,s=1,v=1,m=0,i=100", "w==");
        assert!(result3.is_some()); // Should return complete image

        let response = result3.unwrap();
        assert!(response.graphic_data.is_some());

        // Clean up
        INCOMPLETE_IMAGES.lock().unwrap().clear();
    }

    #[test]
    fn test_file_transmission_medium() {
        // Create a temporary file
        use std::io::Write;
        let temp_path = "/tmp/test_kitty_image.rgba";
        let mut file = std::fs::File::create(temp_path).unwrap();
        file.write_all(&[255, 0, 0, 255]).unwrap(); // 1x1 red pixel
        drop(file);

        let result = parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1,i=1", temp_path);
        assert!(result.is_some());

        let response = result.unwrap();
        assert!(response.graphic_data.is_some());

        // Cleanup
        let _ = std::fs::remove_file(temp_path);
    }

    #[test]
    fn test_temp_file_transmission_medium() {
        // Create a temporary file with required naming
        use std::io::Write;
        let temp_path = "/tmp/tty-graphics-protocol-test.rgba";
        let mut file = std::fs::File::create(temp_path).unwrap();
        file.write_all(&[255, 0, 0, 255]).unwrap(); // 1x1 red pixel
        drop(file);

        let result = parse_kitty_graphics_protocol("a=t,t=t,f=32,s=1,v=1,i=1", temp_path);

        // File should be deleted after reading
        assert!(!std::path::Path::new(temp_path).exists());

        assert!(result.is_some());
        let response = result.unwrap();
        assert!(response.graphic_data.is_some());
    }

    #[test]
    fn test_security_checks() {
        // Should reject sensitive paths
        let result =
            parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1", "/proc/self/environ");
        assert!(result.is_none());

        let result =
            parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1", "/sys/class/net");
        assert!(result.is_none());

        let result = parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1", "/dev/null");
        assert!(result.is_none());
    }

    #[test]
    fn test_quiet_mode() {
        // q=1 should suppress OK response for placement
        let result = parse_kitty_graphics_protocol("a=p,i=1,q=1", "");
        assert!(result.is_some());

        let response = result.unwrap();
        // Placement with q=1 should not have response
        assert!(response.response.is_none());

        // q=2 should suppress all responses including query
        let result = parse_kitty_graphics_protocol("a=q,i=1,q=2", "");
        assert!(result.is_some());

        let response = result.unwrap();
        // Query with q=2 should have empty response
        assert_eq!(response.response, Some(String::new()));
    }
}
