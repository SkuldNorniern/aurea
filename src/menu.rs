//! Menu bar and submenu support.

use crate::ffi::c_title;
use crate::ffi::*;
use crate::registry::menu::{next_menu_item_id, register_menu_callback, unregister_menu_callback};
use crate::{AureaError, AureaResult};
use std::cell::{Cell, RefCell};
use std::os::raw::c_int;
use std::os::raw::c_void;
use std::rc::{Rc, Weak};

use crate::keymap::Chord;
use crate::window::KeyCode;

use log::debug;

/// A keyboard shortcut key for menu items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    /// A character key.
    Char(char),
    /// A function key, by number.
    F(u8),
    /// The Enter key.
    Enter,
    /// The Tab key.
    Tab,
    /// The Escape key.
    Escape,
    /// The Space bar.
    Space,
    /// The Backspace key.
    Backspace,
    /// The Delete key.
    Delete,
    /// The Insert key.
    Insert,
    /// The Home key.
    Home,
    /// The End key.
    End,
    /// The Page Up key.
    PageUp,
    /// The Page Down key.
    PageDown,
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// The left arrow.
    Left,
    /// The right arrow.
    Right,
}

/// A portable menu shortcut description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuShortcut {
    key: ShortcutKey,
    primary: bool,
    shift: bool,
    alt: bool,
    ctrl: bool,
    meta: bool,
}

impl MenuShortcut {
    /// Starts a shortcut on the given key, with no modifiers.
    pub fn new(key: ShortcutKey) -> Self {
        Self {
            key,
            primary: false,
            shift: false,
            alt: false,
            ctrl: false,
            meta: false,
        }
    }

    /// Adds the primary modifier: Cmd on macOS, Ctrl elsewhere.
    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    /// Adds the Shift modifier.
    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// Adds the Alt modifier.
    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    /// Adds the Ctrl modifier.
    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    /// Adds the Meta modifier.
    pub fn meta(mut self) -> Self {
        self.meta = true;
        self
    }

