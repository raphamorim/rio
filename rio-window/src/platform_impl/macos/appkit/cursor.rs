use once_cell::sync::Lazy;

use icrate::ns_string;
use icrate::Foundation::{
    NSData, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSPoint, NSString,
};
use objc2::rc::{DefaultId, Id};
use objc2::runtime::Sel;
use objc2::{extern_class, extern_methods, msg_send_id, mutability, sel, ClassType};

use super::NSImage;
use crate::window::CursorIcon;

extern_class!(
    /// <https://developer.apple.com/documentation/appkit/nscursor?language=objc>
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub(crate) struct NSCursor;

    unsafe impl ClassType for NSCursor {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
    }
);

// SAFETY: NSCursor is immutable, stated here:
// https://developer.apple.com/documentation/appkit/nscursor/1527062-image?language=objc
unsafe impl Send for NSCursor {}
unsafe impl Sync for NSCursor {}

macro_rules! def_cursor {
    {$(
        $(#[$($m:meta)*])*
        pub fn $name:ident();
    )*} => {$(
        $(#[$($m)*])*
        pub fn $name() -> Id<Self> {
            unsafe { msg_send_id![Self::class(), $name] }
        }
    )*};
}

macro_rules! def_undocumented_cursor {
    {$(
        $(#[$($m:meta)*])*
        pub fn $name:ident();
    )*} => {$(
        $(#[$($m)*])*
        pub fn $name() -> Id<Self> {
            unsafe { Self::from_selector(sel!($name)).unwrap_or_else(|| Default::default()) }
        }
    )*};
}

extern_methods!(
    /// Documented cursors
    unsafe impl NSCursor {
        def_cursor!(
            pub fn arrowCursor();
            pub fn pointingHandCursor();
            pub fn openHandCursor();
            pub fn closedHandCursor();
            pub fn IBeamCursor();
            pub fn IBeamCursorForVerticalLayout();
            pub fn dragCopyCursor();
            pub fn dragLinkCursor();
            pub fn operationNotAllowedCursor();
            pub fn contextualMenuCursor();
            pub fn crosshairCursor();
            pub fn resizeRightCursor();
            pub fn resizeUpCursor();
            pub fn resizeLeftCursor();
            pub fn resizeDownCursor();
            pub fn resizeLeftRightCursor();
            pub fn resizeUpDownCursor();
        );

        // Creating cursors should be thread-safe, though using them for anything probably isn't.
        pub fn new(image: &NSImage, hotSpot: NSPoint) -> Id<Self> {
            unsafe { msg_send_id![Self::alloc(), initWithImage: image, hotSpot: hotSpot] }
        }

        pub fn invisible() -> Id<Self> {
            // 16x16 GIF data for invisible cursor
            // You can reproduce this via ImageMagick.
            // $ convert -size 16x16 xc:none cursor.gif
            static CURSOR_BYTES: &[u8] = &[
                0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x10, 0x00, 0x10, 0x00, 0xF0, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x21, 0xF9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C,
                0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x10, 0x00, 0x00, 0x02, 0x0E, 0x84, 0x8F, 0xA9,
                0xCB, 0xED, 0x0F, 0xA3, 0x9C, 0xB4, 0xDA, 0x8B, 0xB3, 0x3E, 0x05, 0x00, 0x3B,
            ];

            static CURSOR: Lazy<Id<NSCursor>> = Lazy::new(|| {
                // TODO: Consider using `dataWithBytesNoCopy:`
                let data = NSData::with_bytes(CURSOR_BYTES);
                let image = NSImage::new_with_data(&data);
                NSCursor::new(&image, NSPoint::new(0.0, 0.0))
            });

            CURSOR.clone()
        }
    }

    /// Undocumented cursors
    unsafe impl NSCursor {
        #[method(respondsToSelector:)]
        fn class_responds_to(sel: Sel) -> bool;

        #[method_id(performSelector:)]
        unsafe fn from_selector_unchecked(sel: Sel) -> Id<Self>;

        unsafe fn from_selector(sel: Sel) -> Option<Id<Self>> {
            if Self::class_responds_to(sel) {
                Some(unsafe { Self::from_selector_unchecked(sel) })
            } else {
                warn!("Cursor `{:?}` appears to be invalid", sel);
                None
            }
        }

        def_undocumented_cursor!(
            // Undocumented cursors: https://stackoverflow.com/a/46635398/5435443
            pub fn _helpCursor();
            pub fn _zoomInCursor();
            pub fn _zoomOutCursor();
            pub fn _windowResizeNorthEastCursor();
            pub fn _windowResizeNorthWestCursor();
            pub fn _windowResizeSouthEastCursor();
            pub fn _windowResizeSouthWestCursor();
            pub fn _windowResizeNorthEastSouthWestCursor();
            pub fn _windowResizeNorthWestSouthEastCursor();

            // While these two are available, the former just loads a white arrow,
            // and the latter loads an ugly deflated beachball!
            // pub fn _moveCursor();
            // pub fn _waitCursor();

            // An even more undocumented cursor...
            // https://bugs.eclipse.org/bugs/show_bug.cgi?id=522349
            pub fn busyButClickableCursor();
        );
    }

    /// Webkit cursors
    unsafe impl NSCursor {
        // Note that loading `busybutclickable` with this code won't animate
        // the frames; instead you'll just get them all in a column.
        unsafe fn load_webkit_cursor(name: &NSString) -> Id<Self> {
            // Snatch a cursor from WebKit; They fit the style of the native
            // cursors, and will seem completely standard to macOS users.
            //
            // https://stackoverflow.com/a/21786835/5435443
            let root = ns_string!("/System/Library/Frameworks/ApplicationServices.framework/Versions/A/Frameworks/HIServices.framework/Versions/A/Resources/cursors");
            let cursor_path = root.stringByAppendingPathComponent(name);

            let pdf_path = cursor_path.stringByAppendingPathComponent(ns_string!("cursor.pdf"));
            let image = NSImage::new_by_referencing_file(&pdf_path);

            // TODO: Handle PLists better
            let info_path = cursor_path.stringByAppendingPathComponent(ns_string!("info.plist"));
            let info: Id<NSDictionary<NSObject, NSObject>> = unsafe {
                msg_send_id![
                    <NSDictionary<NSObject, NSObject>>::class(),
                    dictionaryWithContentsOfFile: &*info_path,
                ]
            };
            let mut x = 0.0;
            if let Some(n) = info.get(&*ns_string!("hotx")) {
                if n.is_kind_of::<NSNumber>() {
                    let ptr: *const NSObject = n;
                    let ptr: *const NSNumber = ptr.cast();
                    x = unsafe { &*ptr }.as_cgfloat()
                }
            }
            let mut y = 0.0;
            if let Some(n) = info.get(&*ns_string!("hotx")) {
                if n.is_kind_of::<NSNumber>() {
                    let ptr: *const NSObject = n;
                    let ptr: *const NSNumber = ptr.cast();
                    y = unsafe { &*ptr }.as_cgfloat()
                }
            }

            let hotspot = NSPoint::new(x, y);
            Self::new(&image, hotspot)
        }

        pub fn moveCursor() -> Id<Self> {
            unsafe { Self::load_webkit_cursor(ns_string!("move")) }
        }

        pub fn cellCursor() -> Id<Self> {
            unsafe { Self::load_webkit_cursor(ns_string!("cell")) }
        }
    }
);

impl NSCursor {
    pub fn from_icon(icon: CursorIcon) -> Id<Self> {
        match icon {
            CursorIcon::Default => Default::default(),
            CursorIcon::Pointer => Self::pointingHandCursor(),
            CursorIcon::Grab => Self::openHandCursor(),
            CursorIcon::Grabbing => Self::closedHandCursor(),
            CursorIcon::Text => Self::IBeamCursor(),
            CursorIcon::VerticalText => Self::IBeamCursorForVerticalLayout(),
            CursorIcon::Copy => Self::dragCopyCursor(),
            CursorIcon::Alias => Self::dragLinkCursor(),
            CursorIcon::NotAllowed | CursorIcon::NoDrop => Self::operationNotAllowedCursor(),
            CursorIcon::ContextMenu => Self::contextualMenuCursor(),
            CursorIcon::Crosshair => Self::crosshairCursor(),
            CursorIcon::EResize => Self::resizeRightCursor(),
            CursorIcon::NResize => Self::resizeUpCursor(),
            CursorIcon::WResize => Self::resizeLeftCursor(),
            CursorIcon::SResize => Self::resizeDownCursor(),
            CursorIcon::EwResize | CursorIcon::ColResize => Self::resizeLeftRightCursor(),
            CursorIcon::NsResize | CursorIcon::RowResize => Self::resizeUpDownCursor(),
            CursorIcon::Help => Self::_helpCursor(),
            CursorIcon::ZoomIn => Self::_zoomInCursor(),
            CursorIcon::ZoomOut => Self::_zoomOutCursor(),
            CursorIcon::NeResize => Self::_windowResizeNorthEastCursor(),
            CursorIcon::NwResize => Self::_windowResizeNorthWestCursor(),
            CursorIcon::SeResize => Self::_windowResizeSouthEastCursor(),
            CursorIcon::SwResize => Self::_windowResizeSouthWestCursor(),
            CursorIcon::NeswResize => Self::_windowResizeNorthEastSouthWestCursor(),
            CursorIcon::NwseResize => Self::_windowResizeNorthWestSouthEastCursor(),
            // This is the wrong semantics for `Wait`, but it's the same as
            // what's used in Safari and Chrome.
            CursorIcon::Wait | CursorIcon::Progress => Self::busyButClickableCursor(),
            CursorIcon::Move | CursorIcon::AllScroll => Self::moveCursor(),
            CursorIcon::Cell => Self::cellCursor(),
            _ => Default::default(),
        }
    }
}

impl DefaultId for NSCursor {
    fn default_id() -> Id<Self> {
        Self::arrowCursor()
    }
}
