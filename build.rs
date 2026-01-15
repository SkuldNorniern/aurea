#[cfg(target_os = "linux")]
use std::process::Command;

fn main() {
    let target = std::env::var("TARGET").unwrap();
    let mut build = cc::Build::new();

    // Common configuration
    build.include("c_src").warnings(true);

    // Check if this is an iOS target (including simulator)
    let is_ios = target.contains("apple-ios");

    // Add platform-specific configurations and source files
    #[cfg(target_os = "windows")]
    {
        build
            .file("c_src/platform/windows.c")
            .file("c_src/platform/windows/utils.c")
            .file("c_src/platform/windows/window.c")
            .file("c_src/platform/windows/menu.c")
            .file("c_src/platform/windows/elements/common.c")
            .file("c_src/platform/windows/elements/button.c")
            .file("c_src/platform/windows/elements/label.c")
            .file("c_src/platform/windows/elements/box.c")
            .file("c_src/platform/windows/elements/text_common.c")
            .file("c_src/platform/windows/elements/text_editor.c")
            .file("c_src/platform/windows/elements/text_view.c")
            .file("c_src/platform/windows/elements/canvas.c")
            .file("c_src/platform/windows/elements/slider.c")
            .file("c_src/platform/windows/elements/checkbox.c")
            .file("c_src/platform/windows/elements/progress_bar.c")
            .file("c_src/platform/windows/elements/combo_box.c")
            .define("_WIN32", None);
        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=gdi32");
        println!("cargo:rustc-link-lib=comctl32");
        println!("cargo:rustc-link-lib=Shcore");
        println!("cargo:rustc-link-lib=gdiplus");
        println!("cargo:rustc-link-lib=ole32");

        println!("cargo:rerun-if-changed=c_src/platform/windows.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/utils.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/window.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/menu.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/common.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/button.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/label.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/box.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/text_common.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/text_editor.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/text_view.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/canvas.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/image_view.cpp");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/slider.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/checkbox.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/progress_bar.c");
        println!("cargo:rerun-if-changed=c_src/platform/windows/elements/combo_box.c");
    }

    // iOS targets (including simulator) - check target string, not cfg
    if is_ios {
        println!("cargo:rustc-link-lib=framework=UIKit");
        println!("cargo:rustc-link-lib=framework=Foundation");
        build
            .include("c_src/platform/ios")
            .file("c_src/platform/ios.m")
            .file("c_src/platform/ios/ios.m")
            .file("c_src/platform/ios/main.m")
            .file("c_src/platform/ios/app_delegate.m")
            .file("c_src/platform/ios/view_controller.m")
            .file("c_src/platform/ios/window.m")
            .file("c_src/platform/ios/utils.m")
            .file("c_src/platform/ios/elements/button.m")
            .file("c_src/platform/ios/elements/label.m")
            .file("c_src/platform/ios/elements/box.m")
            .file("c_src/platform/ios/elements/canvas.m")
            .file("c_src/platform/ios/elements/image_view.m")
            .file("c_src/platform/ios/elements/slider.m")
            .file("c_src/platform/ios/elements/checkbox.m")
            .file("c_src/platform/ios/elements/progress_bar.m")
            .file("c_src/platform/ios/elements/combo_box.m")
            .file("c_src/platform/ios/elements/split_view.m")
            .define("__APPLE__", None)
            .define("TARGET_OS_IPHONE", Some("1"))
            .flag("-x")
            .flag("objective-c")
            .flag("-fmodules")
            .flag("-fobjc-arc")
            .flag("-mios-version-min=13.0")
            .flag("-Wno-error=unused-command-line-argument");

        println!("cargo:rerun-if-changed=c_src/platform/ios.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/ios.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/main.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/app_delegate.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/view_controller.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/window.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/utils.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/button.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/label.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/box.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/canvas.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/image_view.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/slider.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/checkbox.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/progress_bar.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/combo_box.m");
        println!("cargo:rerun-if-changed=c_src/platform/ios/elements/split_view.m");
    }
    // macOS targets (only if not iOS)
    else if target.contains("apple-darwin") {
        println!("cargo:rustc-link-lib=framework=Cocoa");
        build
            .file("c_src/platform/macos.m")
            .file("c_src/platform/macos/window.m")
            .file("c_src/platform/macos/menu.m")
            .file("c_src/platform/macos/utils.m")
            .file("c_src/platform/macos/elements/button.m")
            .file("c_src/platform/macos/elements/label.m")
            .file("c_src/platform/macos/elements/box.m")
            .file("c_src/platform/macos/elements/text_common.m")
            .file("c_src/platform/macos/elements/text_editor.m")
            .file("c_src/platform/macos/elements/text_view.m")
            .file("c_src/platform/macos/elements/canvas.m")
            .file("c_src/platform/macos/elements/image_view.m")
            .file("c_src/platform/macos/elements/slider.m")
            .file("c_src/platform/macos/elements/checkbox.m")
            .file("c_src/platform/macos/elements/progress_bar.m")
            .file("c_src/platform/macos/elements/combo_box.m")
            .file("c_src/platform/macos/elements/split_view.m")
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
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/button.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/label.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/box.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/text_common.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/text_editor.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/text_view.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/canvas.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/image_view.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/slider.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/checkbox.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/progress_bar.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/combo_box.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements/split_view.m");
        println!("cargo:rerun-if-changed=c_src/platform/macos/elements.h");
    }

    #[cfg(target_os = "linux")]
    {
        // Check if pkg-config is available
        if !Command::new("pkg-config")
            .arg("--version")
            .status()
            .map_or(false, |s| s.success())
        {
            println!("cargo:warning=pkg-config not found. Please install pkg-config.");
            std::process::exit(1);
        }

        // Check if GTK3 development files are installed
        match pkg_config::Config::new()
            .atleast_version("3.0")
            .probe("gtk+-3.0")
        {
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

        build
            .file("c_src/platform/linux.c")
            .file("c_src/platform/linux/utils.c")
            .file("c_src/platform/linux/window.c")
            .file("c_src/platform/linux/menu.c")
            .file("c_src/platform/linux/elements/button.c")
            .file("c_src/platform/linux/elements/label.c")
            .file("c_src/platform/linux/elements/box.c")
            .file("c_src/platform/linux/elements/text_common.c")
            .file("c_src/platform/linux/elements/text_editor.c")
            .file("c_src/platform/linux/elements/text_view.c")
            .file("c_src/platform/linux/elements/canvas.c")
            .file("c_src/platform/linux/elements/image_view.c")
            .file("c_src/platform/linux/elements/slider.c")
            .file("c_src/platform/linux/elements/checkbox.c")
            .file("c_src/platform/linux/elements/progress_bar.c")
            .file("c_src/platform/linux/elements/combo_box.c");

        println!("cargo:rerun-if-changed=c_src/platform/linux.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/utils.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/window.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/menu.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/button.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/label.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/box.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/text_common.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/text_editor.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/text_view.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/canvas.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/image_view.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/slider.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/checkbox.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/progress_bar.c");
        println!("cargo:rerun-if-changed=c_src/platform/linux/elements/combo_box.c");
    }

    // Compile the sources
    build.compile("native_gui");

    #[cfg(target_os = "windows")]
    {
        let mut cpp_build = cc::Build::new();
        cpp_build
            .cpp(true)
            .include("c_src")
            .file("c_src/platform/windows/elements/image_view.cpp")
            .define("_WIN32", None)
            .compile("native_gui_cpp");
    }

    // Watch for changes in all C source files
    println!("cargo:rerun-if-changed=c_src/native_gui.h");
    println!("cargo:rerun-if-changed=c_src/native_gui.c");
    println!("cargo:rerun-if-changed=c_src/common/types.h");
    println!("cargo:rerun-if-changed=c_src/common/errors.h");
    // Windows files
    println!("cargo:rerun-if-changed=c_src/platform/windows.h");
    println!("cargo:rerun-if-changed=c_src/platform/windows/utils.h");
    println!("cargo:rerun-if-changed=c_src/platform/windows/window.h");
    println!("cargo:rerun-if-changed=c_src/platform/windows/menu.h");
    println!("cargo:rerun-if-changed=c_src/platform/windows/elements.h");

    // macOS files
    println!("cargo:rerun-if-changed=c_src/platform/macos.h");
    println!("cargo:rerun-if-changed=c_src/platform/macos/window.h");
    println!("cargo:rerun-if-changed=c_src/platform/macos/menu.h");
    println!("cargo:rerun-if-changed=c_src/platform/macos/utils.h");
    println!("cargo:rerun-if-changed=c_src/platform/macos/elements.h");

    // Linux files
    println!("cargo:rerun-if-changed=c_src/platform/linux.h");
    println!("cargo:rerun-if-changed=c_src/platform/linux/utils.h");
    println!("cargo:rerun-if-changed=c_src/platform/linux/window.h");
    println!("cargo:rerun-if-changed=c_src/platform/linux/menu.h");
    println!("cargo:rerun-if-changed=c_src/platform/linux/elements.h");
}
