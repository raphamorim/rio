//! Windows of terminal tabs, on GNOME's own `AdwTabBar` / `AdwTabView`:
//! tabs reorder by dragging, drag out into a new window or into another
//! window, have a context menu, and flag a bell in the background.

use crate::panes;
use crate::shared::Shared;
use crate::terminal::TerminalView;
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use rio_backend::config::window::{Decorations, WindowMode};
use std::cell::RefCell;
use std::rc::Rc;

/// Keyboard shortcuts, GNOME terminal style: with shift, so the plain
/// ctrl keys stay the program's.
const SHORTCUTS: &[(&str, &[&str])] = &[
    ("app.new-window", &["<Ctrl><Shift>n"]),
    ("win.new-tab", &["<Ctrl><Shift>t"]),
    ("win.close", &["<Ctrl><Shift>w"]),
    ("win.split-right", &["<Ctrl><Shift>r"]),
    ("win.split-down", &["<Ctrl><Shift>d"]),
    ("win.next-split-or-tab", &["<Ctrl><Shift>bracketright"]),
    ("win.previous-split-or-tab", &["<Ctrl><Shift>bracketleft"]),
    ("win.divider-up", &["<Ctrl><Shift><Alt>Up"]),
    ("win.divider-down", &["<Ctrl><Shift><Alt>Down"]),
    ("win.divider-left", &["<Ctrl><Shift><Alt>Left"]),
    ("win.divider-right", &["<Ctrl><Shift><Alt>Right"]),
    ("win.copy", &["<Ctrl><Shift>c"]),
    ("win.paste", &["<Ctrl><Shift>v"]),
    ("win.paste-selection", &["<Shift>Insert"]),
    ("win.select-all", &["<Ctrl><Shift>a"]),
    ("win.find", &["<Ctrl><Shift>f"]),
    ("win.fullscreen", &["F11"]),
    ("win.open-config", &["<Ctrl><Shift>comma"]),
    (
        "win.zoom-in",
        &["<Ctrl>equal", "<Ctrl>plus", "<Ctrl>KP_Add"],
    ),
    ("win.zoom-out", &["<Ctrl>minus", "<Ctrl>KP_Subtract"]),
    ("win.zoom-reset", &["<Ctrl>0", "<Ctrl>KP_0"]),
    ("win.next-tab", &["<Ctrl>Page_Down", "<Ctrl>Tab"]),
    (
        "win.previous-tab",
        &["<Ctrl>Page_Up", "<Ctrl><Shift>ISO_Left_Tab"],
    ),
    ("win.move-tab-left", &["<Ctrl><Shift>Page_Up"]),
    ("win.move-tab-right", &["<Ctrl><Shift>Page_Down"]),
];

/// The app's actions and shortcuts, once at startup.
pub fn install(app: &adw::Application) {
    let css = gtk::CssProvider::new();
    css.load_from_data("window.rio-translucent { background-color: transparent; }");
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    let new_window = gio::ActionEntry::builder("new-window")
        .activate(|app: &adw::Application, _, _| app.activate())
        .build();
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| {
            for window in app.windows() {
                window.close();
            }
        })
        .build();
    app.add_action_entries([new_window, quit]);
}

/// The shortcuts, less those the config binds to something else.
pub fn apply_shortcuts(app: &gtk::Application, bindings: &[crate::bindings::Binding]) {
    let taken: Vec<String> = bindings
        .iter()
        .filter_map(|binding| binding.accelerator())
        .collect();
    let free = |accels: &[String]| -> Vec<String> {
        accels
            .iter()
            .filter(|accel| {
                let parsed = gtk::accelerator_parse(accel.as_str());
                !taken
                    .iter()
                    .any(|t| gtk::accelerator_parse(t.as_str()) == parsed)
            })
            .cloned()
            .collect()
    };
    let mut all: Vec<(String, Vec<String>)> = SHORTCUTS
        .iter()
        .map(|(action, accels)| {
            (
                action.to_string(),
                accels.iter().map(|a| a.to_string()).collect(),
            )
        })
        .collect();
    // Alt+1..8 picks that tab, Alt+9 the last, as in GNOME's terminals.
    for n in 1..=9 {
        all.push((format!("win.select-tab({n})"), vec![format!("<Alt>{n}")]));
    }
    for (action, accels) in all {
        let accels = free(&accels);
        let accels: Vec<&str> = accels.iter().map(String::as_str).collect();
        app.set_accels_for_action(&action, &accels);
    }
}

pub struct Window;

