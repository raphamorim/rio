use std::ffi::{CStr, CString, IntoStringError};
use std::os::raw::{c_char, c_ulong, c_ushort};
use std::sync::{Arc, Mutex};
use std::{env, fmt, ptr};

use super::super::atoms::*;
use super::{ffi, util, XConnection, XError};
use x11rb::protocol::xproto;

static GLOBAL_LOCK: Mutex<()> = Mutex::new(());

unsafe fn open_im(xconn: &Arc<XConnection>, locale_modifiers: &CStr) -> Option<ffi::XIM> {
    let _lock = GLOBAL_LOCK.lock();

    // XSetLocaleModifiers returns...
    // * The current locale modifiers if it's given a NULL pointer.
    // * The new locale modifiers if we succeeded in setting them.
    // * NULL if the locale modifiers string is malformed or if the current locale is not supported
    //   by Xlib.
    unsafe { (xconn.xlib.XSetLocaleModifiers)(locale_modifiers.as_ptr()) };

    let im = unsafe {
        (xconn.xlib.XOpenIM)(
            xconn.display,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };

    if im.is_null() {
        None
    } else {
        Some(im)
    }
}

#[derive(Debug)]
pub struct InputMethod {
    pub im: ffi::XIM,
    pub preedit_style: Style,
    pub none_style: Style,
    _name: String,
}

impl InputMethod {
    fn new(xconn: &Arc<XConnection>, im: ffi::XIM, name: String) -> Option<Self> {
        let mut styles: *mut XIMStyles = std::ptr::null_mut();

        // Query the styles supported by the XIM.
        unsafe {
            if !(xconn.xlib.XGetIMValues)(
                im,
                ffi::XNQueryInputStyle_0.as_ptr() as *const _,
                (&mut styles) as *mut _,
                std::ptr::null_mut::<()>(),
            )
            .is_null()
            {
                return None;
            }
        }

        let mut preedit_style = None;
        let mut none_style = None;

        unsafe {
            std::slice::from_raw_parts(
                (*styles).supported_styles,
                (*styles).count_styles as _,
            )
            .iter()
            .for_each(|style| match *style {
                XIM_PREEDIT_STYLE => {
                    preedit_style = Some(Style::Preedit(*style));
                }
                XIM_NOTHING_STYLE if preedit_style.is_none() => {
                    preedit_style = Some(Style::Nothing(*style))
                }
                XIM_NONE_STYLE => none_style = Some(Style::None(*style)),
                _ => (),
            });

            (xconn.xlib.XFree)(styles.cast());
        };

        if preedit_style.is_none() && none_style.is_none() {
            return None;
        }

        let preedit_style = preedit_style.unwrap_or_else(|| none_style.unwrap());
        let none_style = none_style.unwrap_or(preedit_style);

        Some(InputMethod {
            im,
            _name: name,
            preedit_style,
            none_style,
        })
    }
}

const XIM_PREEDIT_STYLE: XIMStyle =
    (ffi::XIMPreeditCallbacks | ffi::XIMStatusNothing) as XIMStyle;
const XIM_NOTHING_STYLE: XIMStyle =
    (ffi::XIMPreeditNothing | ffi::XIMStatusNothing) as XIMStyle;
const XIM_NONE_STYLE: XIMStyle = (ffi::XIMPreeditNone | ffi::XIMStatusNone) as XIMStyle;

/// Style of the IME context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// Preedit callbacks.
    Preedit(XIMStyle),

    /// Nothing.
    Nothing(XIMStyle),

    /// No IME.
    None(XIMStyle),
}

impl Default for Style {
    fn default() -> Self {
        Style::None(XIM_NONE_STYLE)
    }
}

#[repr(C)]
#[derive(Debug)]
struct XIMStyles {
    count_styles: c_ushort,
    supported_styles: *const XIMStyle,
}

pub(crate) type XIMStyle = c_ulong;

