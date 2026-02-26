#![allow(dead_code)]
use std::collections::HashMap;

use pane_protocol::layout::TabId;
use crate::window::WindowId;

/// Maps internal UUIDs to sequential tmux-style integer IDs.
/// Panes get `%N` IDs and windows (Windows) get `@N` IDs.
pub struct IdMap {
    next_pane: u32,
    next_window: u32,
    pane_map: HashMap<TabId, u32>,
    window_map: HashMap<WindowId, u32>,
    reverse_pane: HashMap<u32, TabId>,
    reverse_window: HashMap<u32, WindowId>,
}

impl IdMap {
    pub fn new() -> Self {
        Self {
            next_pane: 0,
            next_window: 0,
            pane_map: HashMap::new(),
            window_map: HashMap::new(),
            reverse_pane: HashMap::new(),
            reverse_window: HashMap::new(),
        }
    }

    /// Register a pane and return its sequential `%N` style ID.
    /// If already registered, returns the existing ID.
    pub fn register_pane(&mut self, id: TabId) -> u32 {
        if let Some(&n) = self.pane_map.get(&id) {
            return n;
        }
        let n = self.next_pane;
        self.next_pane += 1;
        self.pane_map.insert(id, n);
        self.reverse_pane.insert(n, id);
        n
    }

    /// Register a window (Window) and return its sequential `@N` style ID.
    /// If already registered, returns the existing ID.
    pub fn register_window(&mut self, id: WindowId) -> u32 {
        if let Some(&n) = self.window_map.get(&id) {
            return n;
        }
        let n = self.next_window;
        self.next_window += 1;
        self.window_map.insert(id, n);
        self.reverse_window.insert(n, id);
        n
    }

    /// Look up a pane UUID by its sequential ID.
    pub fn pane_id(&self, n: u32) -> Option<TabId> {
        self.reverse_pane.get(&n).copied()
    }

    /// Look up a window UUID by its sequential ID.
    pub fn window_id(&self, n: u32) -> Option<WindowId> {
        self.reverse_window.get(&n).copied()
    }

    /// Get the sequential ID for a pane UUID, if registered.
    pub fn pane_number(&self, id: &TabId) -> Option<u32> {
        self.pane_map.get(id).copied()
    }

    /// Get the sequential ID for a window UUID, if registered.
    pub fn window_number(&self, id: &WindowId) -> Option<u32> {
        self.window_map.get(id).copied()
    }

    /// Remove a pane from the map.
    pub fn unregister_pane(&mut self, id: &TabId) {
        if let Some(n) = self.pane_map.remove(id) {
            self.reverse_pane.remove(&n);
        }
    }

    /// Remove a window from the map.
    pub fn unregister_window(&mut self, id: &WindowId) {
        if let Some(n) = self.window_map.remove(id) {
            self.reverse_window.remove(&n);
        }
    }
}

impl Default for IdMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_pane() {
        let mut map = IdMap::new();
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();

        assert_eq!(map.register_pane(id1), 0);
        assert_eq!(map.register_pane(id2), 1);
    }

    #[test]
    fn test_register_pane_idempotent() {
        let mut map = IdMap::new();
        let id = TabId::new_v4();

        assert_eq!(map.register_pane(id), 0);
        assert_eq!(map.register_pane(id), 0); // Same ID returns same number
    }

    #[test]
    fn test_register_window() {
        let mut map = IdMap::new();
        let id1 = WindowId::new_v4();
        let id2 = WindowId::new_v4();

        assert_eq!(map.register_window(id1), 0);
        assert_eq!(map.register_window(id2), 1);
    }

    #[test]
    fn test_register_window_idempotent() {
        let mut map = IdMap::new();
        let id = WindowId::new_v4();

        assert_eq!(map.register_window(id), 0);
        assert_eq!(map.register_window(id), 0);
    }

    #[test]
    fn test_pane_and_window_independent_counters() {
        let mut map = IdMap::new();
        let pane_id = TabId::new_v4();
        let window_id = WindowId::new_v4();

        // Both start at 0 since they have independent counters
        assert_eq!(map.register_pane(pane_id), 0);
        assert_eq!(map.register_window(window_id), 0);
    }

    #[test]
    fn test_pane_id_lookup() {
        let mut map = IdMap::new();
        let id = TabId::new_v4();
        let n = map.register_pane(id);

        assert_eq!(map.pane_id(n), Some(id));
        assert_eq!(map.pane_id(999), None);
    }

    #[test]
    fn test_window_id_lookup() {
        let mut map = IdMap::new();
        let id = WindowId::new_v4();
        let n = map.register_window(id);

        assert_eq!(map.window_id(n), Some(id));
        assert_eq!(map.window_id(999), None);
    }

    #[test]
    fn test_pane_number_lookup() {
        let mut map = IdMap::new();
        let id = TabId::new_v4();
        let unknown = TabId::new_v4();
        map.register_pane(id);

        assert_eq!(map.pane_number(&id), Some(0));
        assert_eq!(map.pane_number(&unknown), None);
    }

    #[test]
    fn test_window_number_lookup() {
        let mut map = IdMap::new();
        let id = WindowId::new_v4();
        let unknown = WindowId::new_v4();
        map.register_window(id);

        assert_eq!(map.window_number(&id), Some(0));
        assert_eq!(map.window_number(&unknown), None);
    }

    #[test]
    fn test_unregister_pane() {
        let mut map = IdMap::new();
        let id = TabId::new_v4();
        let n = map.register_pane(id);

        map.unregister_pane(&id);
        assert_eq!(map.pane_id(n), None);
        assert_eq!(map.pane_number(&id), None);
    }

    #[test]
    fn test_unregister_window() {
        let mut map = IdMap::new();
        let id = WindowId::new_v4();
        let n = map.register_window(id);

        map.unregister_window(&id);
        assert_eq!(map.window_id(n), None);
        assert_eq!(map.window_number(&id), None);
    }

    #[test]
    fn test_unregister_nonexistent_is_noop() {
        let mut map = IdMap::new();
        let id = TabId::new_v4();
        map.unregister_pane(&id); // Should not panic
    }

    #[test]
    fn test_sequential_ids_after_unregister() {
        let mut map = IdMap::new();
        let id1 = TabId::new_v4();
        let id2 = TabId::new_v4();
        let id3 = TabId::new_v4();

        assert_eq!(map.register_pane(id1), 0);
        assert_eq!(map.register_pane(id2), 1);
        map.unregister_pane(&id1);
        // id3 still gets the next sequential number (IDs are not reused)
        assert_eq!(map.register_pane(id3), 2);
    }

    #[test]
    fn test_many_registrations() {
        let mut map = IdMap::new();
        let ids: Vec<TabId> = (0..100).map(|_| TabId::new_v4()).collect();
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(map.register_pane(*id), i as u32);
        }
        // Verify reverse lookups
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(map.pane_id(i as u32), Some(*id));
        }
    }
}
