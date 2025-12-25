use aurea::{Window, AureaResult};
use aurea::elements::TextEditor;

fn main() -> AureaResult<()> {
    let mut window = Window::new("Notepad", 800, 600)?;
    
    let mut menu_bar = window.create_menu_bar()?;
    
    {
        let mut file_menu = menu_bar.add_submenu("File")?;
        file_menu.add_item("New", || println!("New"))?;
        file_menu.add_item("Open...", || println!("Open"))?;
        file_menu.add_item("Save", || println!("Save"))?;
    }
    
    {
        let mut edit_menu = menu_bar.add_submenu("Edit")?;
        edit_menu.add_item("Cut", || println!("Cut"))?;
        edit_menu.add_item("Copy", || println!("Copy"))?;
        edit_menu.add_item("Paste", || println!("Paste"))?;
    }

    let mut editor = TextEditor::new()?;
    editor.set_content("Welcome to Notepad!")?;
    
    window.set_content(editor)?;

    window.run()?;
    Ok(())
}