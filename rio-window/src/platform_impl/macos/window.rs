#![allow(clippy::unnecessary_cast)]

use std::cell::RefCell;

use objc2::rc::{autoreleasepool, Retained};
use objc2::{
    declare_class, msg_send, msg_send_id, mutability, sel, ClassType, DeclaredClass,
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSColor, NSLayoutAttribute, NSLayoutConstraint,
    NSResponder, NSTitlebarAccessoryViewController, NSView, NSWindow, NSWindowButton,
    NSWindowOrderingMode,
};
use objc2_foundation::{
    MainThreadBound, MainThreadMarker, NSArray, NSObject, NSObjectNSDelayedPerforming,
};

use super::event_loop::ActiveEventLoop;
use super::window_delegate::WindowDelegate;
use crate::error::OsError as RootOsError;
use crate::window::WindowAttributes;

pub(crate) struct Window {
    window: MainThreadBound<Retained<WinitWindow>>,
    /// The window only keeps a weak reference to this, so we must keep it around here.
    delegate: MainThreadBound<Retained<WindowDelegate>>,
}

impl Drop for Window {
    fn drop(&mut self) {
        self.window
            .get_on_main(|window| autoreleasepool(|_| window.close()))
    }
}

impl Window {
    pub(crate) fn new(
        window_target: &ActiveEventLoop,
        attributes: WindowAttributes,
    ) -> Result<Self, RootOsError> {
        let mtm = window_target.mtm;
        let delegate = autoreleasepool(|_| {
            WindowDelegate::new(window_target.app_delegate(), attributes, mtm)
        })?;
        Ok(Window {
            window: MainThreadBound::new(delegate.window().retain(), mtm),
            delegate: MainThreadBound::new(delegate, mtm),
        })
    }

    pub(crate) fn maybe_queue_on_main(
        &self,
        f: impl FnOnce(&WindowDelegate) + Send + 'static,
    ) {
        // For now, don't actually do queuing, since it may be less predictable
        self.maybe_wait_on_main(f)
    }

    pub(crate) fn maybe_wait_on_main<R: Send>(
        &self,
        f: impl FnOnce(&WindowDelegate) -> R + Send,
    ) -> R {
        self.delegate.get_on_main(|delegate| f(delegate))
    }

    #[inline]
    pub(crate) fn raw_window_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawWindowHandle, raw_window_handle::HandleError> {
        if let Some(mtm) = MainThreadMarker::new() {
            Ok(self.delegate.get(mtm).raw_window_handle_raw_window_handle())
        } else {
            Err(raw_window_handle::HandleError::Unavailable)
        }
    }

    #[inline]
    pub(crate) fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::RawDisplayHandle::AppKit(
            raw_window_handle::AppKitDisplayHandle::new(),
        ))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub usize);

impl WindowId {
    pub const unsafe fn dummy() -> Self {
        Self(0)
    }
}

impl From<WindowId> for u64 {
    fn from(window_id: WindowId) -> Self {
        window_id.0 as u64
    }
}

impl From<u64> for WindowId {
    fn from(raw_id: u64) -> Self {
        Self(raw_id as usize)
    }
}

#[derive(Debug)]
pub struct WinitWindowState {
    pub(crate) integrate_native_tabs: bool,
    pub(crate) native_tab_constraints: RefCell<Vec<Retained<NSLayoutConstraint>>>,
    pub(crate) integrated_titlebar_glass: RefCell<Option<super::glass::GlassEffect>>,
}

fn retained(view: &NSView) -> Retained<NSView> {
    unsafe { Retained::retain(view as *const NSView as *mut NSView) }
        .expect("NSView references are never null")
}

fn first_descendant(view: &NSView, class_name: &str) -> Option<Retained<NSView>> {
    if view.class().name() == class_name {
        return Some(retained(view));
    }

    for child in unsafe { view.subviews() }.iter() {
        if let Some(found) = first_descendant(child, class_name) {
            return Some(found);
        }
    }

    None
}

fn first_ancestor(view: &NSView, class_names: &[&str]) -> Option<Retained<NSView>> {
    let mut ancestor = unsafe { view.superview() };
    while let Some(view) = ancestor {
        if class_names.contains(&view.class().name()) {
            return Some(view);
        }
        ancestor = unsafe { view.superview() };
    }

    None
}

unsafe fn set_layer_background_color(layer: &objc2::runtime::AnyObject, color: &NSColor) {
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};

    let layer = layer as *const _ as *mut Object;
    let color = color as *const _ as *mut Object;
    let cg_color: core_graphics::sys::CGColorRef = msg_send![color, CGColor];
    let _: () = msg_send![layer, setBackgroundColor: cg_color];
}

