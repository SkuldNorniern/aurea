//! Native accessibility for canvas controls, behind the `accessibility` feature.

mod platform;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::iter;
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

#[cfg(target_os = "windows")]
use accesskit::Affine;
pub use accesskit::{Action, ActionData, Node, Role};
use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, NodeId, Rect as Bounds,
    TreeId, TreeInfo, TreeUpdate,
};
use aurea_foundation::lock;

use crate::ffi::ng_platform_window_is_focused;
use crate::platform::ui_thread;
use crate::render::{Canvas, CanvasId, Rect, request_canvas_redraw};
use crate::ui::frame::Ui;
use crate::ui::state::{Key, Sense, UiState, WidgetId};
use crate::{AureaError, AureaResult};

const ROOT: NodeId = NodeId(1);
static ATTACHED: LazyLock<Mutex<HashSet<CanvasId>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

struct Attachment(CanvasId);
impl Attachment {
    fn new(canvas: CanvasId) -> AureaResult<Self> {
        if !lock(&ATTACHED).insert(canvas) {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(Self(canvas))
    }
}
impl Drop for Attachment {
    fn drop(&mut self) {
        lock(&ATTACHED).remove(&self.0);
    }
}

struct Shared {
    alive: AtomicBool,
    active: AtomicBool,
    requests: Mutex<Vec<ActionRequest>>,
    canvas: CanvasId,
}

#[derive(Clone)]
pub(super) struct Handler(Arc<Shared>);

impl ActivationHandler for Handler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        if !self.0.alive.load(Ordering::Acquire) {
            return None;
        }
        self.0.active.store(true, Ordering::Release);
        request_canvas_redraw(self.0.canvas);
        None
    }
}

impl ActionHandler for Handler {
    fn do_action(&mut self, request: ActionRequest) {
        let mut requests = lock(&self.0.requests);
        if !self.0.alive.load(Ordering::Acquire) {
            return;
        }
        requests.push(request);
        drop(requests);
        request_canvas_redraw(self.0.canvas);
    }
}

impl DeactivationHandler for Handler {
    fn deactivate_accessibility(&mut self) {
        self.0.active.store(false, Ordering::Release);
        request_canvas_redraw(self.0.canvas);
    }
}

