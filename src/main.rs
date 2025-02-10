use fenestra::{Window, Error};

fn main() -> Result<(), Error> {
    let mut window = Window::new("Quill", 800, 600)?;
    let mut menu_bar = window.create_menu_bar()?;
    
    // File Menu
    {
        let mut file_menu = menu_bar.add_submenu("File")?;
        file_menu.add_item("New", || println!("New"))?;
        file_menu.add_item("Open...", || println!("Open"))?;
        file_menu.add_item("Save", || println!("Save"))?;
    } // file_menu is dropped here
    
    // Edit Menu
    {
        let mut edit_menu = menu_bar.add_submenu("Edit")?;
        edit_menu.add_item("Cut", || println!("Cut"))?;
        edit_menu.add_item("Copy", || println!("Copy"))?;
        edit_menu.add_item("Paste", || println!("Paste"))?;
    } // edit_menu is dropped here

    window.run()?;
    Ok(())
}