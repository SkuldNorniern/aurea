//! One line of editable text: the inside of a text field, without its box.

use std::mem;
use std::time::Instant;

use crate::AureaResult;
use crate::render::{Font, Paint, PaintStyle, Point, Rect, shape_text};
use crate::ui::frame::{Align, Ui, baseline_in, fill};
use crate::ui::state::{BLINK_FOR, Interaction, Key, Typed, WidgetId};
use crate::ui::text_edit::{Direction, Rules, Step, TextEdit};
use crate::ui::theme::Palette;
use crate::window::{KeyCode, Modifiers, clipboard_text, set_clipboard_text};

const MASK: char = '\u{2022}';

/// What editing did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextOutcome {
    pub changed: bool,
    /// Enter was pressed.
    pub submitted: bool,
    /// Escape was pressed; the keyboard has been let go.
    pub cancelled: bool,
}

/// Editable text over a rect: typing, the caret and selection, clipboard, undo and input methods,
/// scrolled to keep the caret in sight.
pub struct EditableText<'v> {
    text: &'v mut String,
    placeholder: &'v str,
    password: bool,
    rules: Rules<'v>,
}

impl<'v> EditableText<'v> {
    pub fn new(text: &'v mut String) -> Self {
        Self {
            text,
            placeholder: "",
            password: false,
            rules: Rules::default(),
        }
    }

    /// Shown, faint, while the text is empty.
    pub fn placeholder(mut self, placeholder: &'v str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// A dot for each character, and nothing copied out.
    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    /// What may be typed or pasted in.
    pub fn rules(mut self, rules: Rules<'v>) -> Self {
        self.rules = rules;
        self
    }

    pub fn show<Id: WidgetId>(
        mut self,
        ui: &mut Ui<'_, Id>,
        (id, part): (Id, u16),
        rect: Rect,
        input: Interaction,
    ) -> AureaResult<TextOutcome> {
        let key = Key::new(id, part);
        let mut outcome = TextOutcome::default();
        if input.focused || ui.state.typed_targets.contains(&Some(key)) {
            self.start_editing(ui, key);
            let typed = self.type_in(ui, key);
            outcome = TextOutcome {
                changed: typed.changed,
                submitted: typed.submitted,
                cancelled: typed.cancelled,
            };
            if typed.cancelled && input.focused {
                ui.clear_focus();
            }
            if (input.focused && self.point(ui, key, rect, input.held)) || typed.moved {
                ui.state.blink_from = Instant::now();
            }
            if typed.changed {
                ui.mark_changed();
            }
        }
        self.draw(ui, rect, input.focused, input.enabled)?;
        Ok(outcome)
    }

    pub(crate) fn start_editing<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, key: Key<Id>) {
        let state = &mut *ui.state;
        if state.edit_for != Some(key) {
            if let Some(previous) = state.edit_for.take() {
                state.edits.insert(previous, mem::take(&mut state.edit));
            }
            let cached = state.edits.remove(&key);
            let fresh = cached.is_none();
            state.edit = cached.unwrap_or_default();
            if state.focus_visible && state.focus == Some(key) {
                state.edit.select_all(self.text);
            } else if fresh {
                state.edit.set_caret(self.text, self.text.len(), false);
            }
            state.edit_for = Some(key);
            state.blink_from = Instant::now();
        }
        state.edit.clamp(self.text);
    }

    fn type_in<Id: WidgetId>(&mut self, ui: &mut Ui<'_, Id>, key: Key<Id>) -> Typing {
        let state = &mut *ui.state;
        let mut typing = Typing::default();
        let mut clipboard = SystemClipboard {
            enabled: !self.password,
        };
        for (typed, target) in state.typed.iter().zip(&state.typed_targets) {
            if *target != Some(key) {
                continue;
            }
            typing.merge(apply(
                &mut state.edit,
                self.text,
                typed,
                self.rules,
                &mut clipboard,
            ));
        }
        typing
    }