    fn encode_for_platform(self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if self.primary {
            #[cfg(target_os = "macos")]
            {
                parts.push("Cmd".to_string());
            }
            #[cfg(not(target_os = "macos"))]
            {
                parts.push("Ctrl".to_string());
            }
        }

        if self.ctrl {
            parts.push(
                if cfg!(target_os = "macos") {
                    "Control"
                } else {
                    "Ctrl"
                }
                .to_string(),
            );
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.meta {
            parts.push("Meta".to_string());
        }

        let key = match self.key {
            ShortcutKey::Char(c) => c.to_ascii_uppercase().to_string(),
            ShortcutKey::F(n) => format!("F{}", n),
            ShortcutKey::Enter => "Enter".to_string(),
            ShortcutKey::Tab => "Tab".to_string(),
            ShortcutKey::Escape => "Escape".to_string(),
            ShortcutKey::Space => "Space".to_string(),
            ShortcutKey::Backspace => "Backspace".to_string(),
            ShortcutKey::Delete => "Delete".to_string(),
            ShortcutKey::Insert => "Insert".to_string(),
            ShortcutKey::Home => "Home".to_string(),
            ShortcutKey::End => "End".to_string(),
            ShortcutKey::PageUp => "PageUp".to_string(),
            ShortcutKey::PageDown => "PageDown".to_string(),
            ShortcutKey::Up => "Up".to_string(),
            ShortcutKey::Down => "Down".to_string(),
            ShortcutKey::Left => "Left".to_string(),
            ShortcutKey::Right => "Right".to_string(),
        };
        parts.push(key);

        parts.join("+")
    }
}

impl MenuShortcut {
    /// The shortcut a keymap chord stands for, so a menu shows what the keymap does.
    pub fn from_chord(chord: Chord) -> Option<Self> {
        let key = shortcut_key(chord.key)?;
        let m = chord.modifiers;
        let mut shortcut = Self::new(key);
        let (primary, other) = if cfg!(target_os = "macos") {
            (m.meta, m.ctrl)
        } else {
            (m.ctrl, m.meta)
        };
        shortcut.primary = primary;
        shortcut.shift = m.shift;
        shortcut.alt = m.alt;
        if cfg!(target_os = "macos") {
            shortcut.ctrl = other;
        } else {
            shortcut.meta = other;
        }
        Some(shortcut)
    }
}

fn shortcut_key(key: KeyCode) -> Option<ShortcutKey> {
    if let Some(code @ 0..=25) = key.to_raw() {
        return char::from_u32(u32::from('A') + code).map(ShortcutKey::Char);
    }
    let digits = [
        KeyCode::Key0,
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
        KeyCode::Key7,
        KeyCode::Key8,
        KeyCode::Key9,
    ];
    if let Some(n) = digits.iter().position(|&d| d == key) {
        return char::from_digit(u32::try_from(n).ok()?, 10).map(ShortcutKey::Char);
    }
    let functions = [
        KeyCode::F1,
        KeyCode::F2,
        KeyCode::F3,
        KeyCode::F4,
        KeyCode::F5,
        KeyCode::F6,
        KeyCode::F7,
        KeyCode::F8,
        KeyCode::F9,
        KeyCode::F10,
        KeyCode::F11,
        KeyCode::F12,
    ];
    if let Some(n) = functions.iter().position(|&f| f == key) {
        return u8::try_from(n + 1).ok().map(ShortcutKey::F);
    }
    Some(match key {
        KeyCode::Enter => ShortcutKey::Enter,
        KeyCode::Tab => ShortcutKey::Tab,
        KeyCode::Escape => ShortcutKey::Escape,
        KeyCode::Space => ShortcutKey::Space,
        KeyCode::Backspace => ShortcutKey::Backspace,
        KeyCode::Delete => ShortcutKey::Delete,
        KeyCode::Insert => ShortcutKey::Insert,
        KeyCode::Home => ShortcutKey::Home,
        KeyCode::End => ShortcutKey::End,
        KeyCode::PageUp => ShortcutKey::PageUp,
        KeyCode::PageDown => ShortcutKey::PageDown,
        KeyCode::Up => ShortcutKey::Up,
        KeyCode::Down => ShortcutKey::Down,
        KeyCode::Left => ShortcutKey::Left,
        KeyCode::Right => ShortcutKey::Right,
        KeyCode::Minus => ShortcutKey::Char('-'),
        KeyCode::Equals => ShortcutKey::Char('='),
        KeyCode::LeftBracket => ShortcutKey::Char('['),
        KeyCode::RightBracket => ShortcutKey::Char(']'),
        KeyCode::Backslash => ShortcutKey::Char('\\'),
        KeyCode::Semicolon => ShortcutKey::Char(';'),
        KeyCode::Apostrophe => ShortcutKey::Char('\''),
        KeyCode::Grave => ShortcutKey::Char('`'),
        KeyCode::Comma => ShortcutKey::Char(','),
        KeyCode::Period => ShortcutKey::Char('.'),
        KeyCode::Slash => ShortcutKey::Char('/'),
        _ => return None,
    })
}

type ItemIds = Rc<RefCell<Vec<u32>>>;

#[derive(Default)]
struct Branch {
    removed: Cell<bool>,
    ids: RefCell<Vec<u32>>,
    children: RefCell<Vec<Rc<Branch>>>,
}

impl Branch {
    fn clear(&self, removed: &mut Vec<u32>) {
        removed.extend(self.ids.borrow_mut().drain(..));
        for child in self.children.borrow_mut().drain(..) {
            child.removed.set(true);
            child.clear(removed);
        }
    }
}

/// A native menu bar attached to a window.
pub struct MenuBar {
    /// The native menu bar handle.
    pub handle: *mut c_void,
    item_ids: ItemIds,
}

/// A submenu inside a menu bar.
pub struct SubMenu {
    /// The native submenu handle.
    pub handle: *mut c_void,
    item_ids: Weak<RefCell<Vec<u32>>>,
    own: Rc<Branch>,
}

/// One item of a submenu, to change after it is added: greyed out, checked, relabelled or removed.
#[derive(Clone)]
pub struct MenuItem {
    menu: *mut c_void,
    id: u32,
    shortcut: Option<MenuShortcut>,
    item_ids: Weak<RefCell<Vec<u32>>>,
}

impl MenuBar {
    /// Creates a menu bar from a native handle.
    pub fn new(handle: *mut c_void) -> Self {
        Self {
            handle,
            item_ids: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Add a submenu to this menu bar.
    pub fn add_submenu(&mut self, title: &str) -> AureaResult<SubMenu> {
        let native = if cfg!(windows) {
            windows_submenu_title(title)
        } else {
            title.to_string()
        };
        let title = c_title(native)?;
        let handle = unsafe { ng_platform_create_submenu(self.handle, title.as_ptr()) };

        if handle.is_null() {
            return Err(AureaError::MenuItemAddFailed);
        }

        debug!("Added submenu '{}'", title.to_string_lossy());

        Ok(SubMenu {
            handle,
            item_ids: Rc::downgrade(&self.item_ids),
            own: Rc::default(),
        })
    }

    /// Return the underlying native handle.
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }
}

impl SubMenu {
    /// Adds a native child menu.
    pub fn add_submenu(&mut self, title: &str) -> AureaResult<SubMenu> {
        self.alive()?;
        let title = c_title(title)?;
        let handle = unsafe { ng_platform_create_submenu(self.handle, title.as_ptr()) };
        if handle.is_null() {
            return Err(AureaError::MenuItemAddFailed);
        }
        let own = Rc::new(Branch::default());
        self.own.children.borrow_mut().push(Rc::clone(&own));
        Ok(SubMenu {
            handle,
            item_ids: self.item_ids.clone(),
            own,
        })
    }

