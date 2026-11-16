//! The same canvas widgets in two looks an app draws itself: glass on the left, Material 3 on the
//! right. Each widget keeps its own behaviour; `look` takes its input and hands its state to the
//! app, which draws it.
//!
//! `AUREA_SNAPSHOT=out.png cargo run --example custom_look` draws one frame to a PNG and exits.

use std::cell::RefCell;
use std::env::var;
use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::rc::{Rc, Weak};

use png::{BitDepth, ColorType, Encoder};

use aurea::elements::{Container, Orientation, Stack};
use aurea::render::{
    Canvas, Color, Corners, CpuRasterizer, DrawingContext, GradientStop, Paint, PaintStyle, Path,
    Point, RadialGradient, Renderer, RendererBackend, request_canvas_redraw,
    request_canvas_redraw_at,
};
use aurea::ui::{
    Align, Button, ButtonParts, Chip, ChipParts, Component, Eased, Look, Panel, Shadow, Slider,
    SliderParts, Theme, Toggle, ToggleParts, Ui, UiState,
};
use aurea::{AureaResult, Rect, Window, WindowEvent, reduced_motion};

const WIDTH: u32 = 880;
const HEIGHT: u32 = 420;
const HALF: f32 = WIDTH as f32 / 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Id {
    GlassPlay,
    GlassShare,
    GlassWifi,
    GlassVolume,
    GlassTag(u8),
    MPrimary,
    MTonal,
    MWifi,
    MVolume,
    MTag(u8),
}

struct Model {
    wifi: bool,
    volume: f32,
    tags: [bool; 3],
    m_wifi: bool,
    m_volume: f32,
    m_tags: [bool; 3],
    plays: u32,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            wifi: true,
            volume: 0.62,
            tags: [true, false, true],
            m_wifi: true,
            m_volume: 0.4,
            m_tags: [false, true, false],
            plays: 0,
        }
    }
}

const fn white(alpha: f32) -> Color {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Color::rgba(255, 255, 255, (alpha * 255.0) as u8)
}

fn stop(offset: f32, color: Color) -> GradientStop {
    GradientStop { offset, color }
}

/// A capsule or card of glass: continuous corners, a frosted face, light on top, a rim brighter
/// along the top, a soft shadow. There is no backdrop blur yet, so the frost is a translucent
/// face over what is behind.
fn glass(ui: &mut Ui<'_, Id>, rect: Rect, radius: f32, lift: f32) -> AureaResult<()> {
    Panel::new()
        .corners(Corners::all(radius).smooth(0.6))
        .shadow(
            Shadow::new(Color::rgba(0, 0, 0, 70))
                .offset(0.0, 6.0)
                .blur(18.0),
        )
        .frost(14.0, 1.5)
        .fill_vertical(white(0.18 + lift), white(0.06 + lift / 2.0))
        .sheen(white(0.28), 0.45)
        .rim_vertical(1.0, white(0.65), white(0.15))
        .paint(ui, rect)
}

fn wallpaper(ui: &mut Ui<'_, Id>, area: Rect) -> AureaResult<()> {
    ui.fill(area, Color::rgb(18, 22, 48))?;
    let blobs = [
        (
            Point::new(area.x + 90.0, area.y + 80.0),
            220.0,
            Color::rgb(255, 92, 140),
        ),
        (
            Point::new(area.x + 360.0, area.y + 120.0),
            240.0,
            Color::rgb(70, 140, 255),
        ),
        (
            Point::new(area.x + 220.0, area.y + 380.0),
            260.0,
            Color::rgb(60, 220, 180),
        ),
    ];
    for (center, radius, color) in blobs {
        let glow = RadialGradient {
            center,
            radius,
            stops: vec![stop(0.0, color), stop(1.0, color.with_alpha(0))],
        };
        ui.ctx.fill_radial_gradient(&glow, area)?;
    }
    // sharp lines and letters, so the frost shows
    let mut x = area.x;
    while x < area.right() {
        ui.fill(Rect::new(x, area.y, 1.0, area.height), white(0.35))?;
        x += 24.0;
    }
    let mut y = area.y;
    while y < area.bottom() {
        ui.fill(Rect::new(area.x, y, area.width, 1.0), white(0.35))?;
        y += 24.0;
    }
    let fonts = ui.fonts();
    for (row, word) in (0u8..).zip(["aurea", "glass", "frost"]) {
        let at = Rect::new(
            area.x + 20.0,
            area.y + 120.0 + 90.0 * f32::from(row),
            area.width,
            40.0,
        );
        ui.text(word, at, &fonts.strong, white(0.9), Align::Start)?;
    }
    Ok(())
}

