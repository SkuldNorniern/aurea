#include "menu.h"
#include "window.h"
#include "common/errors.h"
#include "common/input.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>
#include <ctype.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    GtkWidget* menu;
    GtkWidget* root;
} MenuRootEntry;

typedef struct {
    GtkWidget* root;
    /* The submenu holding the item, to find it again. */
    GtkWidget* menu;
    unsigned int keycode;
    unsigned int modifiers;
    unsigned int id;
} MenuShortcutEntry;

typedef struct {
    GtkWidget* window;
    GtkWidget* root;
} WindowMenuBinding;

static MenuRootEntry g_menu_roots[256] = {0};
static int g_menu_root_count = 0;
static MenuShortcutEntry g_shortcuts[512] = {0};
static int g_shortcut_count = 0;
static WindowMenuBinding g_window_bindings[256] = {0};
static int g_window_binding_count = 0;

static void register_menu_root(GtkWidget* menu, GtkWidget* root) {
    if (!menu) return;
    for (int i = 0; i < g_menu_root_count; i++) {
        if (g_menu_roots[i].menu == menu) {
            g_menu_roots[i].root = root;
            return;
        }
    }
    if (g_menu_root_count < 256) {
        g_menu_roots[g_menu_root_count].menu = menu;
        g_menu_roots[g_menu_root_count].root = root;
        g_menu_root_count++;
    }
}

static GtkWidget* find_menu_root(GtkWidget* menu) {
    for (int i = 0; i < g_menu_root_count; i++) {
        if (g_menu_roots[i].menu == menu) {
            return g_menu_roots[i].root;
        }
    }
    return menu;
}

static void bind_window_root(GtkWidget* window, GtkWidget* root) {
    for (int i = 0; i < g_window_binding_count; i++) {
        if (g_window_bindings[i].window == window) {
            g_window_bindings[i].root = root;
            return;
        }
    }
    if (g_window_binding_count < 256) {
        g_window_bindings[g_window_binding_count].window = window;
        g_window_bindings[g_window_binding_count].root = root;
        g_window_binding_count++;
    }
}

static GtkWidget* root_for_window(GtkWidget* window) {
    for (int i = 0; i < g_window_binding_count; i++) {
        if (g_window_bindings[i].window == window) {
            return g_window_bindings[i].root;
        }
    }
    return NULL;
}

static int token_eq(const char* a, const char* b) {
    while (*a && *b) {
        if (tolower((unsigned char)*a) != tolower((unsigned char)*b)) {
            return 0;
        }
        a++;
        b++;
    }
    return *a == '\0' && *b == '\0';
}

static int shortcut_keycode(const char* token, unsigned int* keycode) {
    size_t len = strlen(token);
    if (len == 1) {
        char c = (char)toupper((unsigned char)token[0]);
        if (c >= 'A' && c <= 'Z') {
            *keycode = NG_KEY_A + (unsigned int)(c - 'A');
            return 1;
        }
        if (c >= '0' && c <= '9') {
            *keycode = NG_KEY_0 + (unsigned int)(c - '0');
            return 1;
        }
    }

    if (token_eq(token, "Enter")) { *keycode = NG_KEY_ENTER; return 1; }
    if (token_eq(token, "Tab")) { *keycode = NG_KEY_TAB; return 1; }
    if (token_eq(token, "Esc") || token_eq(token, "Escape")) { *keycode = NG_KEY_ESCAPE; return 1; }
    if (token_eq(token, "Space")) { *keycode = NG_KEY_SPACE; return 1; }
    if (token_eq(token, "Backspace")) { *keycode = NG_KEY_BACKSPACE; return 1; }
    if (token_eq(token, "Delete")) { *keycode = NG_KEY_DELETE; return 1; }

    if ((token[0] == 'F' || token[0] == 'f') && len <= 3) {
        int n = atoi(token + 1);
        if (n >= 1 && n <= 12) {
            *keycode = NG_KEY_F1 + (unsigned int)(n - 1);
            return 1;
        }
    }
    return 0;
}

