use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::os::raw::{c_char, c_int, c_long, c_ulong};
use std::slice;
use std::sync::{Arc, Mutex};

use x11_dl::xinput2::{
    self, XIDeviceEvent, XIEnterEvent, XIFocusInEvent, XIFocusOutEvent, XIHierarchyEvent,
    XILeaveEvent, XIModifierState, XIRawEvent,
};
use x11_dl::xlib::{
    self, Display as XDisplay, Window as XWindow, XAnyEvent, XClientMessageEvent,
    XConfigureEvent, XDestroyWindowEvent, XEvent, XExposeEvent, XKeyEvent, XMapEvent,
    XPropertyEvent, XReparentEvent, XSelectionEvent, XVisibilityEvent, XkbAnyEvent,
    XkbStateRec,
};
use x11rb::protocol::xinput;
use x11rb::protocol::xkb::ID as XkbId;
use x11rb::protocol::xproto::{self, ConnectionExt as _, ModMask};
use x11rb::x11_utils::{ExtensionInformation, Serialize};
use xkbcommon_dl::xkb_mod_mask_t;

use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::event::{
    DeviceEvent, ElementState, Event, Ime, InnerSizeWriter, MouseButton,
    MouseScrollDelta, RawKeyEvent, Touch, TouchPhase, WindowEvent,
};
use crate::event_loop::ActiveEventLoop as RootAEL;
use crate::keyboard::ModifiersState;
use crate::platform_impl::common::xkb::{self, XkbState};
use crate::platform_impl::platform::common::xkb::Context;
use crate::platform_impl::platform::x11::ime::{ImeEvent, ImeEventReceiver, ImeRequest};
use crate::platform_impl::platform::x11::ActiveEventLoop;
use crate::platform_impl::platform::ActiveEventLoop as PlatformActiveEventLoop;
use crate::platform_impl::x11::atoms::*;
use crate::platform_impl::x11::util::cookie::GenericEventCookie;
use crate::platform_impl::x11::{
    mkdid, mkwid, util, CookieResultExt, Device, DeviceId, DeviceInfo, Dnd, DndState,
    ImeReceiver, ScrollOrientation, UnownedWindow, WindowId,
};

/// The maximum amount of X modifiers to replay.
pub const MAX_MOD_REPLAY_LEN: usize = 32;

/// The X11 documentation states: "Keycodes lie in the inclusive range `[8, 255]`".
const KEYCODE_OFFSET: u8 = 8;

pub struct EventProcessor {
    pub dnd: Dnd,
    pub ime_receiver: ImeReceiver,
    pub ime_event_receiver: ImeEventReceiver,
    pub randr_event_offset: u8,
    pub devices: RefCell<HashMap<DeviceId, Device>>,
    pub xi2ext: ExtensionInformation,
    pub xkbext: ExtensionInformation,
    pub target: RootAEL,
    pub xkb_context: Context,
    // Number of touch events currently in progress
    pub num_touch: u32,
    // This is the last pressed key that is repeatable (if it hasn't been
    // released).
    //
    // Used to detect key repeats.
    pub held_key_press: Option<u32>,
    pub first_touch: Option<u64>,
    // Currently focused window belonging to this process
    pub active_window: Option<xproto::Window>,
    /// Latest modifiers we've sent for the user to trigger change in event.
    pub modifiers: Cell<ModifiersState>,
    pub xfiltered_modifiers: VecDeque<c_ulong>,
    pub xmodmap: util::ModifierKeymap,
    pub is_composing: bool,
}

impl EventProcessor {
    pub fn process_event<T: 'static, F>(&mut self, xev: &mut XEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        self.process_xevent(xev, &mut callback);

        let window_target = Self::window_target_mut(&mut self.target);

        // Handle IME requests.
        while let Ok(request) = self.ime_receiver.try_recv() {
            let ime = match window_target.ime.as_mut() {
                Some(ime) => ime,
                None => continue,
            };
            let ime = ime.get_mut();
            match request {
                ImeRequest::Position(window_id, x, y) => {
                    ime.send_xim_spot(window_id, x, y);
                }
                ImeRequest::Allow(window_id, allowed) => {
                    ime.set_ime_allowed(window_id, allowed);
                }
            }
        }

