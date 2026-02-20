#include "common.h"
#include "../elements.h"
#include "common/errors.h"
#include <richedit.h>
#include <string.h>

int get_box_orientation(HWND box) {
    if (GetPropA(box, BOX_ORIENTATION_PROP)) {
        return (int)(INT_PTR)GetPropA(box, BOX_ORIENTATION_PROP);
    }
    return 1;
}

void calculate_text_size(HDC hdc, const char* text, int* width, int* height) {
    LOG_TRACE("calculate_text_size: called with text='%s'", text ? text : "(null)");
    
    if (!hdc || !text || !width || !height) {
        LOG_WARN("calculate_text_size: Invalid parameters");
        return;
    }
    
    SIZE text_size;
    int text_len = (int)strlen(text);
    
    if (GetTextExtentPoint32A(hdc, text, text_len, &text_size)) {
        *width = text_size.cx;
        *height = text_size.cy;
    } else {
        *width = 100;
        *height = 20;
    }
}

void layout_box_children(HWND box) {
    LOG_TRACE("layout_box_children: called with box=%p", box);
    
    if (!box || !IsWindow(box)) {
        LOG_WARN("layout_box_children: Invalid box");
        return;
    }

    int is_vertical = get_box_orientation(box);
    RECT box_rect;
    GetClientRect(box, &box_rect);
    int box_width = box_rect.right - box_rect.left;
    int box_height = box_rect.bottom - box_rect.top;

    HWND parent = GetParent(box);
    if (parent) {
        char parent_class[256];
        GetClassNameA(parent, parent_class, sizeof(parent_class));
        
        if (_stricmp(parent_class, "NativeGuiWindow") == 0) {
            RECT parent_rect;
            GetClientRect(parent, &parent_rect);
            box_width = parent_rect.right - parent_rect.left;
            
            if (box_rect.right - box_rect.left != box_width) {
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
            RECT child_rect;
            GetWindowRect(child, &child_rect);
            int width = child_rect.right - child_rect.left;
            int height = child_rect.bottom - child_rect.top;

            char class_name[256];
            GetClassNameA(child, class_name, sizeof(class_name));

            int child_x = x;

            if (!is_vertical && _stricmp(class_name, "AureaCanvas") == 0) {
                int remaining_width = box_width - child_x - PADDING;
                if (remaining_width > 0) {
                    width = remaining_width;
                }
                int available_height = box_height - (PADDING * 2);
                if (available_height > 0) {
                    height = available_height;
                }
            }

            if (is_vertical) {
                if (_stricmp(class_name, "RichEdit20A") == 0 || _stricmp(class_name, "EDIT") == 0) {
                    width = box_width;
                    if (width < 100) width = 100;
                    child_x = 0;
                }
                else if (_stricmp(class_name, "STATIC") == 0) {
                    LONG_PTR label_style = GetWindowLongPtrA(child, GWL_STYLE);
                    DWORD style_type = label_style & SS_TYPEMASK;
                    if (style_type == SS_LEFT || style_type == SS_LEFTNOWORDWRAP) {
                        width = box_width;
                        if (width < 50) width = 50;
                        child_x = 0;
                        if (style_type == SS_LEFTNOWORDWRAP) {
                            SetWindowLongPtrA(child, GWL_STYLE, (label_style & ~SS_TYPEMASK) | SS_LEFT);
                        }
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
                                DrawTextA(hdc, label_text, text_len, &text_rect, 
                                         DT_LEFT | DT_WORDBREAK | DT_CALCRECT);
                                height = text_rect.bottom + LABEL_PADDING * 2;
                                if (height < 20) height = 20;
                            }
                            
                            SelectObject(hdc, old_font);
                            ReleaseDC(child, hdc);
                        }
                    }
                }
            }
            if (_stricmp(class_name, "BUTTON") == 0) {
                if (width < BUTTON_MIN_WIDTH) width = BUTTON_MIN_WIDTH;
                if (height < BUTTON_MIN_HEIGHT) height = BUTTON_MIN_HEIGHT;
            }

            SetWindowPos(child, NULL, child_x, y, width, height,
                        SWP_NOZORDER | SWP_SHOWWINDOW);

            if (_stricmp(class_name, "RichEdit20A") == 0) {
                SendMessageA(child, EM_SETMARGINS, EC_LEFTMARGIN | EC_RIGHTMARGIN, MAKELONG(0, 0));
                HDC hdc = GetDC(child);
                if (hdc) {
                    SendMessageA(child, EM_SETTARGETDEVICE, (WPARAM)hdc, width);
                    ReleaseDC(child, hdc);
                }
            }

            if (is_vertical) {
                y += height + SPACING;
                x = PADDING;
            } else {
                x += width + SPACING;
            }
        }

        child = GetWindow(child, GW_HWNDNEXT);
    }
}

