//! Menus as data: one tree of rows that becomes a native bar and an in-app popup, updated in place
//! when the tree's labels, greys or ticks change.

use crate::keymap::Keymap;
use crate::menu::{MenuBar, MenuItem, SubMenu};
use crate::ui::{OwnedMenuItem, Ui, WidgetId};
use crate::{AureaResult, Point};

/// A row of a menu.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuRow<C> {
    /// Something to run.
    Item {
        /// Its text.
        label: String,
        /// What it runs, and what the [`Keymap`] gives its chord by.
        command: C,
        /// Whether it can run now.
        enabled: bool,
        /// Its tick, if it has one. A row built with or without one stays that way.
        checked: Option<bool>,
    },
    /// A line between rows.
    Line,
    /// Rows that open beside this one.
    Sub {
        /// Its text.
        title: String,
        /// What it opens.
        rows: Vec<MenuRow<C>>,
    },
    /// A submenu the app fills itself, such as open recent. Find it with [`NativeMenus::filled`].
    Filled {
        /// Its text.
        title: String,
    },
}

impl<C> MenuRow<C> {
    /// A row running `command`.
    pub fn item(label: impl Into<String>, command: C) -> Self {
        Self::Item {
            label: label.into(),
            command,
            enabled: true,
            checked: None,
        }
    }

    /// Greys this row out unless `enabled`.
    #[must_use]
    pub fn enabled(mut self, on: bool) -> Self {
        if let Self::Item { enabled, .. } = &mut self {
            *enabled = on;
        }
        self
    }

    /// Gives this row a tick, shown while `on`.
    #[must_use]
    pub fn checked(mut self, on: bool) -> Self {
        if let Self::Item { checked, .. } = &mut self {
            *checked = Some(on);
        }
        self
    }

    /// Rows that open beside this one.
    pub fn sub(title: impl Into<String>, rows: Vec<Self>) -> Self {
        Self::Sub {
            title: title.into(),
            rows,
        }
    }
}

/// A top-level menu: its title and rows.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuSpec<C> {
    /// Its title.
    pub title: String,
    /// Its rows.
    pub rows: Vec<MenuRow<C>>,
}

impl<C> MenuSpec<C> {
    /// A menu called `title`.
    pub fn new(title: impl Into<String>, rows: Vec<MenuRow<C>>) -> Self {
        Self {
            title: title.into(),
            rows,
        }
    }
}

/// What a row shows.
#[derive(Clone, Debug, PartialEq)]
struct Shown {
    label: String,
    enabled: bool,
    checked: Option<bool>,
}

/// What every runnable row of `rows` shows, in build order through submenus.
fn shown<C>(rows: &[MenuRow<C>], out: &mut Vec<Shown>) {
    for row in rows {
        match row {
            MenuRow::Item {
                label,
                enabled,
                checked,
                ..
            } => out.push(Shown {
                label: label.clone(),
                enabled: *enabled,
                checked: *checked,
            }),
            MenuRow::Sub { rows, .. } => shown(rows, out),
            MenuRow::Line | MenuRow::Filled { .. } => {}
        }
    }
}

/// A native menu bar built from [`MenuSpec`]s, on any platform that has one.
pub struct NativeMenus {
    live: Vec<(MenuItem, Shown)>,
    filled: Vec<(String, SubMenu)>,
}

impl NativeMenus {
    /// Adds `specs` to `bar`. `keys` gives the rows' chords, and `run` makes each one's callback.
    ///
    /// # Errors
    ///
    /// What adding a menu or an item reports.
    pub fn build<C: Clone + PartialEq>(
        bar: &mut MenuBar,
        specs: &[MenuSpec<C>],
        keys: &Keymap<C>,
        run: &dyn Fn(C) -> Box<dyn Fn()>,
    ) -> AureaResult<Self> {
        let mut menus = Self {
            live: Vec::new(),
            filled: Vec::new(),
        };
        for spec in specs {
            let mut menu = bar.add_submenu(&spec.title)?;
            menus.add_rows(&mut menu, &spec.rows, keys, run)?;
        }
        Ok(menus)
    }