        // Drain IME events.
        while let Ok((window, event)) = self.ime_event_receiver.try_recv() {
            let window_id = mkwid(window as xproto::Window);
            let event = match event {
                ImeEvent::Enabled => WindowEvent::Ime(Ime::Enabled),
                ImeEvent::Start => {
                    self.is_composing = true;
                    WindowEvent::Ime(Ime::Preedit("".to_owned(), None))
                }
                ImeEvent::Update(text, position) if self.is_composing => {
                    WindowEvent::Ime(Ime::Preedit(text, Some((position, position))))
                }
                ImeEvent::End => {
                    self.is_composing = false;
                    // Issue empty preedit on `Done`.
                    WindowEvent::Ime(Ime::Preedit(String::new(), None))
                }
                ImeEvent::Disabled => {
                    self.is_composing = false;
                    WindowEvent::Ime(Ime::Disabled)
                }
                _ => continue,
            };

            callback(&self.target, Event::WindowEvent { window_id, event });
        }
    }

    /// XFilterEvent tells us when an event has been discarded by the input method.
    /// Specifically, this involves all of the KeyPress events in compose/pre-edit sequences,
    /// along with an extra copy of the KeyRelease events. This also prevents backspace and
    /// arrow keys from being detected twice.
    fn filter_event(&mut self, xev: &mut XEvent) -> bool {
        let wt = Self::window_target(&self.target);
        unsafe {
            (wt.xconn.xlib.XFilterEvent)(xev, {
                let xev: &XAnyEvent = xev.as_ref();
                xev.window
            }) == xlib::True
        }
    }

    fn process_xevent<T: 'static, F>(&mut self, xev: &mut XEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let event_type = xev.get_type();

        if self.filter_event(xev) {
            if event_type == xlib::KeyPress || event_type == xlib::KeyRelease {
                let xev: &XKeyEvent = xev.as_ref();
                if self.xmodmap.is_modifier(xev.keycode as u8) {
                    // Don't grow the buffer past the `MAX_MOD_REPLAY_LEN`. This could happen
                    // when the modifiers are consumed entirely or serials are altered.
                    //
                    // Both cases shouldn't happen in well behaving clients.
                    if self.xfiltered_modifiers.len() == MAX_MOD_REPLAY_LEN {
                        self.xfiltered_modifiers.pop_back();
                    }
                    self.xfiltered_modifiers.push_front(xev.serial);
                }
            }
            return;
        }

        match event_type {
            xlib::ClientMessage => self.client_message(xev.as_ref(), &mut callback),
            xlib::SelectionNotify => self.selection_notify(xev.as_ref(), &mut callback),
            xlib::ConfigureNotify => self.configure_notify(xev.as_ref(), &mut callback),
            xlib::ReparentNotify => self.reparent_notify(xev.as_ref()),
            xlib::MapNotify => self.map_notify(xev.as_ref(), &mut callback),
            xlib::DestroyNotify => self.destroy_notify(xev.as_ref(), &mut callback),
            xlib::PropertyNotify => self.property_notify(xev.as_ref(), &mut callback),
            xlib::VisibilityNotify => self.visibility_notify(xev.as_ref(), &mut callback),
            xlib::Expose => self.expose(xev.as_ref(), &mut callback),
            // Note that in compose/pre-edit sequences, we'll always receive KeyRelease events.
            ty @ xlib::KeyPress | ty @ xlib::KeyRelease => {
                let state = if ty == xlib::KeyPress {
                    ElementState::Pressed
                } else {
                    ElementState::Released
                };

                self.xinput_key_input(xev.as_mut(), state, &mut callback);
            }
            xlib::GenericEvent => {
                let wt = Self::window_target(&self.target);
                let xev: GenericEventCookie =
                    match GenericEventCookie::from_event(wt.xconn.clone(), *xev) {
                        Some(xev) if xev.extension() == self.xi2ext.major_opcode => xev,
                        _ => return,
                    };

                let evtype = xev.evtype();

                match evtype {
                    ty @ xinput2::XI_ButtonPress | ty @ xinput2::XI_ButtonRelease => {
                        let state = if ty == xinput2::XI_ButtonPress {
                            ElementState::Pressed
                        } else {
                            ElementState::Released
                        };

                        let xev: &XIDeviceEvent = unsafe { xev.as_event() };
                        self.update_mods_from_xinput2_event(
                            &xev.mods,
                            &xev.group,
                            false,
                            &mut callback,
                        );
                        self.xinput2_button_input(xev, state, &mut callback);
                    }
                    xinput2::XI_Motion => {
                        let xev: &XIDeviceEvent = unsafe { xev.as_event() };
                        self.update_mods_from_xinput2_event(
                            &xev.mods,
                            &xev.group,
                            false,
                            &mut callback,
                        );
                        self.xinput2_mouse_motion(xev, &mut callback);
                    }
                    xinput2::XI_Enter => {
                        let xev: &XIEnterEvent = unsafe { xev.as_event() };
                        self.xinput2_mouse_enter(xev, &mut callback);
                    }
                    xinput2::XI_Leave => {
                        let xev: &XILeaveEvent = unsafe { xev.as_event() };
                        self.update_mods_from_xinput2_event(
                            &xev.mods,
                            &xev.group,
                            false,
                            &mut callback,
                        );
                        self.xinput2_mouse_left(xev, &mut callback);
                    }
                    xinput2::XI_FocusIn => {
                        let xev: &XIFocusInEvent = unsafe { xev.as_event() };
                        self.xinput2_focused(xev, &mut callback);
                    }
                    xinput2::XI_FocusOut => {
                        let xev: &XIFocusOutEvent = unsafe { xev.as_event() };
                        self.xinput2_unfocused(xev, &mut callback);
                    }
                    xinput2::XI_TouchBegin
                    | xinput2::XI_TouchUpdate
                    | xinput2::XI_TouchEnd => {
                        let phase = match evtype {
                            xinput2::XI_TouchBegin => TouchPhase::Started,
                            xinput2::XI_TouchUpdate => TouchPhase::Moved,
                            xinput2::XI_TouchEnd => TouchPhase::Ended,
                            _ => unreachable!(),
                        };

                        let xev: &XIDeviceEvent = unsafe { xev.as_event() };
                        self.xinput2_touch(xev, phase, &mut callback);
                    }
                    xinput2::XI_RawButtonPress | xinput2::XI_RawButtonRelease => {
                        let state = match evtype {
                            xinput2::XI_RawButtonPress => ElementState::Pressed,
                            xinput2::XI_RawButtonRelease => ElementState::Released,
                            _ => unreachable!(),
                        };

                        let xev: &XIRawEvent = unsafe { xev.as_event() };
                        self.xinput2_raw_button_input(xev, state, &mut callback);
                    }
                    xinput2::XI_RawMotion => {
                        let xev: &XIRawEvent = unsafe { xev.as_event() };
                        self.xinput2_raw_mouse_motion(xev, &mut callback);
                    }
                    xinput2::XI_RawKeyPress | xinput2::XI_RawKeyRelease => {
                        let state = match evtype {
                            xinput2::XI_RawKeyPress => ElementState::Pressed,
                            xinput2::XI_RawKeyRelease => ElementState::Released,
                            _ => unreachable!(),
                        };

                        let xev: &xinput2::XIRawEvent = unsafe { xev.as_event() };
                        self.xinput2_raw_key_input(xev, state, &mut callback);
                    }

                    xinput2::XI_HierarchyChanged => {
                        let xev: &XIHierarchyEvent = unsafe { xev.as_event() };
                        self.xinput2_hierarchy_changed(xev, &mut callback);
                    }
                    _ => {}
                }
            }
            _ => {
                if event_type == self.xkbext.first_event as _ {
                    let xev: &XkbAnyEvent =
                        unsafe { &*(xev as *const _ as *const XkbAnyEvent) };
                    self.xkb_event(xev, &mut callback);
                }
                if event_type == self.randr_event_offset as c_int {
                    self.process_dpi_change(&mut callback);
                }
            }
        }
    }

    pub fn poll(&self) -> bool {
        let window_target = Self::window_target(&self.target);
        let result =
            unsafe { (window_target.xconn.xlib.XPending)(window_target.xconn.display) };

        result != 0
    }

    pub unsafe fn poll_one_event(&mut self, event_ptr: *mut XEvent) -> bool {
        let window_target = Self::window_target(&self.target);
        // This function is used to poll and remove a single event
        // from the Xlib event queue in a non-blocking, atomic way.
        // XCheckIfEvent is non-blocking and removes events from queue.
        // XNextEvent can't be used because it blocks while holding the
        // global Xlib mutex.
        // XPeekEvent does not remove events from the queue.
        unsafe extern "C" fn predicate(
            _display: *mut XDisplay,
            _event: *mut XEvent,
            _arg: *mut c_char,
        ) -> c_int {
            // This predicate always returns "true" (1) to accept all events
            1
        }

        let result = unsafe {
            (window_target.xconn.xlib.XCheckIfEvent)(
                window_target.xconn.display,
                event_ptr,
                Some(predicate),
                std::ptr::null_mut(),
            )
        };

        result != 0
    }

    pub fn init_device(&self, device: xinput::DeviceId) {
        let window_target = Self::window_target(&self.target);
        let mut devices = self.devices.borrow_mut();
        if let Some(info) = DeviceInfo::get(&window_target.xconn, device as _) {
            for info in info.iter() {
                devices.insert(DeviceId(info.deviceid as _), Device::new(info));
            }
        }
    }

    pub fn with_window<F, Ret>(
        &self,
        window_id: xproto::Window,
        callback: F,
    ) -> Option<Ret>
    where
        F: Fn(&Arc<UnownedWindow>) -> Ret,
    {
        let mut deleted = false;
        let window_id = WindowId(window_id as _);
        let window_target = Self::window_target(&self.target);
        let result = window_target
            .windows
            .borrow()
            .get(&window_id)
            .and_then(|window| {
                let arc = window.upgrade();
                deleted = arc.is_none();
                arc
            })
            .map(|window| callback(&window));

        if deleted {
            // Garbage collection
            window_target.windows.borrow_mut().remove(&window_id);
        }

        result
    }

    // NOTE: we avoid `self` to not borrow the entire `self` as not mut.
    /// Get the platform window target.
    pub fn window_target(window_target: &RootAEL) -> &ActiveEventLoop {
        match &window_target.p {
            PlatformActiveEventLoop::X(target) => target,
            #[cfg(wayland_platform)]
            _ => unreachable!(),
        }
    }

    /// Get the platform window target.
    pub fn window_target_mut(window_target: &mut RootAEL) -> &mut ActiveEventLoop {
        match &mut window_target.p {
            PlatformActiveEventLoop::X(target) => target,
            #[cfg(wayland_platform)]
            _ => unreachable!(),
        }
    }

    fn client_message<T: 'static, F>(
        &mut self,
        xev: &XClientMessageEvent,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);
        let atoms = wt.xconn.atoms();

        let window = xev.window as xproto::Window;
        let window_id = mkwid(window);

        if xev.data.get_long(0) as xproto::Atom == wt.wm_delete_window {
            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
            };
            callback(&self.target, event);
            return;
        }

        if xev.data.get_long(0) as xproto::Atom == wt.net_wm_ping {
            let client_msg = xproto::ClientMessageEvent {
                response_type: xproto::CLIENT_MESSAGE_EVENT,
                format: xev.format as _,
                sequence: xev.serial as _,
                window: wt.root,
                type_: xev.message_type as _,
                data: xproto::ClientMessageData::from({
                    let [a, b, c, d, e]: [c_long; 5] =
                        xev.data.as_longs().try_into().unwrap();
                    [a as u32, b as u32, c as u32, d as u32, e as u32]
                }),
            };

            wt.xconn
                .xcb_connection()
                .send_event(
                    false,
                    wt.root,
                    xproto::EventMask::SUBSTRUCTURE_NOTIFY
                        | xproto::EventMask::SUBSTRUCTURE_REDIRECT,
                    client_msg.serialize(),
                )
                .expect_then_ignore_error("Failed to send `ClientMessage` event.");
            return;
        }

        if xev.message_type == atoms[XdndEnter] as c_ulong {
            let source_window = xev.data.get_long(0) as xproto::Window;
            let flags = xev.data.get_long(1);
            let version = flags >> 24;
            self.dnd.version = Some(version);
            let has_more_types = flags - (flags & (c_long::MAX - 1)) == 1;
            if !has_more_types {
                let type_list = vec![
                    xev.data.get_long(2) as xproto::Atom,
                    xev.data.get_long(3) as xproto::Atom,
                    xev.data.get_long(4) as xproto::Atom,
                ];
                self.dnd.type_list = Some(type_list);
            } else if let Ok(more_types) =
                unsafe { self.dnd.get_type_list(source_window) }
            {
                self.dnd.type_list = Some(more_types);
            }
            return;
        }

        if xev.message_type == atoms[XdndPosition] as c_ulong {
            // This event occurs every time the mouse moves while a file's being dragged
            // over our window. We emit HoveredFile in response; while the macOS backend
            // does that upon a drag entering, XDND doesn't have access to the actual drop
            // data until this event. For parity with other platforms, we only emit
            // `HoveredFile` the first time, though if winit's API is later extended to
            // supply position updates with `HoveredFile` or another event, implementing
            // that here would be trivial.

            let source_window = xev.data.get_long(0) as xproto::Window;

            // Equivalent to `(x << shift) | y`
            // where `shift = mem::size_of::<c_short>() * 8`
            // Note that coordinates are in "desktop space", not "window space"
            // (in X11 parlance, they're root window coordinates)
            // let packed_coordinates = xev.data.get_long(2);
            // let shift = mem::size_of::<libc::c_short>() * 8;
            // let x = packed_coordinates >> shift;
            // let y = packed_coordinates & !(x << shift);

            // By our own state flow, `version` should never be `None` at this point.
            let version = self.dnd.version.unwrap_or(5);

            // Action is specified in versions 2 and up, though we don't need it anyway.
            // let action = xev.data.get_long(4);

            let accepted = if let Some(ref type_list) = self.dnd.type_list {
                type_list.contains(&atoms[TextUriList])
            } else {
                false
            };

            if !accepted {
                unsafe {
                    self.dnd
                        .send_status(window, source_window, DndState::Rejected)
                        .expect("Failed to send `XdndStatus` message.");
                }
                self.dnd.reset();
                return;
            }

            self.dnd.source_window = Some(source_window);
            if self.dnd.result.is_none() {
                let time = if version >= 1 {
                    xev.data.get_long(3) as xproto::Timestamp
                } else {
                    // In version 0, time isn't specified
                    x11rb::CURRENT_TIME
                };

                // Log this timestamp.
                wt.xconn.set_timestamp(time);

                // This results in the `SelectionNotify` event below
                unsafe {
                    self.dnd.convert_selection(window, time);
                }
            }

            unsafe {
                self.dnd
                    .send_status(window, source_window, DndState::Accepted)
                    .expect("Failed to send `XdndStatus` message.");
            }
            return;
        }

        if xev.message_type == atoms[XdndDrop] as c_ulong {
            let (source_window, state) =
                if let Some(source_window) = self.dnd.source_window {
                    if let Some(Ok(ref path_list)) = self.dnd.result {
                        for path in path_list {
                            let event = Event::WindowEvent {
                                window_id,
                                event: WindowEvent::DroppedFile(path.clone()),
                            };
                            callback(&self.target, event);
                        }
                    }
                    (source_window, DndState::Accepted)
                } else {
                    // `source_window` won't be part of our DND state if we already rejected the drop in
                    // our `XdndPosition` handler.
                    let source_window = xev.data.get_long(0) as xproto::Window;
                    (source_window, DndState::Rejected)
                };

            unsafe {
                self.dnd
                    .send_finished(window, source_window, state)
                    .expect("Failed to send `XdndFinished` message.");
            }

            self.dnd.reset();
            return;
        }

        if xev.message_type == atoms[XdndLeave] as c_ulong {
            self.dnd.reset();
            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::HoveredFileCancelled,
            };
            callback(&self.target, event);
        }
    }

    fn selection_notify<T: 'static, F>(&mut self, xev: &XSelectionEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);
        let atoms = wt.xconn.atoms();

        let window = xev.requestor as xproto::Window;
        let window_id = mkwid(window);

        // Set the timestamp.
        wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

        if xev.property != atoms[XdndSelection] as c_ulong {
            return;
        }

        // This is where we receive data from drag and drop
        self.dnd.result = None;
        if let Ok(mut data) = unsafe { self.dnd.read_data(window) } {
            let parse_result = self.dnd.parse_data(&mut data);
            if let Ok(ref path_list) = parse_result {
                for path in path_list {
                    let event = Event::WindowEvent {
                        window_id,
                        event: WindowEvent::HoveredFile(path.clone()),
                    };
                    callback(&self.target, event);
                }
            }
            self.dnd.result = Some(parse_result);
        }
    }

    fn configure_notify<T: 'static, F>(&self, xev: &XConfigureEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        let xwindow = xev.window as xproto::Window;
        let window_id = mkwid(xwindow);

        let window = match self.with_window(xwindow, Arc::clone) {
            Some(window) => window,
            None => return,
        };

        // So apparently...
        // `XSendEvent` (synthetic `ConfigureNotify`) -> position relative to root
        // `XConfigureNotify` (real `ConfigureNotify`) -> position relative to parent
        // https://tronche.com/gui/x/icccm/sec-4.html#s-4.1.5
        // We don't want to send `Moved` when this is false, since then every `Resized`
        // (whether the window moved or not) is accompanied by an extraneous `Moved` event
        // that has a position relative to the parent window.
        let is_synthetic = xev.send_event == xlib::True;

        // These are both in physical space.
        let new_inner_size = (xev.width as u32, xev.height as u32);
        let new_inner_position = (xev.x, xev.y);

        let (mut resized, moved) = {
            let mut shared_state_lock = window.shared_state_lock();

            let resized = util::maybe_change(&mut shared_state_lock.size, new_inner_size);
            let moved = if is_synthetic {
                util::maybe_change(
                    &mut shared_state_lock.inner_position,
                    new_inner_position,
                )
            } else {
                // Detect when frame extents change.
                // Since this isn't synthetic, as per the notes above, this position is relative to
                // the parent window.
                let rel_parent = new_inner_position;
                if util::maybe_change(
                    &mut shared_state_lock.inner_position_rel_parent,
                    rel_parent,
                ) {
                    // This ensures we process the next `Moved`.
                    shared_state_lock.inner_position = None;
                    // Extra insurance against stale frame extents.
                    shared_state_lock.frame_extents = None;
                }
                false
            };
            (resized, moved)
        };

        let position = window.shared_state_lock().position;

        let new_outer_position = if let (Some(position), false) = (position, moved) {
            position
        } else {
            let mut shared_state_lock = window.shared_state_lock();

            // We need to convert client area position to window position.
            let frame_extents = shared_state_lock
                .frame_extents
                .as_ref()
                .cloned()
                .unwrap_or_else(|| {
                    let frame_extents =
                        wt.xconn.get_frame_extents_heuristic(xwindow, wt.root);
                    shared_state_lock.frame_extents = Some(frame_extents.clone());
                    frame_extents
                });
            let outer = frame_extents
                .inner_pos_to_outer(new_inner_position.0, new_inner_position.1);
            shared_state_lock.position = Some(outer);

            // Unlock shared state to prevent deadlock in callback below
            drop(shared_state_lock);

            if moved {
                callback(
                    &self.target,
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::Moved(outer.into()),
                    },
                );
            }
            outer
        };

        if is_synthetic {
            let mut shared_state_lock = window.shared_state_lock();
            // If we don't use the existing adjusted value when available, then the user can screw
            // up the resizing by dragging across monitors *without* dropping the
            // window.
            let (width, height) = shared_state_lock
                .dpi_adjusted
                .unwrap_or((xev.width as u32, xev.height as u32));

            let last_scale_factor = shared_state_lock.last_monitor.scale_factor;
            let new_scale_factor = {
                let window_rect = util::AaRect::new(new_outer_position, new_inner_size);
                let monitor = wt
                    .xconn
                    .get_monitor_for_window(Some(window_rect))
                    .expect("Failed to find monitor for window");

                if monitor.is_dummy() {
                    // Avoid updating monitor using a dummy monitor handle
                    last_scale_factor
                } else {
                    shared_state_lock.last_monitor = monitor.clone();
                    monitor.scale_factor
                }
            };
            if last_scale_factor != new_scale_factor {
                let (new_width, new_height) = window.adjust_for_dpi(
                    last_scale_factor,
                    new_scale_factor,
                    width,
                    height,
                    &shared_state_lock,
                );

                let old_inner_size = PhysicalSize::new(width, height);
                let new_inner_size = PhysicalSize::new(new_width, new_height);

                // Unlock shared state to prevent deadlock in callback below
                drop(shared_state_lock);

                let inner_size = Arc::new(Mutex::new(new_inner_size));
                callback(
                    &self.target,
                    Event::WindowEvent {
                        window_id,
                        event: WindowEvent::ScaleFactorChanged {
                            scale_factor: new_scale_factor,
                            inner_size_writer: InnerSizeWriter::new(Arc::downgrade(
                                &inner_size,
                            )),
                        },
                    },
                );

                let new_inner_size = *inner_size.lock().unwrap();
                drop(inner_size);

                if new_inner_size != old_inner_size {
                    window.request_inner_size_physical(
                        new_inner_size.width,
                        new_inner_size.height,
                    );
                    window.shared_state_lock().dpi_adjusted = Some(new_inner_size.into());
                    // if the DPI factor changed, force a resize event to ensure the logical
                    // size is computed with the right DPI factor
                    resized = true;
                }
            }
        }

        // NOTE: Ensure that the lock is dropped before handling the resized and
        // sending the event back to user.
        let hittest = {
            let mut shared_state_lock = window.shared_state_lock();
            let hittest = shared_state_lock.cursor_hittest;

            // This is a hack to ensure that the DPI adjusted resize is actually
            // applied on all WMs. KWin doesn't need this, but Xfwm does. The hack
            // should not be run on other WMs, since tiling WMs constrain the window
            // size, making the resize fail. This would cause an endless stream of
            // XResizeWindow requests, making Xorg, the winit client, and the WM
            // consume 100% of CPU.
            if let Some(adjusted_size) = shared_state_lock.dpi_adjusted {
                if new_inner_size == adjusted_size || !util::wm_name_is_one_of(&["Xfwm4"])
                {
                    // When this finally happens, the event will not be synthetic.
                    shared_state_lock.dpi_adjusted = None;
                } else {
                    // Unlock shared state to prevent deadlock in callback below
                    drop(shared_state_lock);
                    window.request_inner_size_physical(adjusted_size.0, adjusted_size.1);
                }
            }

            hittest
        };

        // Reload hittest.
        if hittest.unwrap_or(false) {
            let _ = window.set_cursor_hittest(true);
        }

        if resized {
            callback(
                &self.target,
                Event::WindowEvent {
                    window_id,
                    event: WindowEvent::Resized(new_inner_size.into()),
                },
            );
        }
    }

    /// This is generally a reliable way to detect when the window manager's been
    /// replaced, though this event is only fired by reparenting window managers
    /// (which is almost all of them). Failing to correctly update WM info doesn't
    /// really have much impact, since on the WMs affected (xmonad, dwm, etc.) the only
    /// effect is that we waste some time trying to query unsupported properties.
    fn reparent_notify(&self, xev: &XReparentEvent) {
        let wt = Self::window_target(&self.target);

        wt.xconn.update_cached_wm_info(wt.root);

        self.with_window(xev.window as xproto::Window, |window| {
            window.invalidate_cached_frame_extents();
        });
    }

    fn map_notify<T: 'static, F>(&self, xev: &XMapEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let window = xev.window as xproto::Window;
        let window_id = mkwid(window);

        // NOTE: Re-issue the focus state when mapping the window.
        //
        // The purpose of it is to deliver initial focused state of the newly created
        // window, given that we can't rely on `CreateNotify`, due to it being not
        // sent.
        let focus = self
            .with_window(window, |window| window.has_focus())
            .unwrap_or_default();
        let event = Event::WindowEvent {
            window_id,
            event: WindowEvent::Focused(focus),
        };

        callback(&self.target, event);
    }

    fn destroy_notify<T: 'static, F>(&self, xev: &XDestroyWindowEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        let window = xev.window as xproto::Window;
        let window_id = mkwid(window);

        // In the event that the window's been destroyed without being dropped first, we
        // cleanup again here.
        wt.windows.borrow_mut().remove(&WindowId(window as _));

        // Since all XIM stuff needs to happen from the same thread, we destroy the input
        // context here instead of when dropping the window.
        if let Some(ime) = wt.ime.as_ref() {
            ime.borrow_mut()
                .remove_context(window as XWindow)
                .expect("Failed to destroy input context");
        }

        callback(
            &self.target,
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Destroyed,
            },
        );
    }

    fn property_notify<T: 'static, F>(&mut self, xev: &XPropertyEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);
        let atoms = wt.x_connection().atoms();
        let atom = xev.atom as xproto::Atom;

        if atom == xproto::Atom::from(xproto::AtomEnum::RESOURCE_MANAGER)
            || atom == atoms[_XSETTINGS_SETTINGS]
        {
            self.process_dpi_change(&mut callback);
        }
    }

    fn visibility_notify<T: 'static, F>(&self, xev: &XVisibilityEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let xwindow = xev.window as xproto::Window;

        let event = Event::WindowEvent {
            window_id: mkwid(xwindow),
            event: WindowEvent::Occluded(xev.state == xlib::VisibilityFullyObscured),
        };
        callback(&self.target, event);

        self.with_window(xwindow, |window| {
            window.visibility_notify();
        });
    }

    fn expose<T: 'static, F>(&self, xev: &XExposeEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        // Multiple Expose events may be received for subareas of a window.
        // We issue `RedrawRequested` only for the last event of such a series.
        if xev.count == 0 {
            let window = xev.window as xproto::Window;
            let window_id = mkwid(window);

            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::RedrawRequested,
            };

            callback(&self.target, event);
        }
    }

    fn xinput_key_input<T: 'static, F>(
        &mut self,
        xev: &mut XKeyEvent,
        state: ElementState,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        // Set the timestamp.
        wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

        let window = match self.active_window {
            Some(window) => window,
            None => return,
        };

        let window_id = mkwid(window);
        let device_id = mkdid(util::VIRTUAL_CORE_KEYBOARD);

        let keycode = xev.keycode as _;

        // Update state to track key repeats and determine whether this key was a repeat.
        //
        // Note, when a key is held before focusing on this window the first
        // (non-synthetic) event will not be flagged as a repeat (also note that the
        // synthetic press event that is generated before this when the window gains focus
        // will also not be flagged as a repeat).
        //
        // Only keys that can repeat should change the held_key_press state since a
        // continuously held repeatable key may continue repeating after the press of a
        // non-repeatable key.
        let key_repeats = self
            .xkb_context
            .keymap_mut()
            .map(|k| k.key_repeats(keycode))
            .unwrap_or(false);
        let repeat = if key_repeats {
            let is_latest_held = self.held_key_press == Some(keycode);

            if state == ElementState::Pressed {
                self.held_key_press = Some(keycode);
                is_latest_held
            } else {
                // Check that the released key is the latest repeatable key that has been
                // pressed, since repeats will continue for the latest key press if a
                // different previously pressed key is released.
                if is_latest_held {
                    self.held_key_press = None;
                }
                false
            }
        } else {
            false
        };

        // NOTE: When the modifier was captured by the XFilterEvents the modifiers for the modifier
        // itself are out of sync due to XkbState being delivered before XKeyEvent, since it's
        // being replayed by the XIM, thus we should replay ourselves.
        let replay = if let Some(position) = self
            .xfiltered_modifiers
            .iter()
            .rev()
            .position(|&s| s == xev.serial)
        {
            // We don't have to replay modifiers pressed before the current event if some events
            // were not forwarded to us, since their state is irrelevant.
            self.xfiltered_modifiers
                .resize(self.xfiltered_modifiers.len() - 1 - position, 0);
            true
        } else {
            false
        };

        // Always update the modifiers when we're not replaying.
        if !replay {
            self.update_mods_from_core_event(window_id, xev.state as u16, &mut callback);
        }

        if keycode != 0 && !self.is_composing {
            // Don't alter the modifiers state from replaying.
            if replay {
                self.send_synthic_modifier_from_core(
                    window_id,
                    xev.state as u16,
                    &mut callback,
                );
            }

            if let Some(mut key_processor) = self.xkb_context.key_context() {
                let event = key_processor.process_key_event(keycode, state, repeat);
                let event = Event::WindowEvent {
                    window_id,
                    event: WindowEvent::KeyboardInput {
                        device_id,
                        event,
                        is_synthetic: false,
                    },
                };
                callback(&self.target, event);
            }

            // Restore the client's modifiers state after replay.
            if replay {
                self.send_modifiers(window_id, self.modifiers.get(), true, &mut callback);
            }

            return;
        }

        let wt = Self::window_target(&self.target);

        if let Some(ic) = wt
            .ime
            .as_ref()
            .and_then(|ime| ime.borrow().get_context(window as XWindow))
        {
            let written = wt.xconn.lookup_utf8(ic, xev);
            if !written.is_empty() {
                let event = Event::WindowEvent {
                    window_id,
                    event: WindowEvent::Ime(Ime::Preedit(String::new(), None)),
                };
                callback(&self.target, event);

                let event = Event::WindowEvent {
                    window_id,
                    event: WindowEvent::Ime(Ime::Commit(written)),
                };

                self.is_composing = false;
                callback(&self.target, event);
            }
        }
    }

    fn send_synthic_modifier_from_core<T: 'static, F>(
        &mut self,
        window_id: crate::window::WindowId,
        state: u16,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let keymap = match self.xkb_context.keymap_mut() {
            Some(keymap) => keymap,
            None => return,
        };

        let wt = Self::window_target(&self.target);
        let xcb = wt.xconn.xcb_connection().get_raw_xcb_connection();

        // Use synthetic state since we're replaying the modifier. The user modifier state
        // will be restored later.
        let mut xkb_state = match XkbState::new_x11(xcb, keymap) {
            Some(xkb_state) => xkb_state,
            None => return,
        };

        let mask = self.xkb_mod_mask_from_core(state);
        xkb_state.update_modifiers(mask, 0, 0, 0, 0, Self::core_keyboard_group(state));
        let mods: ModifiersState = xkb_state.modifiers().into();

        let event = Event::WindowEvent {
            window_id,
            event: WindowEvent::ModifiersChanged(mods.into()),
        };

        callback(&self.target, event);
    }

    fn xinput2_button_input<T: 'static, F>(
        &self,
        event: &XIDeviceEvent,
        state: ElementState,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);
        let window_id = mkwid(event.event as xproto::Window);
        let device_id = mkdid(event.deviceid as xinput::DeviceId);

        // Set the timestamp.
        wt.xconn.set_timestamp(event.time as xproto::Timestamp);

        // Deliver multi-touch events instead of emulated mouse events.
        if (event.flags & xinput2::XIPointerEmulated) != 0 {
            return;
        }

        let event = match event.detail as u32 {
            xlib::Button1 => WindowEvent::MouseInput {
                device_id,
                state,
                button: MouseButton::Left,
            },
            xlib::Button2 => WindowEvent::MouseInput {
                device_id,
                state,
                button: MouseButton::Middle,
            },

            xlib::Button3 => WindowEvent::MouseInput {
                device_id,
                state,
                button: MouseButton::Right,
            },

            // Suppress emulated scroll wheel clicks, since we handle the real motion events for
            // those. In practice, even clicky scroll wheels appear to be reported by
            // evdev (and XInput2 in turn) as axis motion, so we don't otherwise
            // special-case these button presses.
            4..=7 => WindowEvent::MouseWheel {
                device_id,
                delta: match event.detail {
                    4 => MouseScrollDelta::LineDelta(0.0, 1.0),
                    5 => MouseScrollDelta::LineDelta(0.0, -1.0),
                    6 => MouseScrollDelta::LineDelta(1.0, 0.0),
                    7 => MouseScrollDelta::LineDelta(-1.0, 0.0),
                    _ => unreachable!(),
                },
                phase: TouchPhase::Moved,
            },
            8 => WindowEvent::MouseInput {
                device_id,
                state,
                button: MouseButton::Back,
            },

            9 => WindowEvent::MouseInput {
                device_id,
                state,
                button: MouseButton::Forward,
            },
            x => WindowEvent::MouseInput {
                device_id,
                state,
                button: MouseButton::Other(x as u16),
            },
        };

        let event = Event::WindowEvent { window_id, event };
        callback(&self.target, event);
    }

    fn xinput2_mouse_motion<T: 'static, F>(&self, event: &XIDeviceEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        // Set the timestamp.
        wt.xconn.set_timestamp(event.time as xproto::Timestamp);

        let device_id = mkdid(event.deviceid as xinput::DeviceId);
        let window = event.event as xproto::Window;
        let window_id = mkwid(window);
        let new_cursor_pos = (event.event_x, event.event_y);

        let cursor_moved = self.with_window(window, |window| {
            let mut shared_state_lock = window.shared_state_lock();
            util::maybe_change(&mut shared_state_lock.cursor_pos, new_cursor_pos)
        });

        if cursor_moved == Some(true) {
            let position = PhysicalPosition::new(event.event_x, event.event_y);

            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::CursorMoved {
                    device_id,
                    position,
                },
            };
            callback(&self.target, event);
        } else if cursor_moved.is_none() {
            return;
        }

        // More gymnastics, for self.devices
        let mask = unsafe {
            slice::from_raw_parts(event.valuators.mask, event.valuators.mask_len as usize)
        };
        let mut devices = self.devices.borrow_mut();
        let physical_device =
            match devices.get_mut(&DeviceId(event.sourceid as xinput::DeviceId)) {
                Some(device) => device,
                None => return,
            };

        let mut events = Vec::new();
        let mut value = event.valuators.values;
        for i in 0..event.valuators.mask_len * 8 {
            if !xinput2::XIMaskIsSet(mask, i) {
                continue;
            }

            let x = unsafe { *value };

            let event = if let Some(&mut (_, ref mut info)) = physical_device
                .scroll_axes
                .iter_mut()
                .find(|&&mut (axis, _)| axis == i as _)
            {
                let delta = (x - info.position) / info.increment;
                info.position = x;
                // X11 vertical scroll coordinates are opposite to winit's
                let delta = match info.orientation {
                    ScrollOrientation::Horizontal => {
                        MouseScrollDelta::LineDelta(-delta as f32, 0.0)
                    }
                    ScrollOrientation::Vertical => {
                        MouseScrollDelta::LineDelta(0.0, -delta as f32)
                    }
                };

                WindowEvent::MouseWheel {
                    device_id,
                    delta,
                    phase: TouchPhase::Moved,
                }
            } else {
                WindowEvent::AxisMotion {
                    device_id,
                    axis: i as u32,
                    value: unsafe { *value },
                }
            };

            events.push(Event::WindowEvent { window_id, event });

            value = unsafe { value.offset(1) };
        }

        for event in events {
            callback(&self.target, event);
        }
    }

    fn xinput2_mouse_enter<T: 'static, F>(&self, event: &XIEnterEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        // Set the timestamp.
        wt.xconn.set_timestamp(event.time as xproto::Timestamp);

        let window = event.event as xproto::Window;
        let window_id = mkwid(window);
        let device_id = mkdid(event.deviceid as xinput::DeviceId);

        if let Some(all_info) = DeviceInfo::get(&wt.xconn, super::ALL_DEVICES.into()) {
            let mut devices = self.devices.borrow_mut();
            for device_info in all_info.iter() {
                // The second expression is need for resetting to work correctly on i3, and
                // presumably some other WMs. On those, `XI_Enter` doesn't include the physical
                // device ID, so both `sourceid` and `deviceid` are the virtual device.
                if device_info.deviceid == event.sourceid
                    || device_info.attachment == event.sourceid
                {
                    let device_id = DeviceId(device_info.deviceid as _);
                    if let Some(device) = devices.get_mut(&device_id) {
                        device.reset_scroll_position(device_info);
                    }
                }
            }
        }

        if self.window_exists(window) {
            let position = PhysicalPosition::new(event.event_x, event.event_y);

            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::CursorEntered { device_id },
            };
            callback(&self.target, event);

            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::CursorMoved {
                    device_id,
                    position,
                },
            };
            callback(&self.target, event);
        }
    }

    fn xinput2_mouse_left<T: 'static, F>(&self, event: &XILeaveEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);
        let window = event.event as xproto::Window;

        // Set the timestamp.
        wt.xconn.set_timestamp(event.time as xproto::Timestamp);

        // Leave, FocusIn, and FocusOut can be received by a window that's already
        // been destroyed, which the user presumably doesn't want to deal with.
        if self.window_exists(window) {
            let event = Event::WindowEvent {
                window_id: mkwid(window),
                event: WindowEvent::CursorLeft {
                    device_id: mkdid(event.deviceid as xinput::DeviceId),
                },
            };
            callback(&self.target, event);
        }
    }

    fn xinput2_focused<T: 'static, F>(&mut self, xev: &XIFocusInEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);
        let window = xev.event as xproto::Window;

        // Set the timestamp.
        wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

        if let Some(ime) = wt.ime.as_ref() {
            ime.borrow_mut()
                .focus(xev.event)
                .expect("Failed to focus input context");
        }

        if self.active_window == Some(window) {
            return;
        }

        self.active_window = Some(window);

        wt.update_listen_device_events(true);

        let window_id = mkwid(window);
        let position = PhysicalPosition::new(xev.event_x, xev.event_y);

        if let Some(window) = self.with_window(window, Arc::clone) {
            window.shared_state_lock().has_focus = true;
        }

        let event = Event::WindowEvent {
            window_id,
            event: WindowEvent::Focused(true),
        };
        callback(&self.target, event);

        // Issue key press events for all pressed keys
        Self::handle_pressed_keys(
            &self.target,
            window_id,
            ElementState::Pressed,
            &mut self.xkb_context,
            &mut callback,
        );

        self.update_mods_from_query(window_id, &mut callback);

        // The deviceid for this event is for a keyboard instead of a pointer,
        // so we have to do a little extra work.
        let pointer_id = self
            .devices
            .borrow()
            .get(&DeviceId(xev.deviceid as xinput::DeviceId))
            .map(|device| device.attachment)
            .unwrap_or(2);

        let event = Event::WindowEvent {
            window_id,
            event: WindowEvent::CursorMoved {
                device_id: mkdid(pointer_id as _),
                position,
            },
        };
        callback(&self.target, event);
    }

    fn xinput2_unfocused<T: 'static, F>(&mut self, xev: &XIFocusOutEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);
        let window = xev.event as xproto::Window;

        // Set the timestamp.
        wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

        if !self.window_exists(window) {
            return;
        }

        if let Some(ime) = wt.ime.as_ref() {
            ime.borrow_mut()
                .unfocus(xev.event)
                .expect("Failed to unfocus input context");
        }

        if self.active_window.take() == Some(window) {
            let window_id = mkwid(window);

            wt.update_listen_device_events(false);

            // Clear the modifiers when unfocusing the window.
            if let Some(xkb_state) = self.xkb_context.state_mut() {
                xkb_state.update_modifiers(0, 0, 0, 0, 0, 0);
                let mods = xkb_state.modifiers();
                self.send_modifiers(window_id, mods.into(), true, &mut callback);
            }

            // Issue key release events for all pressed keys
            Self::handle_pressed_keys(
                &self.target,
                window_id,
                ElementState::Released,
                &mut self.xkb_context,
                &mut callback,
            );

            // Clear this so detecting key repeats is consistently handled when the
            // window regains focus.
            self.held_key_press = None;

            if let Some(window) = self.with_window(window, Arc::clone) {
                window.shared_state_lock().has_focus = false;
            }

            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::Focused(false),
            };
            callback(&self.target, event)
        }
    }

    fn xinput2_touch<T: 'static, F>(
        &mut self,
        xev: &XIDeviceEvent,
        phase: TouchPhase,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        // Set the timestamp.
        wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

        let window = xev.event as xproto::Window;
        if self.window_exists(window) {
            let window_id = mkwid(window);
            let id = xev.detail as u64;
            let location = PhysicalPosition::new(xev.event_x, xev.event_y);

            // Mouse cursor position changes when touch events are received.
            // Only the first concurrently active touch ID moves the mouse cursor.
            if is_first_touch(&mut self.first_touch, &mut self.num_touch, id, phase) {
                let event = Event::WindowEvent {
                    window_id,
                    event: WindowEvent::CursorMoved {
                        device_id: mkdid(util::VIRTUAL_CORE_POINTER),
                        position: location.cast(),
                    },
                };
                callback(&self.target, event);
            }

            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::Touch(Touch {
                    device_id: mkdid(xev.deviceid as xinput::DeviceId),
                    phase,
                    location,
                    force: None, // TODO
                    id,
                }),
            };
            callback(&self.target, event)
        }
    }

    fn xinput2_raw_button_input<T: 'static, F>(
        &self,
        xev: &XIRawEvent,
        state: ElementState,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        // Set the timestamp.
        wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

        if xev.flags & xinput2::XIPointerEmulated == 0 {
            let event = Event::DeviceEvent {
                device_id: mkdid(xev.deviceid as xinput::DeviceId),
                event: DeviceEvent::Button {
                    state,
                    button: xev.detail as u32,
                },
            };
            callback(&self.target, event);
        }
    }

    fn xinput2_raw_mouse_motion<T: 'static, F>(&self, xev: &XIRawEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        // Set the timestamp.
        wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

        let did = mkdid(xev.deviceid as xinput::DeviceId);

        let mask = unsafe {
            slice::from_raw_parts(xev.valuators.mask, xev.valuators.mask_len as usize)
        };
        let mut value = xev.raw_values;
        let mut mouse_delta = util::Delta::default();
        let mut scroll_delta = util::Delta::default();
        for i in 0..xev.valuators.mask_len * 8 {
            if !xinput2::XIMaskIsSet(mask, i) {
                continue;
            }
            let x = unsafe { value.read_unaligned() };

            // We assume that every XInput2 device with analog axes is a pointing device emitting
            // relative coordinates.
            match i {
                0 => mouse_delta.set_x(x),
                1 => mouse_delta.set_y(x),
                2 => scroll_delta.set_x(x as f32),
                3 => scroll_delta.set_y(x as f32),
                _ => {}
            }

            let event = Event::DeviceEvent {
                device_id: did,
                event: DeviceEvent::Motion {
                    axis: i as u32,
                    value: x,
                },
            };
            callback(&self.target, event);

            value = unsafe { value.offset(1) };
        }

        if let Some(mouse_delta) = mouse_delta.consume() {
            let event = Event::DeviceEvent {
                device_id: did,
                event: DeviceEvent::MouseMotion { delta: mouse_delta },
            };
            callback(&self.target, event);
        }

        if let Some(scroll_delta) = scroll_delta.consume() {
            let event = Event::DeviceEvent {
                device_id: did,
                event: DeviceEvent::MouseWheel {
                    delta: MouseScrollDelta::LineDelta(scroll_delta.0, scroll_delta.1),
                },
            };
            callback(&self.target, event);
        }
    }

    fn xinput2_raw_key_input<T: 'static, F>(
        &mut self,
        xev: &XIRawEvent,
        state: ElementState,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        // Set the timestamp.
        wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

        let device_id = mkdid(xev.sourceid as xinput::DeviceId);
        let keycode = xev.detail as u32;
        if keycode < KEYCODE_OFFSET as u32 {
            return;
        }
        let physical_key = xkb::raw_keycode_to_physicalkey(keycode);

        callback(
            &self.target,
            Event::DeviceEvent {
                device_id,
                event: DeviceEvent::Key(RawKeyEvent {
                    physical_key,
                    state,
                }),
            },
        );
    }

    fn xinput2_hierarchy_changed<T: 'static, F>(
        &mut self,
        xev: &XIHierarchyEvent,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        // Set the timestamp.
        wt.xconn.set_timestamp(xev.time as xproto::Timestamp);
        let infos = unsafe { slice::from_raw_parts(xev.info, xev.num_info as usize) };
        for info in infos {
            if 0 != info.flags & (xinput2::XISlaveAdded | xinput2::XIMasterAdded) {
                self.init_device(info.deviceid as xinput::DeviceId);
                callback(
                    &self.target,
                    Event::DeviceEvent {
                        device_id: mkdid(info.deviceid as xinput::DeviceId),
                        event: DeviceEvent::Added,
                    },
                );
            } else if 0
                != info.flags & (xinput2::XISlaveRemoved | xinput2::XIMasterRemoved)
            {
                callback(
                    &self.target,
                    Event::DeviceEvent {
                        device_id: mkdid(info.deviceid as xinput::DeviceId),
                        event: DeviceEvent::Removed,
                    },
                );
                let mut devices = self.devices.borrow_mut();
                devices.remove(&DeviceId(info.deviceid as xinput::DeviceId));
            }
        }
    }

    fn xkb_event<T: 'static, F>(&mut self, xev: &XkbAnyEvent, mut callback: F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);
        match xev.xkb_type {
            xlib::XkbNewKeyboardNotify => {
                let xev = unsafe {
                    &*(xev as *const _ as *const xlib::XkbNewKeyboardNotifyEvent)
                };

                // Set the timestamp.
                wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

                let keycodes_changed_flag = 0x1;
                let geometry_changed_flag = 0x1 << 1;

                let keycodes_changed = util::has_flag(xev.changed, keycodes_changed_flag);
                let geometry_changed = util::has_flag(xev.changed, geometry_changed_flag);

                if xev.device == self.xkb_context.core_keyboard_id
                    && (keycodes_changed || geometry_changed)
                {
                    let xcb = wt.xconn.xcb_connection().get_raw_xcb_connection();
                    self.xkb_context.set_keymap_from_x11(xcb);
                    self.xmodmap.reload_from_x_connection(&wt.xconn);

                    let window_id = match self.active_window.map(super::mkwid) {
                        Some(window_id) => window_id,
                        None => return,
                    };

                    if let Some(state) = self.xkb_context.state_mut() {
                        let mods = state.modifiers().into();
                        self.send_modifiers(window_id, mods, true, &mut callback);
                    }
                }
            }
            xlib::XkbMapNotify => {
                let xcb = wt.xconn.xcb_connection().get_raw_xcb_connection();
                self.xkb_context.set_keymap_from_x11(xcb);
                self.xmodmap.reload_from_x_connection(&wt.xconn);
                let window_id = match self.active_window.map(super::mkwid) {
                    Some(window_id) => window_id,
                    None => return,
                };

                if let Some(state) = self.xkb_context.state_mut() {
                    let mods = state.modifiers().into();
                    self.send_modifiers(window_id, mods, true, &mut callback);
                }
            }
            xlib::XkbStateNotify => {
                let xev =
                    unsafe { &*(xev as *const _ as *const xlib::XkbStateNotifyEvent) };

                // Set the timestamp.
                wt.xconn.set_timestamp(xev.time as xproto::Timestamp);

                if let Some(state) = self.xkb_context.state_mut() {
                    state.update_modifiers(
                        xev.base_mods,
                        xev.latched_mods,
                        xev.locked_mods,
                        xev.base_group as u32,
                        xev.latched_group as u32,
                        xev.locked_group as u32,
                    );

                    let window_id = match self.active_window.map(super::mkwid) {
                        Some(window_id) => window_id,
                        None => return,
                    };

                    let mods = state.modifiers().into();
                    self.send_modifiers(window_id, mods, true, &mut callback);
                }
            }
            _ => {}
        }
    }

    pub fn update_mods_from_xinput2_event<T: 'static, F>(
        &mut self,
        mods: &XIModifierState,
        group: &XIModifierState,
        force: bool,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        if let Some(state) = self.xkb_context.state_mut() {
            state.update_modifiers(
                mods.base as u32,
                mods.latched as u32,
                mods.locked as u32,
                group.base as u32,
                group.latched as u32,
                group.locked as u32,
            );

            // NOTE: we use active window since generally sub windows don't have keyboard input,
            // and winit assumes that unfocused window doesn't have modifiers.
            let window_id = match self.active_window.map(super::mkwid) {
                Some(window_id) => window_id,
                None => return,
            };

            let mods = state.modifiers();
            self.send_modifiers(window_id, mods.into(), force, &mut callback);
        }
    }

    fn update_mods_from_query<T: 'static, F>(
        &mut self,
        window_id: crate::window::WindowId,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);

        let xkb_state = match self.xkb_context.state_mut() {
            Some(xkb_state) => xkb_state,
            None => return,
        };

        unsafe {
            let mut state: XkbStateRec = std::mem::zeroed();
            if (wt.xconn.xlib.XkbGetState)(
                wt.xconn.display,
                XkbId::USE_CORE_KBD.into(),
                &mut state,
            ) == xlib::True
            {
                xkb_state.update_modifiers(
                    state.base_mods as u32,
                    state.latched_mods as u32,
                    state.locked_mods as u32,
                    state.base_group as u32,
                    state.latched_group as u32,
                    state.locked_group as u32,
                );
            }
        }

        let mods = xkb_state.modifiers();
        self.send_modifiers(window_id, mods.into(), true, &mut callback)
    }

    pub fn update_mods_from_core_event<T: 'static, F>(
        &mut self,
        window_id: crate::window::WindowId,
        state: u16,
        mut callback: F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let xkb_mask = self.xkb_mod_mask_from_core(state);
        let xkb_state = match self.xkb_context.state_mut() {
            Some(xkb_state) => xkb_state,
            None => return,
        };

        // NOTE: this is inspired by Qt impl.
        let mut depressed = xkb_state.depressed_modifiers() & xkb_mask;
        let latched = xkb_state.latched_modifiers() & xkb_mask;
        let locked = xkb_state.locked_modifiers() & xkb_mask;
        // Set modifiers in depressed if they don't appear in any of the final masks.
        depressed |= !(depressed | latched | locked) & xkb_mask;

        xkb_state.update_modifiers(
            depressed,
            latched,
            locked,
            0,
            0,
            Self::core_keyboard_group(state),
        );

        let mods = xkb_state.modifiers();
        self.send_modifiers(window_id, mods.into(), false, &mut callback);
    }

    // Bits 13 and 14 report the state keyboard group.
    pub fn core_keyboard_group(state: u16) -> u32 {
        ((state >> 13) & 3) as u32
    }

    pub fn xkb_mod_mask_from_core(&mut self, state: u16) -> xkb_mod_mask_t {
        let mods_indices = match self.xkb_context.keymap_mut() {
            Some(keymap) => keymap.mods_indices(),
            None => return 0,
        };

        // Build the XKB modifiers from the regular state.
        let mut depressed = 0u32;
        if let Some(shift) = mods_indices
            .shift
            .filter(|_| ModMask::SHIFT.intersects(state))
        {
            depressed |= 1 << shift;
        }
        if let Some(caps) = mods_indices
            .caps
            .filter(|_| ModMask::LOCK.intersects(state))
        {
            depressed |= 1 << caps;
        }
        if let Some(ctrl) = mods_indices
            .ctrl
            .filter(|_| ModMask::CONTROL.intersects(state))
        {
            depressed |= 1 << ctrl;
        }
        if let Some(alt) = mods_indices.alt.filter(|_| ModMask::M1.intersects(state)) {
            depressed |= 1 << alt;
        }
        if let Some(num) = mods_indices.num.filter(|_| ModMask::M2.intersects(state)) {
            depressed |= 1 << num;
        }
        if let Some(mod3) = mods_indices.mod3.filter(|_| ModMask::M3.intersects(state)) {
            depressed |= 1 << mod3;
        }
        if let Some(logo) = mods_indices.logo.filter(|_| ModMask::M4.intersects(state)) {
            depressed |= 1 << logo;
        }
        if let Some(mod5) = mods_indices.mod5.filter(|_| ModMask::M5.intersects(state)) {
            depressed |= 1 << mod5;
        }

        depressed
    }

    /// Send modifiers for the active window.
    ///
    /// The event won't be sent when the `modifiers` match the previously `sent` modifiers value,
    /// unless `force` is passed. The `force` should be passed when the active window changes.
    fn send_modifiers<T: 'static, F: FnMut(&RootAEL, Event<T>)>(
        &self,
        window_id: crate::window::WindowId,
        modifiers: ModifiersState,
        force: bool,
        callback: &mut F,
    ) {
        // NOTE: Always update the modifiers to account for case when they've changed
        // and forced was `true`.
        if self.modifiers.replace(modifiers) != modifiers || force {
            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::ModifiersChanged(self.modifiers.get().into()),
            };
            callback(&self.target, event);
        }
    }

    fn handle_pressed_keys<T: 'static, F>(
        target: &RootAEL,
        window_id: crate::window::WindowId,
        state: ElementState,
        xkb_context: &mut Context,
        callback: &mut F,
    ) where
        F: FnMut(&RootAEL, Event<T>),
    {
        let device_id = mkdid(util::VIRTUAL_CORE_KEYBOARD);

        // Update modifiers state and emit key events based on which keys are currently pressed.
        let window_target = Self::window_target(target);
        let xcb = window_target
            .xconn
            .xcb_connection()
            .get_raw_xcb_connection();

        let keymap = match xkb_context.keymap_mut() {
            Some(keymap) => keymap,
            None => return,
        };

        // Send the keys using the synthetic state to not alter the main state.
        let mut xkb_state = match XkbState::new_x11(xcb, keymap) {
            Some(xkb_state) => xkb_state,
            None => return,
        };
        let mut key_processor = match xkb_context.key_context_with_state(&mut xkb_state) {
            Some(key_processor) => key_processor,
            None => return,
        };

        for keycode in window_target
            .xconn
            .query_keymap()
            .into_iter()
            .filter(|k| *k >= KEYCODE_OFFSET)
        {
            let event = key_processor.process_key_event(keycode as u32, state, false);
            let event = Event::WindowEvent {
                window_id,
                event: WindowEvent::KeyboardInput {
                    device_id,
                    event,
                    is_synthetic: true,
                },
            };
            callback(target, event);
        }
    }

    fn process_dpi_change<T: 'static, F>(&self, callback: &mut F)
    where
        F: FnMut(&RootAEL, Event<T>),
    {
        let wt = Self::window_target(&self.target);
        wt.xconn
            .reload_database()
            .expect("failed to reload Xft database");

        // In the future, it would be quite easy to emit monitor hotplug events.
        let prev_list = {
            let prev_list = wt.xconn.invalidate_cached_monitor_list();
            match prev_list {
                Some(prev_list) => prev_list,
                None => return,
            }
        };

        let new_list = wt
            .xconn
            .available_monitors()
            .expect("Failed to get monitor list");
        for new_monitor in new_list {
            // Previous list may be empty, in case of disconnecting and
            // reconnecting the only one monitor. We still need to emit events in
            // this case.
            let maybe_prev_scale_factor = prev_list
                .iter()
                .find(|prev_monitor| prev_monitor.name == new_monitor.name)
                .map(|prev_monitor| prev_monitor.scale_factor);
            if Some(new_monitor.scale_factor) != maybe_prev_scale_factor {
                for window in wt.windows.borrow().iter().filter_map(|(_, w)| w.upgrade())
                {
                    window.refresh_dpi_for_monitor(
                        &new_monitor,
                        maybe_prev_scale_factor,
                        |event| {
                            callback(&self.target, event);
                        },
                    )
                }
            }
        }
    }

    fn window_exists(&self, window_id: xproto::Window) -> bool {
        self.with_window(window_id, |_| ()).is_some()
    }
}

fn is_first_touch(
    first: &mut Option<u64>,
    num: &mut u32,
    id: u64,
    phase: TouchPhase,
) -> bool {
    match phase {
        TouchPhase::Started => {
            if *num == 0 {
                *first = Some(id);
            }
            *num += 1;
        }
        TouchPhase::Cancelled | TouchPhase::Ended => {
            if *first == Some(id) {
                *first = None;
            }
            *num = num.saturating_sub(1);
        }
        _ => (),
    }

    *first == Some(id)
}
