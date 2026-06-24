use cc::Build;
use pkg_config::Config as PkgConfig;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

fn main() {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo"),
    );
    let root = {
        let p = manifest_dir
            .canonicalize()
            .expect("manifest dir exists and is readable");
        // cl.exe rejects \\?\ extended-path prefix in -I flags on Windows.
        let s = p.to_string_lossy();
        PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
    };
    let native = root.join("native");
    let target = env::var("TARGET").expect("TARGET is set by cargo");
    let is_windows = target.contains("windows");
    let is_linux = target.contains("linux") && !target.contains("android");

    fn add_sources(build: &mut Build, root: &Path, sources: &[&str]) {
        for source in sources {
            build.file(root.join(source));
        }
    }

    fn rerun_for(root: &Path, paths: &[&str]) {
        for path in paths {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }

    let common_c: &[&str] = &["native/common/platform.c"];

    let windows_c: &[&str] = &[
        "native/common/platform_dispatch.c",
        "native/platform/windows.c",
        "native/platform/windows/windows_ops.c",
        "native/platform/windows/utils.c",
        "native/platform/windows/window.c",
        "native/platform/windows/menu.c",
        "native/platform/windows/elements/common.c",
        "native/platform/windows/elements/button.c",
        "native/platform/windows/elements/label.c",
        "native/platform/windows/elements/box.c",
        "native/platform/windows/elements/split_view.c",
        "native/platform/windows/elements/text_common.c",
        "native/platform/windows/elements/text_editor.c",
        "native/platform/windows/elements/text_view.c",
        "native/platform/windows/elements/canvas.c",
        "native/platform/windows/elements/slider.c",
        "native/platform/windows/elements/checkbox.c",
        "native/platform/windows/elements/progress_bar.c",
        "native/platform/windows/elements/combo_box.c",
        "native/platform/windows/elements/tab_bar.c",
        "native/platform/windows/elements/sidebar_list.c",
    ];

    let windows_cpp: &[&str] = &["native/platform/windows/elements/image_view.cpp"];

    let ios_sources: &[&str] = &[
        "native/platform/ios.m",
        "native/platform/ios/ios.m",
        "native/platform/ios/main.m",
        "native/platform/ios/app_delegate.m",
        "native/platform/ios/view_controller.m",
        "native/platform/ios/window.m",
        "native/platform/ios/utils.m",
        "native/platform/ios/elements/button.m",
        "native/platform/ios/elements/label.m",
        "native/platform/ios/elements/box.m",
        "native/platform/ios/elements/canvas.m",
        "native/platform/ios/elements/image_view.m",
        "native/platform/ios/elements/slider.m",
        "native/platform/ios/elements/checkbox.m",
        "native/platform/ios/elements/progress_bar.m",
        "native/platform/ios/elements/combo_box.m",
        "native/platform/ios/elements/split_view.m",
    ];

    let macos_sources: &[&str] = &[
        "native/common/platform_dispatch.c",
        "native/platform/macos.m",
        "native/platform/macos/macos_ops.m",
        "native/platform/macos/window.m",
        "native/platform/macos/menu.m",
        "native/platform/macos/utils.m",
        "native/platform/macos/elements/button.m",
        "native/platform/macos/elements/label.m",
        "native/platform/macos/elements/box.m",
        "native/platform/macos/elements/text_common.m",
        "native/platform/macos/elements/text_editor.m",
        "native/platform/macos/elements/text_view.m",
        "native/platform/macos/elements/canvas.m",
        "native/platform/macos/elements/image_view.m",
        "native/platform/macos/elements/slider.m",
        "native/platform/macos/elements/checkbox.m",
        "native/platform/macos/elements/progress_bar.m",
        "native/platform/macos/elements/combo_box.m",
        "native/platform/macos/elements/tab_bar.m",
        "native/platform/macos/elements/sidebar_list.m",
        "native/platform/macos/elements/split_view.m",
        "native/platform/macos/swiftui_host.m",
    ];

    let linux_sources: &[&str] = &[
        "native/common/platform_dispatch.c",
        "native/platform/linux.c",
        "native/platform/linux/linux_ops.c",
        "native/platform/linux/utils.c",
        "native/platform/linux/window.c",
        "native/platform/linux/menu.c",
        "native/platform/linux/elements/button.c",
        "native/platform/linux/elements/label.c",
        "native/platform/linux/elements/box.c",
        "native/platform/linux/elements/split_view.c",
        "native/platform/linux/elements/text_common.c",
        "native/platform/linux/elements/text_editor.c",
        "native/platform/linux/elements/text_view.c",
        "native/platform/linux/elements/canvas.c",
        "native/platform/linux/elements/image_view.c",
        "native/platform/linux/elements/slider.c",
        "native/platform/linux/elements/checkbox.c",
        "native/platform/linux/elements/progress_bar.c",
        "native/platform/linux/elements/combo_box.c",
        "native/platform/linux/elements/tab_bar.c",
        "native/platform/linux/elements/sidebar_list.c",
    ];

    let mut build = Build::new();
    build.include(&native).warnings(true);

    let is_ios = target.contains("apple-ios");
    let is_android = target.contains("android");

    let android_sources: &[&str] = &[
        "native/common/platform.c",
        "native/platform/android.c",
        "native/platform/android/android.c",
        "native/platform/android/window.c",
    ];

    if is_windows {
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
            .include(native.join("platform/ios"))
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
        println!("cargo:rustc-link-lib=framework=CoreVideo");
        println!("cargo:rustc-link-lib=framework=IOSurface");
        println!("cargo:rustc-link-lib=framework=QuartzCore");

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

    if is_linux {
        configure_linux_build(&mut build);
        add_sources(&mut build, &root, common_c);
        add_sources(&mut build, &root, linux_sources);
        rerun_for(&root, common_c);
        rerun_for(&root, linux_sources);
    }

    if is_android {
        add_sources(&mut build, &root, android_sources);
        build
            .include(native.join("platform/android"))
            .define("__ANDROID__", None)
            .flag("-std=c17");
        rerun_for(&root, android_sources);
    }

    build.compile("native_gui");

    if is_windows {
        let mut cpp_build = Build::new();
        cpp_build.cpp(true).include(&native).define("_WIN32", None);
        add_sources(&mut cpp_build, &root, windows_cpp);
        cpp_build.compile("native_gui_cpp");
    }

    let common_headers: &[&str] = &[
        "native/common/types.h",
        "native/common/errors.h",
        "native/common/input.h",
        "native/common/platform_api.h",
        "native/common/platform_ops.h",
        "native/common/rust_callbacks.h",
    ];

    let windows_headers: &[&str] = &[
        "native/platform/windows.h",
        "native/platform/windows/utils.h",
        "native/platform/windows/window.h",
        "native/platform/windows/menu.h",
        "native/platform/windows/elements.h",
    ];

    let macos_headers: &[&str] = &[
        "native/platform/macos.h",
        "native/platform/macos/window.h",
        "native/platform/macos/menu.h",
        "native/platform/macos/utils.h",
        "native/platform/macos/elements.h",
    ];

    let linux_headers: &[&str] = &[
        "native/platform/linux.h",
        "native/platform/linux/utils.h",
        "native/platform/linux/window.h",
        "native/platform/linux/menu.h",
        "native/platform/linux/elements.h",
    ];

    rerun_for(&root, common_headers);
    rerun_for(&root, windows_headers);
    rerun_for(&root, macos_headers);
    rerun_for(&root, linux_headers);
}

/// Probes for GTK3 (and optionally X11-XCB) via pkg-config and configures
/// `build`'s include paths / defines accordingly. Exits the process with an
/// actionable message if pkg-config or GTK3 dev files are missing.
fn configure_linux_build(build: &mut Build) {
    if !Command::new("pkg-config")
        .arg("--version")
        .status()
        .is_ok_and(|status| status.success())
    {
        println!("cargo:warning=pkg-config not found. Please install pkg-config.");
        process::exit(1);
    }

    match PkgConfig::new().atleast_version("3.0").probe("gtk+-3.0") {
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
            process::exit(1);
        }
    }

    if PkgConfig::new().probe("x11-xcb").is_ok() {
        build.define("AUREA_HAVE_X11_XCB", None);
    } else {
        println!("cargo:warning=X11-XCB development files not found; XCB GPU surfaces disabled");
    }
}
