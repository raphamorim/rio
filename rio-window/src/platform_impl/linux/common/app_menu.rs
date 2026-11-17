//! Application menus exported on the session bus as `com.canonical.dbusmenu`.
//!
//! A desktop with a global menu bar reads the menu of the active window
//! from the bus instead of the window drawing it. The windowing backend
//! tells the desktop where each window's menu is (on Wayland through the
//! KDE application menu protocol); this module only serves the objects.
//!
//! One worker thread owns the bus connection and is started with the first
//! menu, so a slow or absent session bus never blocks the event loop.
//! Activations are delivered from that thread through the handler given
//! with the menu.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use zbus::zvariant::{StructureBuilder, Value};

/// Called with the `id` of the activated [`AppMenuItem`], on the thread
/// that serves the menus.
pub type AppMenuHandler = Arc<dyn Fn(u32) + Send + Sync>;

/// An entry of an application menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppMenuItem {
    /// Passed to the handler when the entry is activated. Not used for
    /// separators and submenus.
    pub id: u32,
    pub label: String,
    pub enabled: bool,
    /// The keys shown beside the label, modifiers first: `Control`,
    /// `Shift`, `Alt` and `Super`, then the key (`["Control", "Shift",
    /// "t"]`). Informative only: the desktop does not grab them.
    pub shortcut: Vec<String>,
    pub separator: bool,
    /// A non-empty list makes the entry a submenu.
    pub children: Vec<AppMenuItem>,
}