declare_class!(
    #[derive(Debug)]
    pub struct WinitWindow;

    unsafe impl ClassType for WinitWindow {
        #[inherits(NSResponder, NSObject)]
        type Super = NSWindow;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "WinitWindow";
    }

    impl DeclaredClass for WinitWindow {
        type Ivars = WinitWindowState;
    }

    unsafe impl WinitWindow {
        #[method(canBecomeMainWindow)]
        fn can_become_main_window(&self) -> bool {
            trace_scope!("canBecomeMainWindow");
            true
        }

        #[method(canBecomeKeyWindow)]
        fn can_become_key_window(&self) -> bool {
            trace_scope!("canBecomeKeyWindow");
            true
        }

        #[method(addTitlebarAccessoryViewController:)]
        fn add_titlebar_accessory_view_controller(
            &self,
            child: &NSTitlebarAccessoryViewController,
        ) {
            if self.ivars().integrate_native_tabs {
                unsafe { child.setLayoutAttribute(NSLayoutAttribute::Right) };
            }

            unsafe {
                let _: () = msg_send![
                    super(self),
                    addTitlebarAccessoryViewController: child
                ];
            }

            if self.ivars().integrate_native_tabs {
                self.reattach_integrated_titlebar_glass();
                unsafe {
                    self.performSelector_withObject_afterDelay(
                        sel!(setupNativeTabBar),
                        None,
                        0.0,
                    )
                };
            }
        }

        #[method(becomeMainWindow)]
        fn become_main_window(&self) {
            unsafe {
                let _: () = msg_send![super(self), becomeMainWindow];
            }
            if self.ivars().integrate_native_tabs {
                self.reattach_integrated_titlebar_glass();
                unsafe {
                    self.performSelector_withObject_afterDelay(
                        sel!(setupNativeTabBar),
                        None,
                        0.0,
                    )
                };
            }
        }

        #[method(setupNativeTabBar)]
        fn setup_native_tab_bar(&self) {
            if !self.ivars().integrate_native_tabs
                || !unsafe { self.isMainWindow() }
            {
                return;
            }

            let Some(content_view) = self.contentView() else {
                return;
            };
            let mut root = content_view;
            while let Some(parent) = unsafe { root.superview() } {
                root = parent;
            }

            let Some(tab_bar) = first_descendant(&root, "NSTabBar") else {
                return;
            };
            self.reattach_integrated_titlebar_glass();
            let Some(toolbar) = first_descendant(&root, "NSToolbarView") else {
                return;
            };
            let Some(clip_view) = first_ancestor(
                &tab_bar,
                // AppKit renamed this private container in macOS 27 beta.
                &[
                    "NSTitlebarAccessoryClipView",
                    "NSTitlebarAccessoryContainerView",
                ],
            ) else {
                return;
            };
            let subviews = unsafe { clip_view.subviews() };
            let Some(accessory_view) = subviews.iter().next() else {
                return;
            };

            self.clear_native_tab_constraints();
            let left_inset = self
                .standardWindowButton(NSWindowButton::NSWindowZoomButton)
                .map(|button| {
                    let frame = button.frame();
                    frame.origin.x + frame.size.width
                })
                .unwrap_or(70.0);

            unsafe {
                clip_view.setTranslatesAutoresizingMaskIntoConstraints(false);
                accessory_view.setTranslatesAutoresizingMaskIntoConstraints(false);

                let constraints = vec![
                    clip_view
                        .leftAnchor()
                        .constraintEqualToAnchor_constant(&toolbar.leftAnchor(), left_inset),
                    clip_view
                    .rightAnchor()
                    .constraintEqualToAnchor(&toolbar.rightAnchor()),
                    clip_view
                    .topAnchor()
                    .constraintEqualToAnchor_constant(&toolbar.topAnchor(), 2.0),
                    clip_view
                    .heightAnchor()
                    .constraintEqualToAnchor(&toolbar.heightAnchor()),
                    accessory_view
                    .leftAnchor()
                    .constraintEqualToAnchor(&clip_view.leftAnchor()),
                    accessory_view
                    .rightAnchor()
                    .constraintEqualToAnchor(&clip_view.rightAnchor()),
                    accessory_view
                    .topAnchor()
                    .constraintEqualToAnchor(&clip_view.topAnchor()),
                    accessory_view
                    .heightAnchor()
                    .constraintEqualToAnchor(&clip_view.heightAnchor()),
                ];
                let constraint_refs = constraints
                    .iter()
                    .map(|constraint| &**constraint)
                    .collect::<Vec<_>>();
                NSLayoutConstraint::activateConstraints(&NSArray::from_slice(
                    &constraint_refs,
                ));
                self.ivars().native_tab_constraints.replace(constraints);

                clip_view.setNeedsLayout(true);
                accessory_view.setNeedsLayout(true);
            }
        }
    }
);

