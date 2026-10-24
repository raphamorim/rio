//! GameCube-style window controls prototype (`RIO_GAMECUBE=1`).
//!
//! Recreates the "macOS but it's a GameCube controller" concept: the
//! traffic lights become a big green A (zoom), a small red B (close)
//! and a yellow bean (miniaturize) that overflows the window's
//! top-left corner. Native buttons can't leave the titlebar, so the
//! controls live in a transparent borderless child `NSWindow` glued
//! to the host's corner: the bean parts that hang outside the host
//! frame float over the desktop.
//!
//! Coordinates below are AppKit view coordinates (origin bottom-left,
//! non-flipped) inside the overlay window.

use std::cell::Cell;

use objc2::rc::{Retained, WeakId};
use objc2::runtime::AnyObject;
use objc2::{
    declare_class, msg_send_id, mutability, ClassType, DeclaredClass,
};
use objc2_app_kit::{
    NSBackingStoreType, NSBezierPath, NSColor, NSEvent, NSGradient, NSLineCapStyle,
    NSResponder, NSView, NSWindow, NSWindowButton, NSWindowFullScreenButton,
    NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSObject, NSPoint, NSRect, NSSize};

const OVERLAY_W: f64 = 150.0;
const OVERLAY_H: f64 = 110.0;
/// Where the host window's top-left corner lands inside the overlay,
/// measured from the overlay's left and top edges. Everything left of
/// / above that point floats outside the host frame.
const CORNER_X: f64 = 20.0;
const CORNER_TOP: f64 = 20.0;

// Proportions measured off the reference concept image, in units of
// A's outer radius r: A center at (1.60r, 1.72r) from the corner
// vertex; B level with A, 1.80r away, radius 0.49r; bean concentric
// with A, centerline at 1.76r, 0.75r thick, sweeping ~117°-153° so
// its round-capped ends poke past the top and left window edges
// while its middle hovers just outside the rounded corner.
//
// A button (zoom): the oversized green circle.
const A_CENTER: (f64, f64) = (58.5, 49.0);
const A_RADIUS: f64 = 22.0;
// B button (close): the small red circle, level with A.
const B_CENTER: (f64, f64) = (102.0, 49.0);
const B_RADIUS: f64 = 9.5;
// Bean (miniaturize): a thick round-capped arc concentric with A —
// on the controller the kidney buttons hug A, not the case edge.
const BEAN_CENTER: (f64, f64) = A_CENTER;
const BEAN_ARC_RADIUS: f64 = 42.0;
const BEAN_START_DEG: f64 = 116.0;
const BEAN_END_DEG: f64 = 154.0;
const BEAN_WIDTH: f64 = 14.0;
/// Ring drawn under each button as its rim.
const RIM: f64 = 2.0;

const BTN_NONE: u8 = 0;
const BTN_ZOOM: u8 = 1;
const BTN_CLOSE: u8 = 2;
const BTN_MINI: u8 = 3;

type Rgb = (f64, f64, f64);

/// Top-lit plastic look: every surface is a vertical gradient
/// (lighter top stop, darker bottom stop), rim included.
struct Style {
    top: Rgb,
    bottom: Rgb,
    rim_top: Rgb,
    rim_bottom: Rgb,
    glyph: Rgb,
}

const A_STYLE: Style = Style {
    top: (108.0, 200.0, 118.0),
    bottom: (60.0, 156.0, 78.0),
    rim_top: (66.0, 150.0, 80.0),
    rim_bottom: (40.0, 116.0, 56.0),
    glyph: (24.0, 88.0, 40.0),
};
const B_STYLE: Style = Style {
    top: (242.0, 118.0, 106.0),
    bottom: (208.0, 70.0, 66.0),
    rim_top: (196.0, 74.0, 68.0),
    rim_bottom: (156.0, 44.0, 44.0),
    glyph: (134.0, 34.0, 36.0),
};
const BEAN_STYLE: Style = Style {
    top: (252.0, 198.0, 92.0),
    bottom: (236.0, 152.0, 42.0),
    rim_top: (226.0, 158.0, 52.0),
    rim_bottom: (196.0, 122.0, 22.0),
    glyph: (122.0, 80.0, 20.0),
};

fn srgb(color: Rgb, alpha: f64) -> Retained<NSColor> {
    unsafe {
        NSColor::colorWithSRGBRed_green_blue_alpha(
            color.0 / 255.0,
            color.1 / 255.0,
            color.2 / 255.0,
            alpha,
        )
    }
}

fn dim(color: Rgb) -> Rgb {
    (color.0 * 0.82, color.1 * 0.82, color.2 * 0.82)
}

fn fill_stops(style: &Style, pressed: bool) -> (Rgb, Rgb) {
    if pressed {
        (dim(style.top), dim(style.bottom))
    } else {
        (style.top, style.bottom)
    }
}

