type TabId = u8;
const DEFAULT_TABS_CAPACITY: usize = 10;

#[derive(Clone)]
pub struct Tab {
    pub id: TabId,
}

pub type Tabs = Vec<Tab>;

#[derive(Clone)]
pub struct TabsControl {
    tabs: Tabs,
    current: TabId,
    capacity: usize,
}

impl TabsControl {
    pub fn new() -> Self {
        let initial_tab = Tab { id: 0 };
        TabsControl {
            current: initial_tab.id,
            tabs: vec![initial_tab],
            capacity: DEFAULT_TABS_CAPACITY,
        }
    }

    #[allow(unused)]
    pub fn with_capacity(capacity: usize) -> Self {
        let initial_tab = Tab { id: 0 };
        TabsControl {
            current: initial_tab.id,
            tabs: vec![initial_tab],
            capacity,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    #[inline]
    pub fn tabs(&self) -> &Tabs {
        &self.tabs
    }

    #[cfg(test)]
    pub fn increase_capacity(&mut self, inc_val: usize) {
        self.capacity += inc_val;
    }

    #[inline]
    #[allow(unused)]
    pub fn set_current(&mut self, tab_id: u8) {
        if self.contains(tab_id) {
            self.current = tab_id;
        }
    }

    #[inline]
    #[allow(unused)]
    pub fn contains(&self, tab_id: u8) -> bool {
        self.tabs.iter().any(|i| i.id == tab_id)
    }

    #[inline]
    pub fn position(&self, tab_id: u8) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == tab_id)
    }

    #[inline]
    pub fn close_tab(&mut self, tab_id: u8) {
        if self.tabs.len() <= 1 {
            return;
        }

        if let Some(idx) = self.position(tab_id) {
            let mut reset_current = false;
            if self.tabs[idx].id == self.current {
                reset_current = true;
            }
            self.tabs.remove(idx);

            if reset_current {
                if let Some(first_tab) = self.tabs.first() {
                    self.current = first_tab.id;
                }
            }
        }
    }

    #[inline]
    pub fn current(&self) -> u8 {
        self.current
    }

    #[inline]
    pub fn switch_to_next(&mut self) {
        if let Some(current_position) = self.position(self.current) {
            let (left, right) = self.tabs.split_at(current_position + 1);
            if !right.is_empty() {
                self.current = right[0].id;
            } else {
                self.current = left[0].id;
            }
        }
    }

