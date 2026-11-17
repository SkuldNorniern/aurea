//! The accessibility calls without the `accessibility` feature. Same names and signatures, no work.
//! The types mirror the AccessKit parts widgets use; their docs are AccessKit's.

#![allow(missing_docs)]

use std::fmt;
use std::marker::PhantomData;

use super::AccessibleAction;
use crate::render::Rect;
use crate::ui::frame::Ui;
use crate::ui::state::{Key, Sense, UiState, WidgetId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Alert,
    Button,
    ComboBox,
    DisclosureTriangle,
    Link,
    MultilineTextInput,
    PasswordInput,
    ProgressIndicator,
    RadioButton,
    Slider,
    SpinButton,
    Splitter,
    Switch,
    Tab,
    TextInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Increment,
    Decrement,
    SetValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ActionData {
    NumericValue(f64),
    Value(Box<str>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Live {
    Polite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggled {
    False,
    True,
}

impl From<bool> for Toggled {
    fn from(on: bool) -> Self {
        if on { Self::True } else { Self::False }
    }
}

#[derive(Debug)]
pub struct Node;

impl Node {
    #[inline]
    pub fn set_role(&mut self, _: Role) {}
    #[inline]
    pub fn set_label(&mut self, _: impl Into<Box<str>>) {}
    #[inline]
    pub fn set_value(&mut self, _: impl Into<Box<str>>) {}
    #[inline]
    pub fn clear_value(&mut self) {}
    #[inline]
    pub fn set_toggled(&mut self, _: Toggled) {}
    #[inline]
    pub fn set_expanded(&mut self, _: bool) {}
    #[inline]
    pub fn set_selected(&mut self, _: bool) {}
    #[inline]
    pub fn set_live(&mut self, _: Live) {}
    #[inline]
    pub fn set_numeric_value(&mut self, _: f64) {}
    #[inline]
    pub fn set_min_numeric_value(&mut self, _: f64) {}
    #[inline]
    pub fn set_max_numeric_value(&mut self, _: f64) {}
    #[inline]
    pub fn set_numeric_value_step(&mut self, _: f64) {}
    #[inline]
    pub fn add_action(&mut self, _: Action) {}
}

/// Never built without the feature; `UiState` keeps the slot so its layout is one.
#[allow(dead_code)]
pub(crate) struct Bridge<Id>(PhantomData<fn(Id)>);

impl<Id> fmt::Debug for Bridge<Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessibilityBridge")
    }
}

#[derive(Debug, Default)]
pub(crate) struct Generation;

impl Generation {
    #[inline]
    pub(crate) fn advance(&mut self) {}
}

impl<Id: WidgetId> Ui<'_, Id> {
    #[inline]
    pub fn accessible_label(&mut self, _: Id, _: u16, _: &str) {}

    #[inline]
    pub fn accessible(&mut self, _: Id, _: u16, _: Node) {}

    #[inline]
    pub fn accessibility_actions(&mut self, _: Id, _: u16) -> Vec<AccessibleAction> {
        Vec::new()
    }

    #[inline]
    pub(crate) fn with_accessible(&mut self, _: Id, _: u16, _: impl FnOnce(&mut Node)) {}

    #[inline]
    pub(crate) fn accessible_adjust(&mut self, _: Id, _: u16, _: &mut f32, _: (f32, f32), _: f32) {}

    #[inline]
    pub(crate) fn accessible_range(
        &mut self,
        _: Id,
        _: u16,
        _: Role,
        _: f32,
        _: (f32, f32),
        _: f32,
    ) {
    }

    #[inline]
    pub(crate) fn accessible_description(
        &mut self,
        _: Id,
        _: u16,
        _: Role,
        _: &str,
        _: Option<&str>,
    ) {
    }

    #[inline]
    pub(crate) fn accessible_region(&mut self, _: Id, _: u16, _: Rect) {}

    #[inline]
    pub(crate) fn accessible_menu_row(&mut self, _: Rect, _: usize, _: usize) {}
}

impl<Id: WidgetId> UiState<Id> {
    #[inline]
    pub(crate) fn accessibility_begin(&mut self) {}

    #[inline]
    pub(crate) fn accessibility_focused(&mut self, _: bool) {}

    #[inline]
    pub(crate) fn accessibility_register(&mut self, _: Key<Id>, _: Rect, _: Sense, _: bool) {}

    #[inline]
    pub(crate) fn accessibility_finish(&mut self) {}
}
