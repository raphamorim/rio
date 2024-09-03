use std::collections::hash_map::Entry;
use std::hash::{Hash, Hasher};
use std::iter;
use std::sync::Arc;

use x11rb::connection::Connection;
use x11rb::protocol::render::{self, ConnectionExt as _};
use x11rb::protocol::xproto;

use crate::error::ExternalError;
use crate::platform_impl::OsError;
use crate::platform_impl::PlatformCustomCursorSource;
use crate::window::CursorIcon;

use super::super::ActiveEventLoop;
use super::*;

impl XConnection {
    pub fn set_cursor_icon(
        &self,
        window: xproto::Window,
        cursor: Option<CursorIcon>,
    ) -> Result<(), X11Error> {
        let cursor = {
            let mut cache = self.cursor_cache.lock().unwrap_or_else(|e| e.into_inner());

            match cache.entry(cursor) {
                Entry::Occupied(o) => *o.get(),
                Entry::Vacant(v) => *v.insert(self.get_cursor(cursor)?),
            }
        };

        self.update_cursor(window, cursor)
    }

    pub(crate) fn set_custom_cursor(
        &self,
        window: xproto::Window,
        cursor: &CustomCursor,
    ) -> Result<(), X11Error> {
        self.update_cursor(window, cursor.inner.cursor)
    }

    /// Create a cursor from an image.
    fn create_cursor_from_image(
        &self,
        width: u16,
        height: u16,
        hotspot_x: u16,
        hotspot_y: u16,
        image: &[u8],
    ) -> Result<xproto::Cursor, X11Error> {
        // Create a pixmap for the default root window.
        let root = self.default_root().root;
        let pixmap = xproto::PixmapWrapper::create_pixmap(
            self.xcb_connection(),
            32,
            root,
            width,
            height,
        )?;

        // Create a GC to draw with.
        let gc = xproto::GcontextWrapper::create_gc(
            self.xcb_connection(),
            pixmap.pixmap(),
            &Default::default(),
        )?;

        // Draw the data into it.
        self.xcb_connection()
            .put_image(
                xproto::ImageFormat::Z_PIXMAP,
                pixmap.pixmap(),
                gc.gcontext(),
                width,
                height,
                0,
                0,
                0,
                32,
                image,
            )?
            .ignore_error();
        drop(gc);

        // Create the XRender picture.
        let picture = render::PictureWrapper::create_picture(
            self.xcb_connection(),
            pixmap.pixmap(),
            self.find_argb32_format()?,
            &Default::default(),
        )?;
        drop(pixmap);

        // Create the cursor.
        let cursor = self.xcb_connection().generate_id()?;
        self.xcb_connection()
            .render_create_cursor(cursor, picture.picture(), hotspot_x, hotspot_y)?
            .check()?;

        Ok(cursor)
    }

    /// Find the render format that corresponds to ARGB32.
    fn find_argb32_format(&self) -> Result<render::Pictformat, X11Error> {
        macro_rules! direct {
            ($format:expr, $shift_name:ident, $mask_name:ident, $shift:expr) => {{
                ($format).direct.$shift_name == $shift
                    && ($format).direct.$mask_name == 0xff
            }};
        }

        self.render_formats()
            .formats
            .iter()
            .find(|format| {
                format.type_ == render::PictType::DIRECT
                    && format.depth == 32
                    && direct!(format, red_shift, red_mask, 16)
                    && direct!(format, green_shift, green_mask, 8)
                    && direct!(format, blue_shift, blue_mask, 0)
                    && direct!(format, alpha_shift, alpha_mask, 24)
            })
            .ok_or(X11Error::NoArgb32Format)
            .map(|format| format.id)
    }

    fn create_empty_cursor(&self) -> Result<xproto::Cursor, X11Error> {
        self.create_cursor_from_image(1, 1, 0, 0, &[0, 0, 0, 0])
    }

    fn get_cursor(&self, cursor: Option<CursorIcon>) -> Result<xproto::Cursor, X11Error> {
        let cursor = match cursor {
            Some(cursor) => cursor,
            None => return self.create_empty_cursor(),
        };

        let database = self.database();
        let handle = x11rb::cursor::Handle::new(
            self.xcb_connection(),
            self.default_screen_index(),
            &database,
        )?
        .reply()?;

        let mut last_error = None;
        for &name in iter::once(&cursor.name()).chain(cursor.alt_names().iter()) {
            match handle.load_cursor(self.xcb_connection(), name) {
                Ok(cursor) => return Ok(cursor),
                Err(err) => last_error = Some(err.into()),
            }
        }

        Err(last_error.unwrap())
    }

    fn update_cursor(
        &self,
        window: xproto::Window,
        cursor: xproto::Cursor,
    ) -> Result<(), X11Error> {
        self.xcb_connection()
            .change_window_attributes(
                window,
                &xproto::ChangeWindowAttributesAux::new().cursor(cursor),
            )?
            .ignore_error();

        self.xcb_connection().flush()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectedCursor {
    Custom(CustomCursor),
    Named(CursorIcon),
}

#[derive(Debug, Clone)]
pub struct CustomCursor {
    inner: Arc<CustomCursorInner>,
}

impl Hash for CustomCursor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.inner).hash(state);
    }
}

impl PartialEq for CustomCursor {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for CustomCursor {}

impl CustomCursor {
    pub(crate) fn new(
        event_loop: &ActiveEventLoop,
        mut cursor: PlatformCustomCursorSource,
    ) -> Result<CustomCursor, ExternalError> {
        // Reverse RGBA order to BGRA.
        cursor.0.rgba.chunks_mut(4).for_each(|chunk| {
            let chunk: &mut [u8; 4] = chunk.try_into().unwrap();
            chunk[0..3].reverse();

            // Byteswap if we need to.
            if event_loop.xconn.needs_endian_swap() {
                let value = u32::from_ne_bytes(*chunk).swap_bytes();
                *chunk = value.to_ne_bytes();
            }
        });

        let cursor = event_loop
            .xconn
            .create_cursor_from_image(
                cursor.0.width,
                cursor.0.height,
                cursor.0.hotspot_x,
                cursor.0.hotspot_y,
                &cursor.0.rgba,
            )
            .map_err(|err| ExternalError::Os(os_error!(OsError::XError(err.into()))))?;

        Ok(Self {
            inner: Arc::new(CustomCursorInner {
                xconn: event_loop.xconn.clone(),
                cursor,
            }),
        })
    }
}

#[derive(Debug)]
struct CustomCursorInner {
    xconn: Arc<XConnection>,
    cursor: xproto::Cursor,
}

impl Drop for CustomCursorInner {
    fn drop(&mut self) {
        self.xconn
            .xcb_connection()
            .free_cursor(self.cursor)
            .map(|r| r.ignore_error())
            .ok();
    }
}

impl Default for SelectedCursor {
    fn default() -> Self {
        SelectedCursor::Named(Default::default())
    }
}
