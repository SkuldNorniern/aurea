//! Interaction system for Canvas shapes
//!
//! Enables mouse/touch events on custom-drawn shapes

use crate::command::DrawCommand;
use crate::cpu::hit_test;
use crate::display_list::{DisplayItem, DisplayList};
use crate::types::{InteractiveId, Point};
use aurea_foundation::{lock, AureaResult};
use std::collections::HashMap;
use std::sync::Mutex;

/// Callback for click events
pub type ClickCallback = Box<dyn Fn(Point) -> AureaResult<()> + Send + Sync>;

/// Callback for hover events (point, entered)
pub type HoverCallback = Box<dyn Fn(Point, bool) -> AureaResult<()> + Send + Sync>;

/// Registry for interactive shape callbacks
pub struct InteractionRegistry {
    click_callbacks: Mutex<HashMap<InteractiveId, ClickCallback>>,
    hover_callbacks: Mutex<HashMap<InteractiveId, HoverCallback>>,
    hover_state: Mutex<HashMap<InteractiveId, bool>>, // Track current hover state
}

impl InteractionRegistry {
    pub fn new() -> Self {
        Self {
            click_callbacks: Mutex::new(HashMap::new()),
            hover_callbacks: Mutex::new(HashMap::new()),
            hover_state: Mutex::new(HashMap::new()),
        }
    }

    /// Register a click callback
    pub fn register_click(&self, id: InteractiveId, callback: ClickCallback) {
        let mut callbacks = lock(&self.click_callbacks);
        callbacks.insert(id, callback);
    }

    /// Register a hover callback
    pub fn register_hover(&self, id: InteractiveId, callback: HoverCallback) {
        let mut callbacks = lock(&self.hover_callbacks);
        callbacks.insert(id, callback);
    }

    /// Unregister callbacks for an ID
    pub fn unregister(&self, id: InteractiveId) {
        let mut click_callbacks = lock(&self.click_callbacks);
        click_callbacks.remove(&id);

        let mut hover_callbacks = lock(&self.hover_callbacks);
        hover_callbacks.remove(&id);

        let mut hover_state = lock(&self.hover_state);
        hover_state.remove(&id);
    }

    /// Handle a click event at a point
    pub fn handle_click(&self, display_list: &DisplayList, point: Point) -> AureaResult<()> {
        // Query display list in reverse order (top-to-bottom)
        let items = display_list.items();

        for item in items.iter().rev() {
            if let Some(interactive_id) = item.interactive_id {
                if !item_hit(item, point) {
                    continue;
                }

                // Found a hit, invoke callback
                let callbacks = lock(&self.click_callbacks);
                if let Some(callback) = callbacks.get(&interactive_id) {
                    callback(point)?;
                }
                return Ok(());
            }
        }

        Ok(())
    }

    /// Handle a hover event at a point
    pub fn handle_hover(&self, display_list: &DisplayList, point: Point) -> AureaResult<()> {
        let current_hovered = self.hovered_ids(display_list, point);
        self.dispatch_hover_changes(point, &current_hovered)
    }

    fn hovered_ids(
        &self,
        display_list: &DisplayList,
        point: Point,
    ) -> HashMap<InteractiveId, bool> {
        let items = display_list.items();
        let mut current_hovered = HashMap::new();

        for item in items.iter().rev() {
            if let Some(interactive_id) = item.interactive_id.filter(|_| item_hit(item, point)) {
                current_hovered.insert(interactive_id, true);
            }
        }

        current_hovered
    }

    fn dispatch_hover_changes(
        &self,
        point: Point,
        current_hovered: &HashMap<InteractiveId, bool>,
    ) -> AureaResult<()> {
        let mut hover_state = lock(&self.hover_state);
        let hover_callbacks = lock(&self.hover_callbacks);

        for id in current_hovered.keys() {
            if !hover_state.get(id).copied().unwrap_or(false) {
                if let Some(callback) = hover_callbacks.get(id) {
                    callback(point, true)?;
                }
                hover_state.insert(*id, true);
            }
        }

        let previous_hovered: Vec<InteractiveId> = hover_state.keys().copied().collect();
        for id in previous_hovered {
            if !current_hovered.contains_key(&id) {
                if let Some(callback) = hover_callbacks.get(&id) {
                    callback(point, false)?;
                }
                hover_state.remove(&id);
            }
        }

        Ok(())
    }
}

fn item_hit(item: &DisplayItem, point: Point) -> bool {
    if !hit_test::hit_test_rect(item.bounds, point) {
        return false;
    }

    match &item.command {
        DrawCommand::DrawRect(rect, _) => hit_test::hit_test_rect(*rect, point),
        DrawCommand::DrawCircle(center, radius, _) => {
            hit_test::hit_test_circle(*center, *radius, point)
        }
        DrawCommand::DrawPath(path, _) => hit_test::hit_test_path(path, point),
        _ => false,
    }
}

impl Default for InteractionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