    /// Add a clickable menu item with a callback.
    pub fn add_item<F>(&mut self, title: &str, callback: F) -> AureaResult<MenuItem>
    where
        F: Fn() + 'static,
    {
        self.add(title, None, Kind::Plain, callback)
    }

    /// Add a clickable menu item with a portable keyboard shortcut.
    pub fn add_item_with_shortcut<F>(
        &mut self,
        title: &str,
        shortcut: MenuShortcut,
        callback: F,
    ) -> AureaResult<MenuItem>
    where
        F: Fn() + 'static,
    {
        self.add(title, Some(shortcut), Kind::Plain, callback)
    }

    /// Add an item showing a keymap chord, so the menu and the keys agree.
    pub fn add_item_with_chord<F>(
        &mut self,
        title: &str,
        chord: Chord,
        callback: F,
    ) -> AureaResult<MenuItem>
    where
        F: Fn() + 'static,
    {
        self.add(
            title,
            MenuShortcut::from_chord(chord),
            Kind::Plain,
            callback,
        )
    }

    /// Add an item with a check mark, `checked` to start with.
    pub fn add_check_item<F>(
        &mut self,
        title: &str,
        checked: bool,
        callback: F,
    ) -> AureaResult<MenuItem>
    where
        F: Fn() + 'static,
    {
        self.add(
            title,
            None,
            Kind::Check {
                checked,
                radio: false,
            },
            callback,
        )
    }

    /// Like [`add_check_item`](Self::add_check_item), showing a keymap chord, as for a toggle
    /// that has a key.
    pub fn add_check_item_with_chord<F>(
        &mut self,
        title: &str,
        chord: Chord,
        checked: bool,
        callback: F,
    ) -> AureaResult<MenuItem>
    where
        F: Fn() + 'static,
    {
        let kind = Kind::Check {
            checked,
            radio: false,
        };
        self.add(title, MenuShortcut::from_chord(chord), kind, callback)
    }

    /// Like [`add_check_item`](Self::add_check_item), marked as one choice of several where the
    /// platform draws those differently.
    pub fn add_radio_item<F>(
        &mut self,
        title: &str,
        checked: bool,
        callback: F,
    ) -> AureaResult<MenuItem>
    where
        F: Fn() + 'static,
    {
        self.add(
            title,
            None,
            Kind::Check {
                checked,
                radio: true,
            },
            callback,
        )
    }

