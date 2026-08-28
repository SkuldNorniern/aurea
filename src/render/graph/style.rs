//! Colours, widths and spacing.
//!
//! Every field is public and every struct has a `Default`, so a caller can
//! take a theme and change the one thing they care about instead of filling in
//! a whole configuration.

use aurea_render::{Color, Font};

/// How a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
}

impl Stroke {
    pub fn new(color: Color, width: f32) -> Self {
        Self { color, width }
    }
}

/// The graticule behind the trace.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridStyle {
    /// Lines at labelled ticks.
    pub major: Stroke,
    /// Lines at unlabelled ticks.
    pub minor: Stroke,
    /// The line at zero, drawn over the grid. `None` to leave it out.
    pub zero: Option<Stroke>,
    pub show_vertical: bool,
    pub show_horizontal: bool,
}

/// One axis: its line, ticks and labels.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisStyle {
    /// The axis line itself. `None` to leave it out.
    pub line: Option<Stroke>,
    /// Tick marks poking out of the axis.
    pub tick: Stroke,
    /// How far a major tick sticks out, in pixels.
    pub tick_length: f32,
    /// How far a minor tick sticks out.
    pub minor_tick_length: f32,
    pub label_color: Color,
    pub label_font: Font,
    /// Gap between the axis and its labels.
    pub label_gap: f32,
    pub show_labels: bool,
    pub show_ticks: bool,
}

/// The whole plot.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphStyle {
    /// Behind everything. `None` to draw onto whatever is already there.
    pub background: Option<Color>,
    /// Behind the plotting area only.
    pub plot_background: Option<Color>,
    /// Border around the plotting area.
    pub border: Option<Stroke>,
    pub grid: GridStyle,
    pub x_axis: AxisStyle,
    pub y_axis: AxisStyle,
    /// Space kept for the axes and their labels, in pixels.
    pub margin: Margin,
    /// Colours handed out to series that do not pick one.
    pub palette: Vec<Color>,
}

/// Space around the plotting area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margin {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Margin {
    pub fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }

    /// The same margin on every side.
    pub fn uniform(size: f32) -> Self {
        Self::new(size, size, size, size)
    }
}

impl Default for Margin {
    fn default() -> Self {
        // Room for y labels on the left and x labels underneath.
        Self::new(52.0, 12.0, 12.0, 28.0)
    }
}

impl Default for GridStyle {
    fn default() -> Self {
        Self {
            major: Stroke::new(Color::rgba(255, 255, 255, 38), 1.0),
            minor: Stroke::new(Color::rgba(255, 255, 255, 18), 1.0),
            zero: Some(Stroke::new(Color::rgba(255, 255, 255, 90), 1.0)),
            show_vertical: true,
            show_horizontal: true,
        }
    }
}

impl Default for AxisStyle {
    fn default() -> Self {
        Self {
            line: Some(Stroke::new(Color::rgba(255, 255, 255, 120), 1.0)),
            tick: Stroke::new(Color::rgba(255, 255, 255, 120), 1.0),
            tick_length: 4.0,
            minor_tick_length: 2.0,
            label_color: Color::rgb(170, 175, 185),
            label_font: Font::new("Sans", 11.0),
            label_gap: 4.0,
            show_labels: true,
            show_ticks: true,
        }
    }
}

impl Default for GraphStyle {
    fn default() -> Self {
        Self::dark()
    }
}

impl GraphStyle {
    /// Light trace on a dark field, the way an instrument display reads.
    pub fn dark() -> Self {
        Self {
            background: Some(Color::rgb(16, 18, 22)),
            plot_background: Some(Color::rgb(10, 12, 15)),
            border: Some(Stroke::new(Color::rgba(255, 255, 255, 60), 1.0)),
            grid: GridStyle::default(),
            x_axis: AxisStyle::default(),
            y_axis: AxisStyle::default(),
            margin: Margin::default(),
            palette: default_palette(),
        }
    }

    /// Dark trace on a light field, for a plot sitting in a normal window.
    pub fn light() -> Self {
        let ink = Color::rgb(40, 44, 52);
        let axis = AxisStyle {
            line: Some(Stroke::new(Color::rgba(0, 0, 0, 120), 1.0)),
            tick: Stroke::new(Color::rgba(0, 0, 0, 120), 1.0),
            label_color: ink,
            ..AxisStyle::default()
        };
        Self {
            background: Some(Color::rgb(250, 250, 252)),
            plot_background: Some(Color::rgb(255, 255, 255)),
            border: Some(Stroke::new(Color::rgba(0, 0, 0, 50), 1.0)),
            grid: GridStyle {
                major: Stroke::new(Color::rgba(0, 0, 0, 30), 1.0),
                minor: Stroke::new(Color::rgba(0, 0, 0, 14), 1.0),
                zero: Some(Stroke::new(Color::rgba(0, 0, 0, 80), 1.0)),
                show_vertical: true,
                show_horizontal: true,
            },
            x_axis: axis.clone(),
            y_axis: axis,
            margin: Margin::default(),
            palette: default_palette(),
        }
    }

    /// Nothing but the trace: no grid, axes, border or background.
    ///
    /// For a sparkline, or for a plot drawn over something else.
    pub fn bare() -> Self {
        Self {
            background: None,
            plot_background: None,
            border: None,
            grid: GridStyle {
                zero: None,
                show_vertical: false,
                show_horizontal: false,
                ..GridStyle::default()
            },
            x_axis: AxisStyle {
                line: None,
                show_labels: false,
                show_ticks: false,
                ..AxisStyle::default()
            },
            y_axis: AxisStyle {
                line: None,
                show_labels: false,
                show_ticks: false,
                ..AxisStyle::default()
            },
            margin: Margin::uniform(1.0),
            palette: default_palette(),
        }
    }

    /// The colour a series gets when it does not choose one.
    ///
    /// Wraps around, so an empty palette is the only way to get no colour and
    /// that falls back to plain white rather than failing.
    pub fn palette_color(&self, index: usize) -> Color {
        if self.palette.is_empty() {
            return Color::rgb(255, 255, 255);
        }
        self.palette[index % self.palette.len()]
    }
}

/// Distinguishable at a glance, and still distinguishable next to each other.
fn default_palette() -> Vec<Color> {
    vec![
        Color::rgb(120, 200, 255),
        Color::rgb(255, 190, 90),
        Color::rgb(130, 230, 150),
        Color::rgb(255, 130, 150),
        Color::rgb(190, 160, 255),
        Color::rgb(120, 230, 225),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_wraps() {
        let style = GraphStyle::dark();
        let len = style.palette.len();
        assert_eq!(style.palette_color(0), style.palette_color(len));
    }

    #[test]
    fn an_empty_palette_still_gives_a_colour() {
        let style = GraphStyle {
            palette: Vec::new(),
            ..GraphStyle::dark()
        };
        assert_eq!(style.palette_color(3), Color::rgb(255, 255, 255));
    }

    #[test]
    fn bare_style_turns_off_the_furniture() {
        let style = GraphStyle::bare();
        assert!(style.background.is_none());
        assert!(style.border.is_none());
        assert!(!style.grid.show_vertical);
        assert!(!style.x_axis.show_labels);
        assert!(!style.x_axis.show_ticks, "ticks are furniture too");
    }

    #[test]
    fn a_theme_can_be_changed_one_field_at_a_time() {
        let style = GraphStyle {
            margin: Margin::uniform(4.0),
            ..GraphStyle::light()
        };
        assert_eq!(style.margin.left, 4.0);
        assert_eq!(style.background, GraphStyle::light().background);
    }
}
