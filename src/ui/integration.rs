//! Contracts shared by applications embedding editors and custom controls.

use super::{
    Button, Response, ScrollArea, ScrollStyle, Slider, TextArea, TextField, Theme, Ui, UiState,
};
use crate::render::{Color, CpuRasterizer, Rect, Renderer};
use crate::window::{KeyCode, Modifiers, MouseButton, MouseButtons, WindowEvent};

struct Rig {
    state: UiState<u8>,
    raster: CpuRasterizer,
    theme: Theme,
}

impl Rig {
    fn new() -> Self {
        Self {
            state: UiState::new(),
            raster: CpuRasterizer::new(240, 160),
            theme: Theme::dark(),
        }
    }

    fn frame<R>(&mut self, draw: impl FnOnce(&mut Ui<'_, u8>) -> R) -> R {
        let mut ctx = self.raster.begin_frame().expect("frame");
        let out = draw(&mut self.state.frame(&mut *ctx, &self.theme));
        drop(ctx);
        self.raster.end_frame().expect("end");
        out
    }

    fn wheel(&mut self, x: f64, y: f64, dx: f64, dy: f64) {
        self.state.handle(&WindowEvent::MouseMove {
            x,
            y,
            buttons: MouseButtons::default(),
            modifiers: Modifiers::default(),
            time: Default::default(),
        });
        self.state.handle(&WindowEvent::MouseWheel {
            delta_x: dx,
            delta_y: dy,
            modifiers: Modifiers::default(),
        });
    }

    fn click(&mut self, x: f64, y: f64) {
        for pressed in [true, false] {
            self.state.handle(&WindowEvent::MouseButton {
                button: MouseButton::Left,
                pressed,
                x,
                y,
                click_count: 1,
                modifiers: Modifiers::default(),
                time: Default::default(),
            });
        }
    }

    fn key(&mut self, key: KeyCode) {
        self.state.handle(&WindowEvent::KeyInput {
            key,
            pressed: true,
            modifiers: Modifiers::default(),
        });
    }
}

fn nested(ui: &mut Ui<'_, u8>, outer: &mut f32, inner: &mut f32) -> (f32, f32) {
    let style = ScrollStyle {
        step: 20.0,
        smooth: false,
        ..ScrollStyle::new(ui.theme)
    };
    ScrollArea::new(outer, 400.0)
        .style(style)
        .show(
            ui,
            1,
            Rect::new(0.0, 0.0, 220.0, 100.0),
            |ui, content, _| {
                ScrollArea::new(inner, 90.0).style(style).show(
                    ui,
                    2,
                    Rect::new(content.x, content.y, 180.0, 60.0),
                    |_, _, _| Ok(()),
                )?;
                Ok(())
            },
        )
        .expect("nested scroll");
    ui.wheel()
}

#[test]
fn inner_scroll_consumes_first_and_only_its_distance() {
    let mut rig = Rig::new();
    let (mut outer, mut inner) = (0.0, 0.0);
    rig.frame(|ui| nested(ui, &mut outer, &mut inner));
    rig.wheel(40.0, 30.0, 2.0, -2.0);
    let remaining = rig.frame(|ui| nested(ui, &mut outer, &mut inner));
    assert_eq!((inner, outer), (30.0, 10.0));
    assert_eq!(
        remaining,
        (2.0, 0.0),
        "horizontal input survives vertical scrolling"
    );
    assert!(
        rig.state.wants_frame(),
        "post-order movement must request its paint"
    );
    rig.frame(|ui| nested(ui, &mut outer, &mut inner));
    rig.wheel(40.0, 30.0, 0.0, 1.0);
    rig.frame(|ui| nested(ui, &mut outer, &mut inner));
    assert_eq!(
        (inner, outer),
        (10.0, 10.0),
        "child also gets first use going up"
    );
}

fn slider_in_scroll(ui: &mut Ui<'_, u8>, offset: &mut f32, value: &mut f32, enabled: bool) {
    let style = ScrollStyle {
        step: 20.0,
        smooth: false,
        ..ScrollStyle::new(ui.theme)
    };
    ScrollArea::new(offset, 300.0)
        .style(style)
        .show(
            ui,
            1,
            Rect::new(0.0, 0.0, 220.0, 100.0),
            |ui, content, _| {
                Slider::new(value, 0.0..=10.0)
                    .step(1.0)
                    .enabled(enabled)
                    .show(ui, 2, Rect::new(content.x, content.y, 180.0, 24.0))?;
                Ok(())
            },
        )
        .expect("slider scroll");
}

#[test]
fn a_slider_handles_wheel_before_its_parent_and_disabled_controls_pass_it_on() {
    let mut rig = Rig::new();
    let (mut offset, mut value) = (0.0, 1.0);
    rig.frame(|ui| slider_in_scroll(ui, &mut offset, &mut value, true));
    rig.wheel(40.0, 12.0, 0.0, -2.0);
    rig.frame(|ui| slider_in_scroll(ui, &mut offset, &mut value, true));
    assert_eq!(
        (value, offset),
        (0.0, 20.0),
        "unused notch bubbles at the range end"
    );
    offset = 0.0;
    value = 5.0;
    rig.frame(|ui| slider_in_scroll(ui, &mut offset, &mut value, false));
    rig.wheel(40.0, 12.0, 0.0, -1.0);
    rig.frame(|ui| slider_in_scroll(ui, &mut offset, &mut value, false));
    assert_eq!((value, offset), (5.0, 20.0));
}

#[test]
fn measured_layout_recomputes_overflow_on_resize() {
    let mut rig = Rig::new();
    let mut offset = 120.0;
    for (width, expected) in [(80.0, 100.0), (160.0, 0.0)] {
        let mut measures = 0;
        let mut draws = 0;
        rig.frame(|ui| {
            ScrollArea::measured(&mut offset)
                .show_measured(
                    ui,
                    1,
                    Rect::new(0.0, 0.0, width, 100.0),
                    |_, available| {
                        measures += 1;
                        Ok((if available < 100.0 { 200.0 } else { 80.0 }, available))
                    },
                    |_, content, _, measured_width| {
                        assert_eq!(measured_width, width);
                        draws += 1;
                        Ok(content.height)
                    },
                )
                .expect("measured");
        });
        assert_eq!(offset, expected);
        assert_eq!((measures, draws), (1, 1));
    }
}

fn editor(ui: &mut Ui<'_, u8>, text: &mut String, multiline: bool, enabled: bool) -> Response {
    let rect = Rect::new(0.0, 0.0, 160.0, 40.0);
    if multiline {
        TextArea::new(text)
            .enabled(enabled)
            .commit_on_blur(true)
            .show(ui, 1, rect)
    } else {
        TextField::new(text)
            .enabled(enabled)
            .commit_on_blur(true)
            .show(ui, 1, rect)
    }
    .expect("editor")
}

#[test]
fn text_commits_before_a_dependent_button_action_in_the_same_batch() {
    for multiline in [false, true] {
        let mut rig = Rig::new();
        let mut text = String::new();
        let mut events = Vec::new();
        let mut draw = |ui: &mut Ui<'_, u8>| {
            let response = editor(ui, &mut text, multiline, true);
            if response.committed {
                events.push("commit");
            }
            if Button::new("Apply")
                .show(ui, 2, Rect::new(0.0, 70.0, 120.0, 30.0))
                .expect("button")
                .clicked
            {
                events.push("apply");
            }
            response
        };
        rig.frame(&mut draw);
        rig.state.focus(1, 0);
        assert!(rig.frame(&mut draw).editing_started);
        assert!(!rig.frame(&mut draw).editing_started);
        rig.state
            .handle(&WindowEvent::TextInput { text: "new".into() });
        rig.click(30.0, 80.0);
        let response = rig.frame(&mut draw);
        assert!(response.changed && response.focus_lost && response.committed);
        assert!(!rig.frame(&mut draw).focus_lost);
        assert_eq!(events, ["commit", "apply"]);
        assert_eq!(text, "new");
    }
}