static void register_shortcut(GtkWidget* menu, const char* title, unsigned int id) {
    const char* tab = strchr(title, '\t');
    if (!tab || tab[1] == '\0') return;

    unsigned int mods = 0;
    unsigned int keycode = NG_KEY_UNKNOWN;
    char buf[128];
    strncpy(buf, tab + 1, sizeof(buf) - 1);
    buf[sizeof(buf) - 1] = '\0';

    char* part = strtok(buf, "+");
    while (part) {
        while (*part == ' ') part++;
        if (token_eq(part, "Ctrl") || token_eq(part, "Control")) {
            mods |= NG_MOD_CTRL;
        } else if (token_eq(part, "Shift")) {
            mods |= NG_MOD_SHIFT;
        } else if (token_eq(part, "Alt") || token_eq(part, "Option")) {
            mods |= NG_MOD_ALT;
        } else if (token_eq(part, "Cmd") || token_eq(part, "Command") || token_eq(part, "Meta")) {
            mods |= NG_MOD_META;
        } else {
            shortcut_keycode(part, &keycode);
        }
        part = strtok(NULL, "+");
    }

    if (keycode == NG_KEY_UNKNOWN || g_shortcut_count >= 512) return;

    g_shortcuts[g_shortcut_count].root = find_menu_root(menu);
    g_shortcuts[g_shortcut_count].menu = menu;
    g_shortcuts[g_shortcut_count].keycode = keycode;
    g_shortcuts[g_shortcut_count].modifiers = mods;
    g_shortcuts[g_shortcut_count].id = id;
    g_shortcut_count++;
}

static void menu_item_clicked(GtkMenuItem *item, gpointer user_data) {
    (void)user_data;
    guint id = GPOINTER_TO_UINT(g_object_get_data(G_OBJECT(item), "menu-id"));
    ng_invoke_menu_callback(id);
}

/* The item `id` in `menu`, or NULL. */
static GtkWidget* item_with_id(GtkWidget* menu, unsigned int id) {
    if (!menu) return NULL;
    GtkWidget* found = NULL;
    GList* children = gtk_container_get_children(GTK_CONTAINER(menu));
    for (GList* l = children; l && !found; l = l->next) {
        GObject* child = G_OBJECT(l->data);
        if (g_object_get_data(child, "aurea-item") &&
            GPOINTER_TO_UINT(g_object_get_data(child, "menu-id")) == id) {
            found = GTK_WIDGET(child);
        }
    }
    g_list_free(children);
    return found;
}

/* Drops the shortcuts of item `id`, gone or relabelled. */
static void forget_shortcuts(unsigned int id) {
    int kept = 0;
    for (int i = 0; i < g_shortcut_count; i++) {
        if (g_shortcuts[i].id != id) g_shortcuts[kept++] = g_shortcuts[i];
    }
    g_shortcut_count = kept;
}

/* The handle owns a reference. Its window destroys the menu bar along with
   everything else, and that used to free it before the handle was done with
   it, so the handle's own destroy then ran on freed memory. */
NGMenuHandle ng_linux_create_menu(void) {
    GtkWidget *menubar = gtk_menu_bar_new();
    g_object_ref_sink(menubar);
    register_menu_root(menubar, menubar);
    return (NGMenuHandle)menubar;
}

void ng_linux_destroy_menu(NGMenuHandle handle) {
    if (!handle) return;
    gtk_widget_destroy((GtkWidget*)handle);
    g_object_unref(handle);
}

int ng_linux_attach_menu(NGHandle window, NGMenuHandle menu) {
    if (!window || !menu) return NG_ERROR_INVALID_HANDLE;

    GtkWidget* vbox = (GtkWidget*)ng_linux_window_get_content_view(window);
    if (!vbox) return NG_ERROR_PLATFORM_SPECIFIC;

    GtkWidget* menu_widget = (GtkWidget*)menu;
    GtkWidget* parent = gtk_widget_get_parent(menu_widget);
    if (parent && parent != vbox) {
        gtk_container_remove(GTK_CONTAINER(parent), menu_widget);
    }
    if (!gtk_widget_get_parent(menu_widget)) {
        gtk_box_pack_start(GTK_BOX(vbox), menu_widget, FALSE, FALSE, 0);
    }

    // Keep menu at top, above content.
    gtk_box_reorder_child(GTK_BOX(vbox), menu_widget, 0);
    bind_window_root((GtkWidget*)window, find_menu_root(menu_widget));
    gtk_widget_show_all((GtkWidget*)window);
    return NG_SUCCESS;
}

