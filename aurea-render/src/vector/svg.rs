//! A small SVG reader for icons and simple vector art.

use aurea_foundation::AureaResult;

use super::path_data::{SvgError, append_path_data};
use crate::drawing::DrawingContext;
use crate::numeric::f32_to_u8_clamped;
use crate::types::{Color, FillRule, Paint, PaintStyle, Path, PathCommand, Point, Rect, Transform};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Ink {
    None,
    Color(Color),
    /// Whatever the image is drawn in.
    Current,
}

#[derive(Debug, Clone)]
struct Shape {
    path: Path,
    fill: Ink,
    stroke: Ink,
    stroke_width: f32,
    fill_opacity: f32,
    stroke_opacity: f32,
}

/// A parsed SVG, drawn at any size.
#[derive(Debug, Clone)]
pub struct VectorImage {
    /// The viewBox: what the image's coordinates span.
    pub view_box: Rect,
    shapes: Vec<Shape>,
}

#[derive(Clone, Copy)]
struct Style {
    fill: Ink,
    stroke: Ink,
    stroke_width: f32,
    fill_rule: FillRule,
    opacity: f32,
    fill_opacity: f32,
    stroke_opacity: f32,
    transform: Transform,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Ink::Color(Color::rgb(0, 0, 0)),
            stroke: Ink::None,
            stroke_width: 1.0,
            fill_rule: FillRule::NonZero,
            opacity: 1.0,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            transform: Transform::identity(),
        }
    }
}

impl VectorImage {
    /// Reads SVG text.
    pub fn parse(svg: &str) -> Result<Self, SvgError> {
        let mut image = Self {
            view_box: Rect::new(0.0, 0.0, 0.0, 0.0),
            shapes: Vec::new(),
        };
        let mut stack: Vec<Style> = vec![Style::default()];
        let mut tags = Tags { s: svg, i: 0 };
        while let Some(tag) = tags.next()? {
            let Tag {
                name,
                attrs,
                closing,
                empty,
                at,
            } = tag;
            if closing {
                if matches!(name, "g" | "svg") && stack.len() > 1 {
                    stack.pop();
                }
                continue;
            }
            let parent = *stack.last().unwrap_or(&Style::default());
            let style = apply_style(parent, attrs, at)?;
            match name {
                "svg" => {
                    image.view_box = view_box(attrs, at)?;
                }
                "g" => {}
                _ => {
                    if let Some(path) = shape_path(name, attrs, at)? {
                        image.push(path, style);
                    }
                }
            }
            if matches!(name, "g" | "svg") && !empty {
                stack.push(style);
            }
        }
        Ok(image)
    }

    /// An image that draws nothing.
    pub fn empty(view_box: Rect) -> Self {
        Self {
            view_box,
            shapes: Vec::new(),
        }
    }

    /// Whether the image draws nothing.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// A single path from SVG path data, filled in the drawing colour, in a `view_box` that its
    /// coordinates span.
    pub fn from_path_data(d: &str, view_box: Rect) -> Result<Self, SvgError> {
        let mut path = Path::new();
        append_path_data(d, &mut path)?;
        Ok(Self {
            view_box,
            shapes: vec![Shape {
                path,
                fill: Ink::Current,
                stroke: Ink::None,
                stroke_width: 0.0,
                fill_opacity: 1.0,
                stroke_opacity: 1.0,
            }],
        })
    }

    /// Like [`from_path_data`](Self::from_path_data), stroked rather than filled: how line icons
    /// are drawn.
    pub fn stroked_path_data(d: &str, view_box: Rect, width: f32) -> Result<Self, SvgError> {
        let mut image = Self::from_path_data(d, view_box)?;
        if let Some(shape) = image.shapes.first_mut() {
            shape.fill = Ink::None;
            shape.stroke = Ink::Current;
            shape.stroke_width = width;
        }
        Ok(image)
    }

