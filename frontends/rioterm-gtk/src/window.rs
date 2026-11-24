//! Windows of terminal tabs, on GNOME's own `AdwTabBar` / `AdwTabView`:
//! tabs reorder by dragging, drag out into a new window or into another
//! window, have a context menu, and flag a bell in the background.

use crate::shared::Shared;
use crate::terminal::TerminalView;
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::rc::Rc;

/// Keyboard shortcuts, GNOME terminal style: with shift, so the plain
/// ctrl keys stay the program's.
const SHORTCUTS: &[(&str, &[&str])] = &[
    ("app.new-window", &["<Ctrl><Shift>n"]),
    ("win.new-tab", &["<Ctrl><Shift>t"]),
    ("win.close-tab", &["<Ctrl><Shift>w"]),
    ("win.copy", &["<Ctrl><Shift>c"]),
    ("win.paste", &["<Ctrl><Shift>v"]),
    ("win.paste-selection", &["<Shift>Insert"]),
    ("win.select-all", &["<Ctrl><Shift>a"]),
    ("win.find", &["<Ctrl><Shift>f"]),
    (
        "win.zoom-in",
        &["<Ctrl>equal", "<Ctrl>plus", "<Ctrl>KP_Add"],
    ),
    ("win.zoom-out", &["<Ctrl>minus", "<Ctrl>KP_Subtract"]),
    ("win.zoom-reset", &["<Ctrl>0", "<Ctrl>KP_0"]),
    (
        "win.next-tab",
        &["<Ctrl>Page_Down", "<Ctrl>Tab", "<Ctrl><Shift>bracketright"],
    ),
    (
        "win.previous-tab",
        &[
            "<Ctrl>Page_Up",
            "<Ctrl><Shift>ISO_Left_Tab",
            "<Ctrl><Shift>bracketleft",
        ],
    ),
    ("win.move-tab-left", &["<Ctrl><Shift>Page_Up"]),
    ("win.move-tab-right", &["<Ctrl><Shift>Page_Down"]),
];

/// The app's actions and shortcuts, once at startup.
pub fn install(app: &adw::Application) {
    let new_window = gio::ActionEntry::builder("new-window")
        .activate(|app: &adw::Application, _, _| app.activate())
        .build();
    app.add_action_entries([new_window]);
    for (action, accels) in SHORTCUTS {
        app.set_accels_for_action(action, accels);
    }
    // Alt+1..8 picks that tab, Alt+9 the last, as in GNOME's terminals.
    for n in 1..=9 {
        app.set_accels_for_action(
            &format!("win.select-tab({n})"),
            &[&format!("<Alt>{n}")],
        );
    }
}

pub struct Window;

impl Window {
    /// A new window with one tab.
    pub fn open(app: &adw::Application, shared: &Rc<Shared>) {
        let tabs = Self::build(app, shared);
        add_tab(&tabs, shared, None);
        if let Some(window) = tabs.root().and_downcast::<gtk::Window>() {
            window.present();
        }
    }

    /// A new, empty window, presented. Returns its tab view.
    fn build(app: &adw::Application, shared: &Rc<Shared>) -> adw::TabView {
        let tabs = adw::TabView::new();

        let new_tab = gtk::Button::from_icon_name("tab-new-symbolic");
        new_tab.set_tooltip_text(Some("New Tab"));
        new_tab.set_action_name(Some("win.new-tab"));
        new_tab.add_css_class("flat");
        // Always shown, so the tabs and the new tab button are where the
        // user looks for them even with one tab open.
        let tab_bar = adw::TabBar::builder()
            .view(&tabs)
            .autohide(false)
            .end_action_widget(&new_tab)
            .build();

        let search = SearchUi::new(&tabs);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&adw::HeaderBar::new());
        content.append(&tab_bar);
        content.append(&search.bar);
        content.append(&tabs);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Rio")
            .content(&content)
            .build();
        // Natural size: the terminal's own (80x24 cells).
        window.set_default_size(-1, -1);

