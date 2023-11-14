//! Win32 implementation of menubars.

use crate::ui::Error;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::CString;
use std::fmt;
use std::marker::PhantomData;
use std::mem;
use std::num::NonZeroIsize;
use std::ptr;
use std::rc::Rc;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};

use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};

use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuA, AppendMenuW, CreateMenu, CreatePopupMenu, DestroyMenu, GetMenu,
    InsertMenuItemA, SetMenu, SetMenuInfo, MIIM_ID,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{HMENU, MENUINFO, MENUITEMINFOA};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MFT_SEPARATOR, MFT_STRING, MF_POPUP, MF_SEPARATOR, MF_STRING, MIIM_DATA, MIIM_FTYPE,
    MIIM_STRING, MIIM_SUBMENU, MIIM_TYPE, MIM_STYLE, MNS_NOTIFYBYPOS, WM_COMMAND,
    WM_MENUCOMMAND, WM_NCDESTROY,
};

macro_rules! syscall {
    // The null value (0) is an error.
    (nul $fname: ident $($args: tt)*) => {{
        let res = unsafe { $fname $($args)* };
        if res == 0 {
            return Err(crate::ui::Error::last_io_error());
        } else {
            res
        }
    }}
}

// No one else should use this very unique ID.
const SUBCLASS_ID: usize = 4 * 8 * 15 * 16 * 23 * 42;

/// A handle that manages a menu key.
struct MenuKeyHandle {
    key: MenuKey,
    unsend: PhantomData<*mut ()>,
}

impl MenuKeyHandle {
    /// Create a new menu key handle.
    fn new() -> Self {
        std::thread_local! {
            static SLOT_LIST: RefCell<SlotList> = RefCell::new(SlotList {
                menu_keys: Vec::new(),
                next_key: 0,
                len: 0,
            });
        }

        struct SlotList {
            /// A list of menu keys indicating whether they have been freed or not.
            menu_keys: Vec<Slot>,

            /// The next menu key to use.
            next_key: u16,

            /// The current number of occupied slots in the menu key list.
            len: u16,
        }

        enum Slot {
            /// This slot is occupied.
            Occupied,

            /// This slot is free.
            ///
            /// The value inside is the value that should be set to `NEXT_KEY`
            /// when this slot is occupied.
            Vacant(u16),
        }

        impl Drop for MenuKeyHandle {
            fn drop(&mut self) {
                // Free a slot in the key list.
                let _ = SLOT_LIST.try_with(|slot_list| {
                    let mut slot_list = slot_list.borrow_mut();

                    // Decrement length by one.
                    let new_len = slot_list
                        .len
                        .checked_sub(1)
                        .expect("menu key list is corrupt");
                    slot_list.len = new_len;

                    // Mark the slot at vacant.
                    let our_key = self.key.0;
                    slot_list.menu_keys[our_key as usize] =
                        Slot::Vacant(slot_list.next_key);
                    slot_list.next_key = our_key;
                });
            }
        }

        SLOT_LIST.with(|slot_list| {
            let mut slot_list = slot_list.borrow_mut();
            let our_key = slot_list.next_key;

            // Increment length by one.
            {
                let new_len = slot_list.len.checked_add(1).expect("too many menu keys");
                slot_list.len = new_len;
            }

            if slot_list.next_key == slot_list.menu_keys.len() as _ {
                // Allocate a new slot at the end of the list.
                slot_list.menu_keys.push(Slot::Occupied);
                slot_list.next_key += 1;
            } else {
                // Take the vacant slot.
                slot_list.next_key = match slot_list.menu_keys.get(our_key as usize) {
                    Some(Slot::Vacant(next_key)) => *next_key,
                    _ => panic!("menu key list is corrupt"),
                };
                slot_list.menu_keys[our_key as usize] = Slot::Occupied;
            }

            MenuKeyHandle {
                key: MenuKey(our_key),
                unsend: PhantomData,
            }
        })
    }

    /// Get the underlying menu key.
    fn key(&self) -> MenuKey {
        self.key
    }
}

/// Key given to a menu.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct MenuKey(u16);

/// Key given to an item in a menu.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ItemKey(u32);

impl ItemKey {
    /// Create a new `ItemKey` from a menu handle and an item ID.
    fn new(menu: MenuKey, id: u16) -> Self {
        ItemKey(((menu.0 as u32) << 16) | (id as u32))
    }

