use fenestra::{Window, Error};

fn main() -> Result<(), Error> {
    let mut window = Window::new("My Application", 800, 600)?;
    
    let menu = window.create_menu_bar()?;
    
    menu.add_item("File", || {
        println!("File menu clicked!");
    })?;
    
    menu.add_item("Edit", || {
        println!("Edit menu clicked!");
    })?;

    window.run()?;
    Ok(())
}