        // The window shows the selected tab's title; selecting a tab
        // clears its bell mark and focuses it. The window closes with its
        // last tab, whether closed or dragged away.
        tabs.connect_selected_page_notify(glib::clone!(
            #[weak]
            window,
            #[strong]
            search,
            move |tabs| {
                search.retarget(tabs);
                if let Some(page) = tabs.selected_page() {
                    window.set_title(Some(&page.title()));
                    page.set_needs_attention(false);
                    page.child().grab_focus();
                }
            }
        ));
        tabs.connect_n_pages_notify(glib::clone!(
            #[weak]
            window,
            move |tabs| {
                if tabs.n_pages() == 0 {
                    window.close();
                }
            }
        ));
        // Closing a tab or the window running a program asks first.
        tabs.connect_close_page(glib::clone!(
            #[weak]
            window,
            #[strong]
            shared,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |tabs, page| {
                let running = page
                    .child()
                    .downcast::<TerminalView>()
                    .ok()
                    .and_then(|view| view.foreground_program());
                let Some(program) =
                    running.filter(|_| shared.config.borrow().confirm_before_quit)
                else {
                    return glib::Propagation::Proceed;
                };
                let (tabs, page) = (tabs.clone(), page.clone());
                crate::dialog::confirm(
                    Some(window.upcast_ref()),
                    "Close this tab?",
                    &format!("“{program}” is still running in it."),
                    "Close Tab",
                    move |close| tabs.close_page_finish(&page, close),
                );
                glib::Propagation::Stop
            }
        ));
        window.connect_close_request(glib::clone!(
            #[weak]
            tabs,
            #[strong]
            shared,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |window| {
                if !shared.config.borrow().confirm_before_quit {
                    return glib::Propagation::Proceed;
                }
                let running: Vec<String> = (0..tabs.n_pages())
                    .filter_map(|i| {
                        tabs.nth_page(i)
                            .child()
                            .downcast::<TerminalView>()
                            .ok()?
                            .foreground_program()
                    })
                    .collect();
                if running.is_empty() {
                    return glib::Propagation::Proceed;
                }
                let target = window.clone();
                crate::dialog::confirm(
                    Some(window.upcast_ref()),
                    "Close this window?",
                    &format!("Still running: {}.", running.join(", ")),
                    "Close Window",
                    move |close| {
                        if close {
                            target.destroy();
                        }
                    },
                );
                glib::Propagation::Stop
            }
        ));
        // A tab dropped outside every window gets a window of its own.
        tabs.connect_create_window(glib::clone!(
            #[weak]
            app,
            #[strong]
            shared,
            #[upgrade_or]
            None,
            move |_| {
                let tabs = Self::build(&app, &shared);
                if let Some(window) = tabs.root().and_downcast::<gtk::Window>() {
                    window.present();
                }
                Some(tabs)
            }
        ));

        // The tab context menu acts on the tab it was opened on; the
        // shortcuts on the selected one.
        let menu_page: Rc<RefCell<Option<adw::TabPage>>> = Rc::default();
        tabs.set_menu_model(Some(&tab_menu()));
        tabs.connect_setup_menu({
            let menu_page = menu_page.clone();
            move |_, page| {
                menu_page.replace(page.cloned());
            }
        });
        let target = {
            let menu_page = menu_page.clone();
            move |tabs: &adw::TabView| {
                menu_page.borrow().clone().or_else(|| tabs.selected_page())
            }
        };
        let on_tab = |name: &str, act: fn(&adw::TabView, &adw::TabPage)| {
            let target = target.clone();
            gio::ActionEntry::builder(name)
                .activate(glib::clone!(
                    #[weak]
                    tabs,
                    move |_: &adw::ApplicationWindow, _, _| {
                        if let Some(page) = target(&tabs) {
                            act(&tabs, &page);
                        }
                    }
                ))
                .build()
        };

