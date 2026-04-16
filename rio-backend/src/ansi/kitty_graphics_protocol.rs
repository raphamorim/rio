use base64::{
    engine::general_purpose::{STANDARD as BASE64, STANDARD_NO_PAD},
    Engine,
};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use sugarloaf::{ColorType, GraphicData, GraphicId, ResizeCommand, ResizeParameter};
use tracing::debug;

/// Maximum width or height (per axis) we accept for a kitty-graphics
/// image. Matches ghostty / upstream kitty. Anything larger is a DoS
/// vector — we refuse with `EINVAL: dimensions too large`.
const MAX_DIMENSION: u32 = 10_000;

/// Maximum decoded payload size (bytes) we accept. 400 MiB matches
/// ghostty / upstream kitty. Guards against runaway base64 blobs filling
/// memory before `create_graphic_data` validates them.
const MAX_SIZE: usize = 400 * 1024 * 1024;

/// How long an in-progress chunked upload may sit idle before the
/// accumulator drops it. Prevents `incomplete_images` from growing
/// without bound when a client abandons a chunked transmission
/// mid-stream.
const CHUNK_STALE_TIMEOUT: Duration = Duration::from_secs(10);

/// Per-terminal state for Kitty graphics protocol.
/// This stores the accumulated command state for chunked transmissions.
/// Each terminal instance should have its own state to prevent conflicts between tabs.
#[derive(Debug, Default)]
pub struct KittyGraphicsState {
    /// Stores incomplete image transfers (chunked transmissions).
    /// Key is the image_id or image_number from the first chunk.
    incomplete_images: HashMap<u32, KittyGraphicsCommand>,

    /// Tracks the current transmission key for chunks that don't specify an image ID.
    /// This is used for continuation chunks that only have m=1 or m=0.
    current_transmission_key: u32,

    /// Counter for auto-assigned image IDs. Per kitty spec, when a
    /// client transmits an image without an explicit `i=` (or `I=`)
    /// the terminal must allocate one. We allocate from the high half
    /// of the u32 range (`0x80000000..`) so the auto-assigned IDs do
    /// not collide with client-supplied IDs (which clients typically
    /// pick from `1..0x80000000`).
    next_auto_image_id: u32,
}

impl KittyGraphicsState {
    /// Allocate a fresh image_id for an implicit transmission.
    fn allocate_image_id(&mut self) -> u32 {
        if self.next_auto_image_id < 0x80000000 {
            self.next_auto_image_id = 0x80000000;
        }
        let id = self.next_auto_image_id;
        self.next_auto_image_id =
            self.next_auto_image_id.checked_add(1).unwrap_or(0x80000000);
        id
    }
}

#[derive(Debug)]
pub struct KittyGraphicsResponse {
    pub graphic_data: Option<GraphicData>,
    pub placement_request: Option<PlacementRequest>,
    pub delete_request: Option<DeleteRequest>,
    pub response: Option<String>,
    /// True when this "response" is just a chunk-accumulation
    /// acknowledgement — the parser stored the chunk and is waiting
    /// for more. The dispatcher should treat this as a successful
    /// no-op and must NOT log a parse failure for it (yazi and other
    /// TUIs send hundreds of chunked frames per second; spamming
    /// warnings on each chunk is the bug fix this field enables).
    pub incomplete: bool,
}

impl KittyGraphicsResponse {
    /// Sentinel returned for an in-progress chunked transmission. The
    /// dispatcher recognises this as "data accumulated, no action
    /// needed", as opposed to `None` which now means "real parse
    /// error".
    fn pending_chunk() -> Self {
        Self {
            graphic_data: None,
            placement_request: None,
            delete_request: None,
            response: None,
            incomplete: true,
        }
    }
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
    /// Set when the request came in with `U=1` — the kitty Unicode-
    /// placeholder mode. The terminal should only register the
    /// placement metadata; the application emits the U+10EEEE
    /// placeholder cells itself afterwards (see kitty
    /// `kittens/icat/transmit.go:221` `write_unicode_placeholder`). The
    /// renderer scans visible cells for U+10EEEE and composites the
    /// image at the matching positions.
    pub virtual_placement: bool,
    /// Value of `u=N` (decimal codepoint that the application embedded
    /// in the placement). Distinct from `virtual_placement` (which is
    /// the uppercase `U=1` flag). Currently informational only.
    pub unicode_placeholder: u32,
    pub cursor_movement: u8, // 0 = move cursor to after image (default), 1 = don't move cursor
}

#[derive(Debug)]
pub struct DeleteRequest {
    pub action: u8,
    pub image_id: u32,
    /// Image number (I= key) — used by `d=n/N` variants to resolve an
    /// image via the client-assigned number rather than its id.
    pub image_number: u32,
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
    Gray,      // 1 byte per pixel
    GrayAlpha, // 2 bytes per pixel
    Rgb24,     // 3 bytes per pixel
    Rgba32,    // 4 bytes per pixel
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

    /// True when `image_id` was auto-assigned because the client did
    /// not supply `i=` or `I=`. Per kitty spec we must not echo a
    /// response back for these commands even though we now have an id
    /// internally.
    implicit_id: bool,

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

    /// Payload, always stored as already-base64-decoded bytes.
    ///
    /// Matches ghostty: we decode each APC command's base64 payload up
    /// front in `parse()` so that clients which pad every chunk
    /// independently (e.g. chafa) don't produce a concatenated base64
    /// string with `=` bytes stuck in the middle when multiple chunks
    /// are merged.
    ///
    /// 64 bytes inline covers most control-only commands (query, delete,
    /// placement) while still handling large image data by spilling to
    /// heap.
    payload: SmallVec<[u8; 64]>,

    /// Wall-clock time of the most recent chunk for this command. Used
    /// to evict abandoned chunked uploads from the accumulator. Only
    /// meaningful while the command lives in `incomplete_images`.
    last_touched: Instant,
}

impl Default for KittyGraphicsCommand {
    fn default() -> Self {
        Self {
            action: Action::Transmit,
            quiet: 0,
            implicit_id: false,
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
            payload: SmallVec::new(),
            last_touched: Instant::now(),
        }
    }
}

/// Build an APC response string of the form
/// `\x1b_G<keys>;<message>\x1b\\`, matching ghostty's encoder.
///
/// `image_id`, `image_number`, `placement_id` are all emitted (in that
/// order, comma-separated) when non-zero. When *all* of them are zero
/// this returns `None` — per kitty spec we don't send a response
/// without an identifier.
fn encode_response(
    image_id: u32,
    image_number: u32,
    placement_id: u32,
    message: &str,
) -> Option<String> {
    if image_id == 0 && image_number == 0 {
        return None;
    }

    let mut keys = String::new();
    if image_id > 0 {
        keys.push_str(&format!("i={image_id}"));
    }
    if image_number > 0 {
        if !keys.is_empty() {
            keys.push(',');
        }
        keys.push_str(&format!("I={image_number}"));
    }
    if placement_id > 0 {
        if !keys.is_empty() {
            keys.push(',');
        }
        keys.push_str(&format!("p={placement_id}"));
    }

    Some(format!("\x1b_G{keys};{message}\x1b\\"))
}

/// Same as `encode_response` but respects the `q=` quiet setting:
/// - `q=0`: emit both successes and failures
/// - `q=1`: emit only failures
/// - `q=2`: emit nothing
fn encode_response_quiet(
    image_id: u32,
    image_number: u32,
    placement_id: u32,
    message: &str,
    quiet: u8,
    is_error: bool,
) -> Option<String> {
    match quiet {
        0 => encode_response(image_id, image_number, placement_id, message),
        1 if is_error => encode_response(image_id, image_number, placement_id, message),
        _ => None,
    }
}