    fn add_rows<C: Clone + PartialEq>(
        &mut self,
        menu: &mut SubMenu,
        rows: &[MenuRow<C>],
        keys: &Keymap<C>,
        run: &dyn Fn(C) -> Box<dyn Fn()>,
    ) -> AureaResult<()> {
        for row in rows {
            match row {
                MenuRow::Line => menu.add_separator()?,
                MenuRow::Filled { title } => {
                    let sub = menu.add_submenu(title)?;
                    self.filled.push((title.clone(), sub));
                }
                MenuRow::Sub { title, rows } => {
                    let mut sub = menu.add_submenu(title)?;
                    self.add_rows(&mut sub, rows, keys, run)?;
                }
                MenuRow::Item {
                    label,
                    command,
                    enabled,
                    checked,
                } => {
                    let callback = run(command.clone());
                    let chord = keys.chord_for(command);
                    let item = match (chord, checked) {
                        (Some(chord), None) => menu.add_item_with_chord(label, chord, callback)?,
                        (None, None) => menu.add_item(label, callback)?,
                        (Some(chord), Some(on)) => {
                            menu.add_check_item_with_chord(label, chord, *on, callback)?
                        }
                        (None, Some(on)) => menu.add_check_item(label, *on, callback)?,
                    };
                    if !enabled {
                        item.set_enabled(false)?;
                    }
                    let now = Shown {
                        label: label.clone(),
                        enabled: *enabled,
                        checked: *checked,
                    };
                    self.live.push((item, now));
                }
            }
        }
        Ok(())
    }

    /// The submenu made from [`MenuRow::Filled`] called `title`.
    pub fn filled(&mut self, title: &str) -> Option<&mut SubMenu> {
        self.filled
            .iter_mut()
            .find(|(t, _)| t == title)
            .map(|(_, sub)| sub)
    }

    /// Sets the labels, greys and ticks in `specs` that differ from what shows. The tree must have
    /// the shape the bar was built from.
    pub fn update<C>(&mut self, specs: &[MenuSpec<C>]) {
        let mut now = Vec::with_capacity(self.live.len());
        for spec in specs {
            shown(&spec.rows, &mut now);
        }
        for ((item, was), now) in self.live.iter_mut().zip(now) {
            if *was != now {
                show_row(item, was, &now);
                *was = now;
            }
        }
    }
}

/// Tells `item` what differs from `was` in `now`.
fn show_row(item: &MenuItem, was: &Shown, now: &Shown) {
    let said = (was.label != now.label).then(|| item.set_label(&now.label));
    let grey = (was.enabled != now.enabled).then(|| item.set_enabled(now.enabled));
    let tick = now
        .checked
        .filter(|on| was.checked != Some(*on))
        .map(|on| item.set_checked(on));
    for error in [said, grey, tick]
        .into_iter()
        .flatten()
        .filter_map(Result::err)
    {
        log::warn!("menu: {error}");
    }
}

/// [`MenuSpec`]s as an in-app popup, for a phone or a context menu.
#[derive(Clone, Debug)]
pub struct MenuPopup<C> {
    items: Vec<OwnedMenuItem>,
    commands: Vec<C>,
}

impl<C: Clone + PartialEq> MenuPopup<C> {
    /// The rows of `specs`: each menu a submenu under its title, or just its rows when there is
    /// one menu. [`MenuRow::Filled`] shows as a greyed row, as only the app knows what it holds.
    pub fn new(specs: &[MenuSpec<C>], keys: &Keymap<C>) -> Self {
        let mut commands = Vec::new();
        let items = match specs {
            [only] => rows_as_items(&only.rows, keys, &mut commands),
            specs => specs
                .iter()
                .map(|spec| {
                    OwnedMenuItem::new(spec.title.clone()).submenu(rows_as_items(
                        &spec.rows,
                        keys,
                        &mut commands,
                    ))
                })
                .collect(),
        };
        Self { items, commands }
    }

    /// Opens it at `at`.
    pub fn open<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, id: Id, at: Point) {
        ui.open_owned_menu(id, at, &self.items);
    }

    /// The command picked this frame.
    pub fn picked<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, id: Id) -> Option<&C> {
        let tag = ui.menu_picked_tag(id)?;
        self.commands.get(usize::try_from(tag).ok()?)
    }
}

