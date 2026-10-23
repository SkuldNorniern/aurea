//! A native table: rows keep their ids as others come and go; selecting and opening report them.

use aurea::elements::{Button, Container, Label, Orientation, Shared, Stack, Table, TextStyle};
use aurea::{AureaResult, Window};

const ADAPTERS: [(u64, [&str; 3]); 4] = [
    (1, ["FT4232H", "UART, I2C, SPI", "ready"]),
    (2, ["MCP2221A", "UART, I2C", "ready"]),
    (3, ["CP2112", "I2C", "busy"]),
    (4, ["CH347", "UART, I2C, SPI", "not found"]),
];

fn main() -> AureaResult<()> {
    let mut window = Window::new("Aurea table", 560, 360)?;
    window.show();
    let status = Shared::new(Label::new("Select an adapter.")?);
    let table = Shared::new(Table::new(["Adapter", "Buses", "State"])?);
    for (id, cells) in ADAPTERS {
        table.borrow_mut().add_row(id, &cells)?;
    }
    let name = |id: u64| ADAPTERS.iter().find(|a| a.0 == id).map_or("?", |a| a.1[0]);
    let shown = status.clone();
    table.borrow().on_select(move |row| {
        let text = row.map_or("Nothing selected.".to_owned(), |id| {
            format!("{} selected.", name(id))
        });
        let _ = shown.borrow_mut().set_text(&text);
    });
    let opened = status.clone();
    table.borrow().on_activate(move |id| {
        let _ = opened
            .borrow_mut()
            .set_text(&format!("Opening {}…", name(id)));
    });

    let mut buttons = Stack::new(Orientation::Horizontal)?;
    buttons.add_spacer(1.0)?;
    let remove = table.clone();
    buttons.add(Button::with_callback("Remove", move || {
        let mut table = remove.borrow_mut();
        if let Some(id) = table.selected() {
            let _ = table.remove_row(id);
        }
    })?)?;

    let mut root = Stack::new(Orientation::Vertical)?;
    root.add(Label::with_style("Bus adapters", TextStyle::Heading)?)?;
    root.add_weighted(table, 1.0)?;
    root.add(status)?;
    root.add(buttons)?;
    window.set_content(root)?;
    window.run()
}
