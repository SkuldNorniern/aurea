//! A plain multiline editor over a caller-owned string.

use std::mem;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use super::Widget;
use super::text_field::FieldStyle;
use crate::AureaResult;
use crate::render::{Font, LayoutLine, LayoutOptions, Point, Rect, TextLayout, layout_text};
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::edit_value;
#[cfg(feature = "accessibility")]
use crate::ui::accessibility::{Action, Role};
use crate::ui::component::editable::{SystemClipboard, Typing, apply};
use crate::ui::component::{EditableText, FocusRing, TextOutcome};
use crate::ui::frame::{Response, Ui, fill};
use crate::ui::paint::{Component, Surface};
use crate::ui::state::{BLINK_FOR, Interaction, Key, Sense, Typed, WidgetId};
use crate::ui::text_edit::{Rules, TextEdit};
use crate::window::{KeyCode, Modifiers};

#[derive(Clone, Debug)]
struct LayoutCache {
    font: Font,
    options: LayoutOptions,
    layout: TextLayout,
}

#[derive(Clone, Debug)]
struct CompositionCache {
    source: Arc<str>,
    selected: Range<usize>,
    preedit: String,
    layout: TextLayout,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct EditorView {
    scroll_y: f32,
    desired_x: Option<f32>,
    cache: Option<LayoutCache>,
    visual_line: Option<usize>,
    focused: bool,
    composition: Option<CompositionCache>,
    composing: bool,
}

impl EditorView {
    fn prepare(&mut self, text: &str, font: &Font, options: LayoutOptions) -> AureaResult<bool> {
        self.composing = false;
        if !self.cache.as_ref().is_some_and(|cache| {
            cache.font == *font && cache.options == options && cache.layout.text.as_ref() == text
        }) {
            self.cache = Some(LayoutCache {
                font: font.clone(),
                options,
                layout: layout_text(text, font, options)?,
            });
            self.visual_line = None;
            return Ok(true);
        }
        Ok(false)
    }

    fn layout(&self) -> &TextLayout {
        if self.composing {
            return &self
                .composition
                .as_ref()
                .expect("prepared composition")
                .layout;
        }
        &self.cache.as_ref().expect("prepared editor layout").layout
    }

    fn caret(&self, index: usize) -> Rect {
        if let Some(line) = self.visual_line.and_then(|i| self.layout().lines.get(i))
            && (line.range.start..=line.range.end).contains(&index)
        {
            return Rect::new(
                line.x + line.shaped.caret_x(index - line.range.start),
                line.top,
                0.0,
                line.height,
            );
        }
        self.layout().caret(index)
    }

    fn hit(&mut self, point: Point) -> usize {
        let layout = self.layout();
        let at = layout.hit(point);
        self.visual_line = Some(
            layout
                .lines
                .partition_point(|line| point.y >= line.top + line.height)
                .min(layout.lines.len().saturating_sub(1)),
        );
        at
    }
}

/// A multiline text field with wrapping, selection, undo, clipboard and IME.
///
/// ```rust,no_run
/// # use aurea::ui::{TextArea, Ui};
/// # fn notes(ui: &mut Ui<'_, u8>, text: &mut String) -> aurea::AureaResult<()> {
/// TextArea::new(text).placeholder("Notes").show(
///     ui, 1, aurea::Rect::new(20.0, 20.0, 300.0, 160.0),
/// )?;
/// # Ok(()) }
/// ```
pub struct TextArea<'a> {
    text: &'a mut String,
    placeholder: &'a str,
    rules: Rules<'a>,
    style: Option<FieldStyle>,
    enabled: bool,
    commit_on_blur: bool,
    wrap: bool,
    line_height: f32,
}