impl AppMenuItem {
    pub fn new(id: u32, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
            enabled: true,
            ..Self::default()
        }
    }

    pub fn submenu(label: impl Into<String>, children: Vec<AppMenuItem>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
            children,
            ..Self::default()
        }
    }

    pub fn separator() -> Self {
        Self {
            enabled: true,
            separator: true,
            ..Self::default()
        }
    }

    pub fn with_shortcut(mut self, shortcut: Vec<String>) -> Self {
        self.shortcut = shortcut;
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

type Properties = HashMap<String, Value<'static>>;
type Layout = (i32, Properties, Vec<Value<'static>>);

#[derive(PartialEq)]
struct Node {
    item: AppMenuItem,
    children: Vec<usize>,
}

/// A menu flattened so that an entry's index is its dbusmenu id. The root,
/// which dbusmenu fixes at id 0, is the menu bar itself.
struct Tree {
    revision: u32,
    nodes: Vec<Node>,
}

impl Tree {
    fn new(items: Vec<AppMenuItem>) -> Self {
        Self {
            revision: 1,
            nodes: flatten(items),
        }
    }

    /// Replace the entries. Returns `false`, keeping the revision, when
    /// they are the ones already shown.
    fn set(&mut self, items: Vec<AppMenuItem>) -> bool {
        let nodes = flatten(items);
        if nodes == self.nodes {
            return false;
        }
        self.nodes = nodes;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    fn node(&self, id: i32) -> Option<&Node> {
        usize::try_from(id).ok().and_then(|id| self.nodes.get(id))
    }

    /// The handler id of an entry that can be activated.
    fn action(&self, id: i32) -> Option<u32> {
        let node = self.node(id)?;
        (id != 0 && node.children.is_empty() && !node.item.separator && node.item.enabled)
            .then_some(node.item.id)
    }

    /// The properties of an entry that differ from the dbusmenu defaults,
    /// limited to `names` unless it is empty.
    fn properties(&self, node: &Node, names: &[String]) -> Properties {
        let mut properties = Properties::new();
        let mut add = |name: &str, value: Value<'static>| {
            if names.is_empty() || names.iter().any(|wanted| wanted == name) {
                properties.insert(name.to_owned(), value);
            }
        };

        if node.item.separator {
            add("type", Value::from("separator"));
            return properties;
        }
        if !node.item.label.is_empty() {
            // A single underscore marks the next character as a mnemonic.
            add("label", Value::from(node.item.label.replace('_', "__")));
        }
        if !node.item.enabled {
            add("enabled", Value::from(false));
        }
        if !node.item.shortcut.is_empty() {
            add("shortcut", Value::from(vec![node.item.shortcut.clone()]));
        }
        if !node.children.is_empty() {
            add("children-display", Value::from("submenu"));
        }
        properties
    }

    /// `depth` is how many levels of children to include; negative for all.
    fn layout(&self, id: i32, depth: i32, names: &[String]) -> Option<Layout> {
        let node = self.node(id)?;
        let children = if depth == 0 {
            Vec::new()
        } else {
            node.children
                .iter()
                .filter_map(|&child| self.layout(child as i32, depth - 1, names))
                .map(|(id, properties, children)| {
                    Value::Structure(
                        StructureBuilder::new()
                            .add_field(id)
                            .add_field(properties)
                            .add_field(children)
                            .build(),
                    )
                })
                .collect()
        };
        Some((id, self.properties(node, names), children))
    }
}

fn flatten(items: Vec<AppMenuItem>) -> Vec<Node> {
    fn push(nodes: &mut Vec<Node>, mut item: AppMenuItem) -> usize {
        let children = std::mem::take(&mut item.children);
        let index = nodes.len();
        nodes.push(Node {
            item,
            children: Vec::new(),
        });
        let children = children
            .into_iter()
            .map(|child| push(nodes, child))
            .collect();
        nodes[index].children = children;
        index
    }

    let mut nodes = Vec::new();
    push(&mut nodes, AppMenuItem::submenu(String::new(), items));
    nodes
}

/// A window's menu, shared by the bus object and the worker that updates it.
struct Menu {
    tree: Mutex<Tree>,
    handler: Mutex<AppMenuHandler>,
}

struct DbusMenu(Arc<Menu>);

impl DbusMenu {
    fn activate(&self, id: i32, event_id: &str) -> bool {
        let Some(action) = ({
            let tree = self.0.tree.lock().unwrap();
            if tree.node(id).is_none() {
                return false;
            }
            (event_id == "clicked").then(|| tree.action(id)).flatten()
        }) else {
            return true;
        };
        let handler = self.0.handler.lock().unwrap().clone();
        handler(action);
        true
    }
}

fn unknown_item(id: i32) -> zbus::fdo::Error {
    zbus::fdo::Error::InvalidArgs(format!("no menu item with id {id}"))
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl DbusMenu {
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> zbus::fdo::Result<(u32, Layout)> {
        let tree = self.0.tree.lock().unwrap();
        tree.layout(parent_id, recursion_depth, &property_names)
            .map(|layout| (tree.revision, layout))
            .ok_or_else(|| unknown_item(parent_id))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, Properties)> {
        let tree = self.0.tree.lock().unwrap();
        let ids = if ids.is_empty() {
            (0..tree.nodes.len() as i32).collect()
        } else {
            ids
        };
        ids.into_iter()
            .filter_map(|id| {
                let node = tree.node(id)?;
                Some((id, tree.properties(node, &property_names)))
            })
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<Value<'static>> {
        let tree = self.0.tree.lock().unwrap();
        let node = tree.node(id).ok_or_else(|| unknown_item(id))?;
        tree.properties(node, std::slice::from_ref(&name))
            .remove(&name)
            .ok_or_else(|| {
                zbus::fdo::Error::InvalidArgs(format!("item {id} has no {name}"))
            })
    }

    fn event(
        &self,
        id: i32,
        event_id: String,
        _data: Value<'_>,
        _timestamp: u32,
    ) -> zbus::fdo::Result<()> {
        if self.activate(id, &event_id) {
            Ok(())
        } else {
            Err(unknown_item(id))
        }
    }

    fn event_group(&self, events: Vec<(i32, String, Value<'_>, u32)>) -> Vec<i32> {
        events
            .into_iter()
            .filter(|(id, event_id, _, _)| !self.activate(*id, event_id))
            .map(|(id, _, _, _)| id)
            .collect()
    }

    fn about_to_show(&self, id: i32) -> zbus::fdo::Result<bool> {
        let tree = self.0.tree.lock().unwrap();
        tree.node(id).map(|_| false).ok_or_else(|| unknown_item(id))
    }

    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        let tree = self.0.tree.lock().unwrap();
        let unknown = ids
            .into_iter()
            .filter(|id| tree.node(*id).is_none())
            .collect();
        (Vec::new(), unknown)
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(signal)]
    async fn layout_updated(
        context: &zbus::SignalContext<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;
}

/// Called once the menu is served, with the unique bus name of the
/// connection and the object path, on the worker thread.
type Ready = Box<dyn FnOnce(&str, &str) + Send>;

enum Command {
    Export {
        path: String,
        items: Vec<AppMenuItem>,
        handler: AppMenuHandler,
        ready: Ready,
    },
    Update {
        path: String,
        items: Vec<AppMenuItem>,
        handler: AppMenuHandler,
    },
    Remove {
        path: String,
    },
}

/// The menu of one window while it is exported. Dropping it removes the
/// object from the bus.
#[derive(Debug)]
pub struct ExportedAppMenu {
    path: String,
    commands: Sender<Command>,
}

impl ExportedAppMenu {
    /// Serve `items` at a new object path and call `ready` with the address
    /// once the object is on the bus. `ready` is never called when there is
    /// no session bus.
    pub fn new(
        items: Vec<AppMenuItem>,
        handler: AppMenuHandler,
        ready: impl FnOnce(&str, &str) + Send + 'static,
    ) -> Option<Self> {
        static NEXT_PATH: AtomicU32 = AtomicU32::new(1);

        let commands = worker()?;
        let path = format!("/MenuBar/{}", NEXT_PATH.fetch_add(1, Ordering::Relaxed));
        commands
            .send(Command::Export {
                path: path.clone(),
                items,
                handler,
                ready: Box::new(ready),
            })
            .ok()?;
        Some(Self { path, commands })
    }

    /// Replace the menu's entries and its handler.
    pub fn update(&self, items: Vec<AppMenuItem>, handler: AppMenuHandler) {
        let _ = self.commands.send(Command::Update {
            path: self.path.clone(),
            items,
            handler,
        });
    }
}

impl Drop for ExportedAppMenu {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Remove {
            path: std::mem::take(&mut self.path),
        });
    }
}

fn worker() -> Option<Sender<Command>> {
    static WORKER: OnceLock<Option<Sender<Command>>> = OnceLock::new();

    WORKER
        .get_or_init(|| {
            let (sender, receiver) = mpsc::channel();
            std::thread::Builder::new()
                .name("rio-app-menu".into())
                .spawn(move || {
                    if let Err(err) = run(receiver) {
                        tracing::warn!("application menu export stopped: {err}");
                    }
                })
                .map_err(|err| {
                    tracing::warn!("failed to spawn the application menu thread: {err}")
                })
                .ok()
                .map(|_| sender)
        })
        .clone()
}

fn run(commands: mpsc::Receiver<Command>) -> zbus::Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    let name = connection
        .unique_name()
        .map(|name| name.to_string())
        .unwrap_or_default();
    let server = connection.object_server();
    let mut menus: HashMap<String, Arc<Menu>> = HashMap::new();

    for command in commands {
        match command {
            Command::Export {
                path,
                items,
                handler,
                ready,
            } => {
                let menu = Arc::new(Menu {
                    tree: Mutex::new(Tree::new(items)),
                    handler: Mutex::new(handler),
                });
                match server.at(path.as_str(), DbusMenu(menu.clone())) {
                    Ok(true) => {
                        ready(&name, &path);
                        menus.insert(path, menu);
                    }
                    Ok(false) => {}
                    Err(err) => {
                        tracing::warn!("failed to export the application menu: {err}")
                    }
                }
            }
            Command::Update {
                path,
                items,
                handler,
            } => {
                let Some(menu) = menus.get(&path) else {
                    continue;
                };
                *menu.handler.lock().unwrap() = handler;
                let revision = {
                    let mut tree = menu.tree.lock().unwrap();
                    if !tree.set(items) {
                        continue;
                    }
                    tree.revision
                };
                let updated = zbus::SignalContext::new(connection.inner(), path.as_str())
                    .and_then(|context| {
                        zbus::block_on(DbusMenu::layout_updated(&context, revision, 0))
                    });
                if let Err(err) = updated {
                    tracing::warn!("failed to announce the application menu: {err}");
                }
            }
            Command::Remove { path } => {
                if menus.remove(&path).is_some() {
                    let _ = server.remove::<DbusMenu, _>(path.as_str());
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Vec<AppMenuItem> {
        vec![
            AppMenuItem::submenu(
                "File",
                vec![
                    AppMenuItem::new(10, "New_Tab").with_shortcut(vec![
                        "Control".into(),
                        "Shift".into(),
                        "t".into(),
                    ]),
                    AppMenuItem::separator(),
                    AppMenuItem::new(11, "Quit").with_enabled(false),
                ],
            ),
            AppMenuItem::submenu("Edit", vec![AppMenuItem::new(20, "Copy")]),
        ]
    }

    fn ids(children: &[Value<'static>]) -> Vec<i32> {
        children
            .iter()
            .map(|child| {
                let Value::Structure(child) = child else {
                    panic!("a child is a structure");
                };
                i32::try_from(&child.fields()[0]).unwrap()
            })
            .collect()
    }

    #[test]
    fn ids_follow_the_order_of_the_entries() {
        let tree = Tree::new(menu());
        let (id, _, children) = tree.layout(0, -1, &[]).unwrap();
        assert_eq!(id, 0);
        assert_eq!(ids(&children), [1, 5]);

        let (_, _, file) = tree.layout(1, -1, &[]).unwrap();
        assert_eq!(ids(&file), [2, 3, 4]);
        assert!(tree.layout(0, 0, &[]).unwrap().2.is_empty());
        assert!(tree.layout(7, -1, &[]).is_none());
    }

    #[test]
    fn properties_leave_out_the_defaults() {
        let tree = Tree::new(menu());

        let new_tab = tree.properties(tree.node(2).unwrap(), &[]);
        assert_eq!(new_tab.len(), 2);
        assert_eq!(new_tab.get("label"), Some(&Value::from("New__Tab")));
        assert!(new_tab.contains_key("shortcut"));

        let separator = tree.properties(tree.node(3).unwrap(), &[]);
        assert_eq!(separator.len(), 1);
        assert_eq!(separator.get("type"), Some(&Value::from("separator")));

        let quit = tree.properties(tree.node(4).unwrap(), &["enabled".to_owned()]);
        assert_eq!(quit.len(), 1);
        assert_eq!(quit.get("enabled"), Some(&Value::from(false)));
    }

    #[test]
    fn only_enabled_leaves_are_activated() {
        let tree = Tree::new(menu());
        assert_eq!(tree.action(2), Some(10));
        assert_eq!(tree.action(6), Some(20));
        assert_eq!(tree.action(0), None, "the root");
        assert_eq!(tree.action(1), None, "a submenu");
        assert_eq!(tree.action(3), None, "a separator");
        assert_eq!(tree.action(4), None, "a disabled entry");
        assert_eq!(tree.action(9), None, "an unknown id");
    }

    #[test]
    fn the_revision_changes_only_with_the_entries() {
        let mut tree = Tree::new(menu());
        let revision = tree.revision;
        assert!(!tree.set(menu()));
        assert_eq!(tree.revision, revision);

        assert!(tree.set(vec![AppMenuItem::new(1, "Only")]));
        assert_ne!(tree.revision, revision);
        assert_eq!(tree.action(1), Some(1));
    }
}
