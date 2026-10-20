//! A settings window: a sidebar, a form of mixed rows and a growing editor, to check native layout.

use aurea::elements::{
    Button, Checkbox, ComboBox, Container, Element, Label, Orientation, Shared, SidebarList,
    Slider, SplitOrientation, SplitView, Stack, TextEditor, TextField,
};
use aurea::{AureaResult, Window};

const PAGES: [&str; 4] = ["General", "Display", "Storage", "Network"];

fn row(name: &str, field: impl Element + 'static) -> AureaResult<Stack> {
    let mut row = Stack::new(Orientation::Horizontal)?;
    row.add(Label::new(name)?)?;
    row.add_weighted(field, 1.0)?;
    Ok(row)
}

fn form(title: Shared<Label>) -> AureaResult<Stack> {
    let mut form = Stack::new(Orientation::Vertical)?;
    form.add(title)?;
    form.add(Label::new(
        "Changes apply when the machine next starts. Long help text like this wraps to the width of the form.",
    )?)?;
    form.add(row("Name", TextField::with_content("Ubuntu 24.04")?)?)?;
    form.add(row("Memory", Slider::with_value(512.0, 16384.0, 4096.0)?)?)?;
    form.add(row(
        "Boot order",
        ComboBox::with_items_selected(["Disk", "Optical", "Network"], 0)?,
    )?)?;
    form.add(Checkbox::with_checked("Start in headless mode", false)?)?;
    form.add(Label::new("Notes")?)?;
    form.add_weighted(
        TextEditor::with_content("first boot after the 24.04 upgrade\n")?,
        1.0,
    )?;

    let mut buttons = Stack::new(Orientation::Horizontal)?;
    buttons.add_spacer(1.0)?;
    buttons.add(Button::new("Cancel")?)?;
    buttons.add(Button::new("OK")?)?;
    form.add(buttons)?;
    Ok(form)
}

fn main() -> AureaResult<()> {
    let mut window = Window::new("Aurea settings layout", 760, 520)?;
    window.show();
    let title = Shared::new(Label::new(PAGES[0])?);
    let heading = title.clone();
    let mut pages = SidebarList::with_callback(move |index| {
        let page = usize::try_from(index).ok().and_then(|i| PAGES.get(i));
        if let (Some(page), Ok(mut label)) = (page, heading.try_borrow_mut()) {
            let _ = label.set_text(page);
        }
    })?;
    pages.add_section("Machine")?;
    pages.add_items(PAGES, 0)?;
    pages.set_selected(0)?;

    let split = SplitView::with_children(SplitOrientation::Horizontal, pages, form(title)?)?;
    split.set_divider_position(0, 180.0)?;
    window.set_content(split)?;
    window.run()
}