    /// Get the menu key.
    fn menu(&self) -> MenuKey {
        MenuKey((self.0 >> 16) as u16)
    }

    /// Get the item ID.
    fn id(&self) -> u16 {
        self.0 as u16
    }
}

unsafe extern "system" fn menu_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    uidsubclass: usize,
    refdata: usize,
) -> LRESULT {
    abort_on_panic(move || {
        // Early out.
        macro_rules! early_out {
            () => {{
                return DefSubclassProc(hwnd, msg, wparam, lparam);
            }};
        }

        macro_rules! leap {
            ($e: expr) => {{
                match $e {
                    Some(e) => e,
                    None => early_out!(),
                }
            }};
        }

        // If we are being destroyed, free our refdata.
        if msg == WM_NCDESTROY {
            drop(Box::from_raw(refdata as *mut WindowData));
            early_out!();
        } else if msg == WM_COMMAND {
            // Get a reference to the hash map containing our menu item data.
            let map_cell = &*(refdata as *const WindowData);

            // Shouldn't be called reentrantly.
            let mut map = map_cell.data.borrow_mut();

            // Get the item key.
            let key = ItemKey(wparam as u32);

            // Get the item data.
            let data = leap!(map.get_mut(&key));

            // Call the handler.
            (data.handler)();
        }

        early_out!();
    })
}

#[doc(hidden)]
pub enum Empty {}

pub struct Hotkey<'a> {
    // TODO
    key: &'a str,
}

type DataTable = HashMap<ItemKey, MenuItemData, ahash::RandomState>;

struct WindowData {
    /// The table of menu item data.
    data: RefCell<DataTable>,

    /// The menu IDs we are currently holding.
    _ids: Vec<MenuKeyHandle>,
}

/// A menu to be attached to a window.
pub struct Menu {
    /// Handle to the menu.
    ///
    /// This is semantically an `HMENU`, but we use `NonZeroIsize` to avoid
    /// allocating an extra pointer here.
    ///
    /// This is also used to uniquely identify the menu.
    menu: Option<NonZeroIsize>,

    /// Data associated with the menu.
    data: DataTable,

    /// The menu IDs we are currently holding.
    menu_id: Vec<MenuKeyHandle>,

    /// The next item ID to use.
    next_id: u16,

    /// Menus are not thread-safe.
    _marker: PhantomData<*mut ()>,
}

impl Menu {
    /// Create a new menu.
    pub fn new() -> Result<Self, Error> {
        // Create the menu.
        let menu = syscall!(nul CreateMenu());

        unsafe { Ok(Menu::from_hmenu(menu)) }
    }

    /// Create a new, empty popup menu.
    pub fn new_popup() -> Result<Self, Error> {
        // Create the menu.
        let menu = syscall!(nul CreatePopupMenu());

        unsafe { Ok(Menu::from_hmenu(menu)) }
    }

    unsafe fn from_hmenu(menu: HMENU) -> Self {
        Menu {
            menu: Some(NonZeroIsize::new_unchecked(menu)),
            data: DataTable::with_hasher(ahash::RandomState::new()),
            menu_id: vec![MenuKeyHandle::new()],
            next_id: 0,
            _marker: PhantomData,
        }
    }

    /// Add a new menu item to the menu.
    pub fn push<'t, 'h, H: MenuItemHandler>(
        &mut self,
        item: impl Into<MenuItem<'t, 'h, H>>,
    ) -> Result<(), Error> {
        // Create the menu item.
        let item = item.into();
        let hmenu = self.menu.unwrap().get();

        match item.inner {
            Inner::Separator => {
                syscall!(nul AppendMenuA(hmenu, MF_SEPARATOR, 0, ptr::null_mut()));
            }

            Inner::Submenu { text, mut submenu } => {
                // Menu item is a submenu.
                let handle = submenu.menu.take().unwrap().get();
                let items = mem::replace(
                    &mut submenu.data,
                    DataTable::with_hasher(ahash::RandomState::new()),
                );

                // Append items to our items.
                self.data.extend(items.into_iter());

                let text = CString::new(text).unwrap();
                syscall!(nul AppendMenuA(hmenu, MF_POPUP, handle as _, text.as_ptr().cast()));
            }

            Inner::Item {
                text,
                hotkey,
                mut handler,
            } => {
                // Create a new item key.
                let key = {
                    let new_key = ItemKey::new(self.menu_id[0].key, self.next_id);

                    let next_id =
                        self.next_id.checked_add(1).expect("menu item ID overflow");
                    self.next_id = next_id;

                    new_key
                };

                // Add this key to our map.
                self.data.insert(
                    key,
                    MenuItemData {
                        handler: Box::new(move || handler.invoke()),
                    },
                );

                let text = CString::new(text).unwrap();
                syscall!(nul AppendMenuA(hmenu, MF_STRING, key.0 as _, text.as_ptr().cast()));
            }
        };

        Ok(())
    }