impl<'a> TextArea<'a> {
    pub fn new(text: &'a mut String) -> Self {
        Self {
            text,
            placeholder: "",
            rules: Rules {
                multiline: true,
                ..Rules::default()
            },
            style: None,
            enabled: true,
            commit_on_blur: false,
            wrap: true,
            line_height: 1.2,
        }
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }
    pub fn max_chars(mut self, max: usize) -> Self {
        self.rules.max_chars = Some(max);
        self
    }
    pub fn accept(mut self, accept: &'a dyn Fn(char) -> bool) -> Self {
        self.rules.accept = Some(accept);
        self
    }
    pub fn style(mut self, style: FieldStyle) -> Self {
        self.style = Some(style);
        self
    }
    /// Emit `committed` when dirty text loses focus.
    pub fn commit_on_blur(mut self, commit: bool) -> Self {
        self.commit_on_blur = commit;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    /// Disable wrapping to scroll long lines horizontally instead.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }
    /// Positive, finite line spacing relative to the font's ascent and descent.
    pub fn line_height(mut self, height: f32) -> Self {
        self.line_height = height;
        self
    }

    pub fn show<Id: WidgetId>(
        self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
    ) -> AureaResult<Response> {
        Widget::show(self, ui, id, rect)
    }

    fn edit<Id: WidgetId>(
        &mut self,
        ui: &mut Ui<'_, Id>,
        id: Id,
        rect: Rect,
        input: Interaction,
    ) -> AureaResult<Typing> {
        let key = Key::new(id, 0);
        let editing =
            input.enabled && (input.focused || ui.state.typed_targets.contains(&Some(key)));
        if editing {
            EditableText::new(self.text).start_editing(ui, key);
        }
        let mut edit = if ui.state.edit_for == Some(key) {
            mem::take(&mut ui.state.edit)
        } else {
            ui.state.edits.remove(&key).unwrap_or_default()
        };
        edit.clamp(self.text);
        let mut view = edit.multiline.take().unwrap_or_default();
        let fonts = ui.fonts();
        let font = &fonts.text;
        let options = LayoutOptions {
            max_width: self.wrap.then_some(rect.width.max(0.0)),
            line_height: self.line_height,
            ..LayoutOptions::default()
        };
        let result = self.update(
            ui,
            key,
            rect,
            input,
            (&mut edit, &mut view),
            (font, options),
        );
        edit.multiline = Some(view);
        if ui.state.edit_for == Some(key) {
            ui.state.edit = edit;
        } else {
            ui.state.edits.insert(key, edit);
        }
        result
    }

    fn update<Id: WidgetId>(
        &mut self,
        ui: &mut Ui<'_, Id>,
        key: Key<Id>,
        rect: Rect,
        input: Interaction,
        (edit, view): (&mut TextEdit, &mut EditorView),
        (font, options): (&Font, LayoutOptions),
    ) -> AureaResult<Typing> {
        let mut outcome = Typing::default();
        #[cfg(feature = "accessibility")]
        if input.enabled {
            outcome.merge(edit_value(
                edit,
                self.text,
                self.rules,
                ui.accessibility_actions(key.id, key.part),
            ));
        }
        for (typed, target) in ui
            .state
            .typed
            .iter()
            .zip(&ui.state.typed_targets)
            .filter(|_| input.enabled)
        {
            if *target == Some(key) {
                outcome.merge(edit_input(
                    edit,
                    view,
                    self.text,
                    typed,
                    (font, options),
                    self.rules,
                    rect.height,
                )?);
            }
        }
        let reflow = view.prepare(self.text, font, options)?;
        let moved = point(ui, key, rect, input, edit, view, self.text);
        if moved || outcome.moved {
            ui.state.blink_from = Instant::now();
        }
        if outcome.cancelled && input.focused {
            ui.clear_focus();
        }
        if outcome.changed {
            ui.mark_changed();
        }
        let reveal = moved || outcome.moved || (input.focused && (!view.focused || reflow));
        view.focused = input.focused;
        let (caret, composition) =
            self.prepare_display(edit, view, font, options, input.focused)?;
        scroll(ui, rect, edit, view, caret, (reveal, input.hovered));
        ui.clipped(rect, |ui| {
            draw(ui, rect, (edit, view), (font, caret, composition), input)
        })?;
        if self.text.is_empty() && edit.preedit().is_empty() {
            let faint = ui.theme.palette.faint;
            ui.clipped(rect, |ui| {
                ui.text_wrapped(
                    self.placeholder,
                    rect,
                    font,
                    faint,
                    font.size * self.line_height,
                )
                .map(|_| ())
            })?;
        }
        Ok(outcome)
    }

    fn prepare_display(
        &self,
        edit: &TextEdit,
        view: &mut EditorView,
        font: &Font,
        options: LayoutOptions,
        focused: bool,
    ) -> AureaResult<(usize, Range<usize>)> {
        if !focused || edit.preedit().is_empty() {
            view.composition = None;
            view.composing = false;
            return Ok((edit.caret(), 0..0));
        }
        let selection = edit.selection();
        let source = &view.cache.as_ref().expect("document layout").layout.text;
        if !view.composition.as_ref().is_some_and(|cache| {
            Arc::ptr_eq(&cache.source, source)
                && cache.selected == selection
                && cache.preedit == edit.preedit()
        }) {
            let mut composed = String::with_capacity(self.text.len() + edit.preedit().len());
            composed.push_str(&self.text[..selection.start]);
            composed.push_str(edit.preedit());
            composed.push_str(&self.text[selection.end..]);
            view.composition = Some(CompositionCache {
                source: source.clone(),
                selected: selection.clone(),
                preedit: edit.preedit().to_string(),
                layout: layout_text(&composed, font, options)?,
            });
            view.visual_line = None;
        }
        view.composing = true;
        let caret = selection.start + edit.preedit_caret().unwrap_or(edit.preedit().len());
        Ok((
            caret,
            selection.start..selection.start + edit.preedit().len(),
        ))
    }
}

impl<Id: WidgetId> Widget<Id> for TextArea<'_> {
    fn show(mut self, ui: &mut Ui<'_, Id>, id: Id, rect: Rect) -> AureaResult<Response> {
        let input = ui.interact(id, 0, rect, Sense::Text, self.enabled);
        let state = ui.eased(id, 0, input);
        let style = self.style.unwrap_or_else(|| FieldStyle::new(ui.theme));
        FocusRing::new(ui.theme, style.radius, state.focus).paint(ui, rect)?;
        let border = if input.focused {
            style.active_border
        } else {
            style.border
        };
        Surface::new(style.face.at(state))
            .radius(style.radius)
            .border(ui.theme.border_width, border.fade(state.enabled))
            .paint(ui, rect)?;
        let mut inner = rect.inset(style.padding);
        inner.width = inner.width.max(0.0);
        inner.height = inner.height.max(0.0);
        let typed = self.edit(ui, id, inner, input)?;
        let mut outcome = TextOutcome {
            changed: typed.changed,
            submitted: typed.submitted,
            cancelled: typed.cancelled,
            ..TextOutcome::default()
        };
        outcome.lifecycle(ui, Key::new(id, 0), input, self.commit_on_blur);
        #[cfg(feature = "accessibility")]
        {
            ui.accessible_description(
                id,
                0,
                Role::MultilineTextInput,
                self.placeholder,
                Some(self.text),
            );
            if let Some(bridge) = &mut ui.state.accessibility
                && let Some(node) = bridge.node(Key::new(id, 0))
            {
                node.add_action(Action::SetValue);
            }
        }
        let mut response = Response::new(rect, input);
        response.changed = outcome.changed;
        response.submitted = outcome.submitted;
        response.cancelled = outcome.cancelled;
        response.editing_started = outcome.editing_started;
        response.focus_lost = outcome.focus_lost;
        response.committed = outcome.committed;
        response.focused = input.focused && !outcome.cancelled;
        Ok(response)
    }
}

