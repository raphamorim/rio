use rio_backend::config::Shell;

/// State for shell profile selection mode
pub struct ShellSelector {
    /// Whether the selector is active
    active: bool,
    /// Available shell profiles (default shell + additional shells)
    profiles: Vec<Shell>,
    /// Currently selected index
    selected_index: usize,
}

impl ShellSelector {
    /// Create a new shell selector with the default shell and additional profiles
    pub fn new(default_shell: Shell, additional_shells: Vec<Shell>) -> Self {
        let mut profiles = vec![default_shell];
        profiles.extend(additional_shells);

        Self {
            active: false,
            profiles,
            selected_index: 0,
        }
    }

    /// Check if the selector is currently active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Start the selector (show overlay)
    pub fn start(&mut self) {
        self.active = true;
        self.selected_index = 0;
    }

    /// Stop the selector (hide overlay)
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Navigate to the next profile in the list
    pub fn select_next(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.profiles.len();
    }

    /// Navigate to the previous profile in the list
    pub fn select_previous(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else {
            self.selected_index = self.profiles.len() - 1;
        }
    }

    /// Select a profile by index directly
    pub fn select_by_index(&mut self, index: usize) -> bool {
        if index < self.profiles.len() {
            self.selected_index = index;
            true
        } else {
            false
        }
    }

    /// Get the currently selected shell profile
    pub fn selected_profile(&self) -> Option<&Shell> {
        self.profiles.get(self.selected_index)
    }

    /// Get all profiles with their display info
    /// Returns (index, shell_ref, is_selected)
    pub fn profiles_for_display(&self) -> Vec<(usize, &Shell, bool)> {
        self.profiles
            .iter()
            .enumerate()
            .map(|(i, shell)| (i, shell, i == self.selected_index))
            .collect()
    }

    /// Get the display name for a shell (uses name field if present, otherwise program path)
    pub fn display_name(shell: &Shell) -> &str {
        shell.name.as_deref().unwrap_or(&shell.program)
    }
}