struct Entry {
    id: NodeId,
    node: Node,
    seen: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MenuTarget<Id> {
    owner: Key<Id>,
    generation: u64,
    path: Vec<usize>,
}

/// An action requested by an assistive technology for a custom control.
#[derive(Clone, Debug)]
pub struct AccessibleAction {
    pub action: Action,
    pub data: Option<ActionData>,
}

pub(crate) struct Bridge<Id: WidgetId> {
    platform: platform::Adapter,
    canvas: Canvas,
    shared: Arc<Shared>,
    name: String,
    entries: HashMap<Key<Id>, Entry>,
    next_id: u64,
    pending: Vec<(Key<Id>, AccessibleAction)>,
    draw_order: Vec<Key<Id>>,
    menus: HashMap<MenuTarget<Id>, Entry>,
    menu_order: Vec<MenuTarget<Id>>,
    menu_requests: Vec<(MenuTarget<Id>, Action)>,
    menu_focus: Option<NodeId>,
    _attachment: Attachment,
}

impl<Id: WidgetId> fmt::Debug for Bridge<Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessibilityBridge")
            .field("canvas", &self.canvas.id())
            .field("nodes", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl<Id: WidgetId> Bridge<Id> {
    fn new(canvas: &Canvas, name: &str) -> AureaResult<Self> {
        if !ui_thread::is_ui_thread() {
            return Err(AureaError::NotMainThread);
        }
        canvas.ensure_live()?;
        let attachment = Attachment::new(canvas.id())?;
        let shared = Arc::new(Shared {
            alive: AtomicBool::new(true),
            active: AtomicBool::new(false),
            requests: Mutex::new(Vec::new()),
            canvas: canvas.id(),
        });
        let mut platform = platform::Adapter::new(canvas, Handler(shared.clone()))?;
        let window = canvas.window_handle();
        if !window.is_null() {
            platform.focused(unsafe { ng_platform_window_is_focused(window) } != 0);
        }
        Ok(Self {
            platform,
            canvas: canvas.clone(),
            shared,
            name: name.into(),
            entries: HashMap::new(),
            next_id: ROOT.0 + 1,
            pending: Vec::new(),
            draw_order: Vec::new(),
            menus: HashMap::new(),
            menu_order: Vec::new(),
            menu_requests: Vec::new(),
            menu_focus: None,
            _attachment: attachment,
        })
    }

    pub(crate) fn begin(&mut self) -> Vec<(Key<Id>, AccessibleAction)> {
        self.pending.clear();
        self.draw_order.clear();
        self.menu_order.clear();
        self.menu_focus = None;
        let requests = mem::take(&mut *lock(&self.shared.requests));
        let mut actions = Vec::new();
        for request in requests {
            if request.target_tree != TreeId::ROOT {
                continue;
            }
            let found = self
                .entries
                .iter()
                .find(|(_, entry)| entry.id == request.target_node && !entry.node.is_disabled());
            if let Some((key, entry)) = found
                && entry.node.supports_action(request.action)
            {
                actions.push((
                    *key,
                    AccessibleAction {
                        action: request.action,
                        data: request.data,
                    },
                ));
            } else if let Some((target, entry)) = self
                .menus
                .iter()
                .find(|(_, entry)| entry.id == request.target_node)
                && !entry.node.is_disabled()
                && entry.node.supports_action(request.action)
            {
                self.menu_requests.push((target.clone(), request.action));
            }
        }
        for entry in self.entries.values_mut() {
            entry.seen = false;
        }
        for entry in self.menus.values_mut() {
            entry.seen = false;
        }
        actions
    }

    pub(crate) fn register(&mut self, key: Key<Id>, rect: Rect, sense: Sense, enabled: bool) {
        if !self.shared.active.load(Ordering::Acquire) || rect.is_empty() {
            return;
        }
        let id = self.next_id;
        let entry = self.entries.entry(key).or_insert_with(|| {
            self.next_id = self
                .next_id
                .checked_add(1)
                .expect("accessibility node IDs exhausted");
            Entry {
                id: NodeId(id),
                node: Node::new(match sense {
                    Sense::Text => Role::TextInput,
                    Sense::Click => Role::Button,
                    Sense::Adjust => Role::Slider,
                    Sense::Hover => Role::GenericContainer,
                }),
                seen: false,
            }
        });
        if !entry.seen {
            self.draw_order.push(key);
        }
        entry.seen = true;
        entry.node.set_bounds(bounds(rect));
        if enabled {
            entry.node.clear_disabled();
            if sense != Sense::Hover {
                entry.node.add_action(Action::Focus);
            }
            if sense == Sense::Click {
                entry.node.add_action(Action::Click);
            }
        } else {
            entry.node.set_disabled();
        }
    }

    pub(crate) fn describe(&mut self, key: Key<Id>, role: Role, label: &str, value: Option<&str>) {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.node.set_role(role);
            if entry.node.label() != Some(label) {
                entry.node.set_label(label);
            }
            if let Some(value) = value
                && entry.node.value() != Some(value)
            {
                entry.node.set_value(value);
            }
        }
    }

    pub(crate) fn node(&mut self, key: Key<Id>) -> Option<&mut Node> {
        self.entries.get_mut(&key).map(|entry| &mut entry.node)
    }

    pub(crate) fn menu(
        &mut self,
        owner: Key<Id>,
        generation: u64,
        path: Vec<usize>,
        (label, enabled, checked): (&str, bool, bool),
        rect: Rect,
        focused: bool,
    ) {
        if !self.shared.active.load(Ordering::Acquire) || rect.is_empty() {
            return;
        }
        let target = MenuTarget {
            owner,
            generation,
            path,
        };
        let id = self.next_id;
        let entry = self.menus.entry(target.clone()).or_insert_with(|| {
            self.next_id = self
                .next_id
                .checked_add(1)
                .expect("accessibility node IDs exhausted");
            Entry {
                id: NodeId(id),
                node: Node::new(Role::MenuItem),
                seen: false,
            }
        });
        if !entry.seen {
            self.menu_order.push(target);
        }
        entry.seen = true;
        entry.node.set_bounds(bounds(rect));
        if entry.node.label() != Some(label) {
            entry.node.set_label(label);
        }
        if checked {
            entry.node.set_role(Role::MenuItemCheckBox);
            entry.node.set_toggled(true.into());
        } else {
            entry.node.set_role(Role::MenuItem);
            entry.node.clear_toggled();
        }
        if enabled {
            entry.node.clear_disabled();
            entry.node.add_action(Action::Click);
            entry.node.add_action(Action::Focus);
        } else {
            entry.node.set_disabled();
        }
        if focused {
            self.menu_focus = Some(entry.id);
        }
    }

    pub(crate) fn finish(
        &mut self,
        focus: Option<Key<Id>>,
        modal: Option<&[Key<Id>]>,
        menu_open: bool,
    ) {
        if self.canvas.ensure_live().is_err() {
            return;
        }
        self.entries
            .retain(|key, entry| entry.seen && modal.is_none_or(|keys| keys.contains(key)));
        self.menus.retain(|_, entry| entry.seen && menu_open);
        if !self.shared.active.load(Ordering::Acquire) {
            self.entries.clear();
            self.menus.clear();
            return;
        }
        let mut root = Node::new(Role::Window);
        #[cfg(target_os = "windows")]
        root.set_transform(Affine::scale(f64::from(self.canvas.scale_factor())));
        root.set_label(self.name.as_str());
        root.set_children(
            self.draw_order
                .iter()
                .filter_map(|key| self.entries.get(key))
                .map(|entry| entry.id)
                .chain(
                    self.menu_order
                        .iter()
                        .filter_map(|target| self.menus.get(target))
                        .map(|entry| entry.id),
                )
                .collect::<Vec<_>>(),
        );
        let focus = self
            .menu_focus
            .filter(|id| self.menus.values().any(|entry| entry.id == *id))
            .or_else(|| {
                focus
                    .and_then(|key| self.entries.get(&key))
                    .map(|entry| entry.id)
            })
            .unwrap_or(ROOT);
        let update = TreeUpdate {
            nodes: iter::once((ROOT, root))
                .chain(
                    self.entries
                        .values()
                        .map(|entry| (entry.id, entry.node.clone())),
                )
                .chain(
                    self.menus
                        .values()
                        .map(|entry| (entry.id, entry.node.clone())),
                )
                .collect(),
            tree: Some(TreeInfo::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus,
        };
        #[cfg(target_os = "linux")]
        self.platform.geometry(&self.canvas);
        self.platform.update(update);
    }

    pub(crate) fn focused(&mut self, focused: bool) {
        self.platform.focused(focused);
    }
}

impl<Id: WidgetId> UiState<Id> {
    /// Connect this state's canvas widgets to UI Automation, NSAccessibility, or AT-SPI.
    ///
    /// ```rust,no_run
    /// use aurea::render::{Canvas, RendererBackend};
    /// use aurea::ui::UiState;
    /// # fn setup() -> aurea::AureaResult<()> {
    /// let canvas = Canvas::new(320, 240, RendererBackend::Cpu)?;
    /// let mut state = UiState::<u8>::new();
    /// state.enable_accessibility(&canvas, "Brush settings")?;
    /// // Use this state for the canvas's draw callback and input events.
    /// # Ok(()) }
    /// ```
    pub fn enable_accessibility(&mut self, canvas: &Canvas, name: &str) -> AureaResult<()> {
        canvas.ensure_live()?;
        if let Some(bridge) = &mut self.accessibility
            && bridge.canvas.id() == canvas.id()
        {
            bridge.name.clear();
            bridge.name.push_str(name);
        } else {
            self.accessibility = Some(Box::new(Bridge::new(canvas, name)?));
        }
        request_canvas_redraw(canvas.id());
        Ok(())
    }

    pub fn disable_accessibility(&mut self) {
        self.accessibility = None;
    }

    pub(crate) fn accessibility_begin(&mut self) {
        if self
            .accessibility
            .as_ref()
            .is_some_and(|bridge| bridge.canvas.ensure_live().is_err())
        {
            self.accessibility = None;
        }
        let (actions, menus) = self
            .accessibility
            .as_mut()
            .map(|bridge| (bridge.begin(), mem::take(&mut bridge.menu_requests)))
            .unwrap_or_default();
        for (target, action) in menus {
            if let Some(focus) = self.popup.accessible_action(
                target.owner,
                target.generation,
                &target.path,
                action == Action::Click,
            ) {
                self.give_focus(focus);
            }
            self.changed = true;
        }
        for (key, action) in actions {
            let allowed =
                !self.popup.is_open() || (self.popup.is_card() && self.popup.owns_focus(Some(key)));
            if !allowed {
                continue;
            }
            match action.action {
                Action::Focus => self.give_focus(Some(key)),
                Action::Click => self.clicked.push(key),
                Action::SetValue => {
                    self.give_focus(Some(key));
                    if let Some(bridge) = &mut self.accessibility {
                        bridge.pending.push((key, action));
                    }
                }
                _ => {
                    if let Some(bridge) = &mut self.accessibility {
                        bridge.pending.push((key, action));
                    }
                }
            }
            self.changed = true;
        }
    }
}

impl<Id: WidgetId> Drop for Bridge<Id> {
    fn drop(&mut self) {
        self.shared.alive.store(false, Ordering::Release);
        lock(&self.shared.requests).clear();
    }
}

impl<Id: WidgetId> Ui<'_, Id> {
    /// Supply a name for an icon-only control or a field with a separate label.
    pub fn accessible_label(&mut self, id: Id, part: u16, label: &str) {
        if let Some(bridge) = &mut self.state.accessibility
            && let Some(node) = bridge.node(Key::new(id, part))
            && node.label() != Some(label)
        {
            node.set_label(label);
        }
    }
    /// Describe a custom widget after registering it with `interact`.
    pub fn accessible(&mut self, id: Id, part: u16, mut node: Node) {
        if let Some(bridge) = &mut self.state.accessibility
            && let Some(entry) = bridge.entries.get_mut(&Key::new(id, part))
        {
            if let Some(bounds) = entry.node.bounds() {
                node.set_bounds(bounds);
            }
            if entry.node.is_disabled() {
                node.set_disabled();
            }
            for action in [Action::Focus, Action::Click] {
                if entry.node.supports_action(action) {
                    node.add_action(action);
                }
            }
            entry.node = node;
        }
    }

    /// Take custom or value actions for this widget part once.
    pub fn accessibility_actions(&mut self, id: Id, part: u16) -> Vec<AccessibleAction> {
        let Some(bridge) = &mut self.state.accessibility else {
            return Vec::new();
        };
        let key = Key::new(id, part);
        let mut actions = Vec::new();
        bridge.pending.retain_mut(|(target, action)| {
            if *target != key {
                return true;
            }
            actions.push(action.clone());
            false
        });
        actions
    }

    pub(crate) fn accessible_description(
        &mut self,
        id: Id,
        part: u16,
        role: Role,
        label: &str,
        value: Option<&str>,
    ) {
        if let Some(bridge) = &mut self.state.accessibility {
            bridge.describe(Key::new(id, part), role, label, value);
        }
    }
}

pub(crate) fn edit_value(
    edit: &mut super::text_edit::TextEdit,
    text: &mut String,
    rules: super::text_edit::Rules<'_>,
    actions: Vec<AccessibleAction>,
) -> super::component::editable::Typing {
    use super::text_edit::{Direction, Step};
    let mut changed = false;
    for action in actions {
        if action.action == Action::SetValue
            && let Some(ActionData::Value(value)) = action.data
            && value.as_ref() != text
        {
            edit.set_preedit(text, "", None);
            edit.select_all(text);
            changed |= if value.is_empty() {
                edit.delete(text, Step::Char, Direction::Back)
            } else {
                edit.insert(text, &value, rules)
            };
        }
    }
    super::component::editable::Typing {
        changed,
        moved: changed,
        ..Default::default()
    }
}

fn bounds(rect: Rect) -> Bounds {
    Bounds::new(
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.right()),
        f64::from(rect.bottom()),
    )
}

