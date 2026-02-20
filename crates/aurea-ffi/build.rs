#[cfg(target_os = "linux")]
use std::process::Command;

fn main() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir.join("../..").canonicalize().unwrap();
    let c_src = root.join("c_src");

    fn add_sources(build: &mut cc::Build, root: &std::path::Path, sources: &[&str]) {
        for source in sources {
            build.file(root.join(source));
        }
    }

    fn rerun_for(root: &std::path::Path, paths: &[&str]) {
        for path in paths {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }

    let common_c: &[&str] = &["c_src/common/platform.c"];

    #[cfg(target_os = "windows")]
    let windows_c: &[&str] = &[
        "c_src/platform/windows.c",
        "c_src/platform/windows/utils.c",
        "c_src/platform/windows/window.c",
        "c_src/platform/windows/menu.c",
        "c_src/platform/windows/elements/common.c",
        "c_src/platform/windows/elements/button.c",
        "c_src/platform/windows/elements/label.c",
        "c_src/platform/windows/elements/box.c",
        "c_src/platform/windows/elements/split_view.c",
        "c_src/platform/windows/elements/text_common.c",
        "c_src/platform/windows/elements/text_editor.c",
        "c_src/platform/windows/elements/text_view.c",
        "c_src/platform/windows/elements/canvas.c",
        "c_src/platform/windows/elements/slider.c",
        "c_src/platform/windows/elements/checkbox.c",
        "c_src/platform/windows/elements/progress_bar.c",
        "c_src/platform/windows/elements/combo_box.c",
        "c_src/platform/windows/elements/tab_bar.c",
        "c_src/platform/windows/elements/sidebar_list.c",
    ];

    #[cfg(target_os = "windows")]
    let windows_cpp: &[&str] = &["c_src/platform/windows/elements/image_view.cpp"];

    let ios_sources: &[&str] = &[
        "c_src/platform/ios.m",
        "c_src/platform/ios/ios.m",
        "c_src/platform/ios/main.m",
        "c_src/platform/ios/app_delegate.m",
        "c_src/platform/ios/view_controller.m",
        "c_src/platform/ios/window.m",
        "c_src/platform/ios/utils.m",
        "c_src/platform/ios/elements/button.m",
        "c_src/platform/ios/elements/label.m",
        "c_src/platform/ios/elements/box.m",
        "c_src/platform/ios/elements/canvas.m",
        "c_src/platform/ios/elements/image_view.m",
        "c_src/platform/ios/elements/slider.m",
        "c_src/platform/ios/elements/checkbox.m",
        "c_src/platform/ios/elements/progress_bar.m",
        "c_src/platform/ios/elements/combo_box.m",
        "c_src/platform/ios/elements/split_view.m",
    ];

    let macos_sources: &[&str] = &[
        "c_src/platform/macos.m",
        "c_src/platform/macos/window.m",
        "c_src/platform/macos/menu.m",
        "c_src/platform/macos/utils.m",
        "c_src/platform/macos/elements/button.m",
        "c_src/platform/macos/elements/label.m",
        "c_src/platform/macos/elements/box.m",
        "c_src/platform/macos/elements/text_common.m",
        "c_src/platform/macos/elements/text_editor.m",
        "c_src/platform/macos/elements/text_view.m",
        "c_src/platform/macos/elements/canvas.m",
        "c_src/platform/macos/elements/image_view.m",
        "c_src/platform/macos/elements/slider.m",
        "c_src/platform/macos/elements/checkbox.m",
        "c_src/platform/macos/elements/progress_bar.m",
        "c_src/platform/macos/elements/combo_box.m",
        "c_src/platform/macos/elements/tab_bar.m",
        "c_src/platform/macos/elements/sidebar_list.m",
        "c_src/platform/macos/elements/split_view.m",
    ];

    #[cfg(target_os = "linux")]
    let linux_sources: &[&str] = &[
        "c_src/platform/linux.c",
        "c_src/platform/linux/utils.c",
        "c_src/platform/linux/window.c",
        "c_src/platform/linux/menu.c",
        "c_src/platform/linux/elements/button.c",
        "c_src/platform/linux/elements/label.c",
        "c_src/platform/linux/elements/box.c",
        "c_src/platform/linux/elements/split_view.c",
        "c_src/platform/linux/elements/text_common.c",
        "c_src/platform/linux/elements/text_editor.c",
        "c_src/platform/linux/elements/text_view.c",
        "c_src/platform/linux/elements/canvas.c",
        "c_src/platform/linux/elements/image_view.c",
        "c_src/platform/linux/elements/slider.c",
        "c_src/platform/linux/elements/checkbox.c",
        "c_src/platform/linux/elements/progress_bar.c",
        "c_src/platform/linux/elements/combo_box.c",
        "c_src/platform/linux/elements/tab_bar.c",
        "c_src/platform/linux/elements/sidebar_list.c",
    ];

    let target = std::env::var("TARGET").unwrap();
    let mut build = cc::Build::new();
    build.include(&c_src).warnings(true);

    let is_ios = target.contains("apple-ios");

    #[cfg(target_os = "windows")]
    {
        add_sources(&mut build, &root, common_c);
        add_sources(&mut build, &root, windows_c);
        build.define("_WIN32", None);

        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=gdi32");
        println!("cargo:rustc-link-lib=comctl32");
        println!("cargo:rustc-link-lib=Shcore");
        println!("cargo:rustc-link-lib=gdiplus");
        println!("cargo:rustc-link-lib=ole32");

        rerun_for(&root, common_c);
        rerun_for(&root, windows_c);
        rerun_for(&root, windows_cpp);
    }

    if is_ios {
        println!("cargo:rustc-link-lib=framework=UIKit");
        println!("cargo:rustc-link-lib=framework=Foundation");

        add_sources(&mut build, &root, common_c);
        add_sources(&mut build, &root, ios_sources);
        build
            .include(c_src.join("platform/ios"))
            .define("__APPLE__", None)
            .define("TARGET_OS_IPHONE", Some("1"))
            .flag("-x")
            .flag("objective-c")
            .flag("-fmodules")
            .flag("-fobjc-arc")
            .flag("-mios-version-min=13.0")
            .flag("-Wno-error=unused-command-line-argument");

        rerun_for(&root, common_c);
        rerun_for(&root, ios_sources);
    } else if target.contains("apple-darwin") {
        println!("cargo:rustc-link-lib=framework=Cocoa");

        add_sources(&mut build, &root, common_c);
        add_sources(&mut build, &root, macos_sources);
        build
            .define("__APPLE__", None)
            .flag("-x")
            .flag("objective-c")
            .flag("-fmodules")
            .flag("-fobjc-arc")
            .flag("-Wno-error=unused-command-line-argument");

        rerun_for(&root, common_c);
        rerun_for(&root, macos_sources);
    }

    #[cfg(target_os = "linux")]
    {
        if !Command::new("pkg-config")
            .arg("--version")
            .status()
            .is_ok_and(|status| status.success())
        {
            println!("cargo:warning=pkg-config not found. Please install pkg-config.");
            std::process::exit(1);
        }

        match pkg_config::Config::new()
            .atleast_version("3.0")
            .probe("gtk+-3.0")
        {
            Ok(gtk) => {
                for include in gtk.include_paths {
                    build.include(include);
                }

                for (name, value) in gtk.defines {
                    build.define(&name, value.as_deref());
                }
            }
            Err(error) => {
                println!("cargo:warning=GTK3 development files not found: {}", error);
                println!("cargo:warning=On Ubuntu/Debian, install them with:");
                println!("cargo:warning=    sudo apt-get install libgtk-3-dev");
                println!("cargo:warning=On Fedora:");
                println!("cargo:warning=    sudo dnf install gtk3-devel");
                println!("cargo:warning=On Arch Linux:");
                println!("cargo:warning=    sudo pacman -S gtk3");
                std::process::exit(1);
            }
        }

        add_sources(&mut build, &root, common_c);
        add_sources(&mut build, &root, linux_sources);
        rerun_for(&root, common_c);
        rerun_for(&root, linux_sources);
    }

    build.compile("native_gui");

    #[cfg(target_os = "windows")]
    {
        let mut cpp_build = cc::Build::new();
        cpp_build
            .cpp(true)
            .include(&c_src)
            .define("_WIN32", None);
        add_sources(&mut cpp_build, &root, windows_cpp);
        cpp_build.compile("native_gui_cpp");
    }

    let common_headers: &[&str] = &[
        "c_src/common/types.h",
        "c_src/common/errors.h",
        "c_src/common/input.h",
        "c_src/common/platform_api.h",
        "c_src/common/rust_callbacks.h",
    ];

    #[cfg(target_os = "windows")]
    let windows_headers: &[&str] = &[
        "c_src/platform/windows.h",
        "c_src/platform/windows/utils.h",
        "c_src/platform/windows/window.h",
        "c_src/platform/windows/menu.h",
        "c_src/platform/windows/elements.h",
    ];

    let macos_headers: &[&str] = &[
        "c_src/platform/macos.h",
        "c_src/platform/macos/window.h",
        "c_src/platform/macos/menu.h",
        "c_src/platform/macos/utils.h",
        "c_src/platform/macos/elements.h",
    ];

    #[cfg(target_os = "linux")]
    let linux_headers: &[&str] = &[
        "c_src/platform/linux.h",
        "c_src/platform/linux/utils.h",
        "c_src/platform/linux/window.h",
        "c_src/platform/linux/menu.h",
        "c_src/platform/linux/elements.h",
    ];

    rerun_for(&root, common_headers);
    #[cfg(target_os = "windows")]
    rerun_for(&root, windows_headers);
    rerun_for(&root, macos_headers);
    #[cfg(target_os = "linux")]
    rerun_for(&root, linux_headers);
}
