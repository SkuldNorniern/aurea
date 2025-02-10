fn main() {
    let mut build = cc::Build::new();
    
    // Add platform-specific configurations
    #[cfg(target_os = "windows")]
    {
        build.define("_WIN32", None);
        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=gdi32");
    }
    
    #[cfg(target_os = "macos")]
    {
        build.define("__APPLE__", None);
        // Add macOS-specific frameworks if needed
        println!("cargo:rustc-link-lib=framework=Cocoa");
    }
    
    #[cfg(target_os = "linux")]
    {
        // Use pkg-config to get the correct flags for GTK3
        if let Ok(gtk) = pkg_config::probe_library("gtk+-3.0") {
            for include in gtk.include_paths {
                build.include(include);
            }
            
            // Fixed: directly iterate over the defines as (name, value) pairs
            for (name, value) in gtk.defines {
                build.define(&name, value.as_deref());
            }
        } else {
            println!("cargo:warning=GTK3 development files not found. Please install libgtk-3-dev");
            println!("cargo:rustc-link-lib=gtk-3");
            println!("cargo:rustc-link-lib=gdk-3");
        }
    }
    
    // Build the C code
    build
        .file("c_src/native_gui.c")
        .include("c_src")
        .warnings(true)
        .compile("native_gui");
        
    // Tell cargo to invalidate the built crate whenever the C sources change
    println!("cargo:rerun-if-changed=c_src/native_gui.h");
    println!("cargo:rerun-if-changed=c_src/native_gui.c");
} 