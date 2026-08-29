//! Interaction system for Canvas shapes
//!
//! Enables mouse/touch events on custom-drawn shapes

use crate::command::DrawCommand;
use crate::cpu::hit_test;
use crate::display_list::{DisplayItem, DisplayList};
use crate::types::{InteractiveId, Point};
use aurea_foundation::{AureaResult, lock};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Callback for click events.
///
/// `Arc` so a dispatch can clone it out and drop the registry lock before
/// running it. A callback that registers or unregisters while it runs would
/// otherwise deadlock on the lock that is dispatching it.
pub type ClickCallback = Arc<dyn Fn(Point) -> AureaResult<()> + Send + Sync>;

/// Callback for hover events (point, entered). `Arc` for the same reason as
/// [`ClickCallback`].
pub type HoverCallback = Arc<dyn Fn(Point, bool) -> AureaResult<()> + Send + Sync>;

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

                // Clone the callback out and drop the lock before running it:
                // the callback is free to register or unregister.
                let callback = lock(&self.click_callbacks).get(&interactive_id).cloned();
                if let Some(callback) = callback {
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

    /// Works out which ids entered or left, updates the hover state, then runs
    /// the callbacks with no lock held.
    fn dispatch_hover_changes(
        &self,
        point: Point,
        current_hovered: &HashMap<InteractiveId, bool>,
    ) -> AureaResult<()> {
        let pending: Vec<(HoverCallback, bool)> = {
            let mut hover_state = lock(&self.hover_state);
            let hover_callbacks = lock(&self.hover_callbacks);
            let mut pending = Vec::new();

            for id in current_hovered.keys() {
                if !hover_state.get(id).copied().unwrap_or(false) {
                    if let Some(callback) = hover_callbacks.get(id) {
                        pending.push((Arc::clone(callback), true));
                    }
                    hover_state.insert(*id, true);
                }
            }

            let previous_hovered: Vec<InteractiveId> = hover_state.keys().copied().collect();
            for id in previous_hovered {
                if !current_hovered.contains_key(&id) {
                    if let Some(callback) = hover_callbacks.get(&id) {
                        pending.push((Arc::clone(callback), false));
                    }
                    hover_state.remove(&id);
                }
            }

            pending
        };

        for (callback, entered) in pending {
            callback(point, entered)?;
        }

        Ok(())
    }
}

fn item_hit(item: &DisplayItem, point: Point) -> bool {
    if !hit_test::hit_test_rect(item.bounds, point) {
        return false;
    }

    // What was clipped away was never drawn, and a viewer cannot click on
    // something they cannot see. Rendering has honoured clips for a while;
    // hit testing did not, leaving the trimmed-off part of a shape reacting
    // to clicks from under whatever was covering it.
    if let Some(clip) = item.clip
        && !hit_test::hit_test_rect(clip, point)
    {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::DrawCommand;
    use crate::display_list::{CacheKey, DisplayIndex, DisplayItem};
    use crate::types::{BlendMode, Paint, Rect};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn interactive_rect(id: InteractiveId, rect: Rect) -> DisplayItem {
        DisplayItem::new_interactive(
            DisplayIndex(0),
            CacheKey::from_hash(1),
            rect,
            false,
            id,
            BlendMode::Normal,
            DrawCommand::DrawRect(rect, Paint::new()),
        )
    }

    /// A shape clipped down to part of itself is only interactive where it is
    /// visible: the rest was never drawn.
    #[test]
    fn a_click_outside_the_clip_misses() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 100.0);
        let mut item = interactive_rect(InteractiveId(1), bounds);
        item.clip = Some(Rect::new(0.0, 0.0, 20.0, 100.0));

        assert!(item_hit(&item, Point::new(10.0, 50.0)), "inside the clip");
        assert!(
            !item_hit(&item, Point::new(60.0, 50.0)),
            "clipped away, so not on screen and not clickable"
        );
    }

    /// With no clip the whole shape stays interactive.
    #[test]
    fn an_unclipped_shape_is_interactive_throughout() {
        let bounds = Rect::new(0.0, 0.0, 100.0, 100.0);
        let item = interactive_rect(InteractiveId(1), bounds);

        assert!(item_hit(&item, Point::new(60.0, 50.0)));
    }

    /// A click callback that touches the registry while it runs must not
    /// deadlock: dispatch clones the callback out and drops the lock first.
    #[test]
    fn click_callback_can_reenter_the_registry() {
        let registry = Arc::new(InteractionRegistry::new());
        let inner = Arc::clone(&registry);
        let ran = Arc::new(AtomicUsize::new(0));
        let ran_clone = Arc::clone(&ran);

        registry.register_click(
            InteractiveId(1),
            Arc::new(move |_| {
                // Both of these take the same lock the dispatch was holding.
                inner.register_click(InteractiveId(2), Arc::new(|_| Ok(())));
                inner.unregister(InteractiveId(2));
                ran_clone.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        let mut list = DisplayList::new();
        list.push(interactive_rect(
            InteractiveId(1),
            Rect::new(0.0, 0.0, 10.0, 10.0),
        ));

        registry
            .handle_click(&list, Point::new(5.0, 5.0))
            .expect("handle_click");
        assert_eq!(ran.load(Ordering::Relaxed), 1);
    }

    /// Same for hover, which used to hold two locks while dispatching.
    #[test]
    fn hover_callback_can_reenter_the_registry() {
        let registry = Arc::new(InteractionRegistry::new());
        let inner = Arc::clone(&registry);
        let ran = Arc::new(AtomicUsize::new(0));
        let ran_clone = Arc::clone(&ran);

        registry.register_hover(
            InteractiveId(1),
            Arc::new(move |_, _| {
                inner.register_hover(InteractiveId(2), Arc::new(|_, _| Ok(())));
                inner.unregister(InteractiveId(2));
                ran_clone.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }),
        );

        let mut list = DisplayList::new();
        list.push(interactive_rect(
            InteractiveId(1),
            Rect::new(0.0, 0.0, 10.0, 10.0),
        ));

        registry
            .handle_hover(&list, Point::new(5.0, 5.0))
            .expect("enter");
        registry
            .handle_hover(&list, Point::new(50.0, 50.0))
            .expect("leave");
        assert_eq!(ran.load(Ordering::Relaxed), 2, "enter and leave both ran");
    }
}
