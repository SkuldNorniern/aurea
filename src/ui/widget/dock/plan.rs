//! A dock arrangement as a plain value, to save between runs and load back.
//!
//! [`DockTree::plan`] takes one, [`DockTree::from_plan`] rebuilds the tree.
//! [`DockPlan::to_text`] and [`DockPlan::from_text`] write it as one short
//! line for a preferences file, panels named however the app likes:
//!
//! ```text
//! row(0.22, tabs(0: tools), col(0.7, tabs(1: canvas, preview), tabs(0: layers)))
//! ```

use std::fmt::Write;

use super::tree::{DockAxis, DockTree};

/// A dock arrangement: groups of tabs, split side by side or one above the
/// other.
#[derive(Clone, Debug, PartialEq)]
pub enum DockPlan<P> {
    /// One group: its panels in tab order, and which shows.
    Tabs { panels: Vec<P>, active: usize },
    /// Two parts sharing the room, `ratio` of it the first's.
    Split {
        axis: DockAxis,
        ratio: f32,
        first: Box<DockPlan<P>>,
        second: Box<DockPlan<P>>,
    },
}

impl<P: Copy + PartialEq> DockPlan<P> {
    /// The plan as text, each panel written by `name`. Names may not hold
    /// `( ) , :` or start or end with spaces.
    pub fn to_text(&self, name: impl Fn(P) -> String + Copy) -> String {
        let mut out = String::new();
        self.write(&mut out, name);
        out
    }

    fn write(&self, out: &mut String, name: impl Fn(P) -> String + Copy) {
        match self {
            Self::Tabs { panels, active } => {
                let _ = write!(out, "tabs({active}:");
                for (i, p) in panels.iter().enumerate() {
                    out.push_str(if i == 0 { " " } else { ", " });
                    out.push_str(&name(*p));
                }
                out.push(')');
            }
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let kind = match axis {
                    DockAxis::Row => "row",
                    DockAxis::Column => "col",
                };
                let _ = write!(out, "{kind}({ratio}, ");
                first.write(out, name);
                out.push_str(", ");
                second.write(out, name);
                out.push(')');
            }
        }
    }

    /// Reads [`to_text`](Self::to_text)'s output, each name read back by
    /// `parse`. `None` when the text is not a plan, or names a panel `parse`
    /// does not know: an app that dropped a panel since gets its default
    /// layout rather than a broken one.
    pub fn from_text(text: &str, parse: impl Fn(&str) -> Option<P> + Copy) -> Option<Self> {
        let mut r = Reader { s: text, i: 0 };
        let plan = r.plan(parse)?;
        r.skip_spaces();
        (r.i == r.s.len()).then_some(plan)
    }
}

struct Reader<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Reader<'a> {
    fn skip_spaces(&mut self) {
        let rest = &self.s[self.i..];
        self.i += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, token: &str) -> Option<()> {
        self.skip_spaces();
        self.s[self.i..]
            .starts_with(token)
            .then(|| self.i += token.len())
    }

    /// Up to the next `( ) , :`, trimmed.
    fn word(&mut self) -> Option<&'a str> {
        self.skip_spaces();
        let s: &'a str = self.s;
        let rest = &s[self.i..];
        let end = rest.find(['(', ')', ',', ':']).unwrap_or(rest.len());
        self.i += end;
        let word = rest[..end].trim();
        (!word.is_empty()).then_some(word)
    }

    fn plan<P>(&mut self, parse: impl Fn(&str) -> Option<P> + Copy) -> Option<DockPlan<P>> {
        let kind = self.word()?;
        self.eat("(")?;
        let plan = match kind {
            "tabs" => self.tabs(parse)?,
            "row" => self.split(DockAxis::Row, parse)?,
            "col" => self.split(DockAxis::Column, parse)?,
            _ => return None,
        };
        self.eat(")")?;
        Some(plan)
    }

    fn tabs<P>(&mut self, parse: impl Fn(&str) -> Option<P> + Copy) -> Option<DockPlan<P>> {
        let active = self.word()?.parse().ok()?;
        self.eat(":")?;
        let mut panels = vec![parse(self.word()?)?];
        while self.eat(",").is_some() {
            panels.push(parse(self.word()?)?);
        }
        Some(DockPlan::Tabs { panels, active })
    }

    fn split<P>(
        &mut self,
        axis: DockAxis,
        parse: impl Fn(&str) -> Option<P> + Copy,
    ) -> Option<DockPlan<P>> {
        let ratio = self.word()?.parse().ok()?;
        self.eat(",")?;
        let first = Box::new(self.plan(parse)?);
        self.eat(",")?;
        let second = Box::new(self.plan(parse)?);
        Some(DockPlan::Split {
            axis,
            ratio,
            first,
            second,
        })
    }
}