#[test]
fn submission_does_not_commit_again_on_blur_and_escape_does_not_commit() {
    let mut rig = Rig::new();
    let mut text = String::new();
    rig.frame(|ui| editor(ui, &mut text, false, true));
    rig.state.focus(1, 0);
    rig.state
        .handle(&WindowEvent::TextInput { text: "one".into() });
    rig.frame(|ui| editor(ui, &mut text, false, true));
    rig.key(KeyCode::Enter);
    let submitted = rig.frame(|ui| editor(ui, &mut text, false, true));
    assert!(submitted.submitted && submitted.committed);
    rig.state.clear_focus();
    let blurred = rig.frame(|ui| editor(ui, &mut text, false, true));
    assert!(blurred.focus_lost && !blurred.committed);
    rig.state.focus(1, 0);
    rig.state
        .handle(&WindowEvent::TextInput { text: "two".into() });
    rig.key(KeyCode::Escape);
    let cancelled = rig.frame(|ui| editor(ui, &mut text, false, true));
    assert!(cancelled.cancelled && cancelled.focus_lost && !cancelled.committed);
    assert!(!cancelled.focused);
}

#[test]
fn disabling_an_editor_rejects_queued_text_without_committing() {
    for multiline in [false, true] {
        let mut rig = Rig::new();
        let mut text = "saved".to_string();
        rig.frame(|ui| editor(ui, &mut text, multiline, true));
        rig.state.focus(1, 0);
        rig.frame(|ui| editor(ui, &mut text, multiline, true));
        rig.state.handle(&WindowEvent::TextInput {
            text: "discard".into(),
        });
        let response = rig.frame(|ui| editor(ui, &mut text, multiline, false));
        assert_eq!(text, "saved");
        assert!(response.focus_lost && !response.changed && !response.committed);
    }
}

