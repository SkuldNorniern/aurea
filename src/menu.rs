//! Menu bar and submenu support.

use crate::ffi::*;
use crate::registry::menu::{next_menu_item_id, register_menu_callback, unregister_menu_callback};
use crate::{AureaError, AureaResult};
use std::cell::RefCell;
use std::os::raw::c_int;
use std::rc::{Rc, Weak};
use std::{ffi::CString, os::raw::c_void};

use crate::keymap::Chord;
use crate::window::KeyCode;

use log::debug;

/// A keyboard shortcut key for menu items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    Char(char),
    F(u8),
    Enter,
    Tab,
    Escape,
    Space,
    Backspace,
    Delete,
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

    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

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
            parts.push("Ctrl".to_string());
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
        _ => return None,
    })
}

type ItemIds = Rc<RefCell<Vec<u32>>>;

/// A native menu bar attached to a window.
pub struct MenuBar {
    pub handle: *mut c_void,
    item_ids: ItemIds,
}

/// A submenu inside a menu bar.
pub struct SubMenu {
    pub handle: *mut c_void,
    item_ids: Weak<RefCell<Vec<u32>>>,
    own: Rc<RefCell<Vec<u32>>>,
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
    pub fn new(handle: *mut c_void) -> Self {
        Self {
            handle,
            item_ids: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Add a submenu to this menu bar.
    pub fn add_submenu(&mut self, title: &str) -> AureaResult<SubMenu> {
        let title = CString::new(title).map_err(|_| AureaError::InvalidTitle)?;
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
        let mut ids = ids.borrow_mut();
        ids.retain(|&id| {
            let mine = self.mine(id);
            if mine {
                unregister_menu_callback(id);
            }
            !mine
        });
        self.own.borrow_mut().clear();
        Ok(())
    }

    /// Return the underlying native handle.
    pub fn handle(&self) -> *mut c_void {
        self.handle
    }

    fn alive(&self) -> AureaResult<ItemIds> {
        self.item_ids
            .upgrade()
            .ok_or(AureaError::ElementOperationFailed)
    }

    fn mine(&self, id: u32) -> bool {
        self.own.borrow().contains(&id)
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
        let label = CString::new(label(title, shortcut)).map_err(|_| AureaError::InvalidTitle)?;
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
        self.own.borrow_mut().push(id);
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

    pub fn set_checked(&self, checked: bool) -> AureaResult<()> {
        self.native(|menu, id| unsafe {
            ng_platform_menu_item_set_checked(menu, id, c_int::from(checked))
        })
    }

    /// A new label, the shortcut kept: "Undo Brush Stroke", or another language.
    pub fn set_label(&self, title: &str) -> AureaResult<()> {
        let label =
            CString::new(label(title, self.shortcut)).map_err(|_| AureaError::InvalidTitle)?;
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
        assert_eq!(MenuShortcut::from_chord(Chord::key(KeyCode::Up)), None);
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
}