    /// Draws the image into `dest`, scaled to fit and centred, with `currentColor` as `color`.
    pub fn draw(&self, ctx: &mut dyn DrawingContext, dest: Rect, color: Color) -> AureaResult<()> {
        let vb = self.view_box;
        if vb.width <= 0.0 || vb.height <= 0.0 || dest.width <= 0.0 || dest.height <= 0.0 {
            return Ok(());
        }
        let scale = (dest.width / vb.width).min(dest.height / vb.height);
        let x = dest.x + (dest.width - vb.width * scale) / 2.0 - vb.x * scale;
        let y = dest.y + (dest.height - vb.height * scale) / 2.0 - vb.y * scale;
        let place = |p: Point| Point::new(x + p.x * scale, y + p.y * scale);
        let mut placed = Path::new();
        for shape in &self.shapes {
            placed.commands.clear();
            placed.fill_rule = shape.path.fill_rule;
            placed
                .commands
                .extend(shape.path.commands.iter().map(|c| map_command(*c, &place)));
            if let Some(c) = resolve(shape.fill, color, shape.fill_opacity) {
                ctx.draw_path(&placed, &Paint::new().color(c))?;
            }
            if let Some(c) = resolve(shape.stroke, color, shape.stroke_opacity) {
                let paint = Paint::new()
                    .color(c)
                    .style(PaintStyle::Stroke)
                    .stroke_width(shape.stroke_width * scale);
                ctx.draw_path(&placed, &paint)?;
            }
        }
        Ok(())
    }

    fn push(&mut self, mut path: Path, style: Style) {
        path.fill_rule = style.fill_rule;
        let t = style.transform;
        if t != Transform::identity() {
            for c in &mut path.commands {
                *c = map_command(*c, &|p| t.map_point(p));
            }
        }
        let det = (t.m11 * t.m22 - t.m12 * t.m21).abs();
        self.shapes.push(Shape {
            path,
            fill: style.fill,
            stroke: style.stroke,
            stroke_width: style.stroke_width * det.sqrt(),
            fill_opacity: style.fill_opacity * style.opacity,
            stroke_opacity: style.stroke_opacity * style.opacity,
        });
    }
}

fn map_command(c: PathCommand, f: &impl Fn(Point) -> Point) -> PathCommand {
    match c {
        PathCommand::MoveTo(p) => PathCommand::MoveTo(f(p)),
        PathCommand::LineTo(p) => PathCommand::LineTo(f(p)),
        PathCommand::QuadTo(a, p) => PathCommand::QuadTo(f(a), f(p)),
        PathCommand::CubicTo(a, b, p) => PathCommand::CubicTo(f(a), f(b), f(p)),
        PathCommand::Close => PathCommand::Close,
    }
}

fn resolve(ink: Ink, current: Color, opacity: f32) -> Option<Color> {
    let c = match ink {
        Ink::None => return None,
        Ink::Color(c) => c,
        Ink::Current => current,
    };
    let a = (f32::from(c.a) * opacity.clamp(0.0, 1.0)).round();
    (a > 0.0).then(|| Color::rgba(c.r, c.g, c.b, f32_to_u8_clamped(a)))
}

struct Tag<'a> {
    name: &'a str,
    attrs: &'a str,
    closing: bool,
    empty: bool,
    at: usize,
}

struct Tags<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Tags<'a> {
    fn next(&mut self) -> Result<Option<Tag<'a>>, SvgError> {
        loop {
            let Some(open) = self.s[self.i..].find('<') else {
                return Ok(None);
            };
            let at = self.i + open;
            let rest = &self.s[at..];
            let skip = |end: &str| {
                rest.find(end).map(|e| at + e + end.len()).ok_or(SvgError {
                    at,
                    reason: "unterminated markup",
                })
            };
            if rest.starts_with("<!--") {
                self.i = skip("-->")?;
                continue;
            }
            if rest.starts_with("<?") || rest.starts_with("<!") {
                self.i = skip(">")?;
                continue;
            }
            let end = tag_end(rest).ok_or(SvgError {
                at,
                reason: "unterminated tag",
            })?;
            self.i = at + end + 1;
            let body = &rest[1..end];
            let closing = body.starts_with('/');
            let body = body.trim_start_matches('/');
            let empty = body.ends_with('/');
            let body = body.trim_end_matches('/');
            let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
            let name = &body[..name_end];
            let name = name.rsplit(':').next().unwrap_or(name);
            return Ok(Some(Tag {
                name,
                attrs: &body[name_end..],
                closing,
                empty,
                at,
            }));
        }
    }
}

fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

fn attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs;
    loop {
        let eq = rest.find('=')?;
        let key = rest[..eq].trim();
        let after = rest[eq + 1..].trim_start();
        let q = after.chars().next()?;
        if q != '"' && q != '\'' {
            return None;
        }
        let close = after[1..].find(q)?;
        let value = &after[1..1 + close];
        if key == name {
            return Some(value);
        }
        rest = &after[close + 2..];
    }
}

fn property<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let from_style = attr(attrs, "style").and_then(|style| {
        style.split(';').find_map(|decl| {
            let (k, v) = decl.split_once(':')?;
            (k.trim() == name).then(|| v.trim())
        })
    });
    from_style.or_else(|| attr(attrs, name))
}

fn number(attrs: &str, name: &str, at: usize) -> Result<f32, SvgError> {
    match attr(attrs, name) {
        None => Ok(0.0),
        Some(v) => length(v).ok_or(SvgError {
            at,
            reason: "bad number",
        }),
    }
}

fn length(v: &str) -> Option<f32> {
    v.trim().trim_end_matches("px").parse().ok()
}

fn numbers(s: &str) -> impl Iterator<Item = f32> + '_ {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse().ok())
}

fn view_box(attrs: &str, at: usize) -> Result<Rect, SvgError> {
    if let Some(v) = attr(attrs, "viewBox") {
        let n: Vec<f32> = numbers(v).collect();
        if let [x, y, w, h] = n[..] {
            return Ok(Rect::new(x, y, w, h));
        }
        return Err(SvgError {
            at,
            reason: "bad viewBox",
        });
    }
    let w = attr(attrs, "width").and_then(length).unwrap_or(0.0);
    let h = attr(attrs, "height").and_then(length).unwrap_or(0.0);
    Ok(Rect::new(0.0, 0.0, w, h))
}

fn apply_style(parent: Style, attrs: &str, at: usize) -> Result<Style, SvgError> {
    let mut s = parent;
    let bad = |reason| SvgError { at, reason };
    if let Some(v) = property(attrs, "fill") {
        s.fill = ink(v).ok_or(bad("bad fill"))?;
    }
    if let Some(v) = property(attrs, "stroke") {
        s.stroke = ink(v).ok_or(bad("bad stroke"))?;
    }
    if let Some(v) = property(attrs, "stroke-width") {
        s.stroke_width = length(v).ok_or(bad("bad stroke-width"))?;
    }
    if let Some(v) = property(attrs, "fill-rule") {
        s.fill_rule = if v == "evenodd" {
            FillRule::EvenOdd
        } else {
            FillRule::NonZero
        };
    }
    let fraction = |v: &str| v.trim().parse::<f32>().ok().map(|f| f.clamp(0.0, 1.0));
    if let Some(v) = property(attrs, "opacity") {
        s.opacity *= fraction(v).ok_or(bad("bad opacity"))?;
    }
    if let Some(v) = property(attrs, "fill-opacity") {
        s.fill_opacity = fraction(v).ok_or(bad("bad fill-opacity"))?;
    }
    if let Some(v) = property(attrs, "stroke-opacity") {
        s.stroke_opacity = fraction(v).ok_or(bad("bad stroke-opacity"))?;
    }
    if let Some(v) = attr(attrs, "transform") {
        s.transform = transform(v)
            .ok_or(bad("bad transform"))?
            .multiply(parent.transform);
    }
    Ok(s)
}

