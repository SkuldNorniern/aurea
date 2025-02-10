fn main() -> Result<(), Box<dyn std::error::Error>> {
    extern "C" fn on_menu_item_click() {
        println!("Menu item clicked!");
    }

    let gui = fenestra::NativeGui::new()?;
    
    gui.add_menu_item("File", on_menu_item_click)?;

    println!("Native menubar initialized successfully.");
    Ok(())
}