fn edit_input(
    edit: &mut TextEdit,
    view: &mut EditorView,
    text: &mut String,
    typed: &Typed,
    (font, options): (&Font, LayoutOptions),
    rules: Rules<'_>,
    height: f32,
) -> AureaResult<Typing> {
    if let Typed::Key(key, mods) = typed {
        match key {
            KeyCode::Enter => {
                return Ok(Typing {
                    changed: !mods.command() && edit.insert(text, "\n", rules),
                    moved: !mods.command(),
                    submitted: mods.command(),
                    ..Typing::default()
                });
            }
            KeyCode::Home
            | KeyCode::End
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown => {
                view.prepare(text, font, options)?;
                navigate(edit, view, text, *key, *mods, height);
                return Ok(Typing {
                    moved: true,
                    ..Typing::default()
                });
            }
            _ => {}
        }
    }
    view.desired_x = None;
    view.visual_line = None;
    Ok(apply(
        edit,
        text,
        typed,
        rules,
        &mut SystemClipboard { enabled: true },
    ))
}

fn navigate(
    edit: &mut TextEdit,
    view: &mut EditorView,
    text: &str,
    key: KeyCode,
    mods: Modifiers,
    height: f32,
) {
    let layout = view.layout();
    let caret = view.caret(edit.caret());
    let line_index = layout.lines.iter().position(|line| line.top == caret.y);
    let line = layout.lines.iter().find(|line| line.top == caret.y);
    let target = match key {
        KeyCode::Home if mods.command() => 0,
        KeyCode::End if mods.command() => text.len(),
        KeyCode::Home => line.map_or(0, |line| line.range.start),
        KeyCode::End => line.map_or(text.len(), |line| line.range.end),
        _ => {
            let x = view.desired_x.unwrap_or(caret.x);
            let delta = match key {
                KeyCode::Up => -caret.height,
                KeyCode::Down => caret.height,
                KeyCode::PageUp => -height.max(caret.height),
                _ => height.max(caret.height),
            };
            let target = view.hit(Point::new(x, caret.y + caret.height * 0.5 + delta));
            view.desired_x = Some(x);
            edit.set_caret(text, target, mods.shift);
            return;
        }
    };
    view.desired_x = None;
    view.visual_line = line_index;
    edit.set_caret(text, target, mods.shift);
}

