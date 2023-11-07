// WA is a fork of https://github.com/rust-windowing/winit/
// Winit is is licensed under Apache 2.0 license https://github.com/rust-windowing/winit/blob/master/LICENSE

use log::trace;
use objc2::Encode;
use objc2::Encoding;
use std::ffi::CStr;
use std::ptr::NonNull;

use icrate::Foundation::NSObject;
use objc2::declare::{IvarBool, IvarEncode};
use objc2::rc::Id;
use objc2::runtime::AnyObject;
use objc2::{declare_class, msg_send, msg_send_id, mutability, ClassType};

use super::app_state::AppState;
use super::appkit::NSApplicationActivationPolicy;

#[repr(transparent)]
struct Url {
    // `NSString` has the same layout / works the same as `AnyObject`.
    items: Vec<*const i8>,
}

unsafe impl Encode for Url {
    const ENCODING: Encoding = Encoding::Object;
}

declare_class!(
    #[derive(Debug)]
    pub(super) struct ApplicationDelegate {
        activation_policy:
            IvarEncode<NSApplicationActivationPolicy, "_activation_policy">,
        default_menu: IvarBool<"_default_menu">,
        activate_ignoring_other_apps: IvarBool<"_activate_ignoring_other_apps">,
    }

    mod ivars;

    unsafe impl ClassType for ApplicationDelegate {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "WinitApplicationDelegate";
    }

    unsafe impl ApplicationDelegate {
        #[method(initWithActivationPolicy:defaultMenu:activateIgnoringOtherApps:)]
        unsafe fn init(
            this: *mut Self,
            activation_policy: NSApplicationActivationPolicy,
            default_menu: bool,
            activate_ignoring_other_apps: bool,
        ) -> Option<NonNull<Self>> {
            let this: Option<&mut Self> = unsafe { msg_send![super(this), init] };
            this.map(|this| {
                *this.activation_policy = activation_policy;
                *this.default_menu = default_menu;
                *this.activate_ignoring_other_apps = activate_ignoring_other_apps;
                NonNull::from(this)
            })
        }

        #[method(applicationDidFinishLaunching:)]
        fn did_finish_launching(&self, _sender: Option<&AnyObject>) {
            trace_scope!("applicationDidFinishLaunching:");
            AppState::launched(
                *self.activation_policy,
                *self.default_menu,
                *self.activate_ignoring_other_apps,
            );
        }

        #[method(applicationWillTerminate:)]
        fn will_terminate(&self, _sender: Option<&AnyObject>) {
            trace_scope!("applicationWillTerminate:");
            // TODO: Notify every window that it will be destroyed, like done in iOS?
            AppState::internal_exit();
        }

        #[method(applicationOpenUrls:)]
        fn application_open_urls(&self, urls: Url) -> () {
            trace!("Trigger `application:openURLs:`");

            let urls = unsafe {
                (0..urls.items.len())
                    .flat_map(|i| {
                        url::Url::parse(&CStr::from_ptr(urls.items[i]).to_string_lossy())
                    })
                    .collect::<Vec<_>>()
            };
            trace!("Get `application:openURLs:` URLs: {:?}", urls);
            AppState::open_urls(urls);
            trace!("Completed `application:openURLs:`");
        }
    }
);

impl ApplicationDelegate {
    pub(super) fn new(
        activation_policy: NSApplicationActivationPolicy,
        default_menu: bool,
        activate_ignoring_other_apps: bool,
    ) -> Id<Self> {
        unsafe {
            msg_send_id![
                Self::alloc(),
                initWithActivationPolicy: activation_policy,
                defaultMenu: default_menu,
                activateIgnoringOtherApps: activate_ignoring_other_apps,
            ]
        }
    }
}