fn label(
    ui: &mut Ui<'_, Id>,
    text: &str,
    rect: Rect,
    color: Color,
    align: Align,
) -> AureaResult<()> {
    let fonts = ui.fonts();
    ui.text(text, rect, &fonts.text, color, align)
}

fn glass_button(ui: &mut Ui<'_, Id>, look: &Look<ButtonParts<'_>>) -> AureaResult<()> {
    let s = look.state;
    let rect = look.rect.inset(s.press * 1.5);
    glass(ui, rect, rect.height / 2.0, 0.12 * s.hover + 0.08 * s.press)?;
    label(ui, look.parts.label, rect, white(0.95), Align::Center)
}

fn glass_toggle(ui: &mut Ui<'_, Id>, look: &Look<ToggleParts<'_>>) -> AureaResult<()> {
    let r = look.rect;
    glass(ui, r, r.height / 2.0, 0.05 * look.state.hover)?;
    let p = look.parts.progress;
    if p > 0.0 {
        ui.round(
            r.inset(2.0),
            (r.height - 4.0) / 2.0,
            Color::rgb(60, 220, 140).fade(0.7 * p),
        )?;
    }
    let d = r.height - 6.0;
    let x = r.x + 3.0 + (r.width - d - 6.0) * p;
    let knob = Rect::new(x, r.y + 3.0, d, d);
    ui.shadow(knob, d / 2.0, 6.0, Color::rgba(0, 0, 0, 90))?;
    ui.round(knob, d / 2.0, white(0.98))
}

fn glass_volume(ui: &mut Ui<'_, Id>, look: &Look<SliderParts<'_>>) -> AureaResult<()> {
    let g = look.parts.groove;
    let track = Rect::new(g.x, g.center().y - 3.0, g.width, 6.0);
    glass(ui, track, 3.0, 0.0)?;
    let filled = Rect::new(
        track.x,
        track.y,
        track.width * look.parts.position,
        track.height,
    );
    ui.round(filled, 3.0, white(0.85))?;
    let k = look.parts.knob;
    let d = 22.0 + 4.0 * look.state.press;
    let knob = Rect::new(k.center().x - d / 2.0, k.center().y - d / 2.0, d, d);
    ui.shadow(knob, d / 2.0, 8.0, Color::rgba(0, 0, 0, 100))?;
    ui.round(knob, d / 2.0, white(0.98))
}

fn glass_tag(ui: &mut Ui<'_, Id>, look: &Look<ChipParts<'_>>) -> AureaResult<()> {
    let on = look.parts.progress;
    glass(
        ui,
        look.rect,
        look.rect.height / 2.0,
        0.18 * on + 0.06 * look.state.hover,
    )?;
    label(
        ui,
        look.parts.label,
        look.rect,
        white(0.75 + 0.25 * on),
        Align::Center,
    )
}

