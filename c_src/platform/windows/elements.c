#include "elements.h"
#include "common/errors.h"
#include <windows.h>
#include <commctrl.h>
#include <richedit.h>
#include <string.h>
#include <stdio.h>

// Logging function declarations (implemented in Rust)
extern void ng_log_error(const char* msg);
extern void ng_log_warn(const char* msg);
extern void ng_log_info(const char* msg);
extern void ng_log_debug(const char* msg);
extern void ng_log_trace(const char* msg);

// Helper macro for formatted logging
#define LOG_ERROR(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_error(buf); \
} while(0)

#define LOG_WARN(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_warn(buf); \
} while(0)

#define LOG_INFO(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_info(buf); \
} while(0)

#define LOG_DEBUG(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_debug(buf); \
} while(0)

#define LOG_TRACE(fmt, ...) do { \
    char buf[512]; \
    sprintf_s(buf, sizeof(buf), fmt, __VA_ARGS__); \
    ng_log_trace(buf); \
} while(0)

// Layout constants
#define PADDING 12
#define SPACING 8
#define BUTTON_MIN_WIDTH 80
#define BUTTON_MIN_HEIGHT 32
#define LABEL_PADDING 4

// Store box orientation in window user data
#define BOX_ORIENTATION_PROP "AureaBoxOrientation"

// Helper to get box orientation
static int get_box_orientation(HWND box) {
    if (GetPropA(box, BOX_ORIENTATION_PROP)) {
        return (int)(INT_PTR)GetPropA(box, BOX_ORIENTATION_PROP);
    }
    return 1; // Default to vertical
}

// Helper to calculate text size
static void calculate_text_size(HDC hdc, const char* text, int* width, int* height) {
    LOG_TRACE("calculate_text_size: called with text='%s'", text ? text : "(null)");
    
    if (!hdc || !text || !width || !height) {
        LOG_WARN("calculate_text_size: Invalid parameters - hdc=%p, text=%p, width=%p, height=%p", 
                 hdc, text, width, height);
        return;
    }
    
    SIZE text_size;
    int text_len = (int)strlen(text);
    LOG_DEBUG("calculate_text_size: text='%s', len=%d", text, text_len);
    
    if (GetTextExtentPoint32A(hdc, text, text_len, &text_size)) {
        *width = text_size.cx;
        *height = text_size.cy;
        LOG_DEBUG("calculate_text_size: SUCCESS - width=%d, height=%d", *width, *height);
    } else {
        DWORD error = GetLastError();
        LOG_WARN("calculate_text_size: GetTextExtentPoint32A failed - error=%lu, using defaults", error);
        *width = 100;
        *height = 20;
    }
}

