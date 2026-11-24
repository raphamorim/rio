//! Splits: each tab holds a tree of terminals in `GtkPaned`s, under a
//! root bin that stays the tab's child while the tree changes.

use crate::terminal::TerminalView;
use adw::prelude::*;

const ROOT: &str = "panes";

pub fn new_root(view: &TerminalView) -> adw::Bin {
    let root = adw::Bin::new();
    root.set_widget_name(ROOT);
    root.set_child(Some(view));
    root
}

fn root_of(widget: &impl IsA<gtk::Widget>) -> Option<adw::Bin> {
    let mut current = widget.parent();
    while let Some(widget) = current {
        if widget.widget_name() == ROOT {
            return widget.downcast().ok();
        }
        current = widget.parent();
    }
    None
}

/// The tab a terminal is in now; tabs move between windows, so this is
/// looked up rather than remembered.
pub fn page_of(view: &TerminalView) -> Option<(adw::TabView, adw::TabPage)> {
    let root = root_of(view)?;
    let tabs = root
        .ancestor(adw::TabView::static_type())
        .and_downcast::<adw::TabView>()?;
    let page = tabs.page(&root);
    Some((tabs, page))
}

/// Every terminal under `widget`, left to right and top to bottom.
pub fn views(widget: &gtk::Widget) -> Vec<TerminalView> {
    let mut out = Vec::new();
    collect(widget, &mut out);
    out
}

fn collect(widget: &gtk::Widget, out: &mut Vec<TerminalView>) {
    if let Some(view) = widget.downcast_ref::<TerminalView>() {
        out.push(view.clone());
    } else if let Some(paned) = widget.downcast_ref::<gtk::Paned>() {
        for child in [paned.start_child(), paned.end_child()]
            .into_iter()
            .flatten()
        {
            collect(&child, out);
        }
    } else if let Some(bin) = widget.downcast_ref::<adw::Bin>() {
        if let Some(child) = bin.child() {
            collect(&child, out);
        }
    }
}

/// The terminal in `page` that last had focus.
pub fn focused(page: &adw::TabPage) -> Option<TerminalView> {
    views(&page.child())
        .into_iter()
        .max_by_key(|view| view.focus_order())
}

/// Put `widget` where `old` is.
fn replace(old: &gtk::Widget, widget: &gtk::Widget) {
    let Some(parent) = old.parent() else {
        return;
    };
    if let Some(paned) = parent.downcast_ref::<gtk::Paned>() {
        if paned.start_child().as_ref() == Some(old) {
            paned.set_start_child(None::<&gtk::Widget>);
            paned.set_start_child(Some(widget));
        } else {
            paned.set_end_child(None::<&gtk::Widget>);
            paned.set_end_child(Some(widget));
        }
    } else if let Some(bin) = parent.downcast_ref::<adw::Bin>() {
        bin.set_child(None::<&gtk::Widget>);
        bin.set_child(Some(widget));
    }
}

/// Split `view`'s space with `new`, after it: to its right for a
/// horizontal split, below it for a vertical one.
pub fn split(view: &TerminalView, new: &TerminalView, orientation: gtk::Orientation) {
    let size = match orientation {
        gtk::Orientation::Horizontal => view.width(),
        _ => view.height(),
    };
    let paned = gtk::Paned::new(orientation);
    paned.set_shrink_start_child(false);
    paned.set_shrink_end_child(false);
    replace(view.upcast_ref(), paned.upcast_ref());
    paned.set_start_child(Some(view));
    paned.set_end_child(Some(new));
    if size > 0 {
        paned.set_position(size / 2);
    }
}

/// Take `view` out of its tab, its sibling taking its space. False when
/// it was the tab's only terminal, which the caller closes instead.
pub fn remove(view: &TerminalView) -> bool {
    let Some(paned) = view.parent().and_downcast::<gtk::Paned>() else {
        return false;
    };
    let sibling = if paned.start_child().as_ref() == Some(view.upcast_ref()) {
        paned.end_child()
    } else {
        paned.start_child()
    };
    let next = sibling
        .as_ref()
        .and_then(|sibling| views(sibling).into_iter().max_by_key(|v| v.focus_order()));
    let mut ancestor = Some(paned.clone().upcast::<gtk::Widget>());
    while let Some(widget) = ancestor {
        if widget.is::<gtk::Paned>() {
            widget.set_focus_child(None::<&gtk::Widget>);
        }
        ancestor = widget.parent();
    }
    paned.set_start_child(None::<&gtk::Widget>);
    paned.set_end_child(None::<&gtk::Widget>);
    if let Some(sibling) = sibling {
        replace(paned.upcast_ref(), &sibling);
    }
    if let Some(next) = next {
        next.grab_focus();
    }
    true
}

/// The terminal after (or before) `view` in its tab, wrapping around.
pub fn neighbor(view: &TerminalView, forward: bool) -> Option<TerminalView> {
    let root = root_of(view)?;
    let all = views(root.upcast_ref());
    if all.len() < 2 {
        return None;
    }
    let index = all.iter().position(|v| v == view)?;
    let next = if forward {
        (index + 1) % all.len()
    } else {
        (index + all.len() - 1) % all.len()
    };
    all.get(next).cloned()
}

/// Move the nearest divider of `orientation` around `view` by `step`
/// of its length.
pub fn resize(view: &TerminalView, orientation: gtk::Orientation, step: f64) {
    let mut current = view.parent();
    while let Some(widget) = current {
        if let Some(paned) = widget.downcast_ref::<gtk::Paned>() {
            if paned.orientation() == orientation {
                let length = match orientation {
                    gtk::Orientation::Horizontal => paned.width(),
                    _ => paned.height(),
                };
                let delta = (length as f64 * step) as i32;
                paned.set_position((paned.position() + delta).max(0));
                return;
            }
        }
        current = widget.parent();
    }
}