fn point<Id: WidgetId>(
    ui: &Ui<'_, Id>,
    key: Key<Id>,
    rect: Rect,
    input: Interaction,
    edit: &mut TextEdit,
    view: &mut EditorView,
    text: &mut String,
) -> bool {
    let press = ui.state.press.filter(|press| press.key == key);
    let pointer = press
        .map(|press| press.at)
        .or_else(|| ui.pointer().filter(|_| input.held));
    let Some(pointer) = pointer else {
        return false;
    };
    let at = view.hit(Point::new(
        pointer.x - rect.x + edit.scroll,
        pointer.y - rect.y + view.scroll_y,
    ));
    let moved = press.is_some() || at != edit.caret();
    if let Some(press) = press {
        edit.set_preedit(text, "", None);
        match press.clicks {
            0 | 1 => edit.set_caret(text, at, press.shift),
            2 => edit.select_word_at(text, at),
            _ => edit.select_all(text),
        }
    } else {
        edit.set_caret(text, at, true);
    }
    view.desired_x = None;
    moved
}

fn scroll<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    rect: Rect,
    edit: &mut TextEdit,
    view: &mut EditorView,
    index: usize,
    (reveal, hovered): (bool, bool),
) {
    let wheel = if hovered { ui.wheel() } else { (0.0, 0.0) };
    let before = (edit.scroll, view.scroll_y);
    let layout = view.layout();
    let caret = view.caret(index);
    let step = caret.height.max(1.0) * 3.0;
    let wheel_x = viewport_scroll(
        before.0 - wheel.0 * step,
        0.0,
        0.0,
        layout.width,
        rect.width,
        false,
    );
    let wheel_y = viewport_scroll(
        before.1 - wheel.1 * step,
        0.0,
        0.0,
        layout.height,
        rect.height,
        false,
    );
    if hovered {
        ui.consume_wheel(((before.0 - wheel_x) / step, (before.1 - wheel_y) / step));
    }
    edit.scroll = viewport_scroll(
        edit.scroll - wheel.0 * step,
        caret.x,
        2.0,
        layout.width,
        rect.width,
        reveal,
    );
    view.scroll_y = viewport_scroll(
        view.scroll_y - wheel.1 * step,
        caret.y,
        caret.height,
        layout.height,
        rect.height,
        reveal,
    );
}

fn viewport_scroll(
    mut scroll: f32,
    caret: f32,
    size: f32,
    total: f32,
    viewport: f32,
    reveal: bool,
) -> f32 {
    if reveal {
        if caret < scroll {
            scroll = caret;
        }
        if caret + size > scroll + viewport {
            scroll = caret + size - viewport;
        }
    }
    scroll.clamp(0.0, (total + 2.0 - viewport).max(0.0))
}