// Layout children in a box based on orientation
void layout_box_children(HWND box) {
    LOG_TRACE("layout_box_children: called with box=%p", box);
    
    if (!box || !IsWindow(box)) {
        LOG_WARN("layout_box_children: Invalid box or not a window - box=%p", box);
        return;
    }

    int is_vertical = get_box_orientation(box);
    RECT box_rect;
    GetClientRect(box, &box_rect);
    int box_width = box_rect.right - box_rect.left;
    int box_height = box_rect.bottom - box_rect.top;
    
    LOG_DEBUG("layout_box_children: initial box_width=%d, box_height=%d, is_vertical=%d", 
              box_width, box_height, is_vertical);

    // If this is the root box (parent is a window), ensure it uses full window width
    HWND parent = GetParent(box);
    if (parent) {
        char parent_class[256];
        GetClassNameA(parent, parent_class, sizeof(parent_class));
        LOG_TRACE("layout_box_children: parent class='%s'", parent_class);
        
        if (_stricmp(parent_class, "NativeGuiWindow") == 0) {
            // Root box - use parent window width directly
            RECT parent_rect;
            GetClientRect(parent, &parent_rect);
            int original_box_width = box_width;
            box_width = parent_rect.right - parent_rect.left;
            
            LOG_INFO("layout_box_children: Root box detected - original_width=%d, window_width=%d", 
                     original_box_width, box_width);
            
            // Ensure box is actually this width
            if (box_rect.right - box_rect.left != box_width) {
                LOG_INFO("layout_box_children: Resizing root box from %d to %d", 
                         box_rect.right - box_rect.left, box_width);
                SetWindowPos(box, NULL, 0, 0, box_width, box_height,
                           SWP_NOMOVE | SWP_NOZORDER);
            }
        }
    }

    int x = PADDING;
    int y = PADDING;

    HWND child = GetWindow(box, GW_CHILD);
    while (child) {
        if (IsWindowVisible(child)) {
            // Get current child dimensions
            RECT child_rect;
            GetWindowRect(child, &child_rect);
            int width = child_rect.right - child_rect.left;
            int height = child_rect.bottom - child_rect.top;

            // Check control type and adjust sizing
            char class_name[256];
            GetClassNameA(child, class_name, sizeof(class_name));

            int child_x = x; // Default x position

            // Text controls and labels should fill entire box width with no padding
            // This maximizes text area and prevents unnecessary word wrapping
            if (is_vertical) {
                if (_stricmp(class_name, "RichEdit20A") == 0 || _stricmp(class_name, "EDIT") == 0) {
                    int old_width = width;
                    width = box_width; // Fill entire box width - no padding reduction
                    if (width < 100) width = 100; // Minimum width
                    child_x = 0; // Position text controls at left edge of box
                    LOG_DEBUG("layout_box_children: Text control - old_width=%d, new_width=%d, box_width=%d", 
                             old_width, width, box_width);
                }
                // Labels should also fill width to prevent text clipping
                // Windows class names can be "Static" or "STATIC" - check both
                else if (_stricmp(class_name, "STATIC") == 0) {
                    // Check if it's a label (not a box container)
                    LONG_PTR label_style = GetWindowLongPtrA(child, GWL_STYLE);
                    DWORD style_type = label_style & SS_TYPEMASK;
                    if (style_type == SS_LEFT || style_type == SS_LEFTNOWORDWRAP) {
                        int old_width = width;
                        width = box_width; // Fill entire box width
                        if (width < 50) width = 50; // Minimum width
                        child_x = 0; // Position labels at left edge of box
                        LOG_INFO("layout_box_children: Label resizing - old_width=%d, new_width=%d, box_width=%d", 
                                 old_width, width, box_width);
                        // Enable word wrapping for labels by using SS_LEFT (allows wrapping)
                        if (style_type == SS_LEFTNOWORDWRAP) {
                            SetWindowLongPtrA(child, GWL_STYLE, (label_style & ~SS_TYPEMASK) | SS_LEFT);
                        }
                        // Recalculate height for wrapped text
                        HDC hdc = GetDC(child);
                        if (hdc) {
                            HFONT hfont = (HFONT)SendMessageA(child, WM_GETFONT, 0, 0);
                            if (!hfont) {
                                hfont = (HFONT)GetStockObject(DEFAULT_GUI_FONT);
                            }
                            HFONT old_font = (HFONT)SelectObject(hdc, hfont);
                            
                            RECT text_rect = {0, 0, width, 0};
                            char label_text[512];
                            int text_len = GetWindowTextA(child, label_text, sizeof(label_text));
                            if (text_len > 0) {
                                int old_height = height;
                                DrawTextA(hdc, label_text, text_len, &text_rect, 
                                         DT_LEFT | DT_WORDBREAK | DT_CALCRECT);
                                height = text_rect.bottom + LABEL_PADDING * 2;
                                if (height < 20) height = 20; // Minimum height
                                LOG_DEBUG("layout_box_children: Label height recalc - text='%s', width=%d, old_height=%d, new_height=%d", 
                                         label_text, width, old_height, height);
                            } else {
                                LOG_WARN("layout_box_children: Label has no text");
                            }
                            
                            SelectObject(hdc, old_font);
                            ReleaseDC(child, hdc);
                        }
                    }
                }
            }
            // Buttons keep their calculated size
            // Windows class names can be "Button" or "BUTTON" - check both
            if (_stricmp(class_name, "BUTTON") == 0) {
                if (width < BUTTON_MIN_WIDTH) width = BUTTON_MIN_WIDTH;
                if (height < BUTTON_MIN_HEIGHT) height = BUTTON_MIN_HEIGHT;
            }

            // Position the child
            LOG_INFO("layout_box_children: Positioning child - class='%s', x=%d, y=%d, width=%d, height=%d, box_width=%d", 
                     class_name, child_x, y, width, height, box_width);
            SetWindowPos(child, NULL, child_x, y, width, height,
                        SWP_NOZORDER | SWP_SHOWWINDOW);
            
            // Verify the resize actually happened
            RECT verify_rect;
            GetWindowRect(child, &verify_rect);
            int actual_width = verify_rect.right - verify_rect.left;
            if (actual_width != width) {
                LOG_WARN("layout_box_children: Child resize mismatch - requested=%d, actual=%d", 
                         width, actual_width);
            }

            // Configure RichEdit controls for maximum text area
            if (_stricmp(class_name, "RichEdit20A") == 0) {
                // Set zero margins to maximize text area
                SendMessageA(child, EM_SETMARGINS, EC_LEFTMARGIN | EC_RIGHTMARGIN, MAKELONG(0, 0));
                // Set target device to prevent word wrapping - use a very wide device
                // This allows text to flow horizontally without wrapping
                HDC hdc = GetDC(child);
                if (hdc) {
                    SendMessageA(child, EM_SETTARGETDEVICE, (WPARAM)hdc, width);
                    ReleaseDC(child, hdc);
                }
            }

            if (is_vertical) {
                y += height + SPACING;
                // Reset x for next element
                x = PADDING;
            } else {
                x += width + SPACING;
            }
        }

        child = GetWindow(child, GW_HWNDNEXT);
    }
}