/* Use display part only: "Save\tCtrl+S" -> "Save" for cleaner menu labels. */
static const char* display_title(const char* title, char* buf, size_t buf_size) {
    const char* tab = strchr(title, '\t');
    if (!tab || (size_t)(tab - title) >= buf_size) return title;
    size_t len = (size_t)(tab - title);
    memcpy(buf, title, len);
    buf[len] = '\0';
    return buf;
}

/* GTK marks an access key with "_" and writes a literal "_" as "__".
   Our "&" marks the key and "&&" is a literal "&". Returns the label in buf. */
static const char* gtk_mnemonic_label(const char* title, char* buf, size_t buf_size) {
    size_t out = 0;
    for (const char* p = title; *p && out + 2 < buf_size; p++) {
        if (*p == '&') {
            if (p[1] == '&') {
                buf[out++] = '&';
                p++;
            } else if (p[1]) {
                buf[out++] = '_';
            }
        } else if (*p == '_') {
            buf[out++] = '_';
            buf[out++] = '_';
        } else {
            buf[out++] = *p;
        }
    }
    buf[out] = '\0';
    return buf;
}

NGMenuHandle ng_linux_create_submenu(NGMenuHandle parent_menu, const char* title) {
    if (!parent_menu || !title) return NULL;
    
    char display_buf[256];
    const char* label = display_title(title, display_buf, sizeof(display_buf));
    char label_buf[512];
    GtkWidget *menu_item = gtk_menu_item_new_with_mnemonic(gtk_mnemonic_label(label, label_buf, sizeof(label_buf)));
    GtkWidget *submenu = gtk_menu_new();
    
    gtk_menu_item_set_submenu(GTK_MENU_ITEM(menu_item), submenu);
    gtk_menu_shell_append(GTK_MENU_SHELL(parent_menu), menu_item);
    gtk_widget_show_all(menu_item);

    register_menu_root(submenu, find_menu_root((GtkWidget*)parent_menu));
    
    return (NGMenuHandle)submenu;
}

int ng_linux_add_menu_item(NGMenuHandle menu, const char* title, unsigned int id) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;

    char display_buf[256];
    const char* label = display_title(title, display_buf, sizeof(display_buf));
    char label_buf[512];

    GtkWidget *menu_item = gtk_menu_item_new_with_mnemonic(gtk_mnemonic_label(label, label_buf, sizeof(label_buf)));
    g_object_set_data(G_OBJECT(menu_item), "menu-id", GUINT_TO_POINTER(id));
    g_object_set_data(G_OBJECT(menu_item), "aurea-item", GINT_TO_POINTER(1));
    g_signal_connect(G_OBJECT(menu_item), "activate", 
                     G_CALLBACK(menu_item_clicked), NULL);
    
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), menu_item);
    gtk_widget_show(menu_item);
    register_shortcut((GtkWidget*)menu, title, id);
    
    return NG_SUCCESS;
}

int ng_linux_add_menu_separator(NGMenuHandle menu) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    GtkWidget* separator = gtk_separator_menu_item_new();
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), separator);
    gtk_widget_show(separator);
    return NG_SUCCESS;
}

int ng_linux_handle_menu_shortcut(NGHandle window, unsigned int keycode, unsigned int modifiers) {
    GtkWidget* root = root_for_window((GtkWidget*)window);
    if (!root) return 0;

    for (int i = 0; i < g_shortcut_count; i++) {
        if (g_shortcuts[i].root != root) continue;
        if (g_shortcuts[i].keycode != keycode) continue;
        if (g_shortcuts[i].modifiers != modifiers) continue;
        /* A greyed item's shortcut does nothing, as clicking it would. */
        GtkWidget* item = item_with_id(g_shortcuts[i].menu, g_shortcuts[i].id);
        if (item && !gtk_widget_is_sensitive(item)) continue;
        ng_invoke_menu_callback(g_shortcuts[i].id);
        return 1;
    }
    return 0;
}

