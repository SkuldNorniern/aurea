#[cfg(target_os = "linux")]
use std::process::Command;

fn main() {
    let mut build = cc::Build::new();
    
    // Common configuration
    build
        .include("c_src")
        .warnings(true);
    
    // Add platform-specific configurations and source files
    #[cfg(target_os = "windows")]
    {
        build
            .file("c_src/platform/windows.c")
            .define("_WIN32", None);
        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=gdi32");
    }
    
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Cocoa");
        build
            .file("c_src/platform/macos.m")
            .file("c_src/platform/macos/window.m")
            .file("c_src/platform/macos/menu.m")
            .file("c_src/platform/macos/utils.m")
            .define("__APPLE__", None)
            .flag("-x")
            .flag("objective-c")
            .flag("-fmodules")
            .flag("-fobjc-arc")
            .flag("-Wno-error=unused-command-line-argument");
        
        println!("cargo:rerun-if-changed=c_src/platform/macos.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/window.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/menu.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/utils.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/utils.h");
    }
    
    #[cfg(target_os = "linux")]
    {
        // Check if pkg-config is available
        if !Command::new("pkg-config").arg("--version").status().map_or(false, |s| s.success()) {
            println!("cargo:warning=pkg-config not found. Please install pkg-config.");
            std::process::exit(1);
        }

        // Check if GTK3 development files are installed
        match pkg_config::Config::new().atleast_version("3.0").probe("gtk+-3.0") {
            Ok(gtk) => {
                // Add GTK include paths and compiler flags
                for include in gtk.include_paths {
                    build.include(include);
                }

                // Add any compiler flags from GTK
                for (name, value) in gtk.defines {
                    build.define(&name, value.as_deref());
                }
            }
            Err(e) => {
                println!("cargo:warning=GTK3 development files not found: {}", e);
                println!("cargo:warning=On Ubuntu/Debian, install them with:");
                println!("cargo:warning=    sudo apt-get install libgtk-3-dev");
                println!("cargo:warning=On Fedora:");
                println!("cargo:warning=    sudo dnf install gtk3-devel");
                println!("cargo:warning=On Arch Linux:");
                println!("cargo:warning=    sudo pacman -S gtk3");
                std::process::exit(1);
            }
        }
            
        build.file("c_src/platform/linux.c");
    }
    
    // Compile the sources
    build.compile("native_gui");
        
    // Watch for changes in all C source files
    println!("cargo:rerun-if-changed=c_src/native_gui.h");
    println!("cargo:rerun-if-changed=c_src/native_gui.c");
    println!("cargo:rerun-if-changed=c_src/common/types.h");
    println!("cargo:rerun-if-changed=c_src/common/errors.h");
    println!("cargo:rerun-if-changed=c_src/platform/windows.h");
    println!("cargo:rerun-if-changed=c_src/platform/windows.c");
    println!("cargo:rerun-if-changed=c_src/platform/macos.h");
    println!("cargo:rerun-if-changed=c_src/platform/macos/window.h");
    println!("cargo:rerun-if-changed=c_src/platform/macos/menu.h");
    println!("cargo:rerun-if-changed=c_src/platform/macos/utils.h");
    println!("cargo:rerun-if-changed=c_src/platform/linux.h");
    println!("cargo:rerun-if-changed=c_src/platform/linux.c");
}