#[test]
fn custom_tracks_obey_nested_clips_and_restore_drawing_state() {
    let mut rig = Rig::new();
    let mut value = 0.5;
    rig.frame(|ui| {
        ui.clipped(Rect::new(20.0, 0.0, 60.0, 24.0), |ui| {
            Slider::new(&mut value, 0.0..=1.0).show_with_track(
                ui,
                1,
                Rect::new(0.0, 0.0, 220.0, 24.0),
                |ui, track| {
                    assert!(track.enabled);
                    assert_eq!(track.rect.height, 4.0);
                    ui.fill(Rect::new(0.0, 0.0, 240.0, 160.0), Color::rgb(255, 0, 0))?;
                    ui.ctx.set_alpha(0.1)
                },
            )?;
            Ok(())
        })
        .expect("clipped track");
        ui.fill(Rect::new(100.0, 30.0, 10.0, 10.0), Color::rgb(0, 255, 0))
            .expect("sentinel");
    });
    let out = rig.raster.frame_output().expect("pixels");
    let pixel = |x: usize, y: usize| &out.bytes()[(y * 240 + x) * 4..(y * 240 + x) * 4 + 4];
    assert_eq!(pixel(30, 12), [0, 0, 255, 255]);
    assert_eq!(pixel(30, 3), [0, 0, 0, 0], "track clip");
    assert_eq!(pixel(100, 12), [0, 0, 0, 0], "parent clip");
    assert_eq!(pixel(105, 35), [0, 255, 0, 255], "alpha and clip restored");
}

#[test]
fn custom_track_keeps_native_keyboard_input_and_disabled_opacity() {
    let mut rig = Rig::new();
    let mut value = 0.5;
    let mut draw = |ui: &mut Ui<'_, u8>, enabled| {
        Slider::new(&mut value, 0.0..=1.0)
            .step(0.1)
            .enabled(enabled)
            .show_with_track(ui, 1, Rect::new(0.0, 0.0, 220.0, 24.0), |ui, track| {
                if !enabled {
                    assert!(track.opacity < 1.0);
                }
                ui.fill(track.rect, Color::rgb(255, 0, 0))
            })
            .expect("custom slider")
    };
    rig.frame(|ui| draw(ui, true));
    rig.state.focus(1, 0);
    rig.key(KeyCode::Right);
    assert!(rig.frame(|ui| draw(ui, true)).changed);
    rig.key(KeyCode::Right);
    assert!(!rig.frame(|ui| draw(ui, false)).changed);
    assert!((value - 0.6).abs() < 0.0001);
}

#[test]
fn a_text_area_passes_wheel_to_its_parent_only_when_it_cannot_scroll() {
    for (text, expected) in [("short".to_string(), 20.0), ("line\n".repeat(40), 0.0)] {
        let mut rig = Rig::new();
        let mut offset = 0.0;
        let mut text = text;
        let mut draw = |ui: &mut Ui<'_, u8>| {
            let style = ScrollStyle {
                step: 20.0,
                smooth: false,
                ..ScrollStyle::new(ui.theme)
            };
            ScrollArea::new(&mut offset, 300.0)
                .style(style)
                .show(
                    ui,
                    1,
                    Rect::new(0.0, 0.0, 220.0, 100.0),
                    |ui, content, _| {
                        TextArea::new(&mut text).show(
                            ui,
                            2,
                            Rect::new(content.x, content.y, 180.0, 60.0),
                        )?;
                        Ok(())
                    },
                )
                .expect("text scroll");
        };
        rig.frame(&mut draw);
        rig.wheel(40.0, 30.0, 0.0, -1.0);
        rig.frame(&mut draw);
        assert_eq!(offset, expected);
    }
}

#[test]
fn blur_commit_is_opt_in() {
    let mut rig = Rig::new();
    let mut text = String::new();
    let mut draw = |ui: &mut Ui<'_, u8>| {
        TextField::new(&mut text)
            .show(ui, 1, Rect::new(0.0, 0.0, 160.0, 30.0))
            .expect("default field")
    };
    rig.frame(&mut draw);
    rig.state.focus(1, 0);
    rig.state.handle(&WindowEvent::TextInput {
        text: "draft".into(),
    });
    rig.frame(&mut draw);
    rig.state.clear_focus();
    let response = rig.frame(&mut draw);
    assert!(response.focus_lost && !response.committed && !response.submitted);
}
