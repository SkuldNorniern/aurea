//! What a menu row or dropdown option shows beside its text: a mark in front, an icon or a colour,
//! and a badge behind. Borrowed for the call that builds a list, owned for a popup that keeps its
//! rows between frames.

use std::ptr;
use std::sync::Arc;

use crate::AureaResult;
use crate::render::{Color, Icon, Image, Rect, VectorImage};
use crate::ui::frame::Ui;
use crate::ui::paint::{Badge, BadgeTone, Component, IconMark, IconSource};
use crate::ui::state::WidgetId;

/// Space kept round an icon or swatch within its column.
const MARK_INSET: f32 = 4.0;

/// The mark in front of a row's text.
#[derive(Clone, Copy, Debug, Default)]
pub enum Leading<'a> {
    /// Nothing.
    #[default]
    None,
    /// An icon in the text colour; a bitmap is drawn as it is.
    Icon(IconSource<'a>),
    /// A coloured dot.
    Swatch(Color),
}

impl PartialEq for Leading<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Icon(a), Self::Icon(b)) => same_icon(a, b),
            (Self::Swatch(a), Self::Swatch(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Leading<'_> {}

/// Two sources are the same icon: the same built-in, or the same image by address or pixels.
fn same_icon(a: &IconSource<'_>, b: &IconSource<'_>) -> bool {
    match (a, b) {
        (IconSource::Builtin(a), IconSource::Builtin(b)) => a == b,
        (IconSource::Vector(a), IconSource::Vector(b)) => ptr::eq(*a, *b),
        (IconSource::Image(a), IconSource::Image(b)) => a.id() == b.id(),
        _ => false,
    }
}

/// What a row adds to its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Adornment<'a> {
    /// The mark in front of the text.
    pub leading: Leading<'a>,
    /// A badge after the text.
    pub badge: Option<Badge<'a>>,
}

impl<'a> Adornment<'a> {
    /// Nothing extra: plain text.
    pub const NONE: Self = Self {
        leading: Leading::None,
        badge: None,
    };

    /// An icon in front, in the text colour. Replaces a swatch.
    #[must_use]
    pub fn icon(mut self, icon: impl Into<IconSource<'a>>) -> Self {
        self.leading = Leading::Icon(icon.into());
        self
    }

    /// A coloured dot in front. Replaces an icon.
    #[must_use]
    pub const fn swatch(mut self, color: Color) -> Self {
        self.leading = Leading::Swatch(color);
        self
    }

    /// A badge after the text.
    #[must_use]
    pub const fn badge(mut self, badge: Badge<'a>) -> Self {
        self.badge = Some(badge);
        self
    }
}

/// An icon a row owns, for a menu built from data. Vector images are shared between copies of the
/// row, not copied again.
#[derive(Clone, Debug)]
pub enum OwnedIcon {
    /// One of the built-in line icons.
    Builtin(Icon),
    /// A vector image, drawn in the text colour.
    Vector(Arc<VectorImage>),
    /// A bitmap, drawn as it is.
    Image(Image),
}

impl PartialEq for OwnedIcon {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Builtin(a), Self::Builtin(b)) => a == b,
            (Self::Vector(a), Self::Vector(b)) => Arc::ptr_eq(a, b),
            (Self::Image(a), Self::Image(b)) => a.id() == b.id(),
            _ => false,
        }
    }
}

impl From<Icon> for OwnedIcon {
    fn from(icon: Icon) -> Self {
        Self::Builtin(icon)
    }
}

impl From<IconSource<'_>> for OwnedIcon {
    fn from(source: IconSource<'_>) -> Self {
        match source {
            IconSource::Builtin(icon) => Self::Builtin(icon),
            IconSource::Vector(image) => Self::Vector(Arc::new(image.clone())),
            IconSource::Image(image) => Self::Image(image.clone()),
        }
    }
}

impl From<&VectorImage> for OwnedIcon {
    fn from(image: &VectorImage) -> Self {
        IconSource::from(image).into()
    }
}

impl From<&Image> for OwnedIcon {
    fn from(image: &Image) -> Self {
        IconSource::from(image).into()
    }
}

impl OwnedIcon {
    pub(crate) fn view(&self) -> IconSource<'_> {
        match self {
            Self::Builtin(icon) => IconSource::Builtin(*icon),
            Self::Vector(image) => IconSource::Vector(image),
            Self::Image(image) => IconSource::Image(image),
        }
    }
}

/// The mark in front of an owned row's text.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum OwnedLeading {
    /// Nothing.
    #[default]
    None,
    /// An icon.
    Icon(OwnedIcon),
    /// A coloured dot.
    Swatch(Color),
}

impl OwnedLeading {
    pub(crate) fn of(leading: Leading<'_>) -> Self {
        match leading {
            Leading::None => Self::None,
            Leading::Icon(source) => Self::Icon(source.into()),
            Leading::Swatch(color) => Self::Swatch(color),
        }
    }

    pub(crate) fn view(&self) -> Leading<'_> {
        match self {
            Self::None => Leading::None,
            Self::Icon(icon) => Leading::Icon(icon.view()),
            Self::Swatch(color) => Leading::Swatch(*color),
        }
    }
}

/// A badge an owned row keeps its text in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedBadge {
    /// The text shown and read out.
    pub text: String,
    /// Which colour it takes.
    pub tone: BadgeTone,
    /// Filled with the tone's colour; otherwise a tint of it.
    pub solid: bool,
}

impl OwnedBadge {
    /// A neutral, tinted badge showing `text`.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: BadgeTone::Neutral,
            solid: false,
        }
    }

    /// Sets the tone.
    #[must_use]
    pub fn tone(mut self, tone: BadgeTone) -> Self {
        self.tone = tone;
        self
    }

    /// Fills the badge with its tone.
    #[must_use]
    pub fn solid(mut self) -> Self {
        self.solid = true;
        self
    }

    pub(crate) fn view(&self) -> Badge<'_> {
        Badge {
            text: &self.text,
            tone: self.tone,
            solid: self.solid,
        }
    }
}

impl From<Badge<'_>> for OwnedBadge {
    fn from(badge: Badge<'_>) -> Self {
        Self {
            text: badge.text.into(),
            tone: badge.tone,
            solid: badge.solid,
        }
    }
}

/// Draws `leading` in `slot`: an icon in `ink`, a swatch in its own colour, both faded by `fade`.
pub(crate) fn paint_leading<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    slot: Rect,
    leading: Leading<'_>,
    ink: Color,
    fade: f32,
) -> AureaResult<()> {
    match leading {
        Leading::None => Ok(()),
        Leading::Icon(source) => {
            IconMark::new(source, ink.fade(fade)).paint(ui, slot.inset(MARK_INSET))
        }
        Leading::Swatch(color) => {
            let inner = slot.inset(MARK_INSET);
            let d = inner.width.min(inner.height).max(0.0);
            ui.round(inner.centered(d, d), d / 2.0, color.fade(fade))
        }
    }
}