    fn point<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        key: Key<Id>,
        rect: Rect,
        held: bool,
    ) -> bool {
        let fonts = ui.fonts();
        let font = &fonts.text;
        let origin = rect.x - ui.state.edit.scroll;
        if let Some(press) = ui.state.press.filter(|press| press.key == key) {
            let at = self.index_at(ui, font, press.at.x - origin);
            let edit = &mut ui.state.edit;
            match press.clicks {
                0 | 1 => edit.set_caret(self.text, at, press.shift),
                2 => edit.select_word_at(self.text, at),
                _ => edit.select_all(self.text),
            }
            return true;
        }
        let Some(pointer) = ui.pointer().filter(|_| held) else {
            return false;
        };
        let at = self.index_at(ui, font, pointer.x - origin);
        let moved = at != ui.state.edit.caret();
        ui.state.edit.set_caret(self.text, at, true);
        moved
    }

    fn index_at<Id: WidgetId>(&self, ui: &mut Ui<'_, Id>, font: &Font, x: f32) -> usize {
        let mut shown = mem::take(&mut ui.state.scratch);
        self.shown_into(&mut shown);
        let at = shape_text(&shown, font).map_or(0, |shaped| shaped.hit(x));
        let count = shown[..at].chars().count();
        ui.state.scratch = shown;
        self.text
            .char_indices()
            .nth(count)
            .map_or(self.text.len(), |(i, _)| i)
    }

    fn shown_into(&self, out: &mut String) {
        out.clear();
        if self.password {
            out.extend(self.text.chars().map(|_| MASK));
        } else {
            out.push_str(self.text);
        }
    }

    fn shown_offset(&self, at: usize) -> usize {
        if self.password {
            self.text[..at].chars().count() * MASK.len_utf8()
        } else {
            at
        }
    }

    fn draw<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        rect: Rect,
        editing: bool,
        enabled: bool,
    ) -> AureaResult<()> {
        let fonts = ui.fonts();
        let font = &fonts.text;
        let fade = if enabled {
            1.0
        } else {
            ui.theme.disabled_alpha
        };
        let mut shown = mem::take(&mut ui.state.probe);
        self.shown_into(&mut shown);
        let caret = if editing && !ui.state.edit.preedit().is_empty() {
            self.shown_offset(ui.state.edit.selection().start)
        } else if editing {
            self.shown_offset(ui.state.edit.caret())
        } else {
            0
        };
        let preedit = if editing {
            ui.state.edit.preedit().to_string()
        } else {
            String::new()
        };
        let empty = shown.is_empty() && preedit.is_empty();
        let result = if empty {
            let faint = ui.theme.palette.faint.fade(fade);
            ui.text_clipped(self.placeholder, rect, font, faint, Align::Start)
                .and_then(|_| {
                    if editing {
                        caret_at(ui, rect, font, rect.x)
                    } else {
                        Ok(())
                    }
                })
        } else {
            self.draw_text(ui, rect, font, (&shown, caret, &preedit), (editing, fade))
        };
        ui.state.probe = shown;
        result
    }

    fn draw_text<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        rect: Rect,
        font: &Font,
        (shown, caret, preedit): (&str, usize, &str),
        (editing, fade): (bool, f32),
    ) -> AureaResult<()> {
        let p = ui.theme.palette;
        let in_preedit = ui.state.edit.preedit_caret().unwrap_or(preedit.len());
        let after_start = if preedit.is_empty() {
            caret
        } else {
            self.shown_offset(ui.state.edit.selection().end)
        };
        let mut line = mem::take(&mut ui.state.scratch);
        line.clear();
        line.push_str(&shown[..caret]);
        line.push_str(preedit);
        line.push_str(&shown[after_start..]);
        let result = self.draw_line(
            ui,
            rect,
            font,
            (&line, caret, preedit.len(), in_preedit),
            (editing, fade, p),
        );
        ui.state.scratch = line;
        result
    }

    fn draw_line<Id: WidgetId>(
        &self,
        ui: &mut Ui<'_, Id>,
        rect: Rect,
        font: &Font,
        (line, caret, preedit, in_preedit): (&str, usize, usize, usize),
        (editing, fade, p): (bool, f32, Palette),
    ) -> AureaResult<()> {
        let shaped = shape_text(line, font)?;
        let caret_x = shaped.caret_x(caret + in_preedit);
        let total = shaped.width;
        let scroll = keep_in_sight(ui.state.edit.scroll, caret_x, total, rect.width);
        ui.state.edit.scroll = if editing { scroll } else { 0.0 };
        let x0 = rect.x - ui.state.edit.scroll;

        ui.ctx.save()?;
        ui.ctx.clip_rect(Rect::new(
            rect.x - 1.0,
            rect.y,
            rect.width + 2.0,
            rect.height,
        ))?;
        if editing && ui.state.edit.has_selection() && preedit == 0 {
            let sel = ui.state.edit.selection();
            let (a, b) = (self.shown_offset(sel.start), self.shown_offset(sel.end));
            let h = font.size * 1.35;
            for (l, r) in shaped.selection_spans(a..b) {
                let band = Rect::new(x0 + l, rect.y + (rect.height - h) / 2.0, r - l, h);
                ui.round(band, 3.0, p.selection)?;
            }
        }
        let color = p.text.fade(fade);
        let m = ui.measure(line, font);
        let baseline = baseline_in(rect, &m, font);
        let paint = fill(color);
        ui.ctx
            .draw_text_with_font(line, Point::new(x0, baseline), font, &paint)?;
        let underline = Paint::new().color(color).style(PaintStyle::Fill);
        for (l, r) in shaped.selection_spans(caret..caret + preedit) {
            let under = Rect::new(x0 + l, baseline + 2.0, r - l, 1.0);
            ui.ctx.draw_rect(under, &underline)?;
        }
        if editing {
            caret_at(ui, rect, font, x0 + caret_x)?;
        }
        ui.ctx.restore()
    }
}