    /// Add a visual separator in the submenu.
    pub fn add_separator(&mut self) -> AureaResult<()> {
        self.alive()?;
        let result = unsafe { ng_platform_add_menu_separator(self.handle) };
        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }
        Ok(())
    }

    /// Remove every item, to fill the submenu again.
    pub fn clear(&mut self) -> AureaResult<()> {
        let ids = self.alive()?;
        if unsafe { ng_platform_menu_clear(self.handle) } != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        let mut removed = Vec::new();
        self.own.clear(&mut removed);
        ids.borrow_mut().retain(|id| !removed.contains(id));
        for id in removed {
            unregister_menu_callback(id);
        }
        Ok(())
    }

    /// Return the underlying native handle.
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }

    fn alive(&self) -> AureaResult<ItemIds> {
        if self.own.removed.get() {
            return Err(AureaError::ElementOperationFailed);
        }
        self.item_ids
            .upgrade()
            .ok_or(AureaError::ElementOperationFailed)
    }

    fn add<F>(
        &mut self,
        title: &str,
        shortcut: Option<MenuShortcut>,
        kind: Kind,
        callback: F,
    ) -> AureaResult<MenuItem>
    where
        F: Fn() + 'static,
    {
        let ids = self.alive()?;
        let id = next_menu_item_id();
        let label = c_title(label(title, shortcut))?;
        let result = match kind {
            Kind::Plain => unsafe { ng_platform_add_menu_item(self.handle, label.as_ptr(), id) },
            Kind::Check { checked, radio } => unsafe {
                ng_platform_add_menu_check_item(
                    self.handle,
                    label.as_ptr(),
                    id,
                    c_int::from(checked),
                    c_int::from(radio),
                )
            },
        };
        if result != 0 {
            return Err(AureaError::MenuItemAddFailed);
        }
        register_menu_callback(id, callback);
        ids.borrow_mut().push(id);
        self.own.ids.borrow_mut().push(id);
        debug!(
            "Added menu item '{}' with id {}",
            label.to_string_lossy(),
            id
        );
        Ok(MenuItem {
            menu: self.handle,
            id,
            shortcut,
            item_ids: Rc::downgrade(&ids),
        })
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Plain,
    Check { checked: bool, radio: bool },
}

fn windows_submenu_title(title: &str) -> String {
    if title.contains('&') {
        return title.to_string();
    }
    match title.char_indices().find(|(_, c)| c.is_alphanumeric()) {
        Some((i, _)) => {
            let (head, tail) = title.split_at(i);
            format!("{head}&{tail}")
        }
        None => title.to_string(),
    }
}

fn label(title: &str, shortcut: Option<MenuShortcut>) -> String {
    match shortcut {
        Some(s) => format!("{}\t{}", title, s.encode_for_platform()),
        None => title.to_string(),
    }
}

impl MenuItem {
    /// The id its callback is registered under.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// A disabled item is greyed out, and neither a click nor its shortcut calls it.
    pub fn set_enabled(&self, enabled: bool) -> AureaResult<()> {
        self.native(|menu, id| unsafe {
            ng_platform_menu_item_set_enabled(menu, id, c_int::from(enabled))
        })
    }

    /// Checks or unchecks the item.
    pub fn set_checked(&self, checked: bool) -> AureaResult<()> {
        self.native(|menu, id| unsafe {
            ng_platform_menu_item_set_checked(menu, id, c_int::from(checked))
        })
    }

    /// A new label, the shortcut kept: "Undo Brush Stroke", or another language.
    pub fn set_label(&self, title: &str) -> AureaResult<()> {
        let label = c_title(label(title, self.shortcut))?;
        self.native(|menu, id| unsafe { ng_platform_menu_item_set_title(menu, id, label.as_ptr()) })
    }