impl<P: Copy + PartialEq> DockTree<P> {
    /// The arrangement as a plain value, `None` for an empty tree.
    pub fn plan(&self) -> Option<DockPlan<P>> {
        self.root_plan()
    }

    /// A tree arranged as `plan`. A panel named twice keeps its first
    /// place, empty groups and their splits fold away, ratios are kept to
    /// `0..=1` and the active tab to its group.
    pub fn from_plan(plan: &DockPlan<P>) -> Self {
        let mut seen = Vec::new();
        Self::from_root_plan(plan, &mut seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Rect;
    use crate::ui::{DockLayout, DockMetrics, DockSide};

    fn name(p: u8) -> String {
        ["tools", "canvas", "preview", "layers"][usize::from(p)].to_string()
    }

    fn parse(s: &str) -> Option<u8> {
        ["tools", "canvas", "preview", "layers"]
            .iter()
            .position(|n| *n == s)
            .and_then(|i| u8::try_from(i).ok())
    }

    fn editor() -> DockTree<u8> {
        let mut tree = DockTree::new(1);
        tree.insert_sized(0, 1, DockSide::Left, 0.25);
        tree.insert(2, 1, DockSide::Center);
        tree.insert_sized(3, 1, DockSide::Bottom, 0.3);
        tree.activate(2);
        tree
    }

    fn laid_out(tree: &DockTree<u8>) -> DockLayout<u8> {
        let mut out = DockLayout::default();
        let metrics = DockMetrics {
            strip: 24.0,
            gap: 4.0,
            min_part: 40.0,
        };
        tree.layout(Rect::new(0.0, 0.0, 800.0, 600.0), metrics, &mut out);
        out
    }

    #[test]
    fn a_saved_plan_rebuilds_the_same_layout() {
        let tree = editor();
        let plan = tree.plan().expect("plan");
        let text = plan.to_text(name);
        let back = DockPlan::from_text(&text, parse).expect("parses");
        assert_eq!(back, plan, "{text}");
        let rebuilt = DockTree::from_plan(&back);
        assert_eq!(laid_out(&rebuilt).groups, laid_out(&tree).groups);
        assert!(rebuilt.is_active(2));
    }

    #[test]
    fn bad_or_stale_text_is_none() {
        assert!(DockPlan::from_text("tabs(0: tools", parse).is_none());
        assert!(
            DockPlan::from_text("tabs(0: brushes)", parse).is_none(),
            "unknown panel"
        );
        assert!(DockPlan::from_text("row(x, tabs(0: tools), tabs(0: canvas))", parse).is_none());
        assert!(DockPlan::from_text("tabs(0: tools) extra", parse).is_none());
    }

    #[test]
    fn rebuilding_repairs_what_it_can() {
        let plan = DockPlan::Split {
            axis: DockAxis::Row,
            ratio: 7.0,
            first: Box::new(DockPlan::Tabs {
                panels: vec![0, 1],
                active: 9,
            }),
            second: Box::new(DockPlan::Tabs {
                panels: vec![1],
                active: 0,
            }),
        };
        let tree = DockTree::from_plan(&plan);
        // The second group only repeated panel 1, so it and its split go.
        assert_eq!(
            tree.plan(),
            Some(DockPlan::Tabs {
                panels: vec![0, 1],
                active: 1,
            })
        );
    }
}
