# Aurea

**Aurea** is derived from the Latin word for "golden". It embodies our vision of delivering a **golden standard** for native GUI applications by providing a safe, idiomatic, and efficient Rust interface to the underlying platform APIs.

> **⚠️ Disclaimer:**  
> This project is currently in **pre-alpha** stage and is under active development. Due to the rapidly evolving codebase, it may contain bugs, incomplete features, or unexpected behavior.  
> **Usage Warning:** This software is intended solely for experimentation and development purposes. It is not recommended for production use.  
> Use this software at your own risk; the maintainer(s) are not responsible for any issues, errors, or damages—including, but not limited to, crashes, data loss, or security vulnerabilities—that may arise from its use.  
> Contributions, testing, and feedback are welcome to help improve the project's stability and reliability.

---

## Overview

Aurea is a native GUI toolkit that bridges high-level Rust abstractions with platform-specific implementations. It is designed to offer developers a robust and ergonomic way to build cross-platform applications with native look and feel across Windows, macOS, and Linux.

### Key Features

- **Native Windowing:** Create and manage native windows with ease.
- **Menus & Submenus:** Build native menu bars, complete with submenus and callback-driven menu items.
- **Widgets:** Use basic elements like buttons, labels, text editors, and text views.
- **Layout Management:** Organize UI elements using horizontal or vertical boxes.
- **Safe FFI(WIP):** Leverage a well-defined FFI layer that connects Rust with low-level C code, ensuring robust error handling and minimal overhead.
- **Resource Management:** Efficient memory management without unnecessary ownership overhead.

---

## Architecture

Aurea is structured in three primary layers:

1. **High-Level Rust API:**  
   Provides safe, idiomatic Rust types and functions for creating and managing windows, menus, and elements.

2. **FFI Layer:**  
   Acts as a bridge between Rust and platform-specific implementations written in C. This layer manages conversions, error handling, and minimal resource duplication.

3. **Platform-Specific Implementations:**  
   The native side (located in the `c_src` folder) contains implementations for different operating systems:
   - **Linux (GTK)**
   - **macOS (Cocoa)**
   - **Windows (Win32 API)**

---

## Example Usage

Below is a simple example demonstrating how to create a window with a text editor and a functional menu bar:

```rust
use aurea::{Window, AureaResult};
use aurea::elements::{TextEditor, Box, BoxOrientation};
fn main() -> AureaResult<()> {
    // Create a window
    let mut window = Window::new("Notepad", 800, 600)?;
    // Create and configure menu bar
    let mut menu_bar = window.create_menu_bar()?;
    // Add File menu
    let mut file_menu = menu_bar.add_submenu("File")?;
    file_menu.add_item("New", || println!("New"))?;
    file_menu.add_item("Open...", || println!("Open"))?;
    file_menu.add_item("Save", || println!("Save"))?;
    // Add Edit menu
    let mut edit_menu = menu_bar.add_submenu("Edit")?;
    edit_menu.add_item("Cut", || println!("Cut"))?;
    edit_menu.add_item("Copy", || println!("Copy"))?;
    edit_menu.add_item("Paste", || println!("Paste"))?;
    // Create and set up text editor
    let mut editor = TextEditor::new()?;
    editor.set_content("Welcome to Notepad!")?;
    // Set window content and run
    window.set_content(editor)?;
    window.run()?;
    Ok(())
}
```

---


## Building

The project requires:
- Rust 1.88 or later
- Platform-specific development tools:
  - Windows: MSVC build tools
  - macOS: Xcode command line tools
  - Linux: GTK3 development libraries