fn caret_at<Id: WidgetId>(ui: &mut Ui<'_, Id>, rect: Rect, font: &Font, x: f32) -> AureaResult<()> {
    let h = font.size * 1.3;
    let bar = Rect::new(x.round() - 0.5, rect.y + (rect.height - h) / 2.0, 1.5, h);
    ui.state.ime_area = Some(bar);
    let since = ui.state.blink_from.elapsed();
    let on = !ui.theme.caret_blink || since >= BLINK_FOR || since.as_secs_f32().fract() < 0.6;
    if !on || (ui.state.edit.has_selection() && ui.state.edit.preedit().is_empty()) {
        return Ok(());
    }
    ui.fill(bar, ui.theme.palette.accent)
}

fn keep_in_sight(scroll: f32, caret: f32, total: f32, width: f32) -> f32 {
    let margin = 2.0;
    let mut scroll = scroll;
    if caret - scroll > width - margin {
        scroll = caret - width + margin;
    }
    if caret - scroll < 0.0 {
        scroll = caret;
    }
    scroll.min((total - width + margin).max(0.0)).max(0.0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Typing {
    pub(crate) changed: bool,
    pub(crate) moved: bool,
    pub(crate) submitted: bool,
    pub(crate) cancelled: bool,
}

impl Typing {
    pub(crate) fn merge(&mut self, other: Self) {
        self.changed |= other.changed;
        self.moved |= other.moved;
        self.submitted |= other.submitted;
        self.cancelled |= other.cancelled;
    }

    fn changed(changed: bool) -> Self {
        Self {
            changed,
            moved: true,
            ..Self::default()
        }
    }

    fn moved() -> Self {
        Self {
            moved: true,
            ..Self::default()
        }
    }
}

pub(crate) trait Clipboard {
    /// Whether text may leave: not from a password.
    fn can_copy(&self) -> bool;
    fn get(&mut self) -> Option<String>;
    fn set(&mut self, text: &str);
}

pub(crate) struct SystemClipboard {
    pub(crate) enabled: bool,
}

impl Clipboard for SystemClipboard {
    fn can_copy(&self) -> bool {
        self.enabled
    }

    fn get(&mut self) -> Option<String> {
        clipboard_text()
    }

    fn set(&mut self, text: &str) {
        if let Err(e) = set_clipboard_text(text) {
            log::warn!("clipboard: {e}");
        }
    }
}

pub(crate) fn apply(
    edit: &mut TextEdit,
    text: &mut String,
    typed: &Typed,
    rules: Rules<'_>,
    clip: &mut dyn Clipboard,
) -> Typing {
    match typed {
        Typed::Text(s) => Typing::changed(edit.insert(text, s, rules)),
        Typed::Preedit(s, caret) => {
            edit.set_preedit(text, s, *caret);
            Typing::changed(false)
        }
        Typed::Key(key, mods) => apply_key(edit, text, (*key, *mods), rules, clip),
    }
}

fn step_for(mods: Modifiers) -> Step {
    let apple = cfg!(any(target_os = "macos", target_os = "ios"));
    if apple && mods.meta {
        Step::Line
    } else if (apple && mods.alt) || (!apple && mods.ctrl) {
        Step::Word
    } else {
        Step::Char
    }
}

fn apply_key(
    edit: &mut TextEdit,
    text: &mut String,
    (key, mods): (KeyCode, Modifiers),
    rules: Rules<'_>,
    clip: &mut dyn Clipboard,
) -> Typing {
    let step = step_for(mods);
    let extend = mods.shift;
    match key {
        KeyCode::Left => move_by(edit, text, step, Direction::Back, extend),
        KeyCode::Right => move_by(edit, text, step, Direction::Forward, extend),
        KeyCode::Home | KeyCode::Up => move_by(edit, text, Step::Line, Direction::Back, extend),
        KeyCode::End | KeyCode::Down => move_by(edit, text, Step::Line, Direction::Forward, extend),
        KeyCode::Backspace => Typing::changed(edit.delete(text, step, Direction::Back)),
        KeyCode::Delete => Typing::changed(edit.delete(text, step, Direction::Forward)),
        KeyCode::Enter => Typing {
            submitted: true,
            ..Typing::default()
        },
        KeyCode::Escape => {
            edit.set_preedit(text, "", None);
            Typing {
                cancelled: true,
                ..Typing::default()
            }
        }
        _ if mods.command() => shortcut(edit, text, (key, mods), rules, clip),
        _ => Typing::default(),
    }
}

fn move_by(
    edit: &mut TextEdit,
    text: &str,
    step: Step,
    direction: Direction,
    extend: bool,
) -> Typing {
    edit.move_caret(text, step, direction, extend);
    Typing::moved()
}

fn shortcut(
    edit: &mut TextEdit,
    text: &mut String,
    (key, mods): (KeyCode, Modifiers),
    rules: Rules<'_>,
    clip: &mut dyn Clipboard,
) -> Typing {
    match key {
        KeyCode::A => {
            edit.select_all(text);
            Typing::moved()
        }
        KeyCode::C => {
            if edit.has_selection() && clip.can_copy() {
                clip.set(edit.selected(text));
            }
            Typing::default()
        }
        KeyCode::X if clip.can_copy() => match edit.cut(text) {
            Some(taken) => {
                clip.set(&taken);
                Typing::changed(true)
            }
            None => Typing::default(),
        },
        KeyCode::V => match clip.get() {
            Some(pasted) => Typing::changed(edit.insert(text, &pasted, rules)),
            None => Typing::default(),
        },
        KeyCode::Z if mods.shift => Typing::changed(edit.redo(text)),
        KeyCode::Z => Typing::changed(edit.undo(text)),
        KeyCode::Y => Typing::changed(edit.redo(text)),
        _ => Typing::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Memory(Option<String>);

    impl Clipboard for Memory {
        fn can_copy(&self) -> bool {
            true
        }

        fn get(&mut self) -> Option<String> {
            self.0.clone()
        }

        fn set(&mut self, text: &str) {
            self.0 = Some(text.to_string());
        }
    }

    fn command() -> Modifiers {
        let apple = cfg!(any(target_os = "macos", target_os = "ios"));
        Modifiers {
            ctrl: !apple,
            meta: apple,
            ..Modifiers::default()
        }
    }

    fn run(text: &mut String, edit: &mut TextEdit, clip: &mut Memory, input: &[Typed]) -> Typing {
        let mut typing = Typing::default();
        for typed in input {
            typing.merge(apply(edit, text, typed, Rules::default(), clip));
        }
        typing
    }

    #[test]
    fn select_all_copy_and_paste_round_trip() {
        let (mut text, mut edit, mut clip) =
            ("hello".to_string(), TextEdit::new(), Memory::default());
        let c = command();
        let keys = [
            Typed::Key(KeyCode::A, c),
            Typed::Key(KeyCode::C, c),
            Typed::Key(KeyCode::End, Modifiers::default()),
            Typed::Key(KeyCode::V, c),
        ];
        run(&mut text, &mut edit, &mut clip, &keys);
        assert_eq!(text, "hellohello");
    }

    #[test]
    fn cut_then_undo_brings_it_back() {
        let (mut text, mut edit, mut clip) =
            ("abc".to_string(), TextEdit::new(), Memory::default());
        let c = command();
        let keys = [Typed::Key(KeyCode::A, c), Typed::Key(KeyCode::X, c)];
        run(&mut text, &mut edit, &mut clip, &keys);
        assert_eq!((text.as_str(), clip.0.as_deref()), ("", Some("abc")));
        run(
            &mut text,
            &mut edit,
            &mut clip,
            &[Typed::Key(KeyCode::Z, c)],
        );
        assert_eq!(text, "abc");
    }

    #[test]
    fn a_password_lets_nothing_out() {
        let (mut text, mut edit) = ("secret".to_string(), TextEdit::new());
        let mut clip = SystemClipboard { enabled: false };
        edit.select_all(&text);
        let out = apply_key(
            &mut edit,
            &mut text,
            (KeyCode::X, command()),
            Rules::default(),
            &mut clip,
        );
        assert!(!out.changed);
        assert_eq!(text, "secret");
    }

    #[test]
    fn enter_submits_and_escape_cancels() {
        let (mut text, mut edit, mut clip) = (String::new(), TextEdit::new(), Memory::default());
        let none = Modifiers::default();
        assert!(
            run(
                &mut text,
                &mut edit,
                &mut clip,
                &[Typed::Key(KeyCode::Enter, none)]
            )
            .submitted
        );
        assert!(
            run(
                &mut text,
                &mut edit,
                &mut clip,
                &[Typed::Key(KeyCode::Escape, none)]
            )
            .cancelled
        );
    }

    #[test]
    fn typing_and_word_deletion() {
        let (mut text, mut edit, mut clip) = (String::new(), TextEdit::new(), Memory::default());
        let word = if cfg!(any(target_os = "macos", target_os = "ios")) {
            Modifiers {
                alt: true,
                ..Modifiers::default()
            }
        } else {
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            }
        };
        let keys = [
            Typed::Text("one two".into()),
            Typed::Key(KeyCode::Backspace, word),
        ];
        assert!(run(&mut text, &mut edit, &mut clip, &keys).changed);
        assert_eq!(text, "one ");
    }

    #[test]
    fn the_caret_stays_in_sight() {
        assert_eq!(keep_in_sight(0.0, 150.0, 150.0, 100.0), 52.0);
        assert_eq!(keep_in_sight(52.0, 10.0, 150.0, 100.0), 10.0);
        assert_eq!(keep_in_sight(30.0, 40.0, 60.0, 100.0), 0.0);
    }
}
