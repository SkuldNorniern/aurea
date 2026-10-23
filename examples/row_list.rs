//! A list of rows built from elements: labels and controls in each row, rows keeping their ids.

use aurea::elements::{
    Button, Checkbox, Container, Label, Orientation, RowList, Shared, Stack, TextStyle,
};
use aurea::{AureaResult, Window};

const ADAPTERS: [(u64, &str, &str); 4] = [
    (1, "FT4232H", "UART, I2C, SPI · 4 channels"),
    (2, "MCP2221A", "UART, I2C"),
    (3, "CP2112", "I2C"),
    (4, "CH347", "UART, I2C, SPI"),
];

fn adapter(name: &str, buses: &str) -> AureaResult<Stack> {
    let mut text = Stack::new(Orientation::Vertical)?;
    text.add(Label::with_style(name, TextStyle::Strong)?)?;
    text.add(Label::with_style(buses, TextStyle::Small)?)?;
    let mut row = Stack::new(Orientation::Horizontal)?;
    row.add_weighted(text, 1.0)?;
    row.add(Checkbox::with_checked("Enabled", true)?)?;
    row.add(Button::new("Configure…")?)?;
    Ok(row)
}

fn main() -> AureaResult<()> {
    let mut window = Window::new("Aurea row list", 620, 380)?;
    window.show();
    let status = Shared::new(Label::new("Select an adapter.")?);
    let list = Shared::new(RowList::new()?);
    for (id, name, buses) in ADAPTERS {
        list.borrow_mut().add_row(id, adapter(name, buses)?)?;
    }
    let name = |id: u64| ADAPTERS.iter().find(|a| a.0 == id).map_or("?", |a| a.1);
    let shown = status.clone();
    list.borrow().on_select(move |row| {
        let text = row.map_or("Nothing selected.".to_owned(), |id| {
            format!("{} selected.", name(id))
        });
        let _ = shown.borrow_mut().set_text(&text);
    });
    let opened = status.clone();
    list.borrow().on_activate(move |id| {
        let _ = opened
            .borrow_mut()
            .set_text(&format!("Opening {}…", name(id)));
    });
    list.borrow_mut().set_selected(Some(2))?;

    let mut root = Stack::new(Orientation::Vertical)?;
    root.add(Label::with_style("Bus adapters", TextStyle::Heading)?)?;
    root.add_weighted(list, 1.0)?;
    root.add(status)?;
    window.set_content(root)?;
    window.run()
}