#[derive(Debug)]
pub enum InputMethodResult {
    /// Input method used locale modifier from `XMODIFIERS` environment variable.
    XModifiers(InputMethod),
    /// Input method used internal fallback locale modifier.
    Fallback(InputMethod),
    /// Input method could not be opened using any locale modifier tried.
    Failure,
}

impl InputMethodResult {
    pub fn is_fallback(&self) -> bool {
        matches!(self, InputMethodResult::Fallback(_))
    }

    pub fn ok(self) -> Option<InputMethod> {
        use self::InputMethodResult::*;
        match self {
            XModifiers(im) | Fallback(im) => Some(im),
            Failure => None,
        }
    }
}

#[derive(Debug, Clone)]
enum GetXimServersError {
    XError(#[allow(dead_code)] XError),
    GetPropertyError(#[allow(dead_code)] util::GetPropertyError),
    InvalidUtf8(#[allow(dead_code)] IntoStringError),
}

impl From<util::GetPropertyError> for GetXimServersError {
    fn from(error: util::GetPropertyError) -> Self {
        GetXimServersError::GetPropertyError(error)
    }
}

// The root window has a property named XIM_SERVERS, which contains a list of atoms representing
// the available XIM servers. For instance, if you're using ibus, it would contain an atom named
// "@server=ibus". It's possible for this property to contain multiple atoms, though presumably
// rare. Note that we replace "@server=" with "@im=" in order to match the format of locale
// modifiers, since we don't want a user who's looking at logs to ask "am I supposed to set
// XMODIFIERS to `@server=ibus`?!?"
unsafe fn get_xim_servers(
    xconn: &Arc<XConnection>,
) -> Result<Vec<String>, GetXimServersError> {
    let atoms = xconn.atoms();
    let servers_atom = atoms[XIM_SERVERS];

    let root = unsafe { (xconn.xlib.XDefaultRootWindow)(xconn.display) };

    let mut atoms: Vec<ffi::Atom> = xconn
        .get_property::<xproto::Atom>(
            root as xproto::Window,
            servers_atom,
            xproto::Atom::from(xproto::AtomEnum::ATOM),
        )
        .map_err(GetXimServersError::GetPropertyError)?
        .into_iter()
        .map(ffi::Atom::from)
        .collect::<Vec<_>>();

    let mut names: Vec<*const c_char> = Vec::with_capacity(atoms.len());
    unsafe {
        (xconn.xlib.XGetAtomNames)(
            xconn.display,
            atoms.as_mut_ptr(),
            atoms.len() as _,
            names.as_mut_ptr() as _,
        )
    };
    unsafe { names.set_len(atoms.len()) };

    let mut formatted_names = Vec::with_capacity(names.len());
    for name in names {
        let string = unsafe { CStr::from_ptr(name) }
            .to_owned()
            .into_string()
            .map_err(GetXimServersError::InvalidUtf8)?;
        unsafe { (xconn.xlib.XFree)(name as _) };
        formatted_names.push(string.replace("@server=", "@im="));
    }
    xconn.check_errors().map_err(GetXimServersError::XError)?;
    Ok(formatted_names)
}

#[derive(Clone)]
struct InputMethodName {
    c_string: CString,
    string: String,
}

impl InputMethodName {
    pub fn from_string(string: String) -> Self {
        let c_string = CString::new(string.clone())
            .expect("String used to construct CString contained null byte");
        InputMethodName { c_string, string }
    }

    pub fn from_str(string: &str) -> Self {
        let c_string = CString::new(string)
            .expect("String used to construct CString contained null byte");
        InputMethodName {
            c_string,
            string: string.to_owned(),
        }
    }
}

impl fmt::Debug for InputMethodName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.string.fmt(f)
    }
}

#[derive(Debug, Clone)]
struct PotentialInputMethod {
    name: InputMethodName,
    successful: Option<bool>,
}

impl PotentialInputMethod {
    pub fn from_string(string: String) -> Self {
        PotentialInputMethod {
            name: InputMethodName::from_string(string),
            successful: None,
        }
    }

    pub fn from_str(string: &str) -> Self {
        PotentialInputMethod {
            name: InputMethodName::from_str(string),
            successful: None,
        }
    }

