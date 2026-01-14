//! Interaction system for Canvas shapes
//!
//! Enables mouse/touch events on custom-drawn shapes

use super::cpu::hit_test;
use super::display_list::DisplayList;
use super::types::{InteractiveId, Path, Point, Rect};
use crate::{AureaError, AureaResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
        let mut callbacks = self.click_callbacks.lock().unwrap();
        callbacks.insert(id, callback);
    }

    /// Register a hover callback
    pub fn register_hover(&self, id: InteractiveId, callback: HoverCallback) {
        let mut callbacks = self.hover_callbacks.lock().unwrap();
        callbacks.insert(id, callback);
    }

    /// Unregister callbacks for an ID
    pub fn unregister(&self, id: InteractiveId) {
        let mut click_callbacks = self.click_callbacks.lock().unwrap();
        click_callbacks.remove(&id);

        let mut hover_callbacks = self.hover_callbacks.lock().unwrap();
        hover_callbacks.remove(&id);

        let mut hover_state = self.hover_state.lock().unwrap();
        hover_state.remove(&id);
    }

    /// Handle a click event at a point
    pub fn handle_click(&self, display_list: &DisplayList, point: Point) -> AureaResult<()> {
        // Query display list in reverse order (top-to-bottom)
        let items = display_list.items();

        for item in items.iter().rev() {
            if let Some(interactive_id) = item.interactive_id {
                // Quick bounds check first
                if !hit_test::hit_test_rect(item.bounds, point) {
                    continue;
                }

                // Hit test based on command type
                let hit = match &item.command {
                    super::renderer::DrawCommand::DrawRect(rect, _) => {
                        hit_test::hit_test_rect(*rect, point)
                    }
                    super::renderer::DrawCommand::DrawCircle(center, radius, _) => {
                        hit_test::hit_test_circle(*center, *radius, point)
                    }
                    super::renderer::DrawCommand::DrawPath(path, _) => {
                        hit_test::hit_test_path(path, point)
                    }
                    _ => false,
                };

                if hit {
                    // Found a hit, invoke callback
                    let callbacks = self.click_callbacks.lock().unwrap();
                    if let Some(callback) = callbacks.get(&interactive_id) {
                        callback(point)?;
                    }
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    /// Handle a hover event at a point
    pub fn handle_hover(&self, display_list: &DisplayList, point: Point) -> AureaResult<()> {
        let items = display_list.items();
        let mut current_hovered = HashMap::new();

        // Check all interactive items
        for item in items.iter().rev() {
            if let Some(interactive_id) = item.interactive_id {
                // Quick bounds check first
                if !hit_test::hit_test_rect(item.bounds, point) {
                    continue;
                }

                // Hit test based on command type
                let hit = match &item.command {
                    super::renderer::DrawCommand::DrawRect(rect, _) => {
                        hit_test::hit_test_rect(*rect, point)
                    }
                    super::renderer::DrawCommand::DrawCircle(center, radius, _) => {
                        hit_test::hit_test_circle(*center, *radius, point)
                    }
                    super::renderer::DrawCommand::DrawPath(path, _) => {
                        hit_test::hit_test_path(path, point)
                    }
                    _ => false,
                };

                if hit {
                    current_hovered.insert(interactive_id, true);
                }
            }
        }

        // Check for hover state changes
        let mut hover_state = self.hover_state.lock().unwrap();
        let hover_callbacks = self.hover_callbacks.lock().unwrap();

        // Check for new hovers
        for (id, _) in &current_hovered {
            if !hover_state.get(id).copied().unwrap_or(false) {
                // Entered
                if let Some(callback) = hover_callbacks.get(id) {
                    callback(point, true)?;
                }
                hover_state.insert(*id, true);
            }
        }

        // Check for exited hovers
        let previous_hovered: Vec<InteractiveId> = hover_state.keys().copied().collect();
        for id in previous_hovered {
            if !current_hovered.contains_key(&id) {
                // Exited
                if let Some(callback) = hover_callbacks.get(&id) {
                    callback(point, false)?;
                }
                hover_state.remove(&id);
            }
        }

        Ok(())
    }
}

impl Default for InteractionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