    /// Apply this menu to a raw window handle.
    pub fn apply(
        self,
        handle: impl raw_window_handle::HasRawWindowHandle,
    ) -> Result<(), Error> {
        match handle.raw_window_handle() {
            raw_window_handle::RawWindowHandle::Win32(handle) => unsafe {
                if handle.hwnd.is_null() {
                    return Err(Error::unexpected_window_type());
                }

                self.apply_to_hwnd(handle.hwnd as _)
            },
            _ => Err(Error::unexpected_window_type()),
        }
    }

    /// Apply this menu to a window.
    unsafe fn apply_to_hwnd(mut self, hwnd: HWND) -> Result<(), Error> {
        // If the window already has a menu, error out. We don't want to step on any toes.
        let old_menu = GetMenu(hwnd);
        if old_menu != 0 {
            return Err(Error::menu_exists());
        }

        // Set the menu.
        SetMenu(hwnd, self.menu.take().unwrap().get());

        // Add a subclass to the window.
        let data = mem::replace(
            &mut self.data,
            HashMap::with_hasher(ahash::RandomState::new()),
        );
        let data = Box::into_raw(Box::new(data));
        SetWindowSubclass(hwnd, Some(menu_subclass_proc), SUBCLASS_ID, data as _);

        Ok(())
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        if let Some(menu) = self.menu {
            unsafe {
                DestroyMenu(menu.get());
            }
        }
    }
}

/// Data associated with each menu item.
struct MenuItemData {
    /// The handler for the menu item.
    handler: Box<dyn FnMut()>,
}

/// A menu item.
pub struct MenuItem<'txt, 'hotkey, Handler = Empty> {
    inner: Inner<'txt, 'hotkey, Handler>,
}

enum Inner<'txt, 'hotkey, Handler> {
    /// This is a regular menu item.
    Item {
        /// The text of the menu item.
        text: &'txt str,

        /// Handler for the menu item.
        hotkey: Option<Hotkey<'hotkey>>,

        /// Handler for the menu item.
        handler: Handler,
    },

    /// This is a separator.
    Separator,

    /// This is a submenu.
    Submenu {
        /// The text of the menu item.
        text: &'txt str,

        /// The handle to the submenu.
        submenu: Menu,
    },
}

impl MenuItem<'static, 'static> {
    /// Create a new separator.
    pub fn separator() -> Self {
        MenuItem {
            inner: Inner::Separator,
        }
    }
}

impl<'txt> MenuItem<'txt, 'static> {
    /// Create a drop-down menu item.
    pub fn submenu(text: &'txt str, submenu: Menu) -> Self {
        MenuItem {
            inner: Inner::Submenu { text, submenu },
        }
    }
}

impl<'txt, 'hotkey, Handler: MenuItemHandler> MenuItem<'txt, 'hotkey, Handler> {
    /// Create a new menu item.
    pub fn new(
        text: &'txt str,
        hotkey: Option<Hotkey<'hotkey>>,
        handler: Handler,
    ) -> Self {
        MenuItem {
            inner: Inner::Item {
                text,
                hotkey,
                handler,
            },
        }
    }
}

/// Callback for invoking a menu item's functionality.
///
/// This is implemented for all `F` where `F: FnMut()`.
pub trait MenuItemHandler: 'static {
    fn invoke(&mut self);
}

impl MenuItemHandler for Empty {
    fn invoke(&mut self) {
        match *self {}
    }
}

impl<F> MenuItemHandler for F
where
    F: FnMut() + 'static,
{
    fn invoke(&mut self) {
        self();
    }
}

fn abort_on_panic<R>(f: impl FnOnce() -> R) -> R {
    struct Bomb;

    impl Drop for Bomb {
        fn drop(&mut self) {
            std::process::abort();
        }
    }

    let bomb = Bomb;
    let r = f();
    mem::forget(bomb);
    r
}
