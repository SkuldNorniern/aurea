use aurea::elements::TextEditor;
use aurea::{AureaResult, Window};

fn main() -> AureaResult<()> {
    let mut window = Window::new("Notepad", 800, 600)?;

    // Create menu bar
    let mut menu_bar = window.create_menu_bar()?;

    // File Menu
    {
        let mut file_menu = menu_bar.add_submenu("File")?;
        file_menu.add_item("New", || println!("New"))?;
        file_menu.add_item("Open...", || println!("Open"))?;
        file_menu.add_item("Save", || println!("Save"))?;
    }

    // Edit Menu
    {
        let mut edit_menu = menu_bar.add_submenu("Edit")?;
        edit_menu.add_item("Cut", || println!("Cut"))?;
        edit_menu.add_item("Copy", || println!("Copy"))?;
        edit_menu.add_item("Paste", || println!("Paste"))?;
    }

    // Create text editor
    let mut editor = TextEditor::new()?;
    editor.set_content("Welcome to Notepad!")?;

    // Set the editor as the window's content
    window.set_content(editor)?;

    // Run the application
    window.run()?;
    Ok(())
}
