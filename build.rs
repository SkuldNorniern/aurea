fn main() {
    // Compile the C source files located in the "c_src" directory.
    cc::Build::new()
        .file("c_src/native_gui.c")
        .include("c_src")
        .compile("native_gui");
} 