fn dist(p: NSPoint, c: (f64, f64)) -> f64 {
    ((p.x - c.0).powi(2) + (p.y - c.1).powi(2)).sqrt()
}

fn circle_rect(center: (f64, f64), radius: f64, y_offset: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(center.0 - radius, center.1 - radius + y_offset),
        NSSize::new(radius * 2.0, radius * 2.0),
    )
}

/// Which button (if any) the view-local point lands on.
fn hit(p: NSPoint) -> u8 {
    if dist(p, A_CENTER) <= A_RADIUS {
        return BTN_ZOOM;
    }
    if dist(p, B_CENTER) <= B_RADIUS {
        return BTN_CLOSE;
    }
    // Bean: within half a stroke width of the arc centerline, inside
    // the swept angle range (padded for the round caps).
    let radial = dist(p, BEAN_CENTER);
    let mut angle = (p.y - BEAN_CENTER.1).atan2(p.x - BEAN_CENTER.0).to_degrees();
    if angle < 0.0 {
        angle += 360.0;
    }
    if (radial - BEAN_ARC_RADIUS).abs() <= BEAN_WIDTH / 2.0 + 1.0
        && (BEAN_START_DEG - 13.0..=BEAN_END_DEG + 13.0).contains(&angle)
    {
        return BTN_MINI;
    }
    BTN_NONE
}

pub(crate) struct GameCubeState {
    host: WeakId<NSWindow>,
    pressed: Cell<u8>,
}

declare_class!(
    pub(crate) struct GameCubeView;

    unsafe impl ClassType for GameCubeView {
        #[inherits(NSResponder, NSObject)]
        type Super = NSView;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "RioGameCubeControlsView";
    }

    impl DeclaredClass for GameCubeView {
        type Ivars = GameCubeState;
    }

    unsafe impl GameCubeView {
        #[method(acceptsFirstMouse:)]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[method(mouseDown:)]
        fn mouse_down(&self, event: &NSEvent) {
            let point = self.event_location(event);
            let target = hit(point);
            if target == BTN_NONE {
                // The overlay covers a chunk of the host's titlebar;
                // clicks that miss every button become a host window
                // drag so that region doesn't go dead.
                if let Some(host) = self.ivars().host.load() {
                    host.performWindowDragWithEvent(event);
                }
                return;
            }
            self.ivars().pressed.set(target);
            unsafe { self.setNeedsDisplay(true) };
        }

        #[method(mouseUp:)]
        fn mouse_up(&self, event: &NSEvent) {
            let pressed = self.ivars().pressed.take();
            unsafe { self.setNeedsDisplay(true) };
            if pressed == BTN_NONE || hit(self.event_location(event)) != pressed {
                return;
            }
            let Some(host) = self.ivars().host.load() else {
                return;
            };
            let sender: Option<&AnyObject> = None;
            unsafe {
                match pressed {
                    BTN_ZOOM => host.zoom(sender),
                    BTN_CLOSE => host.performClose(sender),
                    BTN_MINI => host.performMiniaturize(sender),
                    _ => {}
                }
            }
        }

        #[method(drawRect:)]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw_controls();
        }
    }
);

