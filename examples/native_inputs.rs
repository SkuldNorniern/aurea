//! Native input widgets: placeholders, a search field, a spin box, radio buttons and a tooltip.

use aurea::elements::{Container, Element, Orientation, Stack};
use aurea::{AureaResult, RadioGroup, SpinBox, TextField, Window};

fn main() -> AureaResult<()> {
    let mut window = Window::new("Native inputs", 420, 340)?;
    let mut stack = Stack::new(Orientation::Vertical)?;
    stack.set_padding(12)?;
    stack.set_spacing(8)?;

    let name = TextField::new()?;
    name.set_placeholder("Your name")?;
    stack.add(name)?;

    let search = TextField::search()?;
    search.set_placeholder("Search layers")?;
    stack.add(search)?;

    let mut size = SpinBox::new(0.0, 100.0, 2.5)?;
    size.set_value(42.5)?;
    size.set_tooltip("Brush size")?;
    size.on_change(|value| println!("size {value}"))?;
    stack.add(size)?;

    let mut quality = RadioGroup::new(&["Low", "Medium", "High"])?;
    quality.select(1)?;
    quality.on_change(|picked| println!("quality {picked}"))?;
    stack.add(quality)?;

    window.set_content(stack)?;
    window.run()
}