#[cfg(test)]
mod editing_tests {
    use super::*;
    use crate::ui::text_edit::{Rules, TextEdit};

    fn value(value: &str) -> Vec<AccessibleAction> {
        vec![AccessibleAction {
            action: Action::SetValue,
            data: Some(ActionData::Value(value.into())),
        }]
    }

    #[test]
    fn assistive_value_updates_follow_field_rules_and_remain_undoable() {
        let mut text = "old".to_string();
        let mut edit = TextEdit::new();
        let rules = Rules {
            max_chars: Some(5),
            multiline: true,
            ..Rules::default()
        };
        assert!(edit_value(&mut edit, &mut text, rules, value("한국\r\nmore")).changed);
        assert_eq!(text, "한국\nmo");
        assert!(edit.undo(&mut text));
        assert_eq!(text, "old");
        assert!(edit_value(&mut edit, &mut text, rules, value("")).changed);
        assert!(text.is_empty());
        assert!(edit.undo(&mut text));
        assert_eq!(text, "old");
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use crate::Window;
    use crate::render::{CpuRasterizer, Point, Renderer, RendererBackend};
    use crate::ui::{Button, MenuItem, Slider, TextField, Theme, Toggle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
        IUIAutomationValuePattern, UIA_InvokePatternId, UIA_ValuePatternId,
    };
    use windows::core::BSTR;

    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    fn draw(
        state: &mut UiState<u8>,
        raster: &mut CpuRasterizer,
        text: &mut String,
        save: bool,
    ) -> usize {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("begin");
        let clicks = {
            let mut ui = state.frame(&mut *ctx, &theme);
            let clicked = save
                && Button::new("Save")
                    .show(&mut ui, 1, Rect::new(10.0, 10.0, 80.0, 30.0))
                    .expect("button")
                    .clicked;
            TextField::new(text)
                .placeholder("Name")
                .show(&mut ui, 2, Rect::new(10.0, 50.0, 150.0, 30.0))
                .expect("field");
            Button::new("Disabled")
                .enabled(false)
                .show(&mut ui, 5, Rect::new(180.0, 10.0, 100.0, 30.0))
                .expect("disabled");
            let mut on = false;
            Toggle::new(&mut on)
                .label("Flag")
                .show(&mut ui, 3, Rect::new(10.0, 90.0, 150.0, 30.0))
                .expect("toggle");
            let mut value = 0.5;
            Slider::new(&mut value, 0.0..=1.0)
                .show(&mut ui, 4, Rect::new(10.0, 130.0, 150.0, 30.0))
                .expect("slider");
            usize::from(clicked)
        };
        drop(ctx);
        raster.end_frame().expect("end");
        clicks
    }

    fn named(
        automation: &IUIAutomation,
        root: &IUIAutomationElement,
        label: &str,
    ) -> Option<IUIAutomationElement> {
        let walker = unsafe { automation.RawViewWalker() }.expect("walker");
        let mut next = unsafe { walker.GetFirstChildElement(root) }.ok();
        while let Some(node) = next {
            if unsafe { node.CurrentName() }.is_ok_and(|name| name == label) {
                return Some(node);
            }
            next = unsafe { walker.GetNextSiblingElement(&node) }.ok();
        }
        None
    }

    fn draw_menu(
        state: &mut UiState<u8>,
        raster: &mut CpuRasterizer,
        open: bool,
    ) -> Option<Vec<usize>> {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("begin");
        let picked = {
            let mut ui = state.frame(&mut *ctx, &theme);
            assert!(
                !Button::new("Behind")
                    .show(&mut ui, 1, Rect::new(0.0, 0.0, 320.0, 240.0))
                    .expect("behind")
                    .clicked
            );
            if open {
                let children = [MenuItem::new("Leaf")];
                ui.open_menu(
                    9,
                    Point::new(20.0, 20.0),
                    [MenuItem::new("Branch").submenu(&children)],
                );
            }
            ui.menu_picked_path(9)
        };
        drop(ctx);
        raster.end_frame().expect("end");
        picked
    }

    #[test]
    #[ignore = "native UI Automation; run serially on the UI thread"]
    fn native_menu_invocation_opens_submenus_and_selects_leaf_paths() {
        let mut window = Window::new("Aurea menu accessibility test", 320, 240).expect("window");
        let canvas = Canvas::new(320, 240, RendererBackend::Cpu).expect("canvas");
        window.set_content(canvas.clone()).expect("content");
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }.expect("COM");
        let _com = ComGuard;
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.expect("UIA");
        let mut state = UiState::new();
        state.set_reduced_motion(true);
        state.enable_accessibility(&canvas, "Menu").expect("bridge");
        let mut raster = CpuRasterizer::new(320, 240);
        draw_menu(&mut state, &mut raster, true);
        let root =
            unsafe { automation.ElementFromHandle(HWND(canvas.native_handle())) }.expect("root");
        draw_menu(&mut state, &mut raster, false);
        assert!(named(&automation, &root, "Behind").is_none());
        let branch = named(&automation, &root, "Branch").expect("branch");
        let invoke: IUIAutomationInvokePattern =
            unsafe { branch.GetCurrentPatternAs(UIA_InvokePatternId) }.expect("invoke");
        unsafe { invoke.Invoke() }.expect("open child");
        draw_menu(&mut state, &mut raster, false);
        let leaf = named(&automation, &root, "Leaf").expect("leaf");
        let invoke: IUIAutomationInvokePattern =
            unsafe { leaf.GetCurrentPatternAs(UIA_InvokePatternId) }.expect("invoke");
        unsafe { invoke.Invoke() }.expect("select leaf");
        assert_eq!(draw_menu(&mut state, &mut raster, false), Some(vec![0, 0]));
        assert!(named(&automation, &root, "Leaf").is_none());
        assert!(named(&automation, &root, "Behind").is_some());
    }

