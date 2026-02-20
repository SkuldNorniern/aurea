//! Browser Example
//!
//! This example demonstrates a browser UI using:
//! - Native UI elements for browser chrome (address bar, tabs, buttons)
//! - Canvas/viewport for rendering web content only

use aurea::elements::{Box, BoxOrientation, Button, Container, TextView};
use aurea::logger;
use aurea::render::{Canvas, Color, Paint, Rect, RendererBackend, Viewport};
use aurea::{AureaResult, Window};
use log::{LevelFilter, debug, info};

fn main() -> AureaResult<()> {
    logger::init(LevelFilter::Info).unwrap_or_else(|e| {
        eprintln!("Failed to initialize logger: {}", e);
    });

    info!("Starting browser example");

    let mut window = Window::new("Aurea Browser", 1200, 800)?;

    // Create main vertical layout
    let mut main_layout = Box::new(BoxOrientation::Vertical)?;

    // Address bar (native UI) - fixed height at top
    info!("Creating address bar");
    let address_bar = create_address_bar()?;
    main_layout.add(address_bar)?;

    // Tabs (native UI) - fixed height below address bar
    info!("Creating tabs");
    let tabs = create_tabs()?;
    main_layout.add(tabs)?;

    // Content area (canvas for web content) - fills remaining space
    info!("Creating content canvas");
    // Canvas will expand to fill available space via layout constraints
    let mut content_canvas = Canvas::new(1200, 600, RendererBackend::Cpu)?;

    // Create viewport for scrollable web content
    // Viewport size will match canvas size
    let mut viewport = Viewport::new(1200.0, 600.0);
    viewport.set_content_size(1200.0, 2000.0); // Content is taller than viewport

    // Render web content to canvas
    render_web_content(&mut content_canvas, &viewport)?;

    main_layout.add(content_canvas)?;

    // Set the main layout as window content
    info!("Setting window content");
    window.set_content(main_layout)?;

    info!("Running window event loop");
    window.run()?;

    Ok(())
}

/// Create the address bar with navigation buttons and URL field
fn create_address_bar() -> AureaResult<Box> {
    let mut address_bar = Box::new(BoxOrientation::Horizontal)?;

    // Navigation buttons (fixed size, left-aligned)
    let back_button = Button::new("←")?;
    address_bar.add(back_button)?;

    let forward_button = Button::new("→")?;
    address_bar.add(forward_button)?;

    let reload_button = Button::new("↻")?;
    address_bar.add(reload_button)?;

    // URL text field (expands to fill remaining space)
    let mut url_field = TextView::new(true)?;
    url_field.set_content("https://example.com")?;
    address_bar.add(url_field)?;

    info!("Address bar created with navigation buttons and URL field");
    Ok(address_bar)
}

/// Create tabs bar with better tab management
fn create_tabs() -> AureaResult<Box> {
    let mut tabs = Box::new(BoxOrientation::Horizontal)?;

    // Create tab buttons
    // First tab (active) - represents the current page
    info!("Adding tab buttons");
    let tab1 = Button::new("Tab 1")?;
    tabs.add(tab1)?;

    // Second tab
    let tab2 = Button::new("Tab 2")?;
    tabs.add(tab2)?;

    // New tab button (always at the end)
    let new_tab = Button::new("+")?;
    tabs.add(new_tab)?;

    info!("Tabs bar created with 2 tabs and new tab button");
    Ok(tabs)
}

/// Render web content to the canvas (only the actual web page content)
fn render_web_content(canvas: &mut Canvas, viewport: &Viewport) -> AureaResult<()> {
    debug!("Rendering web content to canvas");

    canvas.draw(|ctx| {
        // Clear with white background (typical web page background)
        ctx.clear(Color::rgb(255, 255, 255))?;

        // Apply viewport transform for scrolling
        ctx.save()?;
        ctx.transform(viewport.scroll_transform())?;

        // Render web page content
        render_page_content(ctx)?;

        ctx.restore()?;

        // Render scrollbar (outside viewport transform)
        render_scrollbar(ctx, viewport)?;

        Ok(())
    })?;

    canvas.invalidate();
    Ok(())
}

/// Render actual web page content (HTML-like content)
fn render_page_content(ctx: &mut dyn aurea::render::DrawingContext) -> AureaResult<()> {
    // Page header
    let header_rect = Rect::new(0.0, 0.0, 1200.0, 100.0);
    let header_paint = Paint::new().color(Color::rgb(240, 240, 240));
    ctx.draw_rect(header_rect, &header_paint)?;

    // Main content area
    let content_rect = Rect::new(20.0, 120.0, 1160.0, 500.0);
    let content_paint = Paint::new().color(Color::rgb(255, 255, 255));
    ctx.draw_rect(content_rect, &content_paint)?;

    // Content border
    let border_paint = Paint::new()
        .color(Color::rgb(220, 220, 220))
        .style(aurea::render::PaintStyle::Stroke)
        .stroke_width(1.0);
    ctx.draw_rect(content_rect, &border_paint)?;

    // Sidebar
    let sidebar_rect = Rect::new(20.0, 640.0, 200.0, 400.0);
    let sidebar_paint = Paint::new().color(Color::rgb(250, 250, 250));
    ctx.draw_rect(sidebar_rect, &sidebar_paint)?;

    // Articles/paragraphs
    for i in 0..5 {
        let article_y = 150.0 + (i as f32 * 80.0);
        let article_rect = Rect::new(40.0, article_y, 1120.0, 60.0);
        let article_paint = Paint::new().color(Color::rgb(245, 245, 245));
        ctx.draw_rect(article_rect, &article_paint)?;
    }

    // Footer
    let footer_rect = Rect::new(0.0, 1900.0, 1200.0, 100.0);
    let footer_paint = Paint::new().color(Color::rgb(240, 240, 240));
    ctx.draw_rect(footer_rect, &footer_paint)?;

    Ok(())
}

/// Render scrollbar
fn render_scrollbar(
    ctx: &mut dyn aurea::render::DrawingContext,
    viewport: &Viewport,
) -> AureaResult<()> {
    if !viewport.can_scroll_vertical() {
        return Ok(());
    }

    let (viewport_width, viewport_height) = viewport.viewport_size();
    let scrollbar_width = 15.0;
    let scrollbar_x = viewport_width - scrollbar_width;
    let scrollbar_rect = Rect::new(scrollbar_x, 0.0, scrollbar_width, viewport_height);

    // Scrollbar track
    let track_paint = Paint::new().color(Color::rgb(240, 240, 240));
    ctx.draw_rect(scrollbar_rect, &track_paint)?;

    // Scrollbar thumb
    let (_, content_height) = viewport.content_size();
    let (_, viewport_height) = viewport.viewport_size();
    let thumb_height = (viewport_height / content_height) * viewport_height;
    let (_, scroll_y) = viewport.scroll_offset();
    let (_, max_scroll_y) = viewport.max_scroll();
    let thumb_y = if max_scroll_y > 0.0 {
        (scroll_y / max_scroll_y) * (viewport_height - thumb_height)
    } else {
        0.0
    };

    let thumb_rect = Rect::new(
        scrollbar_x + 2.0,
        thumb_y + 2.0,
        scrollbar_width - 4.0,
        thumb_height - 4.0,
    );
    let thumb_paint = Paint::new().color(Color::rgb(180, 180, 180));
    ctx.draw_rect(thumb_rect, &thumb_paint)?;

    Ok(())
}