impl GameCubeView {
    fn new(mtm: MainThreadMarker, host: &NSWindow, frame: NSRect) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(GameCubeState {
            host: WeakId::new(&host.retain()),
            pressed: Cell::new(BTN_NONE),
        });
        unsafe { msg_send_id![super(this), initWithFrame: frame] }
    }

    fn event_location(&self, event: &NSEvent) -> NSPoint {
        let point = unsafe { event.locationInWindow() };
        self.convertPoint_fromView(point, None)
    }

    /// The bean as a closed, fillable kidney outline: outer arc, round
    /// cap, inner arc back, round cap. A stroked arc can't take a
    /// gradient fill, this can.
    fn bean_outline(&self, half_width: f64, y_offset: f64) -> Retained<NSBezierPath> {
        let c = NSPoint::new(BEAN_CENTER.0, BEAN_CENTER.1 + y_offset);
        let r = BEAN_ARC_RADIUS;
        let at = |deg: f64| {
            NSPoint::new(
                c.x + r * deg.to_radians().cos(),
                c.y + r * deg.to_radians().sin(),
            )
        };
        unsafe {
            let path = NSBezierPath::bezierPath();
            path.appendBezierPathWithArcWithCenter_radius_startAngle_endAngle(
                c,
                r + half_width,
                BEAN_START_DEG,
                BEAN_END_DEG,
            );
            path.appendBezierPathWithArcWithCenter_radius_startAngle_endAngle(
                at(BEAN_END_DEG),
                half_width,
                BEAN_END_DEG,
                BEAN_END_DEG + 180.0,
            );
            path.appendBezierPathWithArcWithCenter_radius_startAngle_endAngle_clockwise(
                c,
                r - half_width,
                BEAN_END_DEG,
                BEAN_START_DEG,
                true,
            );
            path.appendBezierPathWithArcWithCenter_radius_startAngle_endAngle(
                at(BEAN_START_DEG),
                half_width,
                BEAN_START_DEG + 180.0,
                BEAN_START_DEG,
            );
            path.closePath();
            path
        }
    }

    /// Fill `path` with a vertical gradient, `top` at the top.
    fn gradient_fill(&self, path: &NSBezierPath, top: Rgb, bottom: Rgb) {
        unsafe {
            if let Some(gradient) = NSGradient::initWithStartingColor_endingColor(
                NSGradient::alloc(),
                &srgb(bottom, 1.0),
                &srgb(top, 1.0),
            ) {
                gradient.drawInBezierPath_angle(path, 90.0);
            }
        }
    }

    fn gradient_circle(&self, center: (f64, f64), radius: f64, top: Rgb, bottom: Rgb) {
        let path = unsafe {
            NSBezierPath::bezierPathWithOvalInRect(circle_rect(center, radius, 0.0))
        };
        self.gradient_fill(&path, top, bottom);
    }

    fn fill_circle(&self, center: (f64, f64), radius: f64, y_offset: f64, color: &NSColor) {
        unsafe {
            color.setFill();
            NSBezierPath::bezierPathWithOvalInRect(circle_rect(center, radius, y_offset))
                .fill();
        }
    }

    fn stroke_segment(&self, from: (f64, f64), to: (f64, f64), width: f64, color: &NSColor) {
        unsafe {
            let path = NSBezierPath::bezierPath();
            path.moveToPoint(NSPoint::new(from.0, from.1));
            path.lineToPoint(NSPoint::new(to.0, to.1));
            path.setLineWidth(width);
            path.setLineCapStyle(NSLineCapStyle::Round);
            color.setStroke();
            path.stroke();
        }
    }

    /// Translucent white arc hugging the upper inside edge: the sheen
    /// that sells the top-lit plastic.
    fn sheen_arc(
        &self,
        center: (f64, f64),
        radius: f64,
        from_deg: f64,
        to_deg: f64,
        width: f64,
        color: &NSColor,
    ) {
        unsafe {
            let path = NSBezierPath::bezierPath();
            path.appendBezierPathWithArcWithCenter_radius_startAngle_endAngle(
                NSPoint::new(center.0, center.1),
                radius,
                from_deg,
                to_deg,
            );
            path.setLineWidth(width);
            path.setLineCapStyle(NSLineCapStyle::Round);
            color.setStroke();
            path.stroke();
        }
    }

    fn draw_controls(&self) {
        let pressed = self.ivars().pressed.get();
        let shadow = srgb((0.0, 0.0, 0.0), 0.22);
        let highlight = srgb((255.0, 255.0, 255.0), 0.32);
        let half_width = BEAN_WIDTH / 2.0;

        // ---- bean (miniaturize) ----
        unsafe {
            shadow.setFill();
            self.bean_outline(half_width + RIM, -2.0).fill();
        }
        self.gradient_fill(
            &self.bean_outline(half_width + RIM, 0.0),
            BEAN_STYLE.rim_top,
            BEAN_STYLE.rim_bottom,
        );
        let (top, bottom) = fill_stops(&BEAN_STYLE, pressed == BTN_MINI);
        self.gradient_fill(&self.bean_outline(half_width, 0.0), top, bottom);
        self.sheen_arc(
            BEAN_CENTER,
            BEAN_ARC_RADIUS + half_width - 2.5,
            BEAN_START_DEG + 4.0,
            BEAN_END_DEG - 4.0,
            2.0,
            &highlight,
        );
        // Minus glyph at the bean's midpoint: horizontal, like the
        // reference.
        let mid = (
            BEAN_CENTER.0 + BEAN_ARC_RADIUS * 135f64.to_radians().cos(),
            BEAN_CENTER.1 + BEAN_ARC_RADIUS * 135f64.to_radians().sin(),
        );
        self.stroke_segment(
            (mid.0 - 5.0, mid.1),
            (mid.0 + 5.0, mid.1),
            3.0,
            &srgb(BEAN_STYLE.glyph, 1.0),
        );

        // ---- A button (zoom) ----
        self.fill_circle(A_CENTER, A_RADIUS + RIM + 1.0, -2.0, &shadow);
        self.gradient_circle(
            A_CENTER,
            A_RADIUS + RIM,
            A_STYLE.rim_top,
            A_STYLE.rim_bottom,
        );
        let (top, bottom) = fill_stops(&A_STYLE, pressed == BTN_ZOOM);
        self.gradient_circle(A_CENTER, A_RADIUS, top, bottom);
        self.sheen_arc(A_CENTER, A_RADIUS - 2.5, 40.0, 140.0, 2.5, &highlight);
        // Zoom glyph: the two macOS fullscreen triangles.
        unsafe {
            let (cx, cy) = A_CENTER;
            let (h, g) = (7.0, 5.0);
            srgb(A_STYLE.glyph, 1.0).setFill();
            let tl = NSBezierPath::bezierPath();
            tl.moveToPoint(NSPoint::new(cx - h, cy + h));
            tl.lineToPoint(NSPoint::new(cx + g, cy + h));
            tl.lineToPoint(NSPoint::new(cx - h, cy - g));
            tl.closePath();
            tl.fill();
            let br = NSBezierPath::bezierPath();
            br.moveToPoint(NSPoint::new(cx + h, cy - h));
            br.lineToPoint(NSPoint::new(cx - g, cy - h));
            br.lineToPoint(NSPoint::new(cx + h, cy + g));
            br.closePath();
            br.fill();
        }

        // ---- B button (close) ----
        self.fill_circle(B_CENTER, B_RADIUS + RIM + 0.5, -1.5, &shadow);
        self.gradient_circle(
            B_CENTER,
            B_RADIUS + RIM,
            B_STYLE.rim_top,
            B_STYLE.rim_bottom,
        );
        let (top, bottom) = fill_stops(&B_STYLE, pressed == BTN_CLOSE);
        self.gradient_circle(B_CENTER, B_RADIUS, top, bottom);
        self.sheen_arc(B_CENTER, B_RADIUS - 1.8, 40.0, 140.0, 1.8, &highlight);
        let (cx, cy) = B_CENTER;
        let x = 4.0;
        let glyph = srgb(B_STYLE.glyph, 1.0);
        self.stroke_segment((cx - x, cy - x), (cx + x, cy + x), 3.5, &glyph);
        self.stroke_segment((cx - x, cy + x), (cx + x, cy - x), 3.5, &glyph);
    }
}