    #[inline]
    pub fn add_tab(&mut self, redirect: bool) {
        let size = self.tabs.len();
        if size < self.capacity {
            let last_tab: &Tab = &self.tabs[size - 1];
            let new_tab_id = last_tab.id + 1;
            self.tabs.push(Tab { id: new_tab_id });
            if redirect {
                self.current = new_tab_id;
            }
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_capacity() {
        let tabs_control = TabsControl::new();
        assert_eq!(tabs_control.capacity, DEFAULT_TABS_CAPACITY);

        let tabs_control = TabsControl::with_capacity(5);
        assert_eq!(tabs_control.capacity, 5);

        let mut tabs_control = TabsControl::with_capacity(5);
        tabs_control.increase_capacity(3);
        assert_eq!(tabs_control.capacity, 8);
    }

    #[test]
    fn test_add_tab() {
        let mut tabs_control = TabsControl::with_capacity(5);
        assert_eq!(tabs_control.capacity, 5);
        assert_eq!(tabs_control.current, 0);

        let should_redirect = false;
        tabs_control.add_tab(should_redirect);
        assert_eq!(tabs_control.capacity, 5);
        assert_eq!(tabs_control.current, 0);

        let should_redirect = true;
        tabs_control.add_tab(should_redirect);
        assert_eq!(tabs_control.capacity, 5);
        assert_eq!(tabs_control.current, 2);
    }

    #[test]
    fn test_add_tab_with_capacity_limit() {
        let mut tabs_control = TabsControl::with_capacity(3);
        assert_eq!(tabs_control.capacity, 3);
        assert_eq!(tabs_control.current, 0);
        let should_redirect = false;
        tabs_control.add_tab(should_redirect);
        assert_eq!(tabs_control.len(), 2);
        tabs_control.add_tab(should_redirect);
        assert_eq!(tabs_control.len(), 3);

        for _ in 0..20 {
            tabs_control.add_tab(should_redirect);
        }

        assert_eq!(tabs_control.len(), 3);
        assert_eq!(tabs_control.capacity, 3);
    }

    #[test]
    fn test_set_current() {
        let mut tabs_control = TabsControl::with_capacity(8);
        let should_redirect = true;

        tabs_control.add_tab(should_redirect);
        assert_eq!(tabs_control.current, 1);
        tabs_control.set_current(0);
        assert_eq!(tabs_control.current, 0);
        assert_eq!(tabs_control.len(), 2);
        assert_eq!(tabs_control.capacity, 8);

        tabs_control.set_current(8);
        assert_eq!(tabs_control.current, 0);
        tabs_control.set_current(2);
        assert_eq!(tabs_control.current, 0);

        let should_redirect = false;
        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);
        tabs_control.set_current(3);
        assert_eq!(tabs_control.current, 3);
    }

    #[test]
    fn test_close_tab() {
        let mut tabs_control = TabsControl::with_capacity(3);
        let should_redirect = false;

        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);
        assert_eq!(tabs_control.len(), 3);

        assert_eq!(tabs_control.current, 0);
        tabs_control.set_current(2);
        assert_eq!(tabs_control.current, 2);
        tabs_control.set_current(0);

        tabs_control.close_tab(2);
        tabs_control.set_current(2);
        assert_eq!(tabs_control.current, 0);
        assert_eq!(tabs_control.len(), 2);
    }

    #[test]
    fn test_close_tab_upcoming_ids() {
        let mut tabs_control = TabsControl::with_capacity(5);
        let should_redirect = false;

        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);

        tabs_control.close_tab(0);
        tabs_control.close_tab(1);
        tabs_control.close_tab(2);
        tabs_control.close_tab(3);

        assert_eq!(tabs_control.len(), 1);
        assert_eq!(tabs_control.current, 4);

        tabs_control.add_tab(should_redirect);

        assert_eq!(tabs_control.len(), 2);
        tabs_control.set_current(5);
        assert_eq!(tabs_control.current, 5);
        tabs_control.close_tab(4);
        assert_eq!(tabs_control.len(), 1);
        assert_eq!(tabs_control.current, 5);
    }

    #[test]
    fn test_close_last_tab() {
        let mut tabs_control = TabsControl::with_capacity(2);
        let should_redirect = false;

        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);
        assert_eq!(tabs_control.len(), 2);
        assert_eq!(tabs_control.current, 0);

        tabs_control.close_tab(1);
        assert_eq!(tabs_control.len(), 1);

        // Last tab should not be closed
        tabs_control.close_tab(0);
        assert_eq!(tabs_control.len(), 1);
    }

    #[test]
    fn test_switch_to_next() {
        let mut tabs_control = TabsControl::with_capacity(5);
        let should_redirect = false;

        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);
        tabs_control.add_tab(should_redirect);
        assert_eq!(tabs_control.len(), 5);
        assert_eq!(tabs_control.current, 0);

        tabs_control.switch_to_next();
        assert_eq!(tabs_control.current, 1);
        tabs_control.switch_to_next();
        assert_eq!(tabs_control.current, 2);
        tabs_control.switch_to_next();
        assert_eq!(tabs_control.current, 3);
        tabs_control.switch_to_next();
        assert_eq!(tabs_control.current, 4);
        tabs_control.switch_to_next();
        assert_eq!(tabs_control.current, 0);
        tabs_control.switch_to_next();
        assert_eq!(tabs_control.current, 1);
    }
}