    #[test]
    #[ignore = "native UI Automation; run serially on the UI thread"]
    fn native_canvas_exposes_names_values_and_routes_invoke_without_stale_targets() {
        let mut window = Window::new("Aurea accessibility test", 320, 240).expect("window");
        let canvas = Canvas::new(320, 240, RendererBackend::Cpu).expect("canvas");
        window.set_content(canvas.clone()).expect("content");
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }.expect("COM");
        let _com = ComGuard;
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.expect("UIA");
        let mut state = UiState::new();
        state.set_reduced_motion(true);
        state
            .enable_accessibility(&canvas, "Settings")
            .expect("bridge");
        let mut raster = CpuRasterizer::new(320, 240);
        let mut text = "old".to_string();
        draw(&mut state, &mut raster, &mut text, true);
        let root = unsafe { automation.ElementFromHandle(HWND(canvas.native_handle())) }
            .expect("native root");
        draw(&mut state, &mut raster, &mut text, true);
        assert!(named(&automation, &root, "Save").is_some());
        let save = named(&automation, &root, "Save").expect("Save");
        let invoke: IUIAutomationInvokePattern =
            unsafe { save.GetCurrentPatternAs(UIA_InvokePatternId) }.expect("invoke pattern");
        unsafe { invoke.Invoke() }.expect("invoke");
        assert_eq!(draw(&mut state, &mut raster, &mut text, true), 1);
        let field = named(&automation, &root, "Name").expect("Name");
        let value: IUIAutomationValuePattern =
            unsafe { field.GetCurrentPatternAs(UIA_ValuePatternId) }.expect("value pattern");
        unsafe { value.SetValue(&BSTR::from("한글 updated")) }.expect("set value");
        draw(&mut state, &mut raster, &mut text, true);
        assert_eq!(text, "한글 updated");
        let disabled = named(&automation, &root, "Disabled").expect("disabled");
        assert!(
            !unsafe { disabled.CurrentIsEnabled() }
                .expect("enabled")
                .as_bool()
        );
        draw(&mut state, &mut raster, &mut text, false);
        assert!(named(&automation, &root, "Save").is_none());
        assert!(unsafe { invoke.Invoke() }.is_err());
        state.disable_accessibility();
        assert!(state.accessibility.is_none());
    }
}