int ng_linux_add_menu_check_item(NGMenuHandle menu, const char* title, unsigned int id, int checked, int radio) {
    if (!menu || !title) return NG_ERROR_INVALID_PARAMETER;
    char display_buf[256];
    char label_buf[512];
    GtkWidget* item = gtk_check_menu_item_new_with_mnemonic(
        gtk_mnemonic_label(display_title(title, display_buf, sizeof(display_buf)), label_buf, sizeof(label_buf)));
    gtk_check_menu_item_set_draw_as_radio(GTK_CHECK_MENU_ITEM(item), radio ? TRUE : FALSE);
    /* Before the handler, so setting it up does not count as a click. */
    gtk_check_menu_item_set_active(GTK_CHECK_MENU_ITEM(item), checked ? TRUE : FALSE);
    g_object_set_data(G_OBJECT(item), "menu-id", GUINT_TO_POINTER(id));
    g_object_set_data(G_OBJECT(item), "aurea-item", GINT_TO_POINTER(1));
    g_signal_connect(G_OBJECT(item), "activate", G_CALLBACK(menu_item_clicked), NULL);
    gtk_menu_shell_append(GTK_MENU_SHELL(menu), item);
    gtk_widget_show(item);
    register_shortcut((GtkWidget*)menu, title, id);
    return NG_SUCCESS;
}

int ng_linux_menu_item_set_enabled(NGMenuHandle menu, unsigned int id, int enabled) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    gtk_widget_set_sensitive(item, enabled ? TRUE : FALSE);
    return NG_SUCCESS;
}

int ng_linux_menu_item_set_checked(NGMenuHandle menu, unsigned int id, int checked) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item || !GTK_IS_CHECK_MENU_ITEM(item)) return NG_ERROR_INVALID_PARAMETER;
    /* Setting it activates it; that is the app's doing, not a click. */
    g_signal_handlers_block_by_func(item, G_CALLBACK(menu_item_clicked), NULL);
    gtk_check_menu_item_set_active(GTK_CHECK_MENU_ITEM(item), checked ? TRUE : FALSE);
    g_signal_handlers_unblock_by_func(item, G_CALLBACK(menu_item_clicked), NULL);
    return NG_SUCCESS;
}

int ng_linux_menu_item_set_title(NGMenuHandle menu, unsigned int id, const char* title) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item || !title) return NG_ERROR_INVALID_PARAMETER;
    char display_buf[256];
    char label_buf[512];
    gtk_menu_item_set_use_underline(GTK_MENU_ITEM(item), TRUE);
    gtk_menu_item_set_label(GTK_MENU_ITEM(item),
        gtk_mnemonic_label(display_title(title, display_buf, sizeof(display_buf)), label_buf, sizeof(label_buf)));
    forget_shortcuts(id);
    register_shortcut((GtkWidget*)menu, title, id);
    return NG_SUCCESS;
}

int ng_linux_menu_item_remove(NGMenuHandle menu, unsigned int id) {
    GtkWidget* item = item_with_id((GtkWidget*)menu, id);
    if (!item) return NG_ERROR_INVALID_PARAMETER;
    gtk_widget_destroy(item);
    forget_shortcuts(id);
    return NG_SUCCESS;
}

int ng_linux_menu_clear(NGMenuHandle menu) {
    if (!menu) return NG_ERROR_INVALID_HANDLE;
    GList* children = gtk_container_get_children(GTK_CONTAINER(menu));
    for (GList* l = children; l; l = l->next) {
        GObject* child = G_OBJECT(l->data);
        if (g_object_get_data(child, "aurea-item")) {
            forget_shortcuts(GPOINTER_TO_UINT(g_object_get_data(child, "menu-id")));
        }
        gtk_widget_destroy(GTK_WIDGET(child));
    }
    g_list_free(children);
    return NG_SUCCESS;
}