fn glass_panel(ui: &mut Ui<'_, Id>, m: &mut Model) -> AureaResult<()> {
    let area = Rect::new(0.0, 0.0, HALF, HEIGHT as f32);
    wallpaper(ui, area)?;
    let card = Rect::new(36.0, 40.0, HALF - 72.0, 340.0);
    glass(ui, card, 28.0, 0.0)?;
    let fonts = ui.fonts();
    ui.text(
        "Now playing",
        Rect::new(64.0, 60.0, 300.0, 30.0),
        &fonts.strong,
        white(0.95),
        Align::Start,
    )?;
    let plays = format!("Played {} times", m.plays);
    label(
        ui,
        &plays,
        Rect::new(64.0, 88.0, 300.0, 22.0),
        white(0.7),
        Align::Start,
    )?;

    let play = Button::new("Play").look(ui, Id::GlassPlay, Rect::new(64.0, 126.0, 140.0, 44.0));
    glass_button(ui, &play)?;
    if play.response.clicked {
        m.plays += 1;
    }
    let share = Button::new("Share").look(ui, Id::GlassShare, Rect::new(220.0, 126.0, 140.0, 44.0));
    glass_button(ui, &share)?;

    label(
        ui,
        "Wi-Fi",
        Rect::new(64.0, 196.0, 200.0, 32.0),
        white(0.9),
        Align::Start,
    )?;
    let wifi =
        Toggle::new(&mut m.wifi).look(ui, Id::GlassWifi, Rect::new(310.0, 196.0, 52.0, 32.0));
    glass_toggle(ui, &wifi)?;

    label(
        ui,
        "Volume",
        Rect::new(64.0, 244.0, 200.0, 22.0),
        white(0.9),
        Align::Start,
    )?;
    let volume = Slider::new(&mut m.volume, 0.0..=1.0).look(
        ui,
        Id::GlassVolume,
        Rect::new(64.0, 268.0, 298.0, 24.0),
    );
    glass_volume(ui, &volume)?;

    for (i, name) in (0u8..).zip(["Chill", "Focus", "Night"]) {
        let at = Rect::new(64.0 + f32::from(i) * 100.0, 316.0, 90.0, 34.0);
        let tag = Chip::new(name, &mut m.tags[usize::from(i)]).look(ui, Id::GlassTag(i), at);
        glass_tag(ui, &tag)?;
    }
    Ok(())
}

/// Material 3 baseline roles, light scheme.
mod m3 {
    use aurea::render::Color;
    pub const PRIMARY: Color = Color::rgb(103, 80, 164);
    pub const ON_PRIMARY: Color = Color::rgb(255, 255, 255);
    pub const SECONDARY_CONTAINER: Color = Color::rgb(232, 222, 248);
    pub const ON_SECONDARY_CONTAINER: Color = Color::rgb(29, 25, 43);
    pub const SURFACE: Color = Color::rgb(254, 247, 255);
    pub const SURFACE_CONTAINER: Color = Color::rgb(243, 237, 247);
    pub const SURFACE_HIGHEST: Color = Color::rgb(230, 224, 233);
    pub const ON_SURFACE: Color = Color::rgb(29, 27, 32);
    pub const OUTLINE: Color = Color::rgb(121, 116, 126);
    /// State layer opacities: hover, then pressed.
    pub const HOVER: f32 = 0.08;
    pub const PRESSED: f32 = 0.12;
}

/// The state layer over a shape, and the ripple spreading from where the press landed.
fn state_layer(
    ui: &mut Ui<'_, Id>,
    rect: Rect,
    radius: f32,
    ink: Color,
    look_state: Eased,
    held_at: Option<Point>,
) -> AureaResult<()> {
    let layer = m3::HOVER * look_state.hover.max(look_state.press);
    ui.round(rect, radius, ink.fade(layer))?;
    if let Some(at) = held_at {
        let reach = rect.width.hypot(rect.height) * look_state.press;
        let shape = Path::rounded_rect(rect, radius);
        ui.ctx.save()?;
        ui.ctx.clip_path(&shape)?;
        let ripple = Rect::new(at.x - reach, at.y - reach, reach * 2.0, reach * 2.0);
        ui.round(ripple, reach, ink.fade(m3::PRESSED))?;
        ui.ctx.restore()?;
    }
    Ok(())
}

fn filled_button(
    ui: &mut Ui<'_, Id>,
    look: &Look<ButtonParts<'_>>,
    fill: Color,
    ink: Color,
) -> AureaResult<()> {
    let r = look.rect;
    let radius = r.height / 2.0;
    ui.round(r, radius, fill)?;
    state_layer(ui, r, radius, ink, look.state, look.held_at)?;
    label(ui, look.parts.label, r, ink, Align::Center)
}

