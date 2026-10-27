use serde::{Deserialize, Serialize};

use super::{NavigationHistoryMode, Route};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NavigationJournal {
    entries: Vec<Route>,
    current_index: Option<usize>,
    max_entries: usize,
}

impl NavigationJournal {
    pub fn new(max_entries: usize) -> Self {
        assert!(
            max_entries > 0,
            "maximum journal entries must be greater than 0"
        );
        Self {
            entries: Vec::new(),
            current_index: None,
            max_entries,
        }
    }

    pub fn record(&mut self, route: Route, mode: NavigationHistoryMode) {
        match mode {
            NavigationHistoryMode::None => {}
            NavigationHistoryMode::Reset => {
                self.entries.clear();
                self.entries.push(route);
                self.current_index = Some(0);
            }
            NavigationHistoryMode::Replace => {
                if let Some(index) = self.current_index {
                    self.entries[index] = route;
                } else {
                    self.push(route);
                }
            }
            NavigationHistoryMode::Push => {
                self.push(route);
            }
        }
    }

    pub fn try_go_back(&mut self) -> Option<Route> {
        if !self.can_go_back() {
            return None;
        }

        let current_index = self.current_index?;
        let next_index = current_index.checked_sub(1)?;
        self.current_index = Some(next_index);
        self.entries.get(next_index).cloned()
    }

    pub fn try_go_forward(&mut self) -> Option<Route> {
        if !self.can_go_forward() {
            return None;
        }

        let next_index = self.current_index? + 1;
        self.current_index = Some(next_index);
        self.entries.get(next_index).cloned()
    }

    pub fn can_go_back(&self) -> bool {
        self.current_index.is_some_and(|index| index > 0)
    }

    pub fn can_go_forward(&self) -> bool {
        self.current_index
            .is_some_and(|index| index + 1 < self.entries.len())
    }

    pub fn current(&self) -> Option<&Route> {
        self.current_index.and_then(|index| self.entries.get(index))
    }

    pub fn previous(&self) -> Option<&Route> {
        self.current_index
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| self.entries.get(index))
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn iter(&self) -> impl Iterator<Item = &Route> {
        self.entries.iter()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_index = None;
    }

    fn ensure_capacity_before_adding(&mut self) {
        if self.entries.len() < self.max_entries {
            return;
        }

        self.entries.remove(0);
        self.current_index = self.current_index.and_then(|index| index.checked_sub(1));
    }

    fn push(&mut self, route: Route) {
        if self
            .current()
            .is_some_and(|current| current.uri == route.uri)
        {
            return;
        }

        if let Some(current_index) = self.current_index
            && current_index + 1 < self.entries.len()
        {
            self.entries.truncate(current_index + 1);
        }

        self.ensure_capacity_before_adding();
        self.entries.push(route);
        self.current_index = Some(self.entries.len() - 1);
    }
}

impl Default for NavigationJournal {
    fn default() -> Self {
        Self::new(40)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NavigationRegion {
    pub name: String,
    pub journal: NavigationJournal,
    pub current: Option<super::RoutingData>,
}

impl NavigationRegion {
    pub fn new(name: impl Into<String>, max_journal_entries: usize) -> Self {
        Self {
            name: name.into(),
            journal: NavigationJournal::new(max_journal_entries),
            current: None,
        }
    }

    pub fn current_route(&self) -> Option<&Route> {
        self.current.as_ref().map(|data| &data.route)
    }
}