impl WinitWindow {
    pub(super) fn id(&self) -> WindowId {
        WindowId(self as *const Self as usize)
    }

    pub(super) fn uses_integrated_native_tabs(&self) -> bool {
        self.ivars().integrate_native_tabs
    }

    pub(super) fn set_integrated_titlebar_background(
        &self,
        color: &NSColor,
        active: bool,
    ) {
        if !self.ivars().integrate_native_tabs {
            return;
        }

        let Some(content_view) = self.contentView() else {
            return;
        };
        let mut root = content_view;
        while let Some(parent) = unsafe { root.superview() } {
            root = parent;
        }
        let Some(background) = first_descendant(&root, "NSTitlebarContainerView") else {
            return;
        };
        let has_glass = self.ivars().integrated_titlebar_glass.borrow().is_some();
        let background_color = unsafe {
            color.colorWithAlphaComponent(if has_glass {
                0.0
            } else if active {
                1.0
            } else {
                0.0
            })
        };

        unsafe {
            background.setWantsLayer(true);
            let layer: Option<Retained<objc2::runtime::AnyObject>> =
                msg_send_id![&*background, layer];
            if let Some(layer) = layer {
                set_layer_background_color(&layer, &background_color);
            }
        }
    }

    pub(super) fn install_or_update_integrated_titlebar_glass(
        &self,
        style: super::glass::GlassStyle,
        color: &NSColor,
        opacity: f64,
    ) {
        if !self.ivars().integrate_native_tabs {
            return;
        }

        if self.ivars().integrated_titlebar_glass.borrow().is_none() {
            *self.ivars().integrated_titlebar_glass.borrow_mut() =
                super::glass::GlassEffect::new();
        }

        if let Some(glass) = self.ivars().integrated_titlebar_glass.borrow().as_ref() {
            glass.set_style(style);
            glass.set_tint_color_with_opacity(color, opacity);
            glass.set_corner_radius(0.0);
        }
        self.reattach_integrated_titlebar_glass();
        self.set_integrated_titlebar_background(color, self.isKeyWindow());
    }

    pub(super) fn update_integrated_titlebar_glass_tint(
        &self,
        color: &NSColor,
        opacity: f64,
    ) {
        if let Some(glass) = self.ivars().integrated_titlebar_glass.borrow().as_ref() {
            glass.set_tint_color_with_opacity(color, opacity);
        }
    }

    pub(super) fn uninstall_integrated_titlebar_glass(&self) {
        if let Some(glass) = self.ivars().integrated_titlebar_glass.borrow_mut().take() {
            unsafe { glass.as_ns_view().removeFromSuperview() };
        }
    }

    fn reattach_integrated_titlebar_glass(&self) {
        let Some(content_view) = self.contentView() else {
            return;
        };
        let mut root = content_view;
        while let Some(parent) = unsafe { root.superview() } {
            root = parent;
        }
        let Some(titlebar) = first_descendant(&root, "NSTitlebarContainerView") else {
            return;
        };
        let glass_ref = self.ivars().integrated_titlebar_glass.borrow();
        let Some(glass) = glass_ref.as_ref() else {
            return;
        };
        let view = glass.as_ns_view();

        unsafe {
            view.setFrame(titlebar.bounds());
            view.setAutoresizingMask(
                NSAutoresizingMaskOptions::NSViewWidthSizable
                    | NSAutoresizingMaskOptions::NSViewHeightSizable,
            );
            titlebar.addSubview_positioned_relativeTo(
                view,
                NSWindowOrderingMode::NSWindowBelow,
                None,
            );
        }
    }

    fn clear_native_tab_constraints(&self) {
        let constraints = self.ivars().native_tab_constraints.take();
        if constraints.is_empty() {
            return;
        }

        unsafe {
            let constraint_refs = constraints
                .iter()
                .map(|constraint| &**constraint)
                .collect::<Vec<_>>();
            NSLayoutConstraint::deactivateConstraints(&NSArray::from_slice(
                &constraint_refs,
            ));
        }
    }
}