impl Window {
    /// A new window with one tab.
    pub fn open(app: &adw::Application, shared: &Rc<Shared>) {
        let tabs = Self::build(app, shared);
        add_tab(&tabs, shared, None);
        let Some(window) = tabs.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let config = shared.config.borrow();
        window.set_default_size(config.window.width, config.window.height);
        match config.window.mode {
            WindowMode::Maximized => window.maximize(),
            WindowMode::Fullscreen => window.fullscreen(),
            WindowMode::Windowed => {}
        }
        drop(config);
        window.present();
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
        let config = shared.config.borrow();
        let tab_bar = adw::TabBar::builder()
            .view(&tabs)
            .autohide(config.navigation.hide_if_single)
            .end_action_widget(&new_tab)
            .build();

        let search = SearchUi::new(&tabs);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let decorations = config.window.decorations;
        if decorations != Decorations::Disabled {
            let header = adw::HeaderBar::new();
            if decorations == Decorations::Buttonless {
                header.set_show_start_title_buttons(false);
                header.set_show_end_title_buttons(false);
            }
            content.append(&header);
        }
        content.append(&tab_bar);
        content.append(&search.bar);
        content.append(&tabs);

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Rio")
            .content(&content)
            .build();
        if config.window.opacity < 1.0 {
            window.add_css_class("rio-translucent");
        }
        drop(config);
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
                    if let Some(view) = panes::focused(&page) {
                        view.grab_focus();
                    }
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
                let running = running_in(std::slice::from_ref(page));
                let Some(program) = running
                    .first()
                    .filter(|_| shared.config.borrow().confirm_before_quit)
                    .cloned()
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
                let pages: Vec<adw::TabPage> =
                    (0..tabs.n_pages()).map(|i| tabs.nth_page(i)).collect();
                let running = running_in(&pages);
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
            gio::ActionEntry::builder("close")
                .activate(glib::clone!(
                    #[weak]
                    tabs,
                    #[weak]
                    window,
                    #[strong]
                    shared,
                    move |_: &adw::ApplicationWindow, _, _| {
                        close_focused(&window, &tabs, &shared)
                    }
                ))
                .build(),
            split_action(&tabs, shared, "split-right", gtk::Orientation::Horizontal),
            split_action(&tabs, shared, "split-down", gtk::Orientation::Vertical),
            cycle_action(&tabs, "next-split-or-tab", true),
            cycle_action(&tabs, "previous-split-or-tab", false),
            on_view(&tabs, "divider-up", |view| {
                panes::resize(view, gtk::Orientation::Vertical, -0.05)
            }),
            on_view(&tabs, "divider-down", |view| {
                panes::resize(view, gtk::Orientation::Vertical, 0.05)
            }),
            on_view(&tabs, "divider-left", |view| {
                panes::resize(view, gtk::Orientation::Horizontal, -0.05)
            }),
            on_view(&tabs, "divider-right", |view| {
                panes::resize(view, gtk::Orientation::Horizontal, 0.05)
            }),
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
            gio::ActionEntry::builder("fullscreen")
                .activate(|window: &adw::ApplicationWindow, _, _| {
                    if window.is_fullscreen() {
                        window.unfullscreen();
                    } else {
                        window.fullscreen();
                    }
                })
                .build(),
            gio::ActionEntry::builder("open-config")
                .activate(|window: &adw::ApplicationWindow, _, _| {
                    let path = rio_backend::config::config_file_path();
                    if !path.exists() {
                        if let Some(dir) = path.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        let _ = std::fs::write(&path, "");
                    }
                    let uri = gio::File::for_path(&path).uri();
                    let context = WidgetExt::display(window).app_launch_context();
                    if let Err(err) =
                        gio::AppInfo::launch_default_for_uri(&uri, Some(&context))
                    {
                        tracing::warn!("could not open {uri}: {err}");
                    }
                })
                .build(),
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

/// Programs running in `pages`' terminals.
fn running_in(pages: &[adw::TabPage]) -> Vec<String> {
    pages
        .iter()
        .flat_map(|page| panes::views(&page.child()))
        .filter_map(|view| view.foreground_program())
        .collect()
}

/// Close the focused split, or its tab when it is the only one, asking
/// first when a program runs in it.
fn close_focused(
    window: &adw::ApplicationWindow,
    tabs: &adw::TabView,
    shared: &Rc<Shared>,
) {
    let Some(page) = tabs.selected_page() else {
        return;
    };
    if panes::views(&page.child()).len() < 2 {
        tabs.close_page(&page);
        return;
    }
    let Some(view) = panes::focused(&page) else {
        return;
    };
    let running = view
        .foreground_program()
        .filter(|_| shared.config.borrow().confirm_before_quit);
    let Some(program) = running else {
        panes::remove(&view);
        return;
    };
    crate::dialog::confirm(
        Some(window.upcast_ref()),
        "Close this split?",
        &format!("“{program}” is still running in it."),
        "Close",
        move |close| {
            if close {
                panes::remove(&view);
            }
        },
    );
}

fn split_action(
    tabs: &adw::TabView,
    shared: &Rc<Shared>,
    name: &str,
    orientation: gtk::Orientation,
) -> gio::ActionEntry<adw::ApplicationWindow> {
    gio::ActionEntry::builder(name)
        .activate(glib::clone!(
            #[weak]
            tabs,
            #[strong]
            shared,
            move |_: &adw::ApplicationWindow, _, _| {
                if !shared.config.borrow().navigation.use_split {
                    return;
                }
                let Some(view) = selected(&tabs) else {
                    return;
                };
                let dir = inherited_dir(&tabs, &shared);
                let new = match TerminalView::new(&shared, dir) {
                    Ok(new) => new,
                    Err(err) => {
                        tracing::error!("could not start the shell: {err}");
                        return;
                    }
                };
                connect_view(&new);
                panes::split(&view, &new, orientation);
                new.grab_focus();
            }
        ))
        .build()
}

/// Focus the next (or previous) split, or tab when the tab has none.
fn cycle_action(
    tabs: &adw::TabView,
    name: &str,
    forward: bool,
) -> gio::ActionEntry<adw::ApplicationWindow> {
    gio::ActionEntry::builder(name)
        .activate(glib::clone!(
            #[weak]
            tabs,
            move |_: &adw::ApplicationWindow, _, _| {
                if let Some(next) =
                    selected(&tabs).and_then(|v| panes::neighbor(&v, forward))
                {
                    next.grab_focus();
                    return;
                }
                let n = tabs.n_pages();
                if n == 0 {
                    return;
                }
                let current = tabs.selected_page().map_or(0, |p| tabs.page_position(&p));
                let next = if forward {
                    (current + 1) % n
                } else {
                    (current + n - 1) % n
                };
                tabs.set_selected_page(&tabs.nth_page(next));
            }
        ))
        .build()
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
            for view in panes::views(&tabs.nth_page(i).child()) {
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

/// The selected tab's focused terminal.
fn selected(tabs: &adw::TabView) -> Option<TerminalView> {
    panes::focused(&tabs.selected_page()?)
}

/// A new tab running the shell, selected.
fn add_tab(tabs: &adw::TabView, shared: &Rc<Shared>, working_dir: Option<String>) {
    let view = match TerminalView::new(shared, working_dir) {
        Ok(view) => view,
        Err(err) => {
            tracing::error!("could not start the shell: {err}");
            return;
        }
    };
    connect_view(&view);
    let page = tabs.append(&panes::new_root(&view));
    page.set_title("Rio");
    tabs.set_selected_page(&page);
    view.grab_focus();
}

/// A tab shows its focused terminal's title and progress, a bell in the
/// background marks it, and a terminal whose shell exits closes.
fn connect_view(view: &TerminalView) {
    view.connect_title_notify(sync_page);
    view.connect_busy_notify(sync_page);
    view.connect_closure(
        "focused",
        false,
        glib::closure_local!(move |view: TerminalView| sync_page(&view)),
    );
    view.connect_closure(
        "bell",
        false,
        glib::closure_local!(move |view: TerminalView| {
            let indicator = view
                .shared()
                .is_some_and(|shared| shared.config.borrow().bell.tab_indicator);
            if let Some((tabs, page)) = panes::page_of(&view).filter(|_| indicator) {
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
            let Some((tabs, page)) = panes::page_of(&view) else {
                return;
            };
            if !panes::remove(&view) {
                tabs.close_page(&page);
            }
        }),
    );
}

fn sync_page(view: &TerminalView) {
    let Some((tabs, page)) = panes::page_of(view) else {
        return;
    };
    let views = panes::views(&page.child());
    let focused = panes::focused(&page);
    if let Some(shared) = view.shared() {
        let opacity = shared.config.borrow().navigation.unfocused_split_opacity as f64;
        for other in &views {
            let dim = views.len() > 1 && Some(other) != focused.as_ref();
            other.set_opacity(if dim { opacity } else { 1.0 });
        }
    }
    if focused.as_ref() != Some(view) {
        return;
    }
    let title = view.title();
    page.set_title(if title.is_empty() { "Rio" } else { &title });
    page.set_loading(views.iter().any(|v| v.busy()));
    if tabs.selected_page().as_ref() == Some(&page) {
        if let Some(window) = tabs.root().and_downcast::<gtk::Window>() {
            window.set_title(Some(&page.title()));
        }
    }
}