        let actions = [
            gio::ActionEntry::builder("new-tab")
                .activate(glib::clone!(
                    #[weak]
                    tabs,
                    #[strong]
                    shared,
                    move |_: &adw::ApplicationWindow, _, _| {
                        let dir = inherited_dir(&tabs, &shared);
                        add_tab(&tabs, &shared, dir)
                    }
                ))
                .build(),
            on_tab("close-tab", |tabs, page| tabs.close_page(page)),
            on_tab("close-other-tabs", |tabs, page| {
                tabs.close_other_pages(page)
            }),
            on_tab("move-tab-left", |tabs, page| {
                tabs.reorder_backward(page);
            }),
            on_tab("move-tab-right", |tabs, page| {
                tabs.reorder_forward(page);
            }),
            gio::ActionEntry::builder("select-tab")
                .parameter_type(Some(glib::VariantTy::INT32))
                .activate(glib::clone!(
                    #[weak]
                    tabs,
                    move |_: &adw::ApplicationWindow, _, param| {
                        let n = tabs.n_pages();
                        let Some(wanted) = param.and_then(|p| p.get::<i32>()) else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        let index = if wanted >= 9 {
                            n - 1
                        } else {
                            (wanted - 1).min(n - 1)
                        };
                        tabs.set_selected_page(&tabs.nth_page(index));
                    }
                ))
                .build(),
            gio::ActionEntry::builder("next-tab")
                .activate(glib::clone!(
                    #[weak]
                    tabs,
                    move |_: &adw::ApplicationWindow, _, _| {
                        if !tabs.select_next_page() && tabs.n_pages() > 0 {
                            tabs.set_selected_page(&tabs.nth_page(0));
                        }
                    }
                ))
                .build(),
            gio::ActionEntry::builder("previous-tab")
                .activate(glib::clone!(
                    #[weak]
                    tabs,
                    move |_: &adw::ApplicationWindow, _, _| {
                        if !tabs.select_previous_page() && tabs.n_pages() > 0 {
                            tabs.set_selected_page(&tabs.nth_page(tabs.n_pages() - 1));
                        }
                    }
                ))
                .build(),
            gio::ActionEntry::builder("copy")
                .activate(glib::clone!(
                    #[weak]
                    tabs,
                    move |_: &adw::ApplicationWindow, _, _| {
                        if let Some(view) = selected(&tabs) {
                            view.copy();
                        }
                    }
                ))
                .build(),
            on_view(&tabs, "paste", |view| view.paste()),
            on_view(&tabs, "paste-selection", |view| view.paste_from_primary()),
            on_view(&tabs, "select-all", |view| view.select_all()),
            on_view(&tabs, "zoom-in", |view| view.zoom(Some(1.0))),
            on_view(&tabs, "zoom-out", |view| view.zoom(Some(-1.0))),
            on_view(&tabs, "zoom-reset", |view| view.zoom(None)),
            gio::ActionEntry::builder("find")
                .activate(glib::clone!(
                    #[strong]
                    search,
                    move |_: &adw::ApplicationWindow, _, _| search.open()
                ))
                .build(),
        ];
        window.add_action_entries(actions);
        // The menu's target is only good while it is open.
        window.connect_is_active_notify(move |_| {
            menu_page.replace(None);
        });

        tabs
    }
}

/// The window's search bar, searching the selected terminal.
#[derive(Clone)]
struct SearchUi {
    bar: gtk::SearchBar,
    entry: gtk::SearchEntry,
    count: gtk::Label,
    tabs: glib::WeakRef<adw::TabView>,
}

impl SearchUi {
    fn new(tabs: &adw::TabView) -> Self {
        let entry = gtk::SearchEntry::new();
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some("Find"));
        let count = gtk::Label::new(None);
        count.add_css_class("dim-label");
        count.add_css_class("numeric");
        let older = gtk::Button::from_icon_name("go-up-symbolic");
        older.set_tooltip_text(Some("Previous Match"));
        let newer = gtk::Button::from_icon_name("go-down-symbolic");
        newer.set_tooltip_text(Some("Next Match"));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.set_width_request(360);
        row.append(&entry);
        row.append(&count);
        row.append(&older);
        row.append(&newer);
        let bar = gtk::SearchBar::builder()
            .child(&row)
            .show_close_button(true)
            .build();
        bar.connect_entry(&entry);
        let ui = SearchUi {
            bar,
            entry,
            count,
            tabs: tabs.downgrade(),
        };

        ui.entry.connect_search_changed(glib::clone!(
            #[strong]
            ui,
            move |_| ui.run(None)
        ));
        ui.entry.connect_activate(glib::clone!(
            #[strong]
            ui,
            move |_| ui.run(Some(true))
        ));
        ui.entry.connect_next_match(glib::clone!(
            #[strong]
            ui,
            move |_| ui.run(Some(true))
        ));
        ui.entry.connect_previous_match(glib::clone!(
            #[strong]
            ui,
            move |_| ui.run(Some(false))
        ));
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[strong]
            ui,
            move |_, key, _, state| {
                let enter = matches!(key, gdk::Key::Return | gdk::Key::KP_Enter);
                if enter && state.contains(gdk::ModifierType::SHIFT_MASK) {
                    ui.run(Some(false));
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        ));
        ui.entry.add_controller(keys);
        older.connect_clicked(glib::clone!(
            #[strong]
            ui,
            move |_| ui.run(Some(true))
        ));
        newer.connect_clicked(glib::clone!(
            #[strong]
            ui,
            move |_| ui.run(Some(false))
        ));
        ui.bar.connect_search_mode_enabled_notify(glib::clone!(
            #[strong]
            ui,
            move |bar| {
                if bar.is_search_mode() {
                    return;
                }
                if let Some(view) = ui.view() {
                    view.search_clear();
                    view.grab_focus();
                }
            }
        ));
        ui
    }

    fn view(&self) -> Option<TerminalView> {
        selected(&self.tabs.upgrade()?)
    }

    fn open(&self) {
        self.bar.set_search_mode(true);
        self.entry.grab_focus();
        self.run(None);
    }

    fn run(&self, older: Option<bool>) {
        let Some(view) = self.view() else {
            return;
        };
        let text = self.entry.text();
        let (position, count) = view.search(&text, older);
        self.count.set_text(&match count {
            0 if text.is_empty() => String::new(),
            0 => "No matches".to_string(),
            _ => format!("{position} of {count}"),
        });
    }

    /// A new tab was selected: only it shows matches.
    fn retarget(&self, tabs: &adw::TabView) {
        for i in 0..tabs.n_pages() {
            if let Ok(view) = tabs.nth_page(i).child().downcast::<TerminalView>() {
                view.search_clear();
            }
        }
        if self.bar.is_search_mode() {
            self.run(None);
        }
    }
}