pub fn parse(
    params: &[&[u8]],
    state: &mut KittyGraphicsState,
) -> Option<KittyGraphicsResponse> {
    let Some(&b"G") = params.first() else {
        debug!("Kitty graphics parse failed: first param is not 'G'");
        return None;
    };
    debug!(
        "Kitty graphics parse: starting with {} params",
        params.len()
    );
    for (i, param) in params.iter().enumerate() {
        debug!(
            "  param[{}] length={}, preview={:?}",
            i,
            param.len(),
            std::str::from_utf8(&param[..param.len().min(50)])
                .unwrap_or("(invalid utf8)")
        );
    }

    let mut cmd = KittyGraphicsCommand::default();

    // Parse control data if present
    if let Some(control) = params.get(1) {
        if !control.is_empty() {
            let control_data = std::str::from_utf8(control).ok()?;
            parse_control_data(&mut cmd, control_data);
        }
    }

    // Decode payload if present. We always decode base64 up front
    // (matching ghostty) so that each APC command's payload is
    // self-contained: clients like chafa which pad every chunk
    // independently can be merged by simply concatenating the decoded
    // byte streams, rather than trying to splice base64 text and running
    // into stray `=` padding in the middle.
    if let Some(payload) = params.get(2) {
        if !payload.is_empty() {
            let decoded = decode_payload_base64(payload)?;
            cmd.payload = SmallVec::from_vec(decoded);
        }
    }

    // Validation: `i=` and `I=` are mutually exclusive per kitty spec
    // (the image is either referenced by id or by number, never both).
    if cmd.image_id > 0 && cmd.image_number > 0 {
        return Some(KittyGraphicsResponse {
            graphic_data: None,
            placement_request: None,
            delete_request: None,
            response: encode_response_quiet(
                cmd.image_id,
                cmd.image_number,
                cmd.placement_id,
                "EINVAL: image ID and number are mutually exclusive",
                cmd.quiet,
                true,
            ),
            incomplete: false,
        });
    }

    // Handle query action: requires an image id per kitty spec. Without
    // one we cannot even build a response addressed to anything, so we
    // surface EINVAL instead of pretending success.
    if cmd.action == Action::Query {
        if cmd.image_id == 0 {
            return Some(KittyGraphicsResponse {
                graphic_data: None,
                placement_request: None,
                delete_request: None,
                response: encode_response_quiet(
                    cmd.image_id,
                    cmd.image_number,
                    cmd.placement_id,
                    "EINVAL: image ID required",
                    cmd.quiet,
                    true,
                ),
                incomplete: false,
            });
        }
        let response = encode_response_quiet(
            cmd.image_id,
            cmd.image_number,
            cmd.placement_id,
            "OK",
            cmd.quiet,
            false,
        );
        return Some(KittyGraphicsResponse {
            graphic_data: None,
            placement_request: None,
            delete_request: None,
            response,
            incomplete: false,
        });
    }

    // Handle chunked data
    // Determine the key for this chunk:
    // - If this chunk has an explicit image_id or image_number, use that.
    // - If no ID in this chunk and we are mid-transmission (a chunked
    //   command pinned `current_transmission_key`), reuse it.
    // - Otherwise the client sent a fresh command without an explicit id
    //   and we must allocate one per kitty spec.
    //
    // Importantly we only *pin* the key into `current_transmission_key`
    // when this is a chunked command (`cmd.more` is true). Pinning on
    // every command leaked into the next implicit command and made it
    // think it was a continuation chunk.
    let image_key = if cmd.image_id > 0 || cmd.image_number > 0 {
        if cmd.image_id > 0 {
            cmd.image_id
        } else {
            cmd.image_number
        }
    } else if state.current_transmission_key != 0 {
        // Continuation chunk: reuse the in-progress key
        state.current_transmission_key
    } else {
        // Fresh command without explicit id — allocate one. Mark as
        // implicit so we suppress the response per spec.
        let key = state.allocate_image_id();
        cmd.image_id = key;
        cmd.implicit_id = true;
        key
    };

    // Drop any chunked uploads that have been idle for too long. Runs
    // on every chunk event, so worst case we scan `incomplete_images`
    // once per APC — O(n) with n bounded by concurrent uploads.
    evict_stale_chunks(state);

    if cmd.more {
        // Pin the key for continuation chunks. Only chunked commands
        // touch `current_transmission_key` so non-chunked commands
        // don't leak state into subsequent transmissions.
        state.current_transmission_key = image_key;

        // Store chunk for later - preserve all metadata from first chunk.
        // Payload is already base64-decoded at this point, so subsequent
        // chunks can simply append their bytes.
        use std::collections::hash_map::Entry;

        match state.incomplete_images.entry(image_key) {
            Entry::Vacant(e) => {
                // First chunk - move cmd into storage (no clone!)
                // Pre-allocate capacity if size is known to avoid reallocations
                let expected_size = cmd.size as usize;
                if expected_size > 0 && cmd.payload.capacity() < expected_size {
                    cmd.payload
                        .reserve(expected_size.saturating_sub(cmd.payload.len()));
                    debug!(
                        "First chunk for image key {}: {} bytes, reserved {} bytes total",
                        image_key,
                        cmd.payload.len(),
                        expected_size
                    );
                } else {
                    debug!(
                        "First chunk for image key {}: {} bytes",
                        image_key,
                        cmd.payload.len()
                    );
                }
                cmd.last_touched = Instant::now();
                e.insert(cmd);
            }
            Entry::Occupied(mut e) => {
                // Subsequent chunk - append decoded bytes, refusing if
                // the accumulated size would exceed our cap.
                let stored_cmd = e.get_mut();
                if stored_cmd.payload.len().saturating_add(cmd.payload.len()) > MAX_SIZE {
                    debug!(
                        "Dropping chunked upload {}: would exceed MAX_SIZE ({})",
                        image_key, MAX_SIZE
                    );
                    // Evict the abandoned upload so it can't be resumed
                    // into an oversized state.
                    e.remove();
                    state.current_transmission_key = 0;
                    return None;
                }
                stored_cmd.payload.extend_from_slice(&cmd.payload);
                stored_cmd.last_touched = Instant::now();
                debug!(
                    "Appended chunk for image key {}: {} bytes accumulated",
                    image_key,
                    stored_cmd.payload.len()
                );
            }
        }
        // Tell the dispatcher this is an in-progress chunked
        // transmission, not an error. Returning None here would have
        // been logged as "Failed to parse" — yazi sends hundreds of
        // chunks per image preview and that flooded the warning log.
        return Some(KittyGraphicsResponse::pending_chunk());
    } else {
        // Check if we have incomplete data (even if image_id/number is 0)
        if let Some(mut stored_cmd) = state.incomplete_images.remove(&image_key) {
            // Final chunk: append this chunk's decoded bytes to the
            // already-accumulated stored payload.
            if stored_cmd.payload.len().saturating_add(cmd.payload.len()) > MAX_SIZE {
                debug!(
                    "Dropping final chunk {}: would exceed MAX_SIZE ({})",
                    image_key, MAX_SIZE
                );
                state.current_transmission_key = 0;
                return None;
            }
            stored_cmd.payload.extend_from_slice(&cmd.payload);
            cmd = stored_cmd; // Use stored metadata
            debug!(
                "Retrieved accumulated image key {}: total {} bytes",
                image_key,
                cmd.payload.len()
            );
            // Reset current transmission key after completing this transmission
            state.current_transmission_key = 0;
        }
    }

    // Convert to GraphicData based on action
    debug!("Kitty graphics action: {:?}, format={:?}, width={}, height={}, image_id={}, payload_len={}",
        cmd.action, cmd.format, cmd.width, cmd.height, cmd.image_id, cmd.payload.len());
    match cmd.action {
        Action::Transmit | Action::TransmitAndDisplay => {
            debug!("Creating graphic data: format={:?}, medium={:?}, compression={:?}, width={}, height={}, payload_len={}",
                cmd.format, cmd.medium, cmd.compression, cmd.width, cmd.height, cmd.payload.len());
            let graphic_data = match create_graphic_data(&cmd) {
                Ok(g) => g,
                Err(err) => {
                    return Some(KittyGraphicsResponse {
                        graphic_data: None,
                        placement_request: None,
                        delete_request: None,
                        response: if cmd.implicit_id {
                            None
                        } else {
                            encode_response_quiet(
                                cmd.image_id,
                                cmd.image_number,
                                cmd.placement_id,
                                err.message(),
                                cmd.quiet,
                                true,
                            )
                        },
                        incomplete: false,
                    });
                }
            };
            debug!(
                "Graphic data created successfully: {}x{}",
                graphic_data.width, graphic_data.height
            );
            let response = if cmd.implicit_id {
                None
            } else {
                encode_response_quiet(
                    graphic_data.id.get() as u32,
                    cmd.image_number,
                    cmd.placement_id,
                    "OK",
                    cmd.quiet,
                    false,
                )
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
                    virtual_placement: cmd.virtual_placement,
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
                incomplete: false,
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
                virtual_placement: cmd.virtual_placement,
                unicode_placeholder: cmd.unicode_placeholder,
                cursor_movement: cmd.cursor_movement,
            };
            let response = if cmd.implicit_id {
                None
            } else {
                encode_response_quiet(
                    cmd.image_id,
                    cmd.image_number,
                    cmd.placement_id,
                    "OK",
                    cmd.quiet,
                    false,
                )
            };
            Some(KittyGraphicsResponse {
                graphic_data: None,
                placement_request: Some(placement),
                delete_request: None,
                response,
                incomplete: false,
            })
        }
        Action::Delete => {
            // Handle delete request
            let delete_data = cmd.delete_action.is_ascii_uppercase();
            let delete = DeleteRequest {
                action: cmd.delete_action.to_ascii_lowercase(),
                image_id: cmd.image_id,
                image_number: cmd.image_number,
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
                incomplete: false,
            })
        }
        Action::Query => {
            // Query is handled earlier in the function before the
            // chunking branches; the early return makes this arm
            // unreachable in practice.
            unreachable!("Query handled above")
        }
        Action::Frame | Action::Animate | Action::Compose => {
            // Animation actions are not supported. Per the kitty spec we
            // surface this so clients can detect the lack of support and
            // fall back, instead of silently dropping the command.
            // (Any chunked accumulation for this key was already drained
            // above when we entered the final-chunk branch.)
            //
            // Implicit-id transmissions still get no response, so the
            // client never sees stray APC traffic it didn't ask for.
            let response = if cmd.implicit_id {
                None
            } else {
                // Fall back to a bare APC when there's no id at all, so
                // clients that probe without an id still see the error.
                encode_response_quiet(
                    cmd.image_id,
                    cmd.image_number,
                    cmd.placement_id,
                    "EINVAL:unsupported action",
                    cmd.quiet,
                    true,
                )
                .or_else(|| match cmd.quiet {
                    2 => None,
                    _ => Some("\x1b_G;EINVAL:unsupported action\x1b\\".to_string()),
                })
            };
            Some(KittyGraphicsResponse {
                graphic_data: None,
                placement_request: None,
                delete_request: None,
                response,
                incomplete: false,
            })
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
        "8" => Format::Gray,
        "16" => Format::GrayAlpha,
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

/// Evict entries from `incomplete_images` that have not received a
/// chunk within `CHUNK_STALE_TIMEOUT`. Prevents unbounded growth when
/// clients abandon chunked uploads.
fn evict_stale_chunks(state: &mut KittyGraphicsState) {
    if state.incomplete_images.is_empty() {
        return;
    }
    let now = Instant::now();
    let before = state.incomplete_images.len();
    state
        .incomplete_images
        .retain(|_, cmd| now.duration_since(cmd.last_touched) < CHUNK_STALE_TIMEOUT);
    let after = state.incomplete_images.len();
    if after < before {
        debug!(
            "Evicted {} stale chunked uploads (>{}s idle)",
            before - after,
            CHUNK_STALE_TIMEOUT.as_secs()
        );
        // If the pinned transmission key was evicted, clear it so that a
        // new chunkless command can't accidentally resume it.
        if !state
            .incomplete_images
            .contains_key(&state.current_transmission_key)
        {
            state.current_transmission_key = 0;
        }
    }
}

/// Decode a single APC command's base64 payload.
///
/// Tries the standard (padded) decoder first, falling back to the
/// no-padding variant so that chunks from spec-compliant clients
/// (which don't pad intermediate chunks) also decode cleanly.
///
/// Callers decode each APC command's payload independently — the same
/// approach ghostty uses — so that per-chunk padding from clients like
/// chafa is contained within its own chunk instead of contaminating the
/// merged byte stream.
fn decode_payload_base64(payload: &[u8]) -> Option<Vec<u8>> {
    if payload.is_empty() {
        return Some(Vec::new());
    }
    match BASE64.decode(payload) {
        Ok(data) => Some(data),
        Err(_) => match STANDARD_NO_PAD.decode(payload) {
            Ok(data) => Some(data),
            Err(e) => {
                debug!("Base64 payload decode failed: {:?}", e);
                None
            }
        },
    }
}

/// Error emitted from `create_graphic_data`. Maps directly to kitty
/// protocol EINVAL/ENOENT/E2BIG message strings so the caller can
/// surface them in a response.
#[derive(Debug)]
#[allow(dead_code)] // UnsupportedFormat/Medium are platform-gated
enum GraphicError {
    DimensionsTooLarge,
    DimensionsRequired,
    TooLarge,
    UnsupportedFormat,
    UnsupportedMedium,
    InvalidData,
    DecompressionFailed,
    FileNotFound,
}

impl GraphicError {
    fn message(&self) -> &'static str {
        match self {
            GraphicError::DimensionsTooLarge => "EINVAL: dimensions too large",
            GraphicError::DimensionsRequired => "EINVAL: dimensions required",
            GraphicError::TooLarge => "E2BIG: image too large",
            GraphicError::UnsupportedFormat => "EINVAL: unsupported format",
            GraphicError::UnsupportedMedium => "EINVAL: unsupported medium",
            GraphicError::InvalidData => "EINVAL: invalid data",
            GraphicError::DecompressionFailed => "EINVAL: decompression failed",
            GraphicError::FileNotFound => "ENOENT: file not found",
        }
    }
}

fn create_graphic_data(cmd: &KittyGraphicsCommand) -> Result<GraphicData, GraphicError> {
    // Early dimension guard — applies to every non-PNG path. PNG
    // commands may transmit without declaring width/height (we pick
    // them up after decoding); we re-check post-decode.
    if cmd.format != Format::Png
        && (cmd.width > MAX_DIMENSION || cmd.height > MAX_DIMENSION)
    {
        debug!(
            "Rejecting kitty image: {}x{} exceeds {} cap",
            cmd.width, cmd.height, MAX_DIMENSION
        );
        return Err(GraphicError::DimensionsTooLarge);
    }

    // Payload is already base64-decoded by parse(). Pick up the bytes
    // based on the transmission medium — direct means they're the image
    // bytes, file/shm means they're a path/name.
    let raw_data = match cmd.medium {
        TransmissionMedium::Direct => {
            if cmd.payload.len() > MAX_SIZE {
                return Err(GraphicError::TooLarge);
            }
            debug!("Using decoded Direct payload: {} bytes", cmd.payload.len());
            cmd.payload.to_vec()
        }
        TransmissionMedium::File | TransmissionMedium::TempFile => {
            // Read from file
            use std::fs::File;
            use std::io::Read;
            use std::path::Path;

            // Payload is already base64-decoded by parse(); the bytes
            // directly represent the file path.
            debug!("File path payload: {} bytes", cmd.payload.len());
            let path_str = std::str::from_utf8(&cmd.payload)
                .map_err(|_| GraphicError::InvalidData)?;
            debug!("File path: {}", path_str);
            let path = Path::new(path_str);

            // Security checks
            if !path.is_file() {
                return Err(GraphicError::FileNotFound);
            }

            // Check for sensitive paths
            let path_str_lower = path_str.to_lowercase();
            if path_str_lower.contains("/proc/")
                || path_str_lower.contains("/sys/")
                || path_str_lower.contains("/dev/")
            {
                return Err(GraphicError::InvalidData);
            }

            // For temp files, verify it contains "tty-graphics-protocol"
            if cmd.medium == TransmissionMedium::TempFile
                && !path_str.contains("tty-graphics-protocol")
            {
                return Err(GraphicError::InvalidData);
            }

            // Cap the explicit `S=` read size before we allocate. Keeps
            // a malicious `S=<huge>` from exploding our heap.
            if cmd.size as usize > MAX_SIZE {
                return Err(GraphicError::TooLarge);
            }

            let mut file = File::open(path).map_err(|_| GraphicError::FileNotFound)?;
            let mut data = Vec::new();

            if cmd.size > 0 {
                // Read specific size from offset
                if cmd.offset > 0 {
                    use std::io::Seek;
                    file.seek(std::io::SeekFrom::Start(cmd.offset as u64))
                        .map_err(|_| GraphicError::InvalidData)?;
                }
                data.resize(cmd.size as usize, 0);
                file.read_exact(&mut data)
                    .map_err(|_| GraphicError::InvalidData)?;
            } else {
                // Read entire file. Cap the total so a huge file on disk
                // can't be silently loaded through this channel.
                let limit = (MAX_SIZE as u64).saturating_add(1);
                file.take(limit)
                    .read_to_end(&mut data)
                    .map_err(|_| GraphicError::InvalidData)?;
                if data.len() > MAX_SIZE {
                    return Err(GraphicError::TooLarge);
                }
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

                // Payload is already base64-decoded by parse(); the bytes
                // directly represent the shared memory name.
                debug!("Shared memory name payload: {} bytes", cmd.payload.len());
                let shm_name_str = std::str::from_utf8(&cmd.payload)
                    .map_err(|_| GraphicError::InvalidData)?;
                let shm_name =
                    CString::new(shm_name_str).map_err(|_| GraphicError::InvalidData)?;

                debug!(
                    "Opening shared memory: {}, expected size: {}",
                    shm_name_str,
                    cmd.width as usize * cmd.height as usize * 3 // RGB24
                );

                unsafe {
                    // Open shared memory
                    let fd: RawFd = libc::shm_open(shm_name.as_ptr(), libc::O_RDONLY, 0);

                    if fd < 0 {
                        let err = std::io::Error::last_os_error();
                        let errno = err.raw_os_error().unwrap_or(-1);
                        debug!(
                            "Failed to open shared memory '{}': {} (errno: {})",
                            shm_name_str, err, errno
                        );
                        return Err(GraphicError::FileNotFound);
                    }

                    // Get size of shared memory
                    let mut stat: libc::stat = std::mem::zeroed();
                    if libc::fstat(fd, &mut stat) < 0 {
                        libc::close(fd);
                        libc::shm_unlink(shm_name.as_ptr());
                        debug!("Failed to fstat shared memory");
                        return Err(GraphicError::InvalidData);
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
                        debug!(
                            "Requested size {} exceeds shared memory size {}",
                            data_size, shm_size
                        );
                        return Err(GraphicError::InvalidData);
                    }

                    if data_size > MAX_SIZE {
                        libc::close(fd);
                        libc::shm_unlink(shm_name.as_ptr());
                        return Err(GraphicError::TooLarge);
                    }

                    // Map shared memory
                    let ptr = libc::mmap(
                        std::ptr::null_mut(),
                        data_size,
                        libc::PROT_READ,
                        libc::MAP_SHARED,
                        fd,
                        cmd.offset as libc::off_t,
                    );

                    if ptr == libc::MAP_FAILED {
                        libc::close(fd);
                        debug!("Failed to mmap shared memory");
                        return Err(GraphicError::InvalidData);
                    }

                    // Copy data from shared memory
                    let data =
                        std::slice::from_raw_parts(ptr as *const u8, data_size).to_vec();

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
                use std::ffi::OsStr;
                use std::os::windows::ffi::OsStrExt;
                use windows_sys::Win32::Foundation::CloseHandle;
                use windows_sys::Win32::System::Memory::OpenFileMappingW;
                use windows_sys::Win32::System::Memory::{
                    MapViewOfFile, UnmapViewOfFile, VirtualQuery, FILE_MAP_READ,
                    MEMORY_BASIC_INFORMATION,
                };

                // Payload is already base64-decoded by parse(); the bytes
                // directly represent the shared memory name.
                debug!("Shared memory name payload: {} bytes", cmd.payload.len());
                let shm_name_str = std::str::from_utf8(&cmd.payload)
                    .map_err(|_| GraphicError::InvalidData)?;

                debug!("Opening shared memory: {}", shm_name_str);

                unsafe {
                    // Convert to wide string for Windows API
                    let wide_name: Vec<u16> = OsStr::new(shm_name_str)
                        .encode_wide()
                        .chain(std::iter::once(0))
                        .collect();

                    // Open the file mapping
                    let handle = OpenFileMappingW(FILE_MAP_READ, 0, wide_name.as_ptr());

                    if handle.is_null() {
                        let err = std::io::Error::last_os_error();
                        debug!(
                            "Failed to open shared memory '{}': {}",
                            shm_name_str, err
                        );
                        return Err(GraphicError::FileNotFound);
                    }

                    // Map view of file
                    let base_ptr = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0);

                    if base_ptr.Value.is_null() {
                        let err = std::io::Error::last_os_error();
                        debug!("Failed to map view of file: {}", err);
                        CloseHandle(handle);
                        return Err(GraphicError::InvalidData);
                    }

                    // Query memory to get size
                    let mut mem_info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
                    if VirtualQuery(
                        base_ptr.Value,
                        &mut mem_info,
                        std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                    ) == 0
                    {
                        debug!("Failed to query memory information");
                        UnmapViewOfFile(base_ptr);
                        CloseHandle(handle);
                        return Err(GraphicError::InvalidData);
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
                        debug!(
                            "Requested offset {} + size {} exceeds shared memory size {}",
                            cmd.offset, data_size, shm_size
                        );
                        UnmapViewOfFile(base_ptr);
                        CloseHandle(handle);
                        return Err(GraphicError::InvalidData);
                    }

                    if data_size > MAX_SIZE {
                        UnmapViewOfFile(base_ptr);
                        CloseHandle(handle);
                        return Err(GraphicError::TooLarge);
                    }

                    // Copy data from shared memory
                    let data_ptr = (base_ptr.Value as *const u8).add(cmd.offset as usize);
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
                return Err(GraphicError::UnsupportedMedium);
            }
        }
    };

    // Decompress if needed
    let pixel_data = match cmd.compression {
        Compression::None => raw_data,
        Compression::Zlib => {
            use flate2::read::ZlibDecoder;
            use std::io::Read;

            let decoder = ZlibDecoder::new(&raw_data[..]);
            // Cap decompressed output so zip bombs can't wedge the
            // terminal. `Take` passes the limit through the decoder; we
            // then check the size we actually got.
            let mut decompressed = Vec::new();
            decoder
                .take((MAX_SIZE as u64).saturating_add(1))
                .read_to_end(&mut decompressed)
                .map_err(|_| GraphicError::DecompressionFailed)?;
            if decompressed.len() > MAX_SIZE {
                return Err(GraphicError::TooLarge);
            }
            decompressed
        }
    };

    // Parse based on format
    match cmd.format {
        Format::Png => {
            // Decode PNG data
            use image_rs::ImageFormat;

            debug!("Decoding PNG, pixel_data length: {}", pixel_data.len());
            let img = match image_rs::load_from_memory_with_format(
                &pixel_data,
                ImageFormat::Png,
            ) {
                Ok(img) => {
                    debug!("PNG decoded successfully: {}x{}", img.width(), img.height());
                    img
                }
                Err(e) => {
                    debug!("PNG decode failed: {:?}", e);
                    return Err(GraphicError::InvalidData);
                }
            };
            // PNG dimensions come from the decoded header — now enforce
            // the cap we couldn't check up front.
            if img.width() > MAX_DIMENSION || img.height() > MAX_DIMENSION {
                return Err(GraphicError::DimensionsTooLarge);
            }
            let rgba_img = img.to_rgba8();
            let (width, height) = (rgba_img.width() as usize, rgba_img.height() as usize);
            let pixels = rgba_img.into_raw();

            // Check if image is opaque
            let is_opaque = pixels.chunks(4).all(|chunk| chunk[3] == 255);

            // Create resize command if columns/rows specified
            // When both c= and r= are given, stretch to fill (no aspect ratio).
            // When only one is given, compute the other preserving aspect ratio.
            let resize = if cmd.columns > 0 || cmd.rows > 0 {
                let both_specified = cmd.columns > 0 && cmd.rows > 0;
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
                    preserve_aspect_ratio: !both_specified,
                })
            } else {
                None
            };

            Ok(GraphicData {
                id: GraphicId::new(cmd.image_id as u64),
                width,
                height,
                color_type: ColorType::Rgba,
                pixels,
                is_opaque,
                resize,
                display_width: None,
                display_height: None,
                transmit_time: std::time::Instant::now(),
            })
        }
        Format::Gray | Format::GrayAlpha | Format::Rgb24 | Format::Rgba32 => {
            let bytes_per_pixel = match cmd.format {
                Format::Gray => 1,
                Format::GrayAlpha => 2,
                Format::Rgb24 => 3,
                Format::Rgba32 => 4,
                _ => unreachable!(),
            };

            if cmd.width == 0 || cmd.height == 0 {
                return Err(GraphicError::DimensionsRequired);
            }

            // Validate data size
            let expected_size =
                cmd.width as usize * cmd.height as usize * bytes_per_pixel;
            if expected_size > MAX_SIZE {
                return Err(GraphicError::TooLarge);
            }
            if pixel_data.len() < expected_size {
                debug!(
                    "Pixel data size insufficient: got {} bytes, expected at least {}",
                    pixel_data.len(),
                    expected_size
                );
                return Err(GraphicError::InvalidData);
            }

            // Truncate to expected size if we have extra data (e.g., from shared memory padding)
            let pixel_data = if pixel_data.len() > expected_size {
                pixel_data[..expected_size].to_vec()
            } else {
                pixel_data
            };

            // Convert all formats to RGBA (GPU only supports RGBA)
            let (pixels, is_opaque) = match cmd.format {
                Format::Gray => {
                    // 1 bpp: R=G=B=gray, A=255
                    let mut rgba =
                        Vec::with_capacity(cmd.width as usize * cmd.height as usize * 4);
                    for &g in &pixel_data {
                        rgba.extend_from_slice(&[g, g, g, 255]);
                    }
                    (rgba, true)
                }
                Format::GrayAlpha => {
                    // 2 bpp: R=G=B=gray, A=alpha
                    let mut rgba =
                        Vec::with_capacity(cmd.width as usize * cmd.height as usize * 4);
                    let mut opaque = true;
                    for chunk in pixel_data.chunks_exact(2) {
                        let g = chunk[0];
                        let a = chunk[1];
                        if a != 255 {
                            opaque = false;
                        }
                        rgba.extend_from_slice(&[g, g, g, a]);
                    }
                    (rgba, opaque)
                }
                Format::Rgb24 => {
                    // 3 bpp: add A=255
                    let mut rgba =
                        Vec::with_capacity(cmd.width as usize * cmd.height as usize * 4);
                    for chunk in pixel_data.chunks_exact(3) {
                        rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
                    }
                    (rgba, true)
                }
                Format::Rgba32 => {
                    // Already RGBA
                    let is_opaque = pixel_data.chunks(4).all(|chunk| chunk[3] == 255);
                    (pixel_data, is_opaque)
                }
                _ => unreachable!(),
            };

            // Create resize command if columns/rows specified
            // When both c= and r= are given, stretch to fill (no aspect ratio).
            // When only one is given, compute the other preserving aspect ratio.
            let resize = if cmd.columns > 0 || cmd.rows > 0 {
                let both_specified = cmd.columns > 0 && cmd.rows > 0;
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
                    preserve_aspect_ratio: !both_specified,
                })
            } else {
                None
            };

            Ok(GraphicData {
                id: GraphicId::new(cmd.image_id as u64),
                width: cmd.width as usize,
                height: cmd.height as usize,
                color_type: ColorType::Rgba, // Always RGBA after conversion
                pixels,
                is_opaque,
                resize,
                display_width: None,
                display_height: None,
                transmit_time: std::time::Instant::now(),
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

        let mut state = KittyGraphicsState::default();
        parse(&params, &mut state)
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
        // `u=N` is an informational hint, not the virtual-placement
        // trigger — leave that flag clear.
        assert!(!placement.virtual_placement);
    }

    #[test]
    fn test_parse_with_virtual_placement() {
        // What `kitten icat --unicode-placeholder` actually emits:
        // `_Ga=p,U=1,i=N,c=cols,r=rows,q=2\e\` to register the placement.
        // The parser must propagate `U=1` so `place_graphic` routes to
        // the virtual-placement path (metadata only, no cell writes).
        let result = parse_kitty_graphics_protocol("a=p,U=1,i=42,c=10,r=4,q=2", "");
        let response = result.expect("parse ok");
        let placement = response.placement_request.expect("placement");
        assert!(placement.virtual_placement);
        assert_eq!(placement.image_id, 42);
        assert_eq!(placement.columns, 10);
        assert_eq!(placement.rows, 4);
        // `U=1` doesn't set the lowercase `u` field.
        assert_eq!(placement.unicode_placeholder, 0);
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
    fn test_animation_frame_returns_unsupported_error() {
        // a=f (transmit animation frame) is not implemented; per spec we
        // surface EINVAL:unsupported action so clients can fall back.
        let payload = "AAAA";
        let result = parse_kitty_graphics_protocol("a=f,i=1,r=2,s=1,v=1,f=32", payload);
        let response = result.expect("animation actions must produce a response");
        assert!(response.graphic_data.is_none());
        assert!(response.placement_request.is_none());
        assert!(response.delete_request.is_none());
        let body = response.response.expect("error response expected");
        assert!(
            body.contains("i=1"),
            "response should echo image id: {body}"
        );
        assert!(
            body.contains("EINVAL:unsupported action"),
            "response should contain EINVAL: {body}"
        );
        assert!(body.starts_with("\x1b_G"), "response should be APC: {body}");
        assert!(
            body.ends_with("\x1b\\"),
            "response should end with ST: {body}"
        );
    }

    #[test]
    fn test_animation_control_returns_unsupported_error() {
        // a=a (animation control)
        let result = parse_kitty_graphics_protocol("a=a,i=42,s=3", "");
        let response = result.expect("animate action must produce a response");
        let body = response.response.expect("error response expected");
        assert!(body.contains("i=42"));
        assert!(body.contains("EINVAL:unsupported action"));
    }

    #[test]
    fn test_animation_compose_returns_unsupported_error() {
        // a=c (compose frames)
        let result = parse_kitty_graphics_protocol("a=c,i=7,r=1,c=2", "");
        let response = result.expect("compose action must produce a response");
        let body = response.response.expect("error response expected");
        assert!(body.contains("i=7"));
        assert!(body.contains("EINVAL:unsupported action"));
    }

    #[test]
    fn test_animation_error_uses_image_number_when_no_id() {
        // When only I= is given, the response should echo I=
        let result = parse_kitty_graphics_protocol("a=f,I=99,r=2,s=1,v=1,f=32", "AAAA");
        let response = result.expect("animation action must produce a response");
        let body = response.response.expect("error response expected");
        assert!(body.contains("I=99"), "expected I=99 in {body}");
        assert!(body.contains("EINVAL:unsupported action"));
    }

    #[test]
    fn test_animation_error_suppressed_when_quiet_2() {
        // q=2 should suppress error responses too
        let result =
            parse_kitty_graphics_protocol("a=f,i=1,r=2,s=1,v=1,f=32,q=2", "AAAA");
        let response = result.expect("response struct should still exist");
        assert!(
            response.response.is_none(),
            "q=2 should suppress error response"
        );
    }

    #[test]
    fn test_parse_invalid_action() {
        let result = parse_kitty_graphics_protocol("a=x", "");
        assert!(result.is_some()); // Falls back to Transmit
    }

    #[test]
    fn test_parse_empty_keys() {
        let mut state = KittyGraphicsState::default();

        // Empty params should return None
        let result = parse(&[], &mut state);
        assert!(result.is_none());

        // Just "G" with no control data defaults to action=t
        // (Transmit), then fails create_graphic_data because width=0 /
        // height=0 are not valid dimensions. The response carries no id
        // (implicit) so nothing is emitted.
        let result = parse(&[b"G"], &mut state);
        let response = result.expect("response struct must exist even on error");
        assert!(response.graphic_data.is_none());
        assert!(
            response.response.is_none(),
            "implicit-id failure must not emit a response"
        );

        // "G" with empty control data: same path
        let result = parse(&[b"G", b""], &mut state);
        assert!(result.is_some());
    }

    #[test]
    fn test_incomplete_image_accumulation() {
        // Use a single state instance across all chunks.
        // Per kitty spec each chunk must be a multiple of 4 base64 chars
        // (i.e. aligned on 3-byte binary boundaries) with padding only on
        // the final chunk. Full base64 for [255, 0, 0, 255] is "/wAA/w==".
        let mut state = KittyGraphicsState::default();

        // First chunk: 4 chars → 3 decoded bytes [0xFF, 0x00, 0x00]
        let params1 = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,m=1,i=100", b"/wAA"];
        let result1 = parse(&params1, &mut state).expect(
            "intermediate chunks must return a `pending_chunk` response, not None",
        );
        assert!(result1.incomplete, "first chunk must be marked incomplete");
        assert!(result1.graphic_data.is_none());

        // Final chunk: 4 chars with padding → 1 decoded byte [0xFF]
        let params2 = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,m=0,i=100", b"/w=="];
        let result2 = parse(&params2, &mut state);
        let response = result2.expect("final chunk must produce a response");
        assert!(!response.incomplete, "final chunk must not be incomplete");
        let graphic = response
            .graphic_data
            .expect("final chunk must produce graphic data");
        assert_eq!(
            graphic.pixels,
            vec![0xFF, 0x00, 0x00, 0xFF],
            "decoded bytes must equal the full decoded payload"
        );
    }

    #[test]
    fn test_incomplete_image_accumulation_padded_chunks() {
        // Regression for chafa: clients may base64-encode each chunk
        // independently, which leaves `=` padding on intermediate chunks.
        // Concatenating the raw base64 text used to fail with
        // `Base64 decode failed: InvalidByte(..., 61)` because the `=`
        // ended up mid-string. We now decode each chunk independently so
        // the merged binary payload is correct.
        let mut state = KittyGraphicsState::default();

        // Source bytes: 7 bytes of RGB data for a 1x? image. The point
        // of this test is the chunking pattern, not meaningful pixels.
        //
        // Chunk 1: encodes 4 bytes [0xDE, 0xAD, 0xBE, 0xEF]
        //   → 8 chars with padding: "3q2+7w=="
        // Chunk 2: encodes 3 bytes [0xCA, 0xFE, 0xBA]
        //   → 4 chars no padding: "yv66"
        //
        // Concatenated raw base64 would be "3q2+7w==yv66" with `==` in
        // the middle — which a strict base64 decoder rejects.
        let full_binary: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA];
        let expected_pixels = {
            // We'll reinterpret the 7 bytes as a width=7, height=1 grey
            // image by using f=8 (1 bpp). Then create_graphic_data expands
            // each gray byte to RGBA.
            let mut rgba = Vec::with_capacity(full_binary.len() * 4);
            for &g in &full_binary {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
            rgba
        };

        let params1 = vec![b"G".as_ref(), b"a=t,f=8,s=7,v=1,m=1,i=200", b"3q2+7w=="];
        let r1 = parse(&params1, &mut state)
            .expect("padded intermediate chunk must not return None");
        assert!(r1.incomplete);

        let params2 = vec![b"G".as_ref(), b"a=t,f=8,s=7,v=1,m=0,i=200", b"yv66"];
        let r2 = parse(&params2, &mut state).expect("final chunk must parse");
        assert!(!r2.incomplete);
        let graphic = r2.graphic_data.expect("graphic data must be produced");
        assert_eq!(
            graphic.pixels, expected_pixels,
            "chafa-style padded chunks must merge into the correct byte stream",
        );
    }

    #[test]
    fn test_chunked_matches_single_shot() {
        // Parsing an image as a single command or as several chunks must
        // produce the same decoded payload — decoding per chunk must not
        // introduce drift.
        let single_pixel_b64 = "/wAA/w=="; // [0xFF, 0x00, 0x00, 0xFF]

        let single =
            parse_kitty_graphics_protocol("a=t,f=32,s=1,v=1,i=301", single_pixel_b64)
                .and_then(|r| r.graphic_data)
                .expect("single-shot must succeed");

        let mut state = KittyGraphicsState::default();
        let p1 = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,m=1,i=302", b"/wAA"];
        parse(&p1, &mut state).expect("first chunk");
        let p2 = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,m=0,i=302", b"/w=="];
        let chunked = parse(&p2, &mut state)
            .and_then(|r| r.graphic_data)
            .expect("chunked must succeed");

        assert_eq!(
            single.pixels, chunked.pixels,
            "chunked and single-shot decode must agree"
        );
        assert_eq!(single.width, chunked.width);
        assert_eq!(single.height, chunked.height);
    }

    #[test]
    fn test_chunked_preserves_first_chunk_metadata() {
        // Only the first chunk carries the full control data. Subsequent
        // chunks (including the terminating m=0 one) may omit width,
        // height, format, etc. — the stored metadata from the first
        // chunk must be used.
        let mut state = KittyGraphicsState::default();

        let p1 = vec![b"G".as_ref(), b"a=T,f=32,s=1,v=1,i=500,z=7,m=1", b"/wAA"];
        parse(&p1, &mut state).expect("first chunk");

        // Final chunk: only `m=0` and `i=500`. Width/height/format are
        // intentionally missing and must be inherited from the first.
        let p2 = vec![b"G".as_ref(), b"m=0,i=500", b"/w=="];
        let response =
            parse(&p2, &mut state).expect("final chunk must produce a response");

        let graphic = response.graphic_data.expect("graphic data");
        assert_eq!(graphic.width, 1);
        assert_eq!(graphic.height, 1);
        assert_eq!(graphic.pixels, vec![0xFF, 0x00, 0x00, 0xFF]);

        // Placement should reflect the first chunk's z-index because it
        // used `a=T` (transmit and display).
        let placement = response
            .placement_request
            .expect("a=T must emit a placement request");
        assert_eq!(placement.z_index, 7);
    }

    #[test]
    fn test_chunked_with_zlib_compression() {
        // chafa transmits RGBA with zlib compression (o=z). The o= flag
        // is only carried on the first chunk, so the decompression must
        // happen after all chunks are merged.
        use flate2::write::ZlibEncoder;
        use flate2::Compression as FlateCompression;
        use std::io::Write;

        // 2 pixels, 8 bytes RGBA: red + green
        let raw = vec![0xFF, 0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF];
        let mut encoder = ZlibEncoder::new(Vec::new(), FlateCompression::default());
        encoder.write_all(&raw).unwrap();
        let compressed = encoder.finish().unwrap();
        let encoded = BASE64.encode(&compressed);

        // Split the base64 text on a 4-char boundary so each chunk is
        // spec-compliant.
        assert!(encoded.len() >= 4, "test payload should be chunkable");
        let split_at = (encoded.len() / 2) & !0x3; // nearest lower multiple of 4
        let (first, second) = encoded.split_at(split_at);

        let mut state = KittyGraphicsState::default();
        let p1_ctrl = String::from("a=t,f=32,s=2,v=1,o=z,m=1,i=600");
        let p1 = vec![b"G".as_ref(), p1_ctrl.as_bytes(), first.as_bytes()];
        parse(&p1, &mut state).expect("first chunk");

        let p2_ctrl = String::from("m=0,i=600");
        let p2 = vec![b"G".as_ref(), p2_ctrl.as_bytes(), second.as_bytes()];
        let response =
            parse(&p2, &mut state).expect("final chunk must parse and decompress");

        let graphic = response.graphic_data.expect("graphic data");
        assert_eq!(
            graphic.pixels, raw,
            "zlib-compressed chunked payload must decompress to original"
        );
    }

    #[test]
    fn test_max_dimension_rejects_oversized_images() {
        // Width above the cap must fail with EINVAL:dimensions too large.
        let result = parse_kitty_graphics_protocol("a=t,f=32,s=10001,v=1,i=1", "AAAA")
            .expect("response struct must exist");
        assert!(result.graphic_data.is_none());
        let body = result.response.expect("error message expected");
        assert!(
            body.contains("dimensions too large"),
            "expected dimensions-too-large: {body}"
        );

        // Height above the cap must also fail.
        let result = parse_kitty_graphics_protocol("a=t,f=32,s=1,v=10001,i=1", "AAAA")
            .expect("response struct must exist");
        assert!(result.graphic_data.is_none());
        let body = result.response.expect("error message expected");
        assert!(body.contains("dimensions too large"));

        // Exactly 10000 must be accepted (boundary).
        // (We don't actually feed 10000*10000*4 bytes here, the
        // create_graphic_data path will reject due to missing data, but
        // that's a different error than the dimension cap.)
        let result = parse_kitty_graphics_protocol("a=t,f=32,s=10000,v=1,i=1", "AAAA")
            .expect("response struct must exist");
        let body = result.response.unwrap_or_default();
        assert!(
            !body.contains("dimensions too large"),
            "10000 must not trip the dimension cap: {body}"
        );
    }

    #[test]
    fn test_iid_mutually_exclusive() {
        // Setting both i= and I= must be rejected with a specific EINVAL.
        let result = parse_kitty_graphics_protocol("a=t,f=32,s=1,v=1,i=5,I=6", "AAAA")
            .expect("response struct must exist");
        assert!(result.graphic_data.is_none());
        let body = result.response.expect("error message expected");
        assert!(
            body.contains("mutually exclusive"),
            "expected mutually-exclusive error: {body}"
        );
        assert!(body.contains("i=5"));
        assert!(body.contains("I=6"));
    }

    #[test]
    fn test_query_requires_image_id() {
        // Query without i= AND without I=: no identifier to address
        // the response to, so nothing is emitted — matches ghostty.
        let result =
            parse_kitty_graphics_protocol("a=q", "").expect("response struct must exist");
        assert!(result.response.is_none());

        // Query with only I= set (no i=): we surface the EINVAL
        // addressed by image_number, so the client can correlate.
        let result = parse_kitty_graphics_protocol("a=q,I=7", "")
            .expect("response struct must exist");
        let body = result.response.expect("error message expected");
        assert!(
            body.contains("image ID required"),
            "expected image-id-required error: {body}"
        );
        assert!(body.contains("I=7"));

        // With an explicit id, query succeeds with OK.
        let result = parse_kitty_graphics_protocol("a=q,i=42", "")
            .expect("response struct must exist");
        let body = result.response.expect("OK response expected");
        assert!(body.contains("i=42"));
        assert!(body.contains(";OK"));
    }

    #[test]
    fn test_response_combines_image_number_and_placement() {
        // When placement_id is set alongside image_id, the response
        // must carry both keys (matches ghostty's encoder).
        let png_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";
        let result = parse_kitty_graphics_protocol("a=T,f=100,i=7,p=13", png_data)
            .expect("response struct must exist");
        let body = result.response.expect("OK response expected");
        assert!(
            body.contains("i=7") && body.contains("p=13"),
            "both id and placement must appear: {body}"
        );
        assert!(body.contains(";OK"));
    }

    #[test]
    fn test_animation_params_are_parsed() {
        // Frame loading (a=f) should populate frame_number, base_frame,
        // frame_gap, composition_mode, background_color fields. We
        // still return EINVAL for the action itself, but clients can
        // extend support later and the values must already be correct.
        let mut cmd = KittyGraphicsCommand::default();
        parse_control_data(&mut cmd, "a=f,i=1,r=3,c=2,z=100,X=1,Y=4294901760");
        assert_eq!(cmd.action, Action::Frame);
        assert_eq!(cmd.frame_number, 3);
        assert_eq!(cmd.base_frame, 2);
        assert_eq!(cmd.frame_gap, 100);
        assert_eq!(cmd.composition_mode, 1);
        assert_eq!(cmd.background_color, 4_294_901_760);

        // Animation control (a=a) populates animation_state, loop_count,
        // current_frame, frame_number, frame_gap.
        let mut cmd = KittyGraphicsCommand::default();
        parse_control_data(&mut cmd, "a=a,i=2,s=3,v=5,c=1,r=2,z=50");
        assert_eq!(cmd.action, Action::Animate);
        assert_eq!(cmd.animation_state, 3);
        assert_eq!(cmd.loop_count, 5);
        assert_eq!(cmd.current_frame, 1);
        assert_eq!(cmd.frame_number, 2);
        assert_eq!(cmd.frame_gap, 50);
    }

    fn parse_delete(keys: &str) -> DeleteRequest {
        let resp = parse_kitty_graphics_protocol(keys, "")
            .expect("delete must produce a response");
        resp.delete_request
            .expect("delete_request must be populated")
    }

    #[test]
    fn test_delete_variants_all_parse() {
        // d=a/A — delete all
        let d = parse_delete("a=d,d=a");
        assert_eq!(d.action, b'a');
        assert!(!d.delete_data);
        let d = parse_delete("a=d,d=A");
        assert_eq!(d.action, b'a');
        assert!(d.delete_data, "uppercase form must set delete_data=true");

        // d=i/I — by image id
        let d = parse_delete("a=d,d=i,i=7");
        assert_eq!(d.action, b'i');
        assert_eq!(d.image_id, 7);

        // d=n/N — by image number (I= channel)
        let d = parse_delete("a=d,d=n,I=11");
        assert_eq!(d.action, b'n');
        assert_eq!(
            d.image_number, 11,
            "d=n must pick up the number from I=, not i="
        );

        // d=c/C — intersecting cursor
        let d = parse_delete("a=d,d=C");
        assert_eq!(d.action, b'c');
        assert!(d.delete_data);

        // d=p/P — at cell position
        let d = parse_delete("a=d,d=p,x=3,y=5");
        assert_eq!(d.action, b'p');
        assert_eq!(d.x, 3);
        assert_eq!(d.y, 5);

        // d=q/Q — at cell + z-index
        let d = parse_delete("a=d,d=q,x=2,y=4,z=-3");
        assert_eq!(d.action, b'q');
        assert_eq!(d.z_index, -3);

        // d=r/R — id range (x=start, y=end)
        let d = parse_delete("a=d,d=R,x=10,y=20");
        assert_eq!(d.action, b'r');
        assert_eq!(d.x, 10);
        assert_eq!(d.y, 20);
        assert!(d.delete_data);

        // d=x/X — by column
        let d = parse_delete("a=d,d=x,x=5");
        assert_eq!(d.action, b'x');
        assert_eq!(d.x, 5);

        // d=y/Y — by row
        let d = parse_delete("a=d,d=Y,y=9");
        assert_eq!(d.action, b'y');
        assert_eq!(d.y, 9);
        assert!(d.delete_data);

        // d=z/Z — by z-index
        let d = parse_delete("a=d,d=z,z=42");
        assert_eq!(d.action, b'z');
        assert_eq!(d.z_index, 42);
    }

    #[test]
    fn test_stale_chunk_eviction() {
        // An in-progress chunked upload whose `last_touched` is older
        // than CHUNK_STALE_TIMEOUT must be dropped on the next chunk
        // event.
        let mut state = KittyGraphicsState::default();

        // First chunk: legit pending upload.
        let p1 = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,m=1,i=777", b"/wAA"];
        parse(&p1, &mut state).expect("first chunk");
        assert_eq!(state.incomplete_images.len(), 1);

        // Artificially age the stored command past the timeout.
        let stale = Instant::now() - CHUNK_STALE_TIMEOUT - Duration::from_secs(1);
        if let Some(cmd) = state.incomplete_images.get_mut(&777) {
            cmd.last_touched = stale;
        }

        // Any subsequent command triggers eviction on its way in. Use
        // a fresh single-shot image so the eviction scan runs.
        let p2 = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,i=888", b"/wAA/w=="];
        let _ = parse(&p2, &mut state);

        assert!(
            !state.incomplete_images.contains_key(&777),
            "stale chunk must have been evicted"
        );
    }

    #[test]
    fn test_padding_in_middle_of_raw_concat_would_fail() {
        // Sanity check: confirm the exact failure mode we fixed — a
        // naive concat of padded chunks produces a string with `=` in
        // the middle that BASE64.decode rejects. If this ever becomes a
        // non-error it means the decoder changed and our fix may be
        // masking something else.
        let concatenated = b"3q2+7w==yv66".as_slice();
        assert!(
            BASE64.decode(concatenated).is_err(),
            "strict base64 must reject `=` in the middle of the input"
        );
    }

    #[test]
    fn test_pending_chunk_distinct_from_parse_error() {
        // Regression for the yazi log spam: an in-progress chunked
        // transmission must be distinguishable from a real parse error.
        // Pending chunks return `Some { incomplete: true }`; real
        // errors return a response with an EINVAL/ENOENT message
        // instead of a graphic.
        let mut state = KittyGraphicsState::default();

        // m=1: pending — Some(incomplete=true), no graphic, no error msg
        let params = vec![b"G".as_ref(), b"a=t,f=32,s=1,v=1,m=1,i=42", b"/wAA"];
        let resp = parse(&params, &mut state).expect("pending must be Some");
        assert!(resp.incomplete);
        assert!(resp.response.is_none());

        // Real error path: blocked path → error response addressed to
        // i=99. The exact error varies by platform — on Linux the
        // `/proc/` sensitive-path match triggers EINVAL, on macOS the
        // missing file triggers ENOENT. Either is correct.
        let proc_path = BASE64.encode("/proc/self/environ".as_bytes());
        let bad = vec![
            b"G".as_ref(),
            b"a=t,t=f,f=32,s=1,v=1,i=99",
            proc_path.as_bytes(),
        ];
        let resp = parse(&bad, &mut KittyGraphicsState::default())
            .expect("real errors now carry a response, not None");
        assert!(!resp.incomplete, "real errors must not look like pending");
        assert!(resp.graphic_data.is_none());
        let body = resp.response.expect("error message expected");
        assert!(
            body.contains("i=99"),
            "error must be addressed to i=99: {body}"
        );
        assert!(
            body.contains("EINVAL") || body.contains("ENOENT"),
            "blocked path should surface EINVAL or ENOENT: {body}"
        );
    }

    #[test]
    fn test_file_transmission_medium() {
        // Create a temporary file
        use std::io::Write;
        let temp_path = std::env::temp_dir().join("test_kitty_image.rgba");
        let temp_path = temp_path.to_str().unwrap();
        let mut file = std::fs::File::create(temp_path).unwrap();
        file.write_all(&[255, 0, 0, 255]).unwrap(); // 1x1 red pixel
        drop(file);

        // Encode the file path as base64 (as kitty does)
        let encoded_path = BASE64.encode(temp_path.as_bytes());
        let result =
            parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1,i=1", &encoded_path);
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
        let temp_path = std::env::temp_dir().join("tty-graphics-protocol-test.rgba");
        let temp_path = temp_path.to_str().unwrap();
        let mut file = std::fs::File::create(temp_path).unwrap();
        file.write_all(&[255, 0, 0, 255]).unwrap(); // 1x1 red pixel
        drop(file);

        // Encode the file path as base64 (as kitty does)
        let encoded_path = BASE64.encode(temp_path.as_bytes());
        let result =
            parse_kitty_graphics_protocol("a=t,t=t,f=32,s=1,v=1,i=1", &encoded_path);

        // File should be deleted after reading
        assert!(!std::path::Path::new(temp_path).exists());

        assert!(result.is_some());
        let response = result.unwrap();
        assert!(response.graphic_data.is_some());
    }

    #[test]
    fn test_security_checks() {
        // Commands with no i=/I= get an implicit id, which suppresses
        // the response entirely — but we still must NOT load the file.
        // Include i=1 so we can assert on the EINVAL response.
        for path in &["/proc/self/environ", "/sys/class/net", "/dev/null"] {
            let encoded = BASE64.encode(path.as_bytes());
            let response =
                parse_kitty_graphics_protocol("a=t,t=f,f=32,s=1,v=1,i=1", &encoded)
                    .expect("must return a response carrying the EINVAL");
            assert!(response.graphic_data.is_none(), "{path} must not load");
            let body = response.response.expect("error message expected");
            assert!(body.contains("i=1"));
            assert!(
                body.contains("EINVAL") || body.contains("ENOENT"),
                "blocked path should surface EINVAL/ENOENT: {body}"
            );
        }
    }

    #[test]
    fn test_quiet_mode() {
        // q=1 suppresses OK but keeps errors
        let result = parse_kitty_graphics_protocol("a=p,i=1,q=1", "");
        let response = result.expect("response struct must exist");
        assert!(response.response.is_none(), "q=1 must suppress OK");

        // q=2 suppresses everything, including errors. Ghostty / kitty
        // both document this as "absolute silence".
        let result = parse_kitty_graphics_protocol("a=q,i=1,q=2", "");
        let response = result.expect("response struct must exist");
        assert!(response.response.is_none(), "q=2 must suppress all output");
    }
}
