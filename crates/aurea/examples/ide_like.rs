//! VS Code-like IDE layout example
//!
//! Demonstrates:
//! - Activity bar (Explorer, Search, SCM) and sidebar with VS Code / Finder styling
//! - File tree (EXPLORER, OPEN EDITORS, OUTLINE) and Finder-style Favorites/Locations
//! - TabBar with drag-to-detach, popup windows, Return to Main
//! - Panel tabs (Terminal, Problems, Output, Debug Console), status bar
//!
//! SplitView divider position is in pixels.

use aurea::elements::{
    Box, BoxOrientation, Button, Container, Element, Label, SidebarList, SplitOrientation,
    SplitView, TabBar, TextEditor, TextView,
};
use aurea::logger;
use aurea::{AureaResult, Window, WindowManager, WindowType};
use log::LevelFilter;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const WINDOW_WIDTH: i32 = 1280;
const WINDOW_HEIGHT: i32 = 800;
const SIDEBAR_WIDTH: i32 = 260;
const PANEL_HEIGHT: i32 = 200;
const POPUP_WIDTH: i32 = 640;
const POPUP_HEIGHT: i32 = 480;

const FILE_WELCOME: &str = r#"// Welcome to Aurea Editor
// Try: View > Move to New Window (detach tab)
//      Then "Return to Main" in the popup

fn main() {
    println!("Hello, Aurea!");
}"#;

const FILE_MAIN: &str = r#"mod utils;

fn main() {
    let name = "Aurea";
    println!("Hello from {}!", name);
    utils::greet();
}"#;

const FILE_UTILS: &str = r#"pub fn greet() {
    println!("Welcome to the Aurea Editor demo.");
}"#;

const FILE_CARGO: &str = r#"[package]
name = "aurea-editor"
version = "0.1.0"
edition = "2024"

[dependencies]
"#;

const FILE_README: &str = r#"# Aurea Editor

A VS Code-like IDE layout with detachable tabs.
"#;

const TERMINAL_OUTPUT: &str = r#"> cargo build
   Compiling aurea-editor v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in 0.42s

> _"#;

const FILES: &[(&str, &str)] = &[
    ("Welcome", FILE_WELCOME),
    ("main.rs", FILE_MAIN),
    ("utils.rs", FILE_UTILS),
    ("Cargo.toml", FILE_CARGO),
    ("README.md", FILE_README),
];

#[derive(Clone)]
struct DetachedPopup {
    window: Arc<Window>,
    filename: String,
    editor: Arc<Mutex<SendableTextEditor>>,
}

struct AppState {
    file_contents: HashMap<String, String>,
    current_file: Option<String>,
    detached: Vec<DetachedPopup>,
    pending_file_index: Option<i32>,
}

impl AppState {
    fn new() -> Self {
        let mut file_contents = HashMap::new();
        for (name, content) in FILES {
            file_contents.insert((*name).to_string(), (*content).to_string());
        }
        Self {
            file_contents,
            current_file: Some("Welcome".to_string()),
            detached: Vec::new(),
            pending_file_index: None,
        }
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.file_contents.get(name).map(|s| s.as_str())
    }

    fn set(&mut self, name: &str, content: String) {
        self.file_contents.insert(name.to_string(), content);
    }
}

struct SendableTextEditor(TextEditor);

unsafe impl Send for SendableTextEditor {}
unsafe impl Sync for SendableTextEditor {}

impl std::ops::Deref for SendableTextEditor {
    type Target = TextEditor;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SendableTextEditor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

struct SharedSidebarList(Arc<Mutex<SidebarList>>);

impl Element for SharedSidebarList {
    fn handle(&self) -> *mut std::os::raw::c_void {
        self.0.lock().unwrap().handle()
    }

    unsafe fn invalidate_platform(&self, rect: Option<aurea::render::Rect>) {
        let guard = self.0.lock().unwrap();
        unsafe { Element::invalidate_platform(&*guard, rect) }
    }
}

struct SharedEditor(Arc<Mutex<SendableTextEditor>>);

impl Element for SharedEditor {
    fn handle(&self) -> *mut std::os::raw::c_void {
        self.0.lock().unwrap().handle()
    }

