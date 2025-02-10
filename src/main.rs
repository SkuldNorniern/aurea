fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define a simple callback function matching the expected extern "C" signature.
    extern "C" fn on_menu_item_click() {
        println!("Menu item clicked!");
    }

    // Initialize the native GUI menubar.
    let gui = fenestra::NativeGui::new()?;
    
    // Add a menu item.
    gui.add_menu_item("File", on_menu_item_click)?;

    // The menubar and items now exist; in a real application you would enter your event loop here.
    println!("Native menubar initialized successfully.");
    Ok(())
}