fn material_toggle(ui: &mut Ui<'_, Id>, look: &Look<ToggleParts<'_>>) -> AureaResult<()> {
    let r = look.rect;
    let p = look.parts.progress;
    ui.round(r, r.height / 2.0, m3::SURFACE_HIGHEST.mix(m3::PRIMARY, p))?;
    if p < 1.0 {
        let rim = Paint::new()
            .color(m3::OUTLINE.fade(1.0 - p))
            .style(PaintStyle::Stroke)
            .stroke_width(2.0);
        ui.ctx
            .draw_round_rect(r.inset(1.0), r.height / 2.0 - 1.0, &rim)?;
    }
    let d = 16.0 + 8.0 * p + 4.0 * look.state.press;
    let cx = r.x + 16.0 + (r.width - 32.0) * p;
    let thumb = Rect::new(cx - d / 2.0, r.center().y - d / 2.0, d, d);
    ui.round(thumb, d / 2.0, m3::OUTLINE.mix(m3::ON_PRIMARY, p))
}

fn material_volume(ui: &mut Ui<'_, Id>, look: &Look<SliderParts<'_>>) -> AureaResult<()> {
    let g = look.parts.groove;
    let track = Rect::new(g.x, g.center().y - 2.0, g.width, 4.0);
    ui.round(track, 2.0, m3::SECONDARY_CONTAINER)?;
    let active = Rect::new(track.x, track.y, track.width * look.parts.position, 4.0);
    ui.round(active, 2.0, m3::PRIMARY)?;
    let c = look.parts.knob.center();
    let halo = 40.0 * look.state.hover.max(look.state.press);
    if halo > 0.0 {
        ui.round(
            Rect::new(c.x - halo / 2.0, c.y - halo / 2.0, halo, halo),
            halo / 2.0,
            m3::PRIMARY.fade(m3::PRESSED),
        )?;
    }
    ui.round(
        Rect::new(c.x - 10.0, c.y - 10.0, 20.0, 20.0),
        10.0,
        m3::PRIMARY,
    )
}

fn material_tag(ui: &mut Ui<'_, Id>, look: &Look<ChipParts<'_>>) -> AureaResult<()> {
    let r = look.rect;
    let on = look.parts.progress;
    ui.round(r, 8.0, m3::SECONDARY_CONTAINER.fade(on))?;
    if on < 1.0 {
        let rim = Paint::new()
            .color(m3::OUTLINE.fade(1.0 - on))
            .style(PaintStyle::Stroke)
            .stroke_width(1.0);
        ui.ctx.draw_round_rect(r.inset(0.5), 8.0, &rim)?;
    }
    state_layer(ui, r, 8.0, m3::ON_SURFACE, look.state, look.held_at)?;
    let text = if on > 0.5 {
        format!("✓ {}", look.parts.label)
    } else {
        look.parts.label.to_string()
    };
    label(ui, &text, r, m3::ON_SECONDARY_CONTAINER, Align::Center)
}

fn material_panel(ui: &mut Ui<'_, Id>, m: &mut Model) -> AureaResult<()> {
    let area = Rect::new(HALF, 0.0, HALF, HEIGHT as f32);
    ui.fill(area, m3::SURFACE)?;
    let card = Rect::new(HALF + 36.0, 40.0, HALF - 72.0, 340.0);
    ui.shadow(card, 12.0, 6.0, Color::rgba(0, 0, 0, 40))?;
    ui.round(card, 12.0, m3::SURFACE_CONTAINER)?;
    let fonts = ui.fonts();
    let x = HALF + 64.0;
    ui.text(
        "Now playing",
        Rect::new(x, 60.0, 300.0, 30.0),
        &fonts.strong,
        m3::ON_SURFACE,
        Align::Start,
    )?;
    label(
        ui,
        "Material 3, drawn by the app",
        Rect::new(x, 88.0, 300.0, 22.0),
        m3::OUTLINE,
        Align::Start,
    )?;

    let play = Button::new("Play").look(ui, Id::MPrimary, Rect::new(x, 126.0, 140.0, 40.0));
    filled_button(ui, &play, m3::PRIMARY, m3::ON_PRIMARY)?;
    if play.response.clicked {
        m.plays += 1;
    }
    let share = Button::new("Share").look(ui, Id::MTonal, Rect::new(x + 156.0, 126.0, 140.0, 40.0));
    filled_button(
        ui,
        &share,
        m3::SECONDARY_CONTAINER,
        m3::ON_SECONDARY_CONTAINER,
    )?;

    label(
        ui,
        "Wi-Fi",
        Rect::new(x, 196.0, 200.0, 32.0),
        m3::ON_SURFACE,
        Align::Start,
    )?;
    let wifi =
        Toggle::new(&mut m.m_wifi).look(ui, Id::MWifi, Rect::new(x + 246.0, 196.0, 52.0, 32.0));
    material_toggle(ui, &wifi)?;

    label(
        ui,
        "Volume",
        Rect::new(x, 244.0, 200.0, 22.0),
        m3::ON_SURFACE,
        Align::Start,
    )?;
    let volume = Slider::new(&mut m.m_volume, 0.0..=1.0).look(
        ui,
        Id::MVolume,
        Rect::new(x, 268.0, 298.0, 24.0),
    );
    material_volume(ui, &volume)?;

    for (i, name) in (0u8..).zip(["Chill", "Focus", "Night"]) {
        let at = Rect::new(x + f32::from(i) * 100.0, 316.0, 90.0, 32.0);
        let tag = Chip::new(name, &mut m.m_tags[usize::from(i)]).look(ui, Id::MTag(i), at);
        material_tag(ui, &tag)?;
    }
    Ok(())
}