fn ink(v: &str) -> Option<Ink> {
    let v = v.trim();
    if v == "none" || v == "transparent" {
        return Some(Ink::None);
    }
    if v == "currentColor" {
        return Some(Ink::Current);
    }
    if let Some(hex) = v.strip_prefix('#') {
        let digit = |i: usize, n: usize| u8::from_str_radix(hex.get(i..i + n)?, 16).ok();
        return match hex.len() {
            3 => Some(Ink::Color(Color::rgb(
                digit(0, 1)? * 17,
                digit(1, 1)? * 17,
                digit(2, 1)? * 17,
            ))),
            6 => Some(Ink::Color(Color::rgb(
                digit(0, 2)?,
                digit(2, 2)?,
                digit(4, 2)?,
            ))),
            _ => None,
        };
    }
    if let Some(args) = v.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let n: Vec<f32> = numbers(args).collect();
        let byte = |f: f32| f32_to_u8_clamped(f.round());
        if let [r, g, b] = n[..] {
            return Some(Ink::Color(Color::rgb(byte(r), byte(g), byte(b))));
        }
        return None;
    }
    let named = match v {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "gray" | "grey" => (128, 128, 128),
        "orange" => (255, 165, 0),
        _ => return None,
    };
    Some(Ink::Color(Color::rgb(named.0, named.1, named.2)))
}

fn transform(v: &str) -> Option<Transform> {
    let mut total = Transform::identity();
    let mut rest = v.trim();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let n: Vec<f32> = numbers(&rest[open + 1..close]).collect();
        let t = match (name, &n[..]) {
            ("translate", [x]) => Transform::translate(*x, 0.0),
            ("translate", [x, y]) => Transform::translate(*x, *y),
            ("scale", [s]) => Transform::scale(*s, *s),
            ("scale", [x, y]) => Transform::scale(*x, *y),
            ("rotate", [a]) => Transform::rotate(a.to_radians()),
            ("rotate", [a, cx, cy]) => Transform::translate(-cx, -cy)
                .multiply(Transform::rotate(a.to_radians()))
                .multiply(Transform::translate(*cx, *cy)),
            ("matrix", [a, b, c, d, e, f]) => Transform {
                m11: *a,
                m12: *b,
                m21: *c,
                m22: *d,
                m31: *e,
                m32: *f,
                ..Transform::identity()
            },
            _ => return None,
        };
        total = t.multiply(total);
        rest = rest[close + 1..].trim();
    }
    Some(total)
}

fn shape_path(name: &str, attrs: &str, at: usize) -> Result<Option<Path>, SvgError> {
    let n = |k: &str| number(attrs, k, at);
    let mut path = Path::new();
    match name {
        "path" => {
            let Some(d) = attr(attrs, "d") else {
                return Ok(None);
            };
            append_path_data(d, &mut path).map_err(|e| SvgError {
                at: at + e.at,
                reason: e.reason,
            })?;
        }
        "rect" => {
            let (w, h) = (n("width")?, n("height")?);
            if w <= 0.0 || h <= 0.0 {
                return Ok(None);
            }
            let r = Rect::new(n("x")?, n("y")?, w, h);
            let rx = attr(attrs, "rx")
                .or(attr(attrs, "ry"))
                .and_then(length)
                .unwrap_or(0.0);
            if rx > 0.0 {
                path.push_rounded_rect(r, rx);
            } else {
                polygon(
                    &mut path,
                    &[
                        Point::new(r.x, r.y),
                        Point::new(r.x + w, r.y),
                        Point::new(r.x + w, r.y + h),
                        Point::new(r.x, r.y + h),
                    ],
                    true,
                );
            }
        }
        "circle" => {
            let r = n("r")?;
            ellipse(&mut path, Point::new(n("cx")?, n("cy")?), r, r);
        }
        "ellipse" => ellipse(
            &mut path,
            Point::new(n("cx")?, n("cy")?),
            n("rx")?,
            n("ry")?,
        ),
        "line" => polygon(
            &mut path,
            &[
                Point::new(n("x1")?, n("y1")?),
                Point::new(n("x2")?, n("y2")?),
            ],
            false,
        ),
        "polyline" | "polygon" => {
            let pts: Vec<f32> = attr(attrs, "points")
                .map(|p| numbers(p).collect())
                .unwrap_or_default();
            let points: Vec<Point> = pts
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[x, y]| Point::new(x, y))
                .collect();
            polygon(&mut path, &points, name == "polygon");
        }
        _ => return Ok(None),
    }
    Ok((!path.commands.is_empty()).then_some(path))
}

