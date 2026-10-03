use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_DRAWER_WIDTH: f32 = 220.0;
pub const MIN_DRAWER_WIDTH: f32 = 160.0;
pub const MAX_DRAWER_WIDTH: f32 = 420.0;
pub const DRAWER_HEADER_HEIGHT: f32 = 48.0;
pub const DRAWER_ROW_TOP: f32 = 51.0;
pub const DRAWER_ROW_HEIGHT: f32 = 42.0;
pub const DRAWER_ROW_STRIDE: f32 = 48.0;
pub const DRAWER_RESIZE_HIT_HALF_WIDTH: f32 = 4.0;
pub const DRAWER_ADD_HIT_WIDTH: f32 = 44.0;

const DEFAULT_WORKSPACE_NAME: &str = "Main";
const PERSISTED_STATE_VERSION: u32 = 1;
const MAX_RESTORED_TABS: usize = 28;

fn standardized_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut standardized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    standardized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    standardized.pop();
                } else if !path.is_absolute() {
                    standardized.push(component.as_os_str());
                }
            }
            _ => standardized.push(component.as_os_str()),
        }
    }
    standardized
}

/// Match simple-cmux's tab label: once OSC 7 reports a directory, show its
/// last path component and keep the full path only for roots or other paths
/// without a terminal component.
pub(crate) fn tab_title_for_directory(directory: &Path) -> String {
    let directory = standardized_path(directory);
    directory
        .file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| directory.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Match simple-cmux's workspace label. A workspace rooted under the user's
/// home is named for the first directory below home; other paths use their
/// last component. The caller supplies the first tab's directory.
pub(crate) fn workspace_title_for_directory(
    directory: &Path,
    home: Option<&Path>,
) -> String {
    let directory = standardized_path(directory);
    let home = home.map(standardized_path);
    if let Some(relative) = home
        .as_deref()
        .and_then(|home| directory.strip_prefix(home).ok())
    {
        if relative.as_os_str().is_empty() {
            return String::from("~");
        }
        if let Some(component) = relative.components().next() {
            return component.as_os_str().to_string_lossy().into_owned();
        }
    }

    tab_title_for_directory(&directory)
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub name: String,
    pub tabs: Vec<usize>,
    selected_tab: usize,
}

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    workspaces: Vec<Workspace>,
    active: usize,
    drawer_width: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedWorkspaceState {
    pub version: u32,
    pub active_workspace: usize,
    pub workspaces: Vec<PersistedWorkspace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedWorkspace {
    pub name: String,
    pub selected_tab: usize,
    pub tabs: Vec<PersistedTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PersistedTab {
    pub title: String,
    pub current_directory: Option<String>,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            workspaces: vec![Workspace {
                name: DEFAULT_WORKSPACE_NAME.to_string(),
                tabs: vec![0],
                selected_tab: 0,
            }],
            active: 0,
            drawer_width: DEFAULT_DRAWER_WIDTH,
        }
    }

    pub fn load() -> Option<(Self, Vec<PersistedTab>)> {
        let data = fs::read(Self::state_path()).ok()?;
        let state = serde_json::from_slice::<PersistedWorkspaceState>(&data).ok()?;
        Self::from_persisted(state)
    }

    fn from_persisted(
        state: PersistedWorkspaceState,
    ) -> Option<(Self, Vec<PersistedTab>)> {
        if state.version != PERSISTED_STATE_VERSION
            || state.workspaces.is_empty()
            || state
                .workspaces
                .iter()
                .any(|workspace| workspace.tabs.is_empty())
        {
            return None;
        }

        let tab_count: usize = state
            .workspaces
            .iter()
            .map(|workspace| workspace.tabs.len())
            .sum();
        if tab_count == 0 || tab_count > MAX_RESTORED_TABS {
            return None;
        }

        let workspace_count = state.workspaces.len();
        let active_workspace = state.active_workspace.min(workspace_count - 1);
        let mut tabs = Vec::with_capacity(tab_count);
        let mut workspaces = Vec::with_capacity(workspace_count);
        for persisted in state.workspaces {
            let first_tab = tabs.len();
            let tab_len = persisted.tabs.len();
            tabs.extend(persisted.tabs);
            let selected_tab = first_tab + persisted.selected_tab.min(tab_len - 1);
            workspaces.push(Workspace {
                name: persisted.name,
                tabs: (first_tab..first_tab + tab_len).collect(),
                selected_tab,
            });
        }

        Some((
            Self {
                workspaces,
                active: active_workspace,
                drawer_width: DEFAULT_DRAWER_WIDTH,
            },
            tabs,
        ))
    }

    pub fn snapshot<F>(&self, mut tab: F) -> PersistedWorkspaceState
    where
        F: FnMut(usize) -> PersistedTab,
    {
        PersistedWorkspaceState {
            version: PERSISTED_STATE_VERSION,
            active_workspace: self.active,
            workspaces: self
                .workspaces
                .iter()
                .map(|workspace| PersistedWorkspace {
                    name: workspace.name.clone(),
                    selected_tab: workspace
                        .tabs
                        .iter()
                        .position(|&tab_index| tab_index == workspace.selected_tab)
                        .unwrap_or(0),
                    tabs: workspace.tabs.iter().copied().map(&mut tab).collect(),
                })
                .collect(),
        }
    }

    pub fn save_snapshot(state: &PersistedWorkspaceState) -> bool {
        let data = match serde_json::to_vec(state) {
            Ok(data) => data,
            Err(error) => {
                tracing::warn!("could not serialize workspace state: {error}");
                return false;
            }
        };

        let path = Self::state_path();
        let Some(directory) = path.parent() else {
            return false;
        };
        if let Err(error) = fs::create_dir_all(directory) {
            tracing::warn!("could not create workspace state directory: {error}");
            return false;
        }

        let temporary_path = path.with_extension("json.tmp");
        if let Err(error) = fs::write(&temporary_path, data) {
            tracing::warn!("could not write workspace state: {error}");
            return false;
        }
        if let Err(error) = fs::rename(&temporary_path, &path) {
            tracing::warn!("could not replace workspace state: {error}");
            let _ = fs::remove_file(temporary_path);
            return false;
        }
        true
    }

    fn state_path() -> PathBuf {
        dirs::data_local_dir()
            .or_else(dirs::data_dir)
            .unwrap_or_else(|| Path::new(".").to_path_buf())
            .join("rio")
            .join("workspaces.json")
    }

    #[inline]
    pub fn drawer_width(&self) -> f32 {
        self.drawer_width
    }

    #[inline]
    pub fn set_drawer_width(&mut self, width: f32) {
        self.drawer_width = width.clamp(MIN_DRAWER_WIDTH, MAX_DRAWER_WIDTH);
    }

    #[inline]
    pub fn active(&self) -> usize {
        self.active
    }

    pub fn selected_tab_for_active(&self) -> Option<usize> {
        self.workspaces
            .get(self.active)
            .map(|workspace| workspace.selected_tab)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.workspaces.len()
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<&Workspace> {
        self.workspaces.get(index)
    }

    pub fn create(&mut self) -> usize {
        let number = self.workspaces.len() + 1;
        self.workspaces.push(Workspace {
            name: format!("Workspace {number}"),
            tabs: Vec::new(),
            selected_tab: 0,
        });
        self.active = self.workspaces.len() - 1;
        self.active
    }

    pub fn select(&mut self, index: usize) -> Option<usize> {
        let workspace = self.workspaces.get(index)?;
        let tab = workspace
            .tabs
            .iter()
            .copied()
            .find(|&tab| tab == workspace.selected_tab)
            .or_else(|| workspace.tabs.first().copied())?;
        self.active = index;
        Some(tab)
    }

    pub fn workspace_for_tab(&self, tab_index: usize) -> Option<usize> {
        self.workspaces
            .iter()
            .position(|workspace| workspace.tabs.contains(&tab_index))
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.workspaces.len() {
            self.active = index;
        }
    }

    pub fn select_tab(&mut self, tab_index: usize) {
        if let Some(workspace) = self
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.tabs.contains(&tab_index))
        {
            workspace.selected_tab = tab_index;
        }
    }

    pub fn add_tab(&mut self, tab_index: usize) {
        if let Some(workspace) = self.workspaces.get_mut(self.active) {
            workspace.tabs.push(tab_index);
            if workspace.tabs.len() == 1 {
                workspace.selected_tab = tab_index;
            }
        }
    }

    pub fn remove_tab(&mut self, removed: usize) {
        for workspace in &mut self.workspaces {
            workspace.tabs.retain(|&tab| tab != removed);
            for tab in &mut workspace.tabs {
                if *tab > removed {
                    *tab -= 1;
                }
            }
            if workspace.selected_tab == removed {
                workspace.selected_tab = workspace.tabs.first().copied().unwrap_or(0);
            } else if workspace.selected_tab > removed {
                workspace.selected_tab -= 1;
            }
        }

        // A workspace is a useful container only while it has a tab. Keep
        // the last workspace alive so Rio always has somewhere to create a
        // new terminal.
        if self.workspaces.len() > 1 {
            self.workspaces
                .retain(|workspace| !workspace.tabs.is_empty());
        }
        if self.workspaces.is_empty() {
            self.workspaces.push(Workspace {
                name: DEFAULT_WORKSPACE_NAME.to_string(),
                tabs: Vec::new(),
                selected_tab: 0,
            });
        }
        self.active = self.active.min(self.workspaces.len() - 1);
    }

    pub fn swap_tabs(&mut self, first: usize, second: usize) {
        for workspace in &mut self.workspaces {
            if workspace.selected_tab == first {
                workspace.selected_tab = second;
            } else if workspace.selected_tab == second {
                workspace.selected_tab = first;
            }
            for tab in &mut workspace.tabs {
                if *tab == first {
                    *tab = second;
                } else if *tab == second {
                    *tab = first;
                }
            }
            workspace.tabs.sort_unstable();
        }
    }

    pub fn move_tab(&mut self, from: usize, to: usize) {
        for workspace in &mut self.workspaces {
            workspace.selected_tab =
                Self::remap_tab_index(workspace.selected_tab, from, to);
            for tab in &mut workspace.tabs {
                *tab = Self::remap_tab_index(*tab, from, to);
            }
            workspace.tabs.sort_unstable();
        }
    }

    fn remap_tab_index(index: usize, from: usize, to: usize) -> usize {
        if index == from {
            to
        } else if from < to && index > from && index <= to {
            index - 1
        } else if to < from && index >= to && index < from {
            index + 1
        } else {
            index
        }
    }

    pub fn tab_count(&self, index: usize) -> usize {
        self.workspaces
            .get(index)
            .map_or(0, |workspace| workspace.tabs.len())
    }

    pub fn tab_indices(&self, index: usize) -> &[usize] {
        self.workspaces
            .get(index)
            .map_or(&[], |workspace| workspace.tabs.as_slice())
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        standardized_path, tab_title_for_directory, workspace_title_for_directory,
        WorkspaceManager,
    };
    use std::path::{Path, MAIN_SEPARATOR_STR};

    #[test]
    fn tab_titles_use_the_last_pwd_component() {
        assert_eq!(
            tab_title_for_directory(Path::new("/Users/aa/os/rio")),
            "rio"
        );
        assert_eq!(
            tab_title_for_directory(Path::new(MAIN_SEPARATOR_STR)),
            MAIN_SEPARATOR_STR
        );
        assert_eq!(
            tab_title_for_directory(Path::new("/Users/aa/os/../rio")),
            "rio"
        );
        assert_eq!(
            standardized_path(Path::new("../../rio")),
            Path::new("../../rio")
        );
    }

    #[test]
    fn workspace_titles_use_the_first_directory_below_home() {
        let home = Path::new("/Users/aa");
        assert_eq!(
            workspace_title_for_directory(Path::new("/Users/aa/os/rio"), Some(home)),
            "os"
        );
        assert_eq!(
            workspace_title_for_directory(Path::new("/Users/aa"), Some(home)),
            "~"
        );
        assert_eq!(
            workspace_title_for_directory(Path::new("/tmp/rio"), Some(home)),
            "rio"
        );
        assert_eq!(
            workspace_title_for_directory(
                Path::new("/Users/aa/os/../rio/project"),
                Some(home)
            ),
            "rio"
        );
    }

    #[test]
    fn removing_a_tab_keeps_workspace_indices_consistent() {
        let mut manager = WorkspaceManager::new();
        manager.add_tab(1);
        manager.remove_tab(0);
        assert_eq!(manager.tab_indices(0), &[0]);
    }

    #[test]
    fn new_workspaces_are_selected_and_can_receive_a_tab() {
        let mut manager = WorkspaceManager::new();
        let workspace = manager.create();
        manager.add_tab(1);
        assert_eq!(manager.active(), workspace);
        assert_eq!(manager.tab_indices(workspace), &[1]);
    }

    #[test]
    fn tabs_are_kept_in_their_workspace() {
        let mut manager = WorkspaceManager::new();
        manager.add_tab(1);
        let second = manager.create();
        manager.add_tab(2);
        manager.add_tab(3);

        assert_eq!(manager.tab_indices(0), &[0, 1]);
        assert_eq!(manager.tab_indices(second), &[2, 3]);

        manager.set_active(0);
        manager.add_tab(4);
        assert_eq!(manager.tab_indices(0), &[0, 1, 4]);
        assert_eq!(manager.tab_indices(second), &[2, 3]);
    }

    #[test]
    fn moving_a_tab_preserves_membership_and_selection() {
        let mut manager = WorkspaceManager::new();
        manager.add_tab(1);
        manager.add_tab(2);
        manager.select_tab(0);
        manager.move_tab(0, 2);
        assert_eq!(manager.tab_indices(0), &[0, 1, 2]);
        assert_eq!(manager.select(0), Some(2));
    }
}