fn tab_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let order = gio::Menu::new();
    order.append(Some("Move Left"), Some("win.move-tab-left"));
    order.append(Some("Move Right"), Some("win.move-tab-right"));
    menu.append_section(None, &order);
    let close = gio::Menu::new();
    close.append(Some("Close Other Tabs"), Some("win.close-other-tabs"));
    close.append(Some("Close Tab"), Some("win.close-tab"));
    menu.append_section(None, &close);
    menu
}

/// An action on the selected tab's terminal.
fn on_view(
    tabs: &adw::TabView,
    name: &str,
    act: fn(&TerminalView),
) -> gio::ActionEntry<adw::ApplicationWindow> {
    gio::ActionEntry::builder(name)
        .activate(glib::clone!(
            #[weak]
            tabs,
            move |_: &adw::ApplicationWindow, _, _| {
                if let Some(view) = selected(&tabs) {
                    act(&view);
                }
            }
        ))
        .build()
}

/// Where a new tab starts: the selected terminal's directory, when the
/// config asks for it.
fn inherited_dir(tabs: &adw::TabView, shared: &Rc<Shared>) -> Option<String> {
    if !shared.config.borrow().navigation.current_working_directory {
        return None;
    }
    selected(tabs)?.working_dir()
}

fn selected(tabs: &adw::TabView) -> Option<TerminalView> {
    tabs.selected_page()?.child().downcast().ok()
}

/// The tab view a terminal is in now: a tab dragged to another window
/// moves there, so this is looked up rather than remembered.
fn tabs_of(view: &TerminalView) -> Option<(adw::TabView, adw::TabPage)> {
    let tabs = view
        .ancestor(adw::TabView::static_type())
        .and_downcast::<adw::TabView>()?;
    let page = tabs.page(view);
    Some((tabs, page))
}

/// A new tab running the shell, selected. Its title follows the
/// program's, a bell in the background marks it, and it closes when the
/// shell exits.
fn add_tab(tabs: &adw::TabView, shared: &Rc<Shared>, working_dir: Option<String>) {
    let view = match TerminalView::new(shared, working_dir) {
        Ok(view) => view,
        Err(err) => {
            tracing::error!("could not start the shell: {err}");
            return;
        }
    };
    let page = tabs.append(&view);
    view.bind_property("title", &page, "title")
        .transform_to(|_, title: String| {
            Some(if title.is_empty() {
                "Rio".to_string()
            } else {
                title
            })
        })
        .sync_create()
        .build();
    view.bind_property("busy", &page, "loading")
        .sync_create()
        .build();
    view.connect_title_notify(|view| {
        let Some((tabs, page)) = tabs_of(view) else {
            return;
        };
        if tabs.selected_page().as_ref() == Some(&page) {
            if let Some(window) = tabs.root().and_downcast::<gtk::Window>() {
                window.set_title(Some(&page.title()));
            }
        }
    });
    view.connect_closure(
        "bell",
        false,
        glib::closure_local!(move |view: TerminalView| {
            if let Some((tabs, page)) = tabs_of(&view) {
                if tabs.selected_page().as_ref() != Some(&page) {
                    page.set_needs_attention(true);
                }
            }
        }),
    );
    view.connect_closure(
        "exited",
        false,
        glib::closure_local!(move |view: TerminalView| {
            if let Some((tabs, page)) = tabs_of(&view) {
                tabs.close_page(&page);
            }
        }),
    );
    tabs.set_selected_page(&page);
    view.grab_focus();
}