fn rows_as_items<C: Clone + PartialEq>(
    rows: &[MenuRow<C>],
    keys: &Keymap<C>,
    commands: &mut Vec<C>,
) -> Vec<OwnedMenuItem> {
    rows.iter()
        .map(|row| match row {
            MenuRow::Line => OwnedMenuItem::separator(),
            MenuRow::Filled { title } => OwnedMenuItem::new(title.clone()).enabled(false),
            MenuRow::Sub { title, rows } => {
                OwnedMenuItem::new(title.clone()).submenu(rows_as_items(rows, keys, commands))
            }
            MenuRow::Item {
                label,
                command,
                enabled,
                checked,
            } => {
                let tag = u64::try_from(commands.len()).unwrap_or(u64::MAX);
                commands.push(command.clone());
                let mut item = OwnedMenuItem::new(label.clone())
                    .enabled(*enabled)
                    .checked(checked.unwrap_or(false))
                    .tag(tag);
                if let Some(chord) = keys.chord_for(command) {
                    item = item.shortcut(chord.label());
                }
                item
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(windows, target_os = "linux"))]
    use crate::Window;
    use crate::keymap::Chord;
    use crate::window::KeyCode;

    fn tree(undo_on: bool, snap: bool) -> Vec<MenuSpec<u8>> {
        vec![
            MenuSpec::new(
                "Edit",
                vec![
                    MenuRow::item("Undo", 1).enabled(undo_on),
                    MenuRow::Line,
                    MenuRow::Filled {
                        title: "Recent".into(),
                    },
                ],
            ),
            MenuSpec::new(
                "View",
                vec![MenuRow::sub(
                    "Grid",
                    vec![MenuRow::item("Snap", 2).checked(snap)],
                )],
            ),
        ]
    }

    #[test]
    fn rows_are_read_in_build_order_through_submenus() {
        let mut out = Vec::new();
        for spec in tree(false, true) {
            shown(&spec.rows, &mut out);
        }
        let read: Vec<_> = out
            .iter()
            .map(|s| (s.label.as_str(), s.enabled, s.checked))
            .collect();
        assert_eq!(
            read,
            [("Undo", false, None), ("Snap", true, Some(true))],
            "lines and filled menus have no row"
        );
    }

    #[test]
    fn a_popup_follows_the_tree_and_gives_back_its_commands() {
        let mut keys = Keymap::new();
        keys.bind(Chord::key(KeyCode::F2), 1_u8);
        let popup = MenuPopup::new(&tree(true, true), &keys);
        assert_eq!(popup.commands, [1, 2]);
        assert_eq!(popup.items.len(), 2, "a submenu per menu");
        let edit = &popup.items[0];
        assert_eq!(edit.label, "Edit");
        let undo = &edit.children[0];
        assert_eq!(undo.tag, Some(0));
        assert_eq!(
            undo.shortcut.as_deref(),
            Some(Chord::key(KeyCode::F2).label().as_str())
        );
        assert!(
            !edit.children[2].enabled,
            "what the app fills isn't known here"
        );
        let snap = &popup.items[1].children[0].children[0];
        assert!(snap.checked);
        assert_eq!(snap.tag, Some(1));
    }

    #[test]
    fn one_menu_is_its_rows() {
        let only = vec![MenuSpec::new("Context", vec![MenuRow::item("Copy", 7_u8)])];
        let popup = MenuPopup::new(&only, &Keymap::new());
        assert_eq!(popup.items.len(), 1);
        assert_eq!(popup.items[0].label, "Copy");
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    #[ignore = "creates a native window; run with --ignored"]
    fn a_native_bar_builds_updates_and_finds_what_the_app_fills() -> AureaResult<()> {
        let mut window = Window::new("menus", 200, 100)?;
        let bar = window.create_menu_bar()?;
        let mut keys = Keymap::new();
        keys.bind(Chord::key(KeyCode::F2), 1_u8);
        let mut menus = NativeMenus::build(bar, &tree(false, false), &keys, &|_| Box::new(|| {}))?;
        assert_eq!(menus.live.len(), 2);
        menus.update(&tree(true, true));
        assert!(menus.live[0].1.enabled);
        assert_eq!(menus.live[1].1.checked, Some(true));
        let recent = menus.filled("Recent").expect("filled");
        recent.add_item("a.lpaint", || {})?;
        assert!(menus.filled("Nothing").is_none());
        Ok(())
    }
}