fn polygon(path: &mut Path, points: &[Point], closed: bool) {
    let Some((&first, rest)) = points.split_first() else {
        return;
    };
    path.commands.push(PathCommand::MoveTo(first));
    path.commands
        .extend(rest.iter().map(|&p| PathCommand::LineTo(p)));
    if closed {
        path.commands.push(PathCommand::Close);
    }
}

fn ellipse(path: &mut Path, c: Point, rx: f32, ry: f32) {
    if rx <= 0.0 || ry <= 0.0 {
        return;
    }
    let k = 0.552_284_8;
    let (kx, ky) = (rx * k, ry * k);
    let p = Point::new;
    path.commands.extend_from_slice(&[
        PathCommand::MoveTo(p(c.x + rx, c.y)),
        PathCommand::CubicTo(
            p(c.x + rx, c.y + ky),
            p(c.x + kx, c.y + ry),
            p(c.x, c.y + ry),
        ),
        PathCommand::CubicTo(
            p(c.x - kx, c.y + ry),
            p(c.x - rx, c.y + ky),
            p(c.x - rx, c.y),
        ),
        PathCommand::CubicTo(
            p(c.x - rx, c.y - ky),
            p(c.x - kx, c.y - ry),
            p(c.x, c.y - ry),
        ),
        PathCommand::CubicTo(
            p(c.x + kx, c.y - ry),
            p(c.x + rx, c.y - ky),
            p(c.x + rx, c.y),
        ),
        PathCommand::Close,
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON: &str = r##"<?xml version="1.0"?>
        <!-- a test icon -->
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <g transform="translate(2 2)">
            <circle cx="10" cy="10" r="8"/>
            <path d="M6 10h8" style="stroke: #f00; stroke-opacity: 0.5"/>
          </g>
          <rect x="0" y="0" width="4" height="4" fill="black" stroke="none"/>
        </svg>"##;

    #[test]
    fn reads_the_view_box_and_every_shape() {
        let img = VectorImage::parse(ICON).unwrap();
        assert_eq!(img.view_box, Rect::new(0.0, 0.0, 24.0, 24.0));
        assert_eq!(img.shapes.len(), 3);
        let rect = &img.shapes[2];
        assert_eq!(
            (rect.fill, rect.stroke),
            (Ink::Color(Color::rgb(0, 0, 0)), Ink::None)
        );
    }

    fn starts_at(shape: &Shape) -> Option<Point> {
        match shape.path.commands.first() {
            Some(PathCommand::MoveTo(p)) => Some(*p),
            _ => None,
        }
    }

    #[test]
    fn groups_pass_styles_and_transforms_down() {
        let img = VectorImage::parse(ICON).unwrap();
        let circle = &img.shapes[0];
        assert_eq!((circle.fill, circle.stroke), (Ink::None, Ink::Current));
        assert_eq!(circle.stroke_width, 2.0);
        assert_eq!(starts_at(circle), Some(Point::new(20.0, 12.0)));
    }

    #[test]
    fn style_attributes_override_presentation_ones() {
        let line = &VectorImage::parse(ICON).unwrap().shapes[1];
        assert_eq!(line.stroke, Ink::Color(Color::rgb(255, 0, 0)));
        assert_eq!(line.stroke_opacity, 0.5);
        assert_eq!(starts_at(line), Some(Point::new(8.0, 12.0)));
    }

    #[test]
    fn transform_lists_apply_right_to_left() {
        let t = transform("translate(10,0) scale(2)").unwrap();
        assert_eq!(t.map_point(Point::new(1.0, 0.0)), Point::new(12.0, 0.0));
    }

    #[test]
    fn colours() {
        assert_eq!(ink("#fff"), Some(Ink::Color(Color::rgb(255, 255, 255))));
        assert_eq!(ink("#102030"), Some(Ink::Color(Color::rgb(16, 32, 48))));
        assert_eq!(ink("rgb(1, 2, 3)"), Some(Ink::Color(Color::rgb(1, 2, 3))));
        assert_eq!(ink("bogus"), None);
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(VectorImage::parse("<svg viewBox='0 0 1'>").is_err());
        assert!(VectorImage::parse("<svg><path d='M0 0 L'/></svg>").is_err());
        assert!(VectorImage::parse("<svg><rect width=\"x\"").is_err());
    }
}
