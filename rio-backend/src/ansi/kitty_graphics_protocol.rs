use base64::{engine::general_purpose::{STANDARD as BASE64, STANDARD_NO_PAD}, Engine};
use std::collections::HashMap;
use std::sync::Mutex;
use sugarloaf::{ColorType, GraphicData, GraphicId, ResizeCommand, ResizeParameter};
use tracing::debug;

// Global storage for incomplete image transfers
// Stores the accumulated command state for chunked transmissions
lazy_static::lazy_static! {
    static ref INCOMPLETE_IMAGES: Mutex<HashMap<u32, KittyGraphicsCommand>> = Mutex::new(HashMap::new());
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
    pub cursor_movement: u8, // 0 = move cursor to after image (default), 1 = don't move cursor
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

#[derive(Debug, Clone)]
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
    let Some(&b"G") = params.first() else {
        debug!("Kitty graphics parse failed: first param is not 'G'");
        return None;
    };
    debug!("Kitty graphics parse: starting with {} params", params.len());
    for (i, param) in params.iter().enumerate() {
        debug!("  param[{}] length={}, preview={:?}", i, param.len(),
            std::str::from_utf8(&param[..param.len().min(50)]).unwrap_or("(invalid utf8)"));
    }

    let mut cmd = KittyGraphicsCommand::default();

    // Parse control data if present
    if let Some(control) = params.get(1) {
        if !control.is_empty() {
            let control_data = std::str::from_utf8(control).ok()?;
            parse_control_data(&mut cmd, control_data);
        }
    }

    // Parse payload if present
    if let Some(payload) = params.get(2) {
        if !payload.is_empty() {
            cmd.payload = payload.to_vec();
        }
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
    // Determine the key - use image_number if image_id is not set (0)
    // For chunked images, kitty uses image_number=0 as a special "current transmission" key
    let image_key = if cmd.image_id > 0 {
        cmd.image_id
    } else if cmd.image_number > 0 {
        cmd.image_number
    } else {
        // Use 0 as the key for anonymous chunked transmissions
        0
    };

    if cmd.more {
        // Store chunk for later - preserve all metadata from first chunk
        let mut incomplete = INCOMPLETE_IMAGES.lock().unwrap();

        // Get existing command or create new one
        let stored_cmd = incomplete.entry(image_key).or_insert_with(|| cmd.clone());

        // If this isn't the first chunk for this image, just append payload
        if !stored_cmd.payload.is_empty() && stored_cmd.payload != cmd.payload {
            stored_cmd.payload.extend_from_slice(&cmd.payload);
        } else {
            // First chunk - store entire command
            *stored_cmd = cmd.clone();
        }

        debug!("Stored chunk for image key {}: {} bytes accumulated", image_key, stored_cmd.payload.len());
        return None;
    } else {
        // Check if we have incomplete data (even if image_id/number is 0)
        let mut incomplete = INCOMPLETE_IMAGES.lock().unwrap();
        if let Some(mut stored_cmd) = incomplete.remove(&image_key) {
            // Final chunk: use metadata from stored command, append final payload
            stored_cmd.payload.extend_from_slice(&cmd.payload);
            cmd = stored_cmd; // Use stored metadata
            debug!("Retrieved accumulated image key {}: total {} bytes", image_key, cmd.payload.len());
        }
    }

    // Convert to GraphicData based on action
    debug!("Kitty graphics action: {:?}, format={:?}, width={}, height={}, image_id={}, payload_len={}",
        cmd.action, cmd.format, cmd.width, cmd.height, cmd.image_id, cmd.payload.len());
    match cmd.action {
        Action::Transmit | Action::TransmitAndDisplay => {
            debug!("Creating graphic data: format={:?}, medium={:?}, compression={:?}, width={}, height={}, payload_len={}",
                cmd.format, cmd.medium, cmd.compression, cmd.width, cmd.height, cmd.payload.len());
            let graphic_data = create_graphic_data(&cmd)?;
            debug!("Graphic data created successfully: {}x{}", graphic_data.width, graphic_data.height);
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
                    cursor_movement: cmd.cursor_movement,
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
                cursor_movement: cmd.cursor_movement,
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
                    cmd.delete_action = value.as_bytes().first().copied().unwrap_or(b'a')
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
            debug!("Decoding base64 payload, length={}", cmd.payload.len());
            match BASE64.decode(&cmd.payload) {
                Ok(data) => {
                    debug!("Base64 decoded successfully: {} bytes", data.len());
                    data
                }
                Err(e) => {
                    debug!("Base64 decode failed: {:?}", e);
                    return None;
                }
            }
        }
        TransmissionMedium::File | TransmissionMedium::TempFile => {
            // Read from file
            use std::fs::File;
            use std::io::Read;
            use std::path::Path;

            // Decode base64 payload to get file path
            // Try with standard decoder first, then without padding if that fails
            debug!("Decoding base64 file path, payload length={}", cmd.payload.len());
            let path_bytes = match BASE64.decode(&cmd.payload) {
                Ok(bytes) => {
                    debug!("Base64 decoded file path with padding: {} bytes", bytes.len());
                    bytes
                }
                Err(_) => {
                    // Try without padding requirement
                    match STANDARD_NO_PAD.decode(&cmd.payload) {
                        Ok(bytes) => {
                            debug!("Base64 decoded file path without padding: {} bytes", bytes.len());
                            bytes
                        }
                        Err(e) => {
                            debug!("Base64 decode failed (both with and without padding): {:?}", e);
                            return None;
                        }
                    }
                }
            };
            let path_str = std::str::from_utf8(&path_bytes).ok()?;
            debug!("File path: {}", path_str);
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
            if cmd.medium == TransmissionMedium::TempFile && !path_str.contains("tty-graphics-protocol") {
                return None;
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
            #[cfg(unix)]
            {
                use std::ffi::CString;
                use std::os::unix::io::RawFd;

                // Payload contains the base64-encoded shared memory name
                debug!("Decoding shared memory name from base64, payload length={}", cmd.payload.len());
                let shm_name_bytes = match BASE64.decode(&cmd.payload) {
                    Ok(bytes) => {
                        debug!("Base64 decoded shm name: {} bytes", bytes.len());
                        bytes
                    }
                    Err(e) => {
                        debug!("Failed to decode shm name from base64: {:?}", e);
                        return None;
                    }
                };
                let shm_name_str = std::str::from_utf8(&shm_name_bytes).ok()?;
                let shm_name = CString::new(shm_name_str).ok()?;

                debug!(
                    "Opening shared memory: {}, expected size: {}",
                    shm_name_str,
                    cmd.width as usize * cmd.height as usize * 3 // RGB24
                );

                unsafe {
                    // Open shared memory
                    let fd: RawFd = libc::shm_open(
                        shm_name.as_ptr(),
                        libc::O_RDONLY,
                        0,
                    );

                    if fd < 0 {
                        let err = std::io::Error::last_os_error();
                        let errno = err.raw_os_error().unwrap_or(-1);
                        debug!("Failed to open shared memory '{}': {} (errno: {})", shm_name_str, err, errno);
                        return None;
                    }

                    // Get size of shared memory
                    let mut stat: libc::stat = std::mem::zeroed();
                    if libc::fstat(fd, &mut stat) < 0 {
                        libc::close(fd);
                        libc::shm_unlink(shm_name.as_ptr());
                        debug!("Failed to fstat shared memory");
                        return None;
                    }

                    let shm_size = stat.st_size as usize;
                    debug!("Shared memory size: {} bytes", shm_size);

                    // Use cmd.size if specified, otherwise use the full shm size
                    let data_size = if cmd.size > 0 {
                        cmd.size as usize
                    } else {
                        shm_size
                    };

                    if data_size > shm_size {
                        libc::close(fd);
                        libc::shm_unlink(shm_name.as_ptr());
                        debug!("Requested size {} exceeds shared memory size {}", data_size, shm_size);
                        return None;
                    }

                    // Map shared memory
                    let ptr = libc::mmap(
                        std::ptr::null_mut(),
                        data_size,
                        libc::PROT_READ,
                        libc::MAP_SHARED,
                        fd,
                        cmd.offset as i64,
                    );

                    if ptr == libc::MAP_FAILED {
                        libc::close(fd);
                        debug!("Failed to mmap shared memory");
                        return None;
                    }

                    // Copy data from shared memory
                    let data = std::slice::from_raw_parts(ptr as *const u8, data_size).to_vec();

                    // Cleanup
                    libc::munmap(ptr, data_size);
                    libc::close(fd);
                    libc::shm_unlink(shm_name.as_ptr());

                    debug!("Successfully read {} bytes from shared memory", data.len());
                    data
                }
            }
            #[cfg(windows)]
            {
                use std::os::windows::ffi::OsStrExt;
                use std::ffi::OsStr;
                use windows_sys::Win32::System::Memory::{
                    MapViewOfFile, UnmapViewOfFile, VirtualQuery,
                    FILE_MAP_READ, MEMORY_BASIC_INFORMATION,
                };
                use windows_sys::Win32::Foundation::CloseHandle;
                use windows_sys::Win32::System::Memory::OpenFileMappingW;

                // Payload contains the base64-encoded shared memory name
                debug!("Decoding shared memory name from base64, payload length={}", cmd.payload.len());
                let shm_name_bytes = match BASE64.decode(&cmd.payload) {
                    Ok(bytes) => {
                        debug!("Base64 decoded shm name: {} bytes", bytes.len());
                        bytes
                    }
                    Err(e) => {
                        debug!("Failed to decode shm name from base64: {:?}", e);
                        return None;
                    }
                };
                let shm_name_str = std::str::from_utf8(&shm_name_bytes).ok()?;

                debug!("Opening shared memory: {}", shm_name_str);

                unsafe {
                    // Convert to wide string for Windows API
                    let wide_name: Vec<u16> = OsStr::new(shm_name_str)
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();

                    // Open the file mapping
                    let handle = OpenFileMappingW(
                        FILE_MAP_READ,
                        0,
                        wide_name.as_ptr(),
                    );

                    if handle == 0 {
                        let err = std::io::Error::last_os_error();
                        debug!("Failed to open shared memory '{}': {}", shm_name_str, err);
                        return None;
                    }

                    // Map view of file
                    let base_ptr = MapViewOfFile(
                        handle,
                        FILE_MAP_READ,
                        0,
                        0,
                        0,
                    );

                    if base_ptr.is_null() {
                        let err = std::io::Error::last_os_error();
                        debug!("Failed to map view of file: {}", err);
                        CloseHandle(handle);
                        return None;
                    }

                    // Query memory to get size
                    let mut mem_info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
                    if VirtualQuery(
                        base_ptr,
                        &mut mem_info,
                        std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                    ) == 0 {
                        debug!("Failed to query memory information");
                        UnmapViewOfFile(base_ptr);
                        CloseHandle(handle);
                        return None;
                    }

                    let shm_size = mem_info.RegionSize;
                    debug!("Shared memory size: {} bytes", shm_size);

                    // Use cmd.size if specified, otherwise use the full shm size
                    let data_size = if cmd.size > 0 {
                        cmd.size as usize
                    } else {
                        shm_size
                    };

                    // Validate offset and size
                    if cmd.offset as usize + data_size > shm_size {
                        debug!("Requested offset {} + size {} exceeds shared memory size {}",
                            cmd.offset, data_size, shm_size);
                        UnmapViewOfFile(base_ptr);
                        CloseHandle(handle);
                        return None;
                    }

                    // Copy data from shared memory
                    let data_ptr = (base_ptr as *const u8).add(cmd.offset as usize);
                    let data = std::slice::from_raw_parts(data_ptr, data_size).to_vec();

                    // Cleanup
                    UnmapViewOfFile(base_ptr);
                    CloseHandle(handle);

                    debug!("Successfully read {} bytes from shared memory", data.len());
                    data
                }
            }
            #[cfg(not(any(unix, windows)))]
            {
                debug!("SharedMemory transmission not supported on this platform");
                return None;
            }
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

            debug!("Decoding PNG, pixel_data length: {}", pixel_data.len());
            let img = match image_rs::load_from_memory_with_format(&pixel_data, ImageFormat::Png) {
                Ok(img) => {
                    debug!("PNG decoded successfully: {}x{}", img.width(), img.height());
                    img
                }
                Err(e) => {
                    debug!("PNG decode failed: {:?}", e);
                    return None;
                }
            };
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
            let bytes_per_pixel = match cmd.format {
                Format::Rgb24 => 3,
                Format::Rgba32 => 4,
                _ => unreachable!(),
            };

            // Validate data size - for shared memory, we may have more data than needed due to padding
            let expected_size =
                cmd.width as usize * cmd.height as usize * bytes_per_pixel;
            if pixel_data.len() < expected_size {
                debug!("RGB/RGBA data size insufficient: got {} bytes, expected at least {}", pixel_data.len(), expected_size);
                return None;
            }

            // Truncate to expected size if we have extra data (e.g., from shared memory padding)
            let pixel_data = if pixel_data.len() > expected_size {
                debug!("RGB/RGBA data has padding: got {} bytes, using first {} bytes", pixel_data.len(), expected_size);
                pixel_data[..expected_size].to_vec()
            } else {
                pixel_data
            };

            // Convert RGB24 to RGBA32 if needed (sugarloaf only supports RGBA)
            let (pixels, is_opaque) = if cmd.format == Format::Rgb24 {
                // Convert RGB to RGBA by adding alpha=255
                let mut rgba_pixels = Vec::with_capacity(cmd.width as usize * cmd.height as usize * 4);
                for chunk in pixel_data.chunks_exact(3) {
                    rgba_pixels.push(chunk[0]); // R
                    rgba_pixels.push(chunk[1]); // G
                    rgba_pixels.push(chunk[2]); // B
                    rgba_pixels.push(255);      // A (opaque)
                }
                debug!("Converted RGB24 to RGBA32: {} -> {} bytes", pixel_data.len(), rgba_pixels.len());
                (rgba_pixels, true) // RGB is always opaque
            } else {
                // Already RGBA
                let is_opaque = pixel_data.chunks(4).all(|chunk| chunk[3] == 255);
                (pixel_data, is_opaque)
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
                color_type: ColorType::Rgba, // Always RGBA after conversion
                pixels,
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

        // Encode the file path as base64 (as kitty does)
        let encoded_path = BASE64.encode(temp_path.as_bytes());
        let result = parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1,i=1", &encoded_path);
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

        // Encode the file path as base64 (as kitty does)
        let encoded_path = BASE64.encode(temp_path.as_bytes());
        let result = parse_kitty_graphics_protocol("a=t,t=t,f=32,s=1,v=1,i=1", &encoded_path);

        // File should be deleted after reading
        assert!(!std::path::Path::new(temp_path).exists());

        assert!(result.is_some());
        let response = result.unwrap();
        assert!(response.graphic_data.is_some());
    }

    #[test]
    fn test_security_checks() {
        // Should reject sensitive paths - encode as base64
        let proc_path = BASE64.encode("/proc/self/environ".as_bytes());
        let result =
            parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1", &proc_path);
        assert!(result.is_none());

        let sys_path = BASE64.encode("/sys/class/net".as_bytes());
        let result =
            parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1", &sys_path);
        assert!(result.is_none());

        let dev_path = BASE64.encode("/dev/null".as_bytes());
        let result = parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1", &dev_path);
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