NGHandle ng_windows_create_button(const char* title) {
    if (!title) return NULL;

    // Create button as a child window (will be reparented later if needed)
    // Use a temporary parent that will be changed later
    HWND temp_parent = GetDesktopWindow();

    HWND button = CreateWindowExA(
        0,
        "BUTTON",
        title,
        WS_CHILD | BS_PUSHBUTTON | WS_VISIBLE,
        0, 0, BUTTON_MIN_WIDTH, BUTTON_MIN_HEIGHT,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (button) {
        // Calculate optimal button size based on text
        HDC hdc = GetDC(button);
        if (hdc) {
            HFONT hfont = (HFONT)SendMessageA(button, WM_GETFONT, 0, 0);
            if (!hfont) {
                hfont = (HFONT)GetStockObject(DEFAULT_GUI_FONT);
            }
            HFONT old_font = (HFONT)SelectObject(hdc, hfont);

            int text_width, text_height;
            calculate_text_size(hdc, title, &text_width, &text_height);

            int button_width = text_width + 32; // Padding
            int button_height = text_height + 16;

            if (button_width < BUTTON_MIN_WIDTH) button_width = BUTTON_MIN_WIDTH;
            if (button_height < BUTTON_MIN_HEIGHT) button_height = BUTTON_MIN_HEIGHT;

            SetWindowPos(button, NULL, 0, 0, button_width, button_height,
                       SWP_NOMOVE | SWP_NOZORDER);

            SelectObject(hdc, old_font);
            ReleaseDC(button, hdc);
        }
    }

    return (NGHandle)button;
}

NGHandle ng_windows_create_label(const char* text) {
    if (!text) return NULL;

    // Create label as a child window (will be reparented later if needed)
    // Use a temporary parent that will be changed later
    HWND temp_parent = GetDesktopWindow();

    // Create label with word wrapping enabled for better text display
    // SS_LEFT allows text to wrap within the label width
    HWND label = CreateWindowExA(
        0,
        "STATIC",
        text,
        WS_CHILD | SS_LEFT | WS_VISIBLE,
        0, 0, 200, 20,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (label) {
        // Calculate optimal label height based on text
        // Width will be set by layout function to fill available space
        HDC hdc = GetDC(label);
        if (hdc) {
            HFONT hfont = (HFONT)SendMessageA(label, WM_GETFONT, 0, 0);
            if (!hfont) {
                hfont = (HFONT)GetStockObject(DEFAULT_GUI_FONT);
            }
            HFONT old_font = (HFONT)SelectObject(hdc, hfont);

            int text_width, text_height;
            calculate_text_size(hdc, text, &text_width, &text_height);

            // Set initial height based on text, width will be set by layout
            SetWindowPos(label, NULL, 0, 0,
                       200, // Initial width, will be resized by layout
                       text_height + LABEL_PADDING * 2,
                       SWP_NOMOVE | SWP_NOZORDER);

            SelectObject(hdc, old_font);
            ReleaseDC(label, hdc);
        }
    }

    return (NGHandle)label;
}

NGHandle ng_windows_create_box(int is_vertical) {
    // Create a transparent container window for layout
    // Use STATIC class but make it visually transparent
    // Use a temporary parent that will be changed later
    HWND temp_parent = GetDesktopWindow();

    HWND container = CreateWindowExA(
        0,
        "STATIC",
        NULL,
        WS_CHILD | SS_LEFT | WS_VISIBLE,
        0, 0, 100, 100,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    if (container) {
        // Store orientation in window property
        SetPropA(container, BOX_ORIENTATION_PROP, (HANDLE)(INT_PTR)is_vertical);

        // Make container transparent by setting NULL brush background
        SetClassLongPtrA(container, GCLP_HBRBACKGROUND, (LONG_PTR)GetStockObject(NULL_BRUSH));
    }

    return (NGHandle)container;
}

NGHandle ng_windows_create_text_field(void) {
    // Create a simple single-line text field
    // Use a temporary parent that will be changed later
    HWND temp_parent = GetDesktopWindow();

    HWND edit = CreateWindowExA(
        WS_EX_CLIENTEDGE,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | ES_LEFT | ES_AUTOHSCROLL,
        0, 0, 300, 25,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    return (NGHandle)edit;
}

int ng_windows_box_add(NGHandle box, NGHandle element) {
    if (!box || !element) return NG_ERROR_INVALID_HANDLE;

    HWND box_hwnd = (HWND)box;
    HWND element_hwnd = (HWND)element;

    // Set parent of element to box (this automatically makes it a child window)
    SetParent(element_hwnd, box_hwnd);

    // Ensure WS_CHILD style is set and show the window
    LONG_PTR style = GetWindowLongPtrA(element_hwnd, GWL_STYLE);
    SetWindowLongPtrA(element_hwnd, GWL_STYLE, style | WS_CHILD | WS_VISIBLE);

    // Ensure button has proper visual styling
    char class_name[256];
    GetClassNameA(element_hwnd, class_name, sizeof(class_name));
    if (_stricmp(class_name, "BUTTON") == 0) {
        // Force button to redraw with proper visual style
        InvalidateRect(element_hwnd, NULL, TRUE);
        UpdateWindow(element_hwnd);
    }

    ShowWindow(element_hwnd, SW_SHOW);

    // Layout all children in the box
    layout_box_children(box_hwnd);

    // Update box size to fit all children (only if box doesn't have a parent that's a window)
    // If box is nested in another box, auto-size it
    HWND box_parent = GetParent(box_hwnd);
    if (box_parent) {
        // Check if parent is a window (not a box)
        char parent_class[256];
        GetClassNameA(box_parent, parent_class, sizeof(parent_class));
        BOOL is_window_parent = (_stricmp(parent_class, "NativeGuiWindow") == 0);

        // If this is the root content box, ensure it fills the window width
        if (is_window_parent) {
            // Root content box - ensure it fills window width for proper text control sizing
            RECT parent_rect;
            GetClientRect(box_parent, &parent_rect);

            // Account for menu bar height
            HMENU menu = GetMenu(box_parent);
            int menu_height = 0;
            if (menu) {
                RECT menu_rect;
                if (GetMenuItemRect(box_parent, menu, 0, &menu_rect)) {
                    POINT pt = {menu_rect.left, menu_rect.top};
                    ScreenToClient(box_parent, &pt);
                    menu_height = menu_rect.bottom - menu_rect.top + pt.y;
                } else {
                    menu_height = GetSystemMetrics(SM_CYMENU);
                }
            }

            // Always resize root box to match window size
            // This ensures labels and text controls always fill the full window width
            int target_width = parent_rect.right - parent_rect.left;
            int target_height = parent_rect.bottom - parent_rect.top - menu_height;
            
            // Get current box size for logging
            RECT current_box_rect;
            GetClientRect(box_hwnd, &current_box_rect);
            int current_width = current_box_rect.right - current_box_rect.left;
            int current_height = current_box_rect.bottom - current_box_rect.top;
            
            LOG_INFO("ng_windows_box_add: Root box resize - current=%dx%d, target=%dx%d, window_width=%d", 
                     current_width, current_height, target_width, target_height, target_width);
            
            SetWindowPos(box_hwnd, NULL, 0, 0,
                        target_width, target_height,
                        SWP_NOMOVE | SWP_NOZORDER);

            // Always re-layout after adding element to ensure text controls fill width
            layout_box_children(box_hwnd);
        }
        // Only auto-size if parent is another box, not the main window
        else if (!is_window_parent) {
            int max_x = PADDING;
            int max_y = PADDING;

            HWND child = GetWindow(box_hwnd, GW_CHILD);
            while (child) {
                if (IsWindowVisible(child)) {
                    RECT child_rect;
                    GetWindowRect(child, &child_rect);
                    POINT pt = {child_rect.left, child_rect.top};
                    ScreenToClient(box_hwnd, &pt);

                    int child_right = pt.x + (child_rect.right - child_rect.left);
                    int child_bottom = pt.y + (child_rect.bottom - child_rect.top);

                    if (child_right > max_x) max_x = child_right;
                    if (child_bottom > max_y) max_y = child_bottom;
                }
                child = GetWindow(child, GW_HWNDNEXT);
            }

            // Resize box to fit content (with padding)
            if (max_x > PADDING || max_y > PADDING) {
                SetWindowPos(box_hwnd, NULL, 0, 0, max_x + PADDING, max_y + PADDING,
                            SWP_NOMOVE | SWP_NOZORDER);
            }
        }
    }

    return NG_SUCCESS;
}

NGHandle ng_windows_create_text_editor(void) {
    // Load RichEdit library
    LoadLibraryA("riched20.dll");

    // Create as a child window (will be reparented later if needed)
    // Use a temporary parent that will be changed later
    HWND temp_parent = GetDesktopWindow();

    HWND edit = CreateWindowExA(
        WS_EX_CLIENTEDGE,
        "RichEdit20A",
        "",
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL,
        0, 0, 400, 100,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    // Configure for maximum text area - will be set during layout
    // Margins and target device are set in layout_box_children when width is known

    return (NGHandle)edit;
}

NGHandle ng_windows_create_text_view(int is_editable) {
    // Load RichEdit library
    LoadLibraryA("riched20.dll");

    // Create as a child window (will be reparented later if needed)
    // Use a temporary parent that will be changed later
    HWND temp_parent = GetDesktopWindow();

    DWORD style = WS_CHILD | WS_VSCROLL | WS_HSCROLL | ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL;
    if (!is_editable) {
        style |= ES_READONLY;
    }

    HWND edit = CreateWindowExA(
        WS_EX_CLIENTEDGE,
        "RichEdit20A",
        "",
        style | WS_VISIBLE,
        0, 0, 400, 100,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    // Configure for maximum text area - will be set during layout
    // Margins and target device are set in layout_box_children when width is known

    return (NGHandle)edit;
}

int ng_windows_set_text_content(NGHandle text_handle, const char* content) {
    if (!text_handle || !content) return NG_ERROR_INVALID_PARAMETER;
    
    SetWindowTextA((HWND)text_handle, content);
    return NG_SUCCESS;
}

char* ng_windows_get_text_content(NGHandle text_handle) {
    if (!text_handle) return NULL;
    
    int len = GetWindowTextLengthA((HWND)text_handle);
    if (len == 0) {
        char* empty = (char*)malloc(1);
        if (empty) empty[0] = '\0';
        return empty;
    }
    
    char* buffer = (char*)malloc(len + 1);
    if (!buffer) return NULL;
    
    GetWindowTextA((HWND)text_handle, buffer, len + 1);
    return buffer;
}

void ng_windows_free_text_content(char* content) {
    if (content) {
        free(content);
    }
}

NGHandle ng_windows_create_canvas(int width, int height) {
    // Create a window for custom rendering (parent will be set later)
    // This will be extended to support DirectX/OpenGL
    // Use a temporary parent that will be changed later
    HWND temp_parent = GetDesktopWindow();

    HWND hwnd = CreateWindowExA(
        0,
        "STATIC",
        NULL,
        WS_CHILD,
        0, 0, width, height,
        temp_parent,
        NULL,
        GetModuleHandleA(NULL),
        NULL
    );

    return (NGHandle)hwnd;
}

void ng_windows_canvas_invalidate(NGHandle canvas) {
    if (!canvas) return;
    InvalidateRect((HWND)canvas, NULL, FALSE);
}