fn draw<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    rect: Rect,
    (edit, view): (&TextEdit, &EditorView),
    (font, index, composition): (&Font, usize, Range<usize>),
    input: Interaction,
) -> AureaResult<()> {
    let layout = view.layout();
    let origin = Point::new(rect.x - edit.scroll, rect.y - view.scroll_y);
    let ink = ui.theme.palette.text.fade(if input.enabled {
        1.0
    } else {
        ui.theme.disabled_alpha
    });
    let paint = fill(ink);
    let start = layout
        .lines
        .partition_point(|line| line.top + line.height < view.scroll_y);
    for line in &layout.lines[start..] {
        if line.top > view.scroll_y + rect.height {
            break;
        }
        if input.focused {
            selection(ui, line, origin, edit.selection(), &composition)?;
        }
        ui.ctx.draw_text_directed(
            &layout.text[line.range.clone()],
            Point::new(origin.x + line.x, origin.y + line.baseline),
            font,
            &paint,
            Some(line.rtl),
        )?;
    }
    if input.focused {
        let caret = view.caret(index);
        let bar = Rect::new(
            (origin.x + caret.x).round(),
            origin.y + caret.y,
            1.5,
            caret.height,
        );
        ui.state.ime_area = Some(bar);
        let since = ui.state.blink_from.elapsed();
        let on = !ui.theme.caret_blink || since >= BLINK_FOR || since.as_secs_f32().fract() < 0.6;
        if on && (!edit.has_selection() || !composition.is_empty()) {
            ui.fill(bar, ui.theme.palette.accent)?;
        }
    }
    Ok(())
}