    /// Takes the item out of its menu, and its callback with it.
    pub fn remove(self) -> AureaResult<()> {
        self.native(|menu, id| unsafe { ng_platform_menu_item_remove(menu, id) })?;
        unregister_menu_callback(self.id);
        if let Some(ids) = self.item_ids.upgrade() {
            ids.borrow_mut().retain(|&id| id != self.id);
        }
        Ok(())
    }

    fn native(&self, f: impl FnOnce(*mut c_void, u32) -> c_int) -> AureaResult<()> {
        let ids = self
            .item_ids
            .upgrade()
            .ok_or(AureaError::ElementOperationFailed)?;
        if !ids.borrow().contains(&self.id) {
            return Err(AureaError::ElementOperationFailed);
        }
        if f(self.menu, self.id) != 0 {
            return Err(AureaError::ElementOperationFailed);
        }
        Ok(())
    }
}

impl Drop for MenuBar {
    fn drop(&mut self) {
        for id in self.item_ids.borrow().iter() {
            unregister_menu_callback(*id);
        }
        if !self.handle.is_null() {
            unsafe {
                ng_platform_destroy_menu(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_become_shortcuts() {
        let save = MenuShortcut::from_chord(Chord::command(KeyCode::S)).expect("letter");
        assert_eq!(save, MenuShortcut::new(ShortcutKey::Char('S')).primary());
        let f5 = MenuShortcut::from_chord(Chord::key(KeyCode::F5)).expect("function key");
        assert_eq!(f5, MenuShortcut::new(ShortcutKey::F(5)));
        let zoom = MenuShortcut::from_chord(Chord::command(KeyCode::Key0)).expect("digit");
        assert_eq!(zoom, MenuShortcut::new(ShortcutKey::Char('0')).primary());
        let up = MenuShortcut::from_chord(Chord::key(KeyCode::Up)).expect("arrow");
        assert_eq!(up, MenuShortcut::new(ShortcutKey::Up));
        let bigger =
            MenuShortcut::from_chord(Chord::command(KeyCode::Equals)).expect("punctuation");
        assert_eq!(bigger, MenuShortcut::new(ShortcutKey::Char('=')).primary());
        assert_eq!(MenuShortcut::from_chord(Chord::key(KeyCode::Shift)), None);
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn native_menus_read_every_shortcut_a_label_can_carry() {
        use crate::window::Modifiers;
        use std::ffi::{CString, c_char, c_int};
        unsafe extern "C" {
            fn ng_shortcut_parse(
                title: *const c_char,
                keycode: *mut u32,
                modifiers: *mut u32,
            ) -> c_int;
        }
        let parse = |title: &str| {
            let title = CString::new(title).expect("title");
            let (mut key, mut mods) = (0, 0);
            (unsafe { ng_shortcut_parse(title.as_ptr(), &raw mut key, &raw mut mods) } == 1)
                .then(|| (KeyCode::from_raw(key), Modifiers::from_bits(mods)))
        };
        let ctrl = Modifiers::from_bits(0x2);
        assert_eq!(parse("Zoom In\tCtrl++"), Some((KeyCode::Equals, ctrl)));
        assert_eq!(parse("Zoom In\tCtrl+Plus"), Some((KeyCode::Equals, ctrl)));
        assert_eq!(
            parse("Save\tCmd+S"),
            Some((KeyCode::S, ctrl)),
            "a title written for macOS"
        );
        assert_eq!(parse("Top\tCtrl + Home"), Some((KeyCode::Home, ctrl)));
        assert_eq!(
            parse("Back\tAlt+Left"),
            Some((KeyCode::Left, Modifiers::from_bits(0x4)))
        );
        assert_eq!(parse("Plain"), None);
        assert_eq!(parse("Odd\tCtrl+Hyper"), None);
        for key in [
            ShortcutKey::Char(','),
            ShortcutKey::Char('\\'),
            ShortcutKey::Insert,
            ShortcutKey::PageDown,
            ShortcutKey::Right,
            ShortcutKey::F(12),
        ] {
            let title = label("Item", Some(MenuShortcut::new(key).primary().shift()));
            assert!(parse(&title).is_some(), "{title:?}");
        }
    }

    #[test]
    fn labels_carry_the_shortcut_after_a_tab() {
        let s = MenuShortcut::new(ShortcutKey::Char('Z')).primary().shift();
        let expected = if cfg!(target_os = "macos") {
            "Undo\tCmd+Shift+Z"
        } else {
            "Undo\tCtrl+Shift+Z"
        };
        assert_eq!(label("Undo", Some(s)), expected);
        assert_eq!(label("Undo", None), "Undo");
    }

    #[test]
    fn the_control_key_is_not_read_as_cmd_on_macos() {
        let s = MenuShortcut::new(ShortcutKey::Tab).ctrl();
        let expected = if cfg!(target_os = "macos") {
            "Next\tControl+Tab"
        } else {
            "Next\tCtrl+Tab"
        };
        assert_eq!(label("Next", Some(s)), expected);
    }

    #[test]
    fn windows_submenu_gets_a_mnemonic_on_its_first_letter() {
        assert_eq!(windows_submenu_title("File"), "&File");
        assert_eq!(windows_submenu_title("  Edit"), "  &Edit");
        assert_eq!(windows_submenu_title("2nd"), "&2nd");
        assert_eq!(windows_submenu_title("Файл"), "&Файл");
        assert_eq!(windows_submenu_title("..."), "...");
        assert_eq!(windows_submenu_title(""), "");
    }

    #[test]
    fn windows_submenu_keeps_a_title_with_an_ampersand() {
        assert_eq!(windows_submenu_title("&File"), "&File");
        assert_eq!(windows_submenu_title("Save &As"), "Save &As");
        assert_eq!(windows_submenu_title("R&&D"), "R&&D");
    }
}

#[cfg(test)]
mod menu_ownership_tests {
    use crate::{AureaResult, Window};

    #[test]
    #[ignore = "creates a native window; run on the platform UI thread"]
    fn clearing_a_parent_invalidates_descendants_and_releases_callbacks() -> AureaResult<()> {
        use std::rc::Rc;
        let mut window = Window::new("nested menus", 300, 200)?;
        let bar = window.create_menu_bar()?;
        let mut root = bar.add_submenu("Layers")?;
        for _ in 0..300 {
            let mut child = root.add_submenu("Adjustment")?;
            let mut grandchild = child.add_submenu("More")?;
            let captured = Rc::new(());
            let weak = Rc::downgrade(&captured);
            let item = grandchild.add_item_with_shortcut(
                "Levels",
                super::MenuShortcut::new(super::ShortcutKey::Char('L')).primary(),
                move || {
                    let _ = &captured;
                },
            )?;
            assert!(weak.upgrade().is_some());
            root.clear()?;
            assert!(weak.upgrade().is_none());
            assert!(child.add_separator().is_err());
            assert!(grandchild.clear().is_err());
            assert!(item.set_enabled(false).is_err());
            assert!(item.remove().is_err());
        }
        root.add_item("Still alive", || {})?;
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_ticked_row_shows_its_chord() -> AureaResult<()> {
        use crate::{Chord, KeyCode};
        let mut window = Window::new("ticked", 300, 200)?;
        let bar = window.create_menu_bar()?;
        let mut view = bar.add_submenu("View")?;
        let snap =
            view.add_check_item_with_chord("Snap", Chord::key(KeyCode::Semicolon), true, || {})?;
        snap.set_checked(false)?;
        snap.set_label("Snap to Grid")?;
        Ok(())
    }

    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_menu_bar_belongs_to_its_window() -> AureaResult<()> {
        use crate::Capability;

        let mut window = Window::new("menus", 300, 200)?;
        if !window.capabilities().has(Capability::MenuBar) {
            return Ok(());
        }

        {
            let menu_bar = window.create_menu_bar()?;
            let mut file = menu_bar.add_submenu("File")?;
            file.add_item("New", || {})?;
        }

        {
            let menu_bar = window.create_menu_bar()?;
            menu_bar.add_submenu("Edit")?;
        }

        drop(window);
        Ok(())
    }
}