    pub fn reset(&mut self) {
        self.successful = None;
    }

    pub fn open_im(&mut self, xconn: &Arc<XConnection>) -> Option<InputMethod> {
        let im = unsafe { open_im(xconn, &self.name.c_string) };
        self.successful = Some(im.is_some());
        im.and_then(|im| InputMethod::new(xconn, im, self.name.string.clone()))
    }
}

// By logging this struct, you get a sequential listing of every locale modifier tried, where it
// came from, and if it succeeded.
#[derive(Debug, Clone)]
pub(crate) struct PotentialInputMethods {
    // On correctly configured systems, the XMODIFIERS environment variable tells us everything we
    // need to know.
    xmodifiers: Option<PotentialInputMethod>,
    // We have some standard options at our disposal that should ostensibly always work. For users
    // who only need compose sequences, this ensures that the program launches without a hitch
    // For users who need more sophisticated IME features, this is more or less a silent failure.
    // Logging features should be added in the future to allow both audiences to be effectively
    // served.
    fallbacks: [PotentialInputMethod; 2],
    // For diagnostic purposes, we include the list of XIM servers that the server reports as
    // being available.
    _xim_servers: Result<Vec<String>, GetXimServersError>,
}

impl PotentialInputMethods {
    pub fn new(xconn: &Arc<XConnection>) -> Self {
        let xmodifiers = env::var("XMODIFIERS")
            .ok()
            .map(PotentialInputMethod::from_string);
        PotentialInputMethods {
            // Since passing "" to XSetLocaleModifiers results in it defaulting to the value of
            // XMODIFIERS, it's worth noting what happens if XMODIFIERS is also "". If simply
            // running the program with `XMODIFIERS="" cargo run`, then assuming XMODIFIERS is
            // defined in the profile (or parent environment) then that parent XMODIFIERS is used.
            // If that XMODIFIERS value is also "" (i.e. if you ran `export XMODIFIERS=""`), then
            // XSetLocaleModifiers uses the default local input method. Note that defining
            // XMODIFIERS as "" is different from XMODIFIERS not being defined at all, since in
            // that case, we get `None` and end up skipping ahead to the next method.
            xmodifiers,
            fallbacks: [
                // This is a standard input method that supports compose sequences, which should
                // always be available. `@im=none` appears to mean the same thing.
                PotentialInputMethod::from_str("@im=local"),
                // This explicitly specifies to use the implementation-dependent default, though
                // that seems to be equivalent to just using the local input method.
                PotentialInputMethod::from_str("@im="),
            ],
            // The XIM_SERVERS property can have surprising values. For instance, when I exited
            // ibus to run fcitx, it retained the value denoting ibus. Even more surprising is
            // that the fcitx input method could only be successfully opened using "@im=ibus".
            // Presumably due to this quirk, it's actually possible to alternate between ibus and
            // fcitx in a running application.
            _xim_servers: unsafe { get_xim_servers(xconn) },
        }
    }

    // This resets the `successful` field of every potential input method, ensuring we have
    // accurate information when this struct is re-used by the destruction/instantiation callbacks.
    fn reset(&mut self) {
        if let Some(ref mut input_method) = self.xmodifiers {
            input_method.reset();
        }

        for input_method in &mut self.fallbacks {
            input_method.reset();
        }
    }

    pub fn open_im(
        &mut self,
        xconn: &Arc<XConnection>,
        callback: Option<&dyn Fn()>,
    ) -> InputMethodResult {
        use self::InputMethodResult::*;

        self.reset();

        if let Some(ref mut input_method) = self.xmodifiers {
            let im = input_method.open_im(xconn);
            if let Some(im) = im {
                return XModifiers(im);
            } else if let Some(ref callback) = callback {
                callback();
            }
        }

        for input_method in &mut self.fallbacks {
            let im = input_method.open_im(xconn);
            if let Some(im) = im {
                return Fallback(im);
            }
        }

        Failure
    }
}