fn draw(ui: &mut Ui<'_, Id>, m: &mut Model) -> AureaResult<()> {
    glass_panel(ui, m)?;
    material_panel(ui, m)
}

/// Draws one frame on the CPU and writes it to `path`.
fn snapshot(path: &str) -> Result<(), Box<dyn Error>> {
    let mut raster = CpuRasterizer::new(WIDTH, HEIGHT);
    let mut state = UiState::<Id>::new();
    state.set_reduced_motion(true);
    let theme = Theme::light();
    let mut model = Model::default();
    {
        let mut ctx = raster.begin_frame()?;
        let mut ui = state.frame(&mut *ctx, &theme);
        draw(&mut ui, &mut model)?;
    }
    raster.end_frame()?;
    let frame = raster.frame_output().ok_or("no frame drawn")?;
    let mut png = Encoder::new(BufWriter::new(File::create(path)?), WIDTH, HEIGHT);
    png.set_color(ColorType::Rgba);
    png.set_depth(BitDepth::Eight);
    // the rasterizer keeps BGRA; PNG wants RGBA
    let mut rgba = frame.bytes().to_vec();
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    png.write_header()?.write_image_data(&rgba)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    if let Ok(path) = var("AUREA_SNAPSHOT") {
        return snapshot(&path);
    }
    #[allow(clippy::cast_possible_wrap)]
    let mut window = Window::new("Custom looks", WIDTH as i32, HEIGHT as i32)?;
    let canvas = Canvas::new(WIDTH, HEIGHT, RendererBackend::Cpu)?;
    let id = canvas.id();
    let state = Rc::new(RefCell::new(UiState::<Id>::new()));
    state.borrow_mut().set_reduced_motion(reduced_motion());
    let model = Rc::new(RefCell::new(Model::default()));
    let theme = Theme::light();
    let window_ref: Rc<RefCell<Weak<Window>>> = Rc::new(RefCell::new(Weak::new()));

    let drawn = Rc::clone(&state);
    let seen = Rc::clone(&window_ref);
    canvas.set_draw_callback(move |ctx: &mut dyn DrawingContext| {
        let mut state = drawn.borrow_mut();
        {
            let mut ui = state.frame(ctx, &theme);
            draw(&mut ui, &mut model.borrow_mut())?;
        }
        if let Some(window) = seen.borrow().upgrade() {
            state.sync_window(&window);
        }
        if state.wants_frame() {
            request_canvas_redraw(id);
        } else if let Some(at) = state.next_frame_at() {
            request_canvas_redraw_at(id, at);
        }
        Ok(())
    })?;

    let input = Rc::clone(&state);
    window.on_event(move |event| {
        if input.borrow_mut().handle(event) || matches!(event, WindowEvent::Resized { .. }) {
            request_canvas_redraw(id);
        }
    });

    let mut layout = Stack::new(Orientation::Vertical)?;
    layout.set_padding(0)?;
    layout.add(canvas)?;
    window.set_content(layout)?;
    let window = Rc::new(window);
    *window_ref.borrow_mut() = Rc::downgrade(&window);
    window.show();
    window.run()?;
    Ok(())
}