fn selection<Id: WidgetId>(
    ui: &mut Ui<'_, Id>,
    line: &LayoutLine,
    origin: Point,
    selected: Range<usize>,
    composition: &Range<usize>,
) -> AureaResult<()> {
    let range = if composition.is_empty() {
        selected
    } else {
        composition.clone()
    };
    let a = range.start.max(line.range.start);
    let b = range.end.min(line.range.end);
    if a >= b {
        return Ok(());
    }
    for (left, right) in line
        .shaped
        .selection_spans(a - line.range.start..b - line.range.start)
    {
        let band = if composition.is_empty() {
            Rect::new(
                origin.x + line.x + left,
                origin.y + line.top,
                right - left,
                line.height,
            )
        } else {
            Rect::new(
                origin.x + line.x + left,
                origin.y + line.baseline + 1.0,
                right - left,
                1.0,
            )
        };
        let ink = if composition.is_empty() {
            ui.theme.palette.selection
        } else {
            ui.theme.palette.text
        };
        ui.fill(band, ink)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{CpuRasterizer, Renderer};
    use crate::ui::{Theme, UiState};
    use crate::window::WindowEvent;
    use std::sync::Arc;

    fn frame(state: &mut UiState<u8>, raster: &mut CpuRasterizer, text: &mut String) -> Response {
        let theme = Theme::dark();
        let mut ctx = raster.begin_frame().expect("begin");
        let response = {
            let mut ui = state.frame(&mut *ctx, &theme);
            TextArea::new(text)
                .show(&mut ui, 1, Rect::new(0.0, 0.0, 150.0, 80.0))
                .expect("editor")
        };
        drop(ctx);
        raster.end_frame().expect("end");
        response
    }

    fn key(state: &mut UiState<u8>, code: KeyCode, ctrl: bool) {
        state.handle(&WindowEvent::KeyInput {
            key: code,
            pressed: true,
            modifiers: Modifiers {
                ctrl,
                meta: ctrl,
                ..Modifiers::default()
            },
        });
    }

    #[test]
    fn enter_and_multiline_input_edit_with_undo_and_command_enter_submits() {
        let mut state = UiState::new();
        state.set_reduced_motion(true);
        let mut raster = CpuRasterizer::new(150, 80);
        let mut text = "first".to_string();
        frame(&mut state, &mut raster, &mut text);
        state.focus(1, 0);
        frame(&mut state, &mut raster, &mut text);
        key(&mut state, KeyCode::End, true);
        frame(&mut state, &mut raster, &mut text);
        key(&mut state, KeyCode::Enter, false);
        assert!(frame(&mut state, &mut raster, &mut text).changed);
        state.handle(&WindowEvent::TextInput {
            text: "한글\r\nthird\rfourth".into(),
        });
        frame(&mut state, &mut raster, &mut text);
        assert_eq!(text, "first\n한글\nthird\nfourth");
        key(&mut state, KeyCode::Z, true);
        frame(&mut state, &mut raster, &mut text);
        assert_eq!(text, "first\n");
        key(&mut state, KeyCode::Enter, true);
        assert!(frame(&mut state, &mut raster, &mut text).submitted);
        assert_eq!(text, "first\n");
    }

    #[test]
    fn vertical_navigation_keeps_its_column_across_short_lines() {
        let text = "abcdefgh\nx\nabcdefgh";
        let font = Font::new("sans-serif", 14.0);
        let mut view = EditorView::default();
        view.prepare(text, &font, LayoutOptions::default())
            .expect("layout");
        let mut edit = TextEdit::new();
        edit.set_caret(text, 6, false);
        navigate(
            &mut edit,
            &mut view,
            text,
            KeyCode::Down,
            Modifiers::default(),
            80.0,
        );
        assert_eq!(edit.caret(), 10);
        navigate(
            &mut edit,
            &mut view,
            text,
            KeyCode::Down,
            Modifiers::default(),
            80.0,
        );
        assert_eq!(edit.caret(), 17);
        navigate(
            &mut edit,
            &mut view,
            text,
            KeyCode::Home,
            Modifiers::default(),
            80.0,
        );
        assert_eq!(edit.caret(), 11);
        navigate(
            &mut edit,
            &mut view,
            text,
            KeyCode::End,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
            80.0,
        );
        assert_eq!(edit.selected(text), "abcdefgh");
    }

    #[test]
    fn wrapped_line_hits_keep_the_caret_on_the_clicked_line_and_reuse_layout() {
        let text = "abcdefghabcdefghabcdefgh";
        let font = Font::new("sans-serif", 14.0);
        let options = LayoutOptions {
            max_width: Some(50.0),
            ..LayoutOptions::default()
        };
        let mut view = EditorView::default();
        view.prepare(text, &font, options).expect("layout");
        let second = &view.layout().lines[1];
        let (start, top, height) = (second.range.start, second.top, second.height);
        let at = view.hit(Point::new(0.0, top + height * 0.5));
        assert_eq!(at, start);
        assert_eq!(view.caret(at).y, top);
        let shaped = view.layout().lines[1].shaped.clone();
        view.prepare(text, &font, options).expect("same layout");
        assert!(Arc::ptr_eq(&shaped, &view.layout().lines[1].shaped));
        assert_eq!(view.caret(at).y, top);
        let mut edit = TextEdit::new();
        edit.set_caret(text, at, false);
        navigate(
            &mut edit,
            &mut view,
            text,
            KeyCode::Down,
            Modifiers::default(),
            80.0,
        );
        assert!(view.caret(edit.caret()).y > top);
    }

    #[test]
    fn composition_replaces_a_multiline_selection_only_when_committed() {
        let mut state = UiState::new();
        state.set_reduced_motion(true);
        let mut raster = CpuRasterizer::new(150, 80);
        let mut text = "first\nsecond".to_string();
        frame(&mut state, &mut raster, &mut text);
        state.focus(1, 0);
        frame(&mut state, &mut raster, &mut text);
        state.edit.select_all(&text);
        state.handle(&WindowEvent::ImePreedit {
            text: "한\n글".into(),
            cursor: Some(3),
        });
        frame(&mut state, &mut raster, &mut text);
        assert_eq!(text, "first\nsecond");
        assert_eq!(
            state
                .edit
                .multiline
                .as_ref()
                .expect("view")
                .layout()
                .text
                .as_ref(),
            "한\n글"
        );
        state.handle(&WindowEvent::TextInput {
            text: "한\n글".into(),
        });
        frame(&mut state, &mut raster, &mut text);
        assert_eq!(text, "한\n글");
        key(&mut state, KeyCode::Z, true);
        frame(&mut state, &mut raster, &mut text);
        assert_eq!(text, "first\nsecond");
    }
}