#[derive(Debug)]
pub(crate) struct GameCubeOverlay {
    window: Retained<NSWindow>,
}

impl GameCubeOverlay {
    /// Hide the host's native traffic lights and attach the GameCube
    /// controls as a transparent child window over its top-left
    /// corner.
    pub(crate) fn install(host: &NSWindow, mtm: MainThreadMarker) -> Self {
        for button in [
            #[allow(deprecated)]
            NSWindowFullScreenButton,
            NSWindowButton::NSWindowCloseButton,
            NSWindowButton::NSWindowMiniaturizeButton,
            NSWindowButton::NSWindowZoomButton,
        ] {
            if let Some(button) = host.standardWindowButton(button) {
                button.setHidden(true);
            }
        }

        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(OVERLAY_W, OVERLAY_H),
        );
        let overlay: Retained<NSWindow> = unsafe {
            msg_send_id![
                mtm.alloc(),
                initWithContentRect: frame,
                styleMask: NSWindowStyleMask::Borderless,
                backing: NSBackingStoreType::NSBackingStoreBuffered,
                defer: false,
            ]
        };
        unsafe {
            overlay.setReleasedWhenClosed(false);
            overlay.setBackgroundColor(Some(&NSColor::clearColor()));
        }
        overlay.setOpaque(false);
        overlay.setHasShadow(false);
        overlay.setIgnoresMouseEvents(false);

        let view = GameCubeView::new(mtm, host, frame);
        overlay.setContentView(Some(&view));

        unsafe {
            host.addChildWindow_ordered(&overlay, NSWindowOrderingMode::NSWindowAbove)
        };

        let this = GameCubeOverlay { window: overlay };
        this.sync_position(host);
        this
    }

    /// Pin the overlay so its corner anchor coincides with the host's
    /// top-left corner. Child windows follow parent moves on their
    /// own, but resizes shift the top edge, so the delegate calls this
    /// from its resize/move/fullscreen notifications.
    pub(crate) fn sync_position(&self, host: &NSWindow) {
        let host_frame = host.frame();
        let origin = NSPoint::new(
            host_frame.origin.x - CORNER_X,
            host_frame.origin.y + host_frame.size.height + CORNER_TOP - OVERLAY_H,
        );
        unsafe { self.window.setFrameOrigin(origin) };
    }

    /// Detach + hide (fullscreen, miniaturized) or re-attach + show.
    pub(crate) fn set_visible(&self, visible: bool, host: &NSWindow) {
        if visible {
            if !self.window.isVisible() {
                unsafe {
                    host.addChildWindow_ordered(
                        &self.window,
                        NSWindowOrderingMode::NSWindowAbove,
                    )
                };
                self.window.orderFront(None);
            }
        } else if self.window.isVisible() {
            unsafe { host.removeChildWindow(&self.window) };
            self.window.orderOut(None);
        }
    }
}