    unsafe fn invalidate_platform(&self, rect: Option<aurea::render::Rect>) {
        let guard = self.0.lock().unwrap();
        unsafe { Element::invalidate_platform(&**guard, rect) }
    }
}

fn main() -> AureaResult<()> {
    logger::init(LevelFilter::Info).unwrap_or_else(|e| {
        eprintln!("Failed to init logger: {}", e);
    });

    let state = Arc::new(Mutex::new(AppState::new()));
    let manager = Arc::new(WindowManager::new());

    let mut main_win = Window::new("Aurea Editor - VS Code-like", WINDOW_WIDTH, WINDOW_HEIGHT)?;
    let ui = setup_main_ui(&mut main_win, Arc::clone(&state), Arc::clone(&manager))?;
    setup_main_menu(&mut main_win, Arc::clone(&state), &ui, Arc::clone(&manager))?;

    let main_arc = Arc::new(main_win);
    manager.register(main_arc.clone());

    let mut last_sidebar_idx: i32 = 0;

    loop {
        let mut exit = false;
        for event in main_arc.poll_events() {
            if let aurea::WindowEvent::CloseRequested = event {
                exit = true;
                break;
            }
        }
        if exit {
            break;
        }

        let pending = {
            let mut s = state.lock().unwrap();
            s.pending_file_index.take()
        };
        if let Some(idx) = pending {
            if let Some((name, _)) = FILES.get(idx as usize) {
                if let (Ok(mut main_ed), Ok(st), Ok(mut tc), Ok(mut sl)) = (
                    ui.editor.lock(),
                    state.lock(),
                    ui.tab_bar.lock(),
                    ui.sidebar_list.lock(),
                ) {
                    if let Some(c) = st.get(name) {
                        let _ = main_ed.set_content(c);
                        let _ = tc.set_selected(idx);
                        let _ = sl.set_selected(idx);
                    }
                }
                if let Ok(mut s) = state.lock() {
                    s.current_file = Some((*name).to_string());
                }
            }
            last_sidebar_idx = idx;
        }

        if let (Ok(sidebar_list), Ok(mut main_ed), Ok(mut st)) = (
            ui.sidebar_list.lock(),
            ui.editor.lock(),
            state.lock(),
        ) {
            let sidebar_idx = sidebar_list.get_selected();
            if let Some(file_idx) = sidebar_idx_to_file_idx(sidebar_idx) {
                if file_idx != last_sidebar_idx {
                    last_sidebar_idx = file_idx;
                    if let Some((name, _)) = FILES.get(file_idx as usize) {
                        if let Some(c) = st.get(name) {
                            let _ = main_ed.set_content(c);
                            st.current_file = Some((*name).to_string());
                        }
                    }
                    drop((sidebar_list, main_ed, st));
                    if let (Ok(mut tc), Ok(mut sl)) = (ui.tab_bar.lock(), ui.sidebar_list.lock()) {
                        let _ = tc.set_selected(file_idx);
                        let _ = sl.set_selected(file_idx);
                    }
                    continue;
                }
            }
        }

        let detached = {
            let s = state.lock().unwrap();
            s.detached.clone()
        };
        for dp in &detached {
            for event in dp.window.poll_events() {
                if let aurea::WindowEvent::CloseRequested = event {
                    let handle = dp.window.handle();
                    if let (Ok(ed), Ok(mut s), Ok(mut main_ed)) =
                        (dp.editor.lock(), state.lock(), ui.editor.lock())
                    {
                        let content: String = ed.get_content().unwrap_or_default();
                        s.set(&dp.filename, content.clone());
                        s.current_file = Some(dp.filename.clone());
                        let _ = main_ed.set_content(&content);
                        if let Some(idx) = FILES.iter().position(|(n, _)| n == &dp.filename) {
                            last_sidebar_idx = idx as i32;
                            if let (Ok(mut tc), Ok(mut sl)) =
                                (ui.tab_bar.lock(), ui.sidebar_list.lock())
                            {
                                let _ = tc.set_selected(idx as i32);
                                let _ = sl.set_selected(idx as i32);
                            }
                        }
                    }
                    manager.unregister(handle);
                    if let Ok(mut s) = state.lock() {
                        s.detached.retain(|d| d.window.handle() != handle);
                    }
                }
            }
        }

        unsafe { aurea::ffi::ng_platform_poll_events() };
        manager.process_all_frames()?;
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    Ok(())
}

fn setup_main_menu(
    window: &mut Window,
    state: Arc<Mutex<AppState>>,
    ui: &UiRefs,
    manager: Arc<WindowManager>,
) -> AureaResult<()> {
    let mut menu_bar = window.create_menu_bar()?;

    let mut file = menu_bar.add_submenu("File")?;
    file.add_item("New File\tCtrl+N", || println!("File -> New File"))?;
    file.add_item("Open...\tCtrl+O", || println!("File -> Open"))?;
    file.add_separator()?;
    for (i, (title, _)) in FILES.iter().enumerate() {
        let idx = i as i32;
        let st = Arc::clone(&state);
        file.add_item(title, move || {
            if let Ok(mut s) = st.lock() {
                s.pending_file_index = Some(idx);
            }
        })?;
    }
    file.add_separator()?;
    file.add_item("Save\tCtrl+S", || println!("File -> Save"))?;
    file.add_item("Save As...\tCtrl+Shift+S", || println!("File -> Save As"))?;
    file.add_item("Exit\tAlt+F4", || std::process::exit(0))?;

    let mut edit = menu_bar.add_submenu("Edit")?;
    edit.add_item("Undo\tCtrl+Z", || println!("Edit -> Undo"))?;
    edit.add_item("Redo\tCtrl+Y", || println!("Edit -> Redo"))?;
    edit.add_item("Cut\tCtrl+X", || println!("Edit -> Cut"))?;
    edit.add_item("Copy\tCtrl+C", || println!("Edit -> Copy"))?;
    edit.add_item("Paste\tCtrl+V", || println!("Edit -> Paste"))?;

    let mut view = menu_bar.add_submenu("View")?;
    view.add_item("Explorer\tCtrl+Shift+E", || println!("View -> Explorer"))?;
    view.add_item("Terminal\tCtrl+`", || println!("View -> Terminal"))?;
    view.add_item("Toggle Sidebar\tCtrl+B", || println!("View -> Toggle Sidebar"))?;

    let state_clone = Arc::clone(&state);
    let editor_clone = Arc::clone(&ui.editor);
    let manager_clone = Arc::clone(&manager);
    view.add_item("Move to New Window\tCtrl+Shift+W", move || {
        if let (Ok(mut s), Ok(ed)) = (state_clone.lock(), editor_clone.lock()) {
            let name = s.current_file.clone().unwrap_or_else(|| "Untitled".to_string());
            let content = ed.get_content().unwrap_or_default();
            s.set(&name, content.clone());
            drop(s);
            drop(ed);
            if let Ok((popup, popup_editor)) = create_editor_popup(
                &name,
                content,
                Arc::clone(&state_clone),
                Arc::clone(&editor_clone),
            ) {
                let popup_arc = Arc::new(popup);
                popup_arc.show();
                manager_clone.register(popup_arc.clone());
                if let Ok(mut s) = state_clone.lock() {
                    s.detached.push(DetachedPopup {
                        window: popup_arc,
                        filename: name,
                        editor: popup_editor,
                    });
                }
            }
        }
    })?;

    let mut help = menu_bar.add_submenu("Help")?;
    help.add_item("Documentation", || println!("Help -> Documentation"))?;
    help.add_item("About Aurea Editor", || println!("About Aurea Editor"))?;

    Ok(())
}

fn create_editor_popup(
    filename: &str,
    content: String,
    state: Arc<Mutex<AppState>>,
    main_editor: Arc<Mutex<SendableTextEditor>>,
) -> AureaResult<(Window, Arc<Mutex<SendableTextEditor>>)> {
    let mut popup =
        Window::with_type(&format!("{filename} (detached)"), POPUP_WIDTH, POPUP_HEIGHT, WindowType::Utility)?;

    let popup_handle = popup.handle() as usize;

    let mut editor = TextEditor::new()?;
    editor.set_content(&content)?;
    let popup_editor = Arc::new(Mutex::new(SendableTextEditor(editor)));

    let mut box_ = Box::new(BoxOrientation::Vertical)?;

    let name = filename.to_string();
    let pe = Arc::clone(&popup_editor);
    let st = Arc::clone(&state);
    let me = Arc::clone(&main_editor);

    box_.add(SharedEditor(Arc::clone(&popup_editor)))?;
    box_.add(Label::new("")?)?;

    let name2 = name.clone();
    let pe2 = Arc::clone(&pe);
    let st2 = Arc::clone(&st);
    let me2 = Arc::clone(&me);
    let idx = FILES.iter().position(|(n, _)| *n == name2).map(|i| i as i32).unwrap_or(0);
    box_.add(Button::with_callback("Return to Main", move || {
        if let (Ok(ed), Ok(mut s), Ok(mut main_ed)) = (pe2.lock(), st2.lock(), me2.lock()) {
            let content = ed.get_content().unwrap_or_default();
            s.set(&name2, content.clone());
            s.current_file = Some(name2.clone());
            s.pending_file_index = Some(idx);
            let _ = main_ed.set_content(&content);
        }
        unsafe {
            aurea::ffi::ng_platform_window_request_close(popup_handle as *mut std::ffi::c_void);
        }
    })?)?;

    popup.set_content(box_)?;

    Ok((popup, popup_editor))
}

struct SharedTabBar(Arc<Mutex<TabBar>>);

impl Element for SharedTabBar {
    fn handle(&self) -> *mut std::os::raw::c_void {
        self.0.lock().unwrap().handle()
    }

    unsafe fn invalidate_platform(&self, rect: Option<aurea::render::Rect>) {
        let guard = self.0.lock().unwrap();
        unsafe { Element::invalidate_platform(&*guard, rect) }
    }
}

struct UiRefs {
    editor: Arc<Mutex<SendableTextEditor>>,
    tab_bar: Arc<Mutex<TabBar>>,
    sidebar_list: Arc<Mutex<SidebarList>>,
}

fn setup_main_ui(
    window: &mut Window,
    state: Arc<Mutex<AppState>>,
    manager: Arc<WindowManager>,
) -> AureaResult<UiRefs> {
    let mut editor = TextEditor::new()?;
    editor.set_content(FILE_WELCOME)?;

    let editor_arc = Arc::new(Mutex::new(SendableTextEditor(editor)));
    let shared_editor = SharedEditor(Arc::clone(&editor_arc));

    let (tab_bar, tab_bar_arc) =
        build_tab_bar(Arc::clone(&state), Arc::clone(&editor_arc), Arc::clone(&manager))?;
    let (sidebar, sidebar_list) = build_sidebar(Arc::clone(&state))?;
    let panel = build_panel()?;
    let status_bar = build_status_bar()?;

    let mut editor_with_tabs = Box::new(BoxOrientation::Vertical)?;
    editor_with_tabs.add_weighted(tab_bar, 0.05)?;
    editor_with_tabs.add_weighted(shared_editor, 1.0)?;

    let mut editor_area = SplitView::new(SplitOrientation::Horizontal)?;
    editor_area.add(sidebar)?;
    editor_area.add(editor_with_tabs)?;
    editor_area.set_divider_position(0, SIDEBAR_WIDTH as f32)?;

    let mut main_split = SplitView::new(SplitOrientation::Vertical)?;
    main_split.add(editor_area)?;
    main_split.add(panel)?;
    main_split.set_divider_position(0, (WINDOW_HEIGHT - PANEL_HEIGHT) as f32)?;

    let mut content_box = Box::new(BoxOrientation::Vertical)?;
    content_box.add_weighted(main_split, 1.0)?;
    content_box.add_weighted(status_bar, 0.03)?;

    window.set_content(content_box)?;

    Ok(UiRefs {
        editor: editor_arc,
        tab_bar: tab_bar_arc,
        sidebar_list,
    })
}

fn build_tab_bar(
    state: Arc<Mutex<AppState>>,
    editor_arc: Arc<Mutex<SendableTextEditor>>,
    manager: Arc<WindowManager>,
) -> AureaResult<(Box, Arc<Mutex<TabBar>>)> {
    let mut tab_bar_box = Box::new(BoxOrientation::Horizontal)?;

    let state_sel = Arc::clone(&state);
    let editor_sel = Arc::clone(&editor_arc);
    let state_det = Arc::clone(&state);
    let editor_det = Arc::clone(&editor_arc);
    let mut tab_bar = TabBar::with_callbacks(
        move |idx| {
            if let Some((name, _)) = FILES.get(idx as usize) {
                if let (Ok(mut ed), Ok(mut st)) = (editor_sel.lock(), state_sel.lock()) {
                    if let Some(c) = st.get(name) {
                        let _ = ed.set_content(c);
                        st.current_file = Some((*name).to_string());
                        st.pending_file_index = Some(idx);
                    }
                }
            }
        },
        move |idx| {
            if let Some((name, _)) = FILES.get(idx as usize) {
                if let (Ok(mut s), Ok(ed)) = (state_det.lock(), editor_det.lock()) {
                    let content = ed.get_content().unwrap_or_default();
                    s.set(name, content.clone());
                    drop(s);
                    drop(ed);
                    if let Ok((popup, popup_editor)) = create_editor_popup(
                        name,
                        content,
                        Arc::clone(&state_det),
                        Arc::clone(&editor_det),
                    )
                    {
                        let popup_arc = Arc::new(popup);
                        popup_arc.show();
                        manager.register(popup_arc.clone());
                        if let Ok(mut s) = state_det.lock() {
                            s.detached.push(DetachedPopup {
                                window: popup_arc,
                                filename: (*name).to_string(),
                                editor: popup_editor,
                            });
                        }
                    }
                }
            }
        },
    )?;

    for (title, _) in FILES {
        tab_bar.add_tab(title)?;
    }
    tab_bar.set_selected(0)?;

    let tab_bar_arc = Arc::new(Mutex::new(tab_bar));
    tab_bar_box.add_weighted(SharedTabBar(Arc::clone(&tab_bar_arc)), 1.0)?;
    tab_bar_box.add(Button::with_callback("+", || println!("New file"))?)?;

    Ok((tab_bar_box, tab_bar_arc))
}

fn build_activity_bar() -> AureaResult<Box> {
    let mut bar = Box::new(BoxOrientation::Vertical)?;
    bar.add(Button::with_callback("Ex", || {})?)?;
    bar.add(Button::with_callback("Se", || println!("Search"))?)?;
    bar.add(Button::with_callback("Gi", || println!("Source Control"))?)?;
    bar.add(Label::new("")?)?;
    Ok(bar)
}

fn sidebar_idx_to_file_idx(idx: i32) -> Option<i32> {
    match idx {
        0..=4 => Some(idx),
        5..=9 => Some(idx - 5),
        _ => None,
    }
}

fn build_sidebar(state: Arc<Mutex<AppState>>) -> AureaResult<(Box, Arc<Mutex<SidebarList>>)> {
    let st = Arc::clone(&state);
    let mut list = SidebarList::with_callback(move |idx| {
        if let Some(file_idx) = sidebar_idx_to_file_idx(idx) {
            if let Ok(mut s) = st.lock() {
                s.pending_file_index = Some(file_idx);
            }
        }
    })?;

    list.add_section("OPEN EDITORS")?;
    for (title, _) in FILES {
        list.add_item(title, 0)?;
    }

    list.add_section("EXPLORER")?;
    list.add_section("AUREA-EDITOR")?;
    list.add_item("Welcome", 0)?;
    list.add_item("main.rs", 1)?;
    list.add_item("utils.rs", 1)?;
    list.add_item("Cargo.toml", 0)?;
    list.add_item("README.md", 0)?;

    list.add_section("OUTLINE")?;
    list.add_item("main", 0)?;
    list.add_item("greet", 0)?;

    list.add_section("FINDER")?;
    list.add_item("Recent", 0)?;
    list.add_item("Desktop", 0)?;
    list.add_item("Documents", 0)?;
    list.add_section("LOCATIONS")?;
    list.add_item("Aurea-Editor", 0)?;

    list.set_selected(0)?;

    let list_arc = Arc::new(Mutex::new(list));

    let activity_bar = build_activity_bar()?;
    let mut sidebar = Box::new(BoxOrientation::Horizontal)?;
    sidebar.add_weighted(activity_bar, 0.0)?;
    sidebar.add_weighted(SharedSidebarList(Arc::clone(&list_arc)), 1.0)?;

    Ok((sidebar, list_arc))
}

fn build_panel() -> AureaResult<Box> {
    let mut panel = Box::new(BoxOrientation::Vertical)?;
    let mut tabs = Box::new(BoxOrientation::Horizontal)?;
    tabs.add(Button::with_callback("Terminal", || {})?)?;
    tabs.add(Button::with_callback("Problems", || println!("Problems"))?)?;
    tabs.add(Button::with_callback("Output", || println!("Output"))?)?;
    tabs.add(Button::with_callback("Debug Console", || println!("Debug Console"))?)?;
    tabs.add(Label::new("")?)?;
    panel.add_weighted(tabs, 0.04)?;
    let mut terminal = TextView::new(false)?;
    terminal.set_content(TERMINAL_OUTPUT)?;
    panel.add_weighted(terminal, 1.0)?;
    Ok(panel)
}

fn build_status_bar() -> AureaResult<Box> {
    let mut bar = Box::new(BoxOrientation::Horizontal)?;
    bar.add(Label::new("main")?)?;
    bar.add(Label::new("  |  ")?)?;
    bar.add(Label::new("UTF-8")?)?;
    bar.add(Label::new("  |  ")?)?;
    bar.add(Label::new("LF")?)?;
    bar.add(Label::new("  |  ")?)?;
    bar.add(Label::new("Rust")?)?;
    bar.add(Label::new("  |  ")?)?;
    bar.add(Label::new("Ln 1, Col 1")?)?;
    bar.add(Label::new("")?)?;
    Ok(bar)
}
