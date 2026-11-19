#include "tray.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gio/gio.h>
#include <gtk/gtk.h>
#include <string.h>
#include <unistd.h>

#define TRAY_MAX_ITEMS 64
#define SNI_PATH "/StatusNotifierItem"
#define SNI_IFACE "org.kde.StatusNotifierItem"
#define MENU_PATH "/MenuBar"
#define MENU_IFACE "com.canonical.dbusmenu"
#define WATCHER_NAME "org.kde.StatusNotifierWatcher"
#define WATCHER_PATH "/StatusNotifierWatcher"
#define WATCHER_IFACE "org.kde.StatusNotifierWatcher"
#define DBUS_NAME "org.freedesktop.DBus"
#define DBUS_PATH "/org/freedesktop/DBus"

static const char SNI_XML[] =
    "<node><interface name='org.kde.StatusNotifierItem'>"
    "<property name='Category' type='s' access='read'/>"
    "<property name='Id' type='s' access='read'/>"
    "<property name='Title' type='s' access='read'/>"
    "<property name='Status' type='s' access='read'/>"
    "<property name='WindowId' type='u' access='read'/>"
    "<property name='IconName' type='s' access='read'/>"
    "<property name='IconPixmap' type='a(iiay)' access='read'/>"
    "<property name='ToolTip' type='(sa(iiay)ss)' access='read'/>"
    "<property name='ItemIsMenu' type='b' access='read'/>"
    "<property name='Menu' type='o' access='read'/>"
    "<method name='Activate'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>"
    "<method name='SecondaryActivate'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>"
    "<method name='ContextMenu'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>"
    "<method name='Scroll'><arg type='i' direction='in'/><arg type='s' direction='in'/></method>"
    "<signal name='NewTitle'/>"
    "<signal name='NewIcon'/>"
    "<signal name='NewToolTip'/>"
    "</interface></node>";

static const char MENU_XML[] =
    "<node><interface name='com.canonical.dbusmenu'>"
    "<method name='GetLayout'>"
    "<arg type='i' direction='in'/><arg type='i' direction='in'/><arg type='as' direction='in'/>"
    "<arg type='u' direction='out'/><arg type='(ia{sv}av)' direction='out'/></method>"
    "<method name='GetGroupProperties'>"
    "<arg type='ai' direction='in'/><arg type='as' direction='in'/>"
    "<arg type='a(ia{sv})' direction='out'/></method>"
    "<method name='AboutToShow'><arg type='i' direction='in'/><arg type='b' direction='out'/></method>"
    "<method name='Event'>"
    "<arg type='i' direction='in'/><arg type='s' direction='in'/>"
    "<arg type='v' direction='in'/><arg type='u' direction='in'/></method>"
    "<signal name='LayoutUpdated'><arg type='u'/><arg type='i'/></signal>"
    "<property name='Version' type='u' access='read'/>"
    "<property name='TextDirection' type='s' access='read'/>"
    "<property name='Status' type='s' access='read'/>"
    "<property name='IconThemePath' type='as' access='read'/>"
    "</interface></node>";

/* A menu row; a separator has no title. */
typedef struct {
    char* title;
    unsigned int id;
} TrayItem;

typedef struct {
    GDBusConnection* conn;
    GDBusNodeInfo* sni_node;
    GDBusNodeInfo* menu_node;
    guint sni_reg;
    guint menu_reg;
    char* bus_name;
    char* title;
    /* ARGB32, big-endian, as the item spec wants it. */
    unsigned char* argb;
    int width, height;
    unsigned int click_id;
    TrayItem items[TRAY_MAX_ITEMS];
    int item_count;
    unsigned int revision;
} NgLinuxTray;

static unsigned int g_tray_serial = 0;

static void tray_show_menu(const NgLinuxTray* tray);

static void sni_method_call(GDBusConnection* conn, const gchar* sender, const gchar* path,
                            const gchar* iface, const gchar* method, GVariant* params,
                            GDBusMethodInvocation* invocation, gpointer data) {
    (void)conn;
    (void)sender;
    (void)path;
    (void)iface;
    (void)params;
    NgLinuxTray* tray = data;
    if (strcmp(method, "Activate") == 0) {
        g_dbus_method_invocation_return_value(invocation, NULL);
        ng_invoke_menu_callback(tray->click_id);
        return;
    }
    if (strcmp(method, "ContextMenu") == 0 || strcmp(method, "SecondaryActivate") == 0) {
        g_dbus_method_invocation_return_value(invocation, NULL);
        tray_show_menu(tray);
        return;
    }
    g_dbus_method_invocation_return_value(invocation, NULL);
}

static GVariant* icon_pixmap(const NgLinuxTray* tray) {
    GVariantBuilder builder;
    g_variant_builder_init(&builder, G_VARIANT_TYPE("a(iiay)"));
    if (tray->argb) {
        gsize size = (gsize)tray->width * (gsize)tray->height * 4;
        GVariant* bytes = g_variant_new_fixed_array(G_VARIANT_TYPE_BYTE, tray->argb, size, 1);
        g_variant_builder_add_value(&builder, g_variant_new("(ii@ay)", tray->width, tray->height, bytes));
    }
    return g_variant_builder_end(&builder);
}

static GVariant* sni_get_property(GDBusConnection* conn, const gchar* sender, const gchar* path,
                                  const gchar* iface, const gchar* name, GError** error,
                                  gpointer data) {
    (void)conn;
    (void)sender;
    (void)path;
    (void)iface;
    const NgLinuxTray* tray = data;
    if (strcmp(name, "Category") == 0) return g_variant_new_string("ApplicationStatus");
    if (strcmp(name, "Id") == 0) return g_variant_new_string(tray->bus_name);
    if (strcmp(name, "Title") == 0) return g_variant_new_string(tray->title);
    if (strcmp(name, "Status") == 0) return g_variant_new_string("Active");
    if (strcmp(name, "WindowId") == 0) return g_variant_new_uint32(0);
    if (strcmp(name, "IconName") == 0) return g_variant_new_string("");
    if (strcmp(name, "IconPixmap") == 0) return icon_pixmap(tray);
    if (strcmp(name, "ToolTip") == 0) {
        GVariant* no_icons = g_variant_new_array(G_VARIANT_TYPE("(iiay)"), NULL, 0);
        return g_variant_new("(s@a(iiay)ss)", "", no_icons, tray->title, "");
    }
    if (strcmp(name, "ItemIsMenu") == 0) return g_variant_new_boolean(FALSE);
    if (strcmp(name, "Menu") == 0) {
        return g_variant_new_object_path(tray->item_count ? MENU_PATH : "/");
    }
    g_set_error(error, G_DBUS_ERROR, G_DBUS_ERROR_UNKNOWN_PROPERTY, "no property %s", name);
    return NULL;
}

static const GDBusInterfaceVTable sni_vtable = {
    .method_call = sni_method_call,
    .get_property = sni_get_property,
};

/* The properties of one row, in the dbusmenu layout. */
static GVariant* menu_props(const TrayItem* item) {
    GVariantBuilder builder;
    g_variant_builder_init(&builder, G_VARIANT_TYPE("a{sv}"));
    if (item->title) {
        g_variant_builder_add(&builder, "{sv}", "label", g_variant_new_string(item->title));
        g_variant_builder_add(&builder, "{sv}", "enabled", g_variant_new_boolean(TRUE));
        g_variant_builder_add(&builder, "{sv}", "visible", g_variant_new_boolean(TRUE));
    } else {
        g_variant_builder_add(&builder, "{sv}", "type", g_variant_new_string("separator"));
    }
    return g_variant_builder_end(&builder);
}

/* The menu's layout node `parent`; the root holds one child per row, and rows have none. */
static GVariant* menu_layout(const NgLinuxTray* tray, int parent) {
    GVariantBuilder children;
    g_variant_builder_init(&children, G_VARIANT_TYPE("av"));
    GVariant* props;
    if (parent == 0) {
        for (int i = 0; i < tray->item_count; i++) {
            GVariant* row = g_variant_new("(i@a{sv}@av)", i + 1, menu_props(&tray->items[i]),
                                          g_variant_new_array(G_VARIANT_TYPE("v"), NULL, 0));
            g_variant_builder_add(&children, "v", row);
        }
        GVariantBuilder root;
        g_variant_builder_init(&root, G_VARIANT_TYPE("a{sv}"));
        g_variant_builder_add(&root, "{sv}", "children-display", g_variant_new_string("submenu"));
        props = g_variant_builder_end(&root);
    } else if (parent >= 1 && parent <= tray->item_count) {
        props = menu_props(&tray->items[parent - 1]);
    } else {
        props = g_variant_new_array(G_VARIANT_TYPE("{sv}"), NULL, 0);
    }
    return g_variant_new("(i@a{sv}@av)", parent, props, g_variant_builder_end(&children));
}

static void menu_method_call(GDBusConnection* conn, const gchar* sender, const gchar* path,
                             const gchar* iface, const gchar* method, GVariant* params,
                             GDBusMethodInvocation* invocation, gpointer data) {
    (void)conn;
    (void)sender;
    (void)path;
    (void)iface;
    NgLinuxTray* tray = data;
    if (strcmp(method, "GetLayout") == 0) {
        gint32 parent = 0;
        g_variant_get_child(params, 0, "i", &parent);
        g_dbus_method_invocation_return_value(
            invocation, g_variant_new("(u@(ia{sv}av))", tray->revision, menu_layout(tray, parent)));
        return;
    }
    if (strcmp(method, "GetGroupProperties") == 0) {
        GVariantBuilder rows;
        g_variant_builder_init(&rows, G_VARIANT_TYPE("a(ia{sv})"));
        for (int i = 0; i < tray->item_count; i++) {
            g_variant_builder_add(&rows, "(i@a{sv})", i + 1, menu_props(&tray->items[i]));
        }
        g_dbus_method_invocation_return_value(
            invocation, g_variant_new("(@a(ia{sv}))", g_variant_builder_end(&rows)));
        return;
    }
    if (strcmp(method, "AboutToShow") == 0) {
        g_dbus_method_invocation_return_value(invocation, g_variant_new("(b)", FALSE));
        return;
    }
    if (strcmp(method, "Event") == 0) {
        gint32 id = 0;
        g_variant_get_child(params, 0, "i", &id);
        GVariant* kind = g_variant_get_child_value(params, 1);
        const char* event = g_variant_get_string(kind, NULL);
        int clicked = strcmp(event, "clicked") == 0;
        g_variant_unref(kind);
        g_dbus_method_invocation_return_value(invocation, NULL);
        if (clicked && id >= 1 && id <= tray->item_count) {
            ng_invoke_menu_callback(tray->items[id - 1].id);
        }
        return;
    }
    g_dbus_method_invocation_return_error(invocation, G_DBUS_ERROR, G_DBUS_ERROR_UNKNOWN_METHOD,
                                          "no method %s", method);
}

static GVariant* menu_get_property(GDBusConnection* conn, const gchar* sender, const gchar* path,
                                   const gchar* iface, const gchar* name, GError** error,
                                   gpointer data) {
    (void)conn;
    (void)sender;
    (void)path;
    (void)iface;
    (void)data;
    if (strcmp(name, "Version") == 0) return g_variant_new_uint32(3);
    if (strcmp(name, "TextDirection") == 0) return g_variant_new_string("ltr");
    if (strcmp(name, "Status") == 0) return g_variant_new_string("normal");
    if (strcmp(name, "IconThemePath") == 0) return g_variant_new_strv(NULL, 0);
    g_set_error(error, G_DBUS_ERROR, G_DBUS_ERROR_UNKNOWN_PROPERTY, "no property %s", name);
    return NULL;
}

static const GDBusInterfaceVTable menu_vtable = {
    .method_call = menu_method_call,
    .get_property = menu_get_property,
};

static void on_tray_menu_item(GtkMenuItem* item, gpointer data) {
    (void)item;
    ng_invoke_menu_callback(GPOINTER_TO_UINT(data));
}

/* The menu for hosts that ask the item to show it rather than read the dbusmenu. */
static void tray_show_menu(const NgLinuxTray* tray) {
    if (tray->item_count == 0) return;
    GtkWidget* menu = gtk_menu_new();
    for (int i = 0; i < tray->item_count; i++) {
        const TrayItem* row = &tray->items[i];
        GtkWidget* item = row->title ? gtk_menu_item_new_with_label(row->title)
                                     : gtk_separator_menu_item_new();
        if (row->title) {
            g_signal_connect(item, "activate", G_CALLBACK(on_tray_menu_item), GUINT_TO_POINTER(row->id));
        }
        gtk_menu_shell_append(GTK_MENU_SHELL(menu), item);
    }
    gtk_widget_show_all(menu);
    g_signal_connect_swapped(menu, "selection-done", G_CALLBACK(gtk_widget_destroy), menu);
    gtk_menu_popup_at_pointer(GTK_MENU(menu), NULL);
}

static void emit_sni(const NgLinuxTray* tray, const char* signal) {
    if (tray->conn) {
        g_dbus_connection_emit_signal(tray->conn, NULL, SNI_PATH, SNI_IFACE, signal, NULL, NULL);
    }
}

/* Owns the name, exports the item and its menu, and registers with the watcher. False when no
   watcher answers. */
static int tray_publish(NgLinuxTray* tray) {
    GError* error = NULL;
    GVariant* reply = g_dbus_connection_call_sync(
        tray->conn, DBUS_NAME, DBUS_PATH, DBUS_NAME, "RequestName",
        g_variant_new("(su)", tray->bus_name, 4u), G_VARIANT_TYPE("(u)"),
        G_DBUS_CALL_FLAGS_NONE, -1, NULL, &error);
    if (!reply) {
        g_clear_error(&error);
        return 0;
    }
    guint32 owner = 0;
    g_variant_get(reply, "(u)", &owner);
    g_variant_unref(reply);
    if (owner != 1) return 0;

    tray->sni_node = g_dbus_node_info_new_for_xml(SNI_XML, &error);
    if (!tray->sni_node) {
        g_clear_error(&error);
        return 0;
    }
    tray->sni_reg = g_dbus_connection_register_object(
        tray->conn, SNI_PATH, g_dbus_node_info_lookup_interface(tray->sni_node, SNI_IFACE),
        &sni_vtable, tray, NULL, &error);
    if (!tray->sni_reg) {
        g_clear_error(&error);
        return 0;
    }

    tray->menu_node = g_dbus_node_info_new_for_xml(MENU_XML, &error);
    if (!tray->menu_node) {
        g_clear_error(&error);
        return 0;
    }
    tray->menu_reg = g_dbus_connection_register_object(
        tray->conn, MENU_PATH, g_dbus_node_info_lookup_interface(tray->menu_node, MENU_IFACE),
        &menu_vtable, tray, NULL, &error);
    if (!tray->menu_reg) {
        g_clear_error(&error);
        return 0;
    }

    reply = g_dbus_connection_call_sync(
        tray->conn, WATCHER_NAME, WATCHER_PATH, WATCHER_IFACE, "RegisterStatusNotifierItem",
        g_variant_new("(s)", tray->bus_name), NULL, G_DBUS_CALL_FLAGS_NONE, -1, NULL, &error);
    if (!reply) {
        g_clear_error(&error);
        return 0;
    }
    g_variant_unref(reply);
    return 1;
}

NGHandle ng_linux_create_tray(const char* tooltip, unsigned int click_id) {
    GError* error = NULL;
    GDBusConnection* conn = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &error);
    if (!conn) {
        g_clear_error(&error);
        return NULL;
    }
    NgLinuxTray* tray = g_new0(NgLinuxTray, 1);
    tray->conn = conn;
    tray->click_id = click_id;
    tray->title = g_strdup(tooltip ? tooltip : "");
    tray->bus_name = g_strdup_printf("org.kde.StatusNotifierItem-%d-%u", (int)getpid(), ++g_tray_serial);
    if (!tray_publish(tray)) {
        ng_linux_destroy_tray(tray);
        return NULL;
    }
    return tray;
}

void ng_linux_destroy_tray(NGHandle handle) {
    NgLinuxTray* tray = handle;
    if (!tray) return;
    if (tray->conn) {
        if (tray->sni_reg) g_dbus_connection_unregister_object(tray->conn, tray->sni_reg);
        if (tray->menu_reg) g_dbus_connection_unregister_object(tray->conn, tray->menu_reg);
        if (tray->bus_name) {
            GVariant* reply = g_dbus_connection_call_sync(
                tray->conn, DBUS_NAME, DBUS_PATH, DBUS_NAME, "ReleaseName",
                g_variant_new("(s)", tray->bus_name), NULL, G_DBUS_CALL_FLAGS_NONE, -1, NULL, NULL);
            if (reply) g_variant_unref(reply);
        }
        g_object_unref(tray->conn);
    }
    if (tray->sni_node) g_dbus_node_info_unref(tray->sni_node);
    if (tray->menu_node) g_dbus_node_info_unref(tray->menu_node);
    for (int i = 0; i < tray->item_count; i++) g_free(tray->items[i].title);
    g_free(tray->title);
    g_free(tray->bus_name);
    g_free(tray->argb);
    g_free(tray);
}

int ng_linux_tray_set_icon_rgba(NGHandle handle, const unsigned char* rgba, unsigned int width, unsigned int height) {
    NgLinuxTray* tray = handle;
    if (!tray || !rgba || width == 0 || height == 0) return NG_ERROR_INVALID_PARAMETER;
    size_t pixels = (size_t)width * height;
    unsigned char* argb = g_malloc(pixels * 4);
    for (size_t i = 0; i < pixels; i++) {
        const unsigned char* p = rgba + i * 4;
        argb[i * 4 + 0] = p[3];
        argb[i * 4 + 1] = p[0];
        argb[i * 4 + 2] = p[1];
        argb[i * 4 + 3] = p[2];
    }
    g_free(tray->argb);
    tray->argb = argb;
    tray->width = (int)width;
    tray->height = (int)height;
    emit_sni(tray, "NewIcon");
    return NG_SUCCESS;
}

int ng_linux_tray_set_tooltip(NGHandle handle, const char* tooltip) {
    NgLinuxTray* tray = handle;
    if (!tray) return NG_ERROR_INVALID_PARAMETER;
    g_free(tray->title);
    tray->title = g_strdup(tooltip ? tooltip : "");
    emit_sni(tray, "NewTitle");
    emit_sni(tray, "NewToolTip");
    return NG_SUCCESS;
}

int ng_linux_tray_add_item(NGHandle handle, const char* title, unsigned int id) {
    NgLinuxTray* tray = handle;
    if (!tray) return NG_ERROR_INVALID_PARAMETER;
    if (tray->item_count >= TRAY_MAX_ITEMS) return NG_ERROR_LIMIT_EXCEEDED;
    TrayItem* row = &tray->items[tray->item_count++];
    row->title = title ? g_strdup(title) : NULL;
    row->id = id;
    tray->revision++;
    if (tray->conn) {
        g_dbus_connection_emit_signal(tray->conn, NULL, MENU_PATH, MENU_IFACE, "LayoutUpdated",
                                      g_variant_new("(ui)", tray->revision, 0), NULL);
    }
    return NG_SUCCESS;
}

int ng_linux_tray_notify(NGHandle handle, const char* title, const char* body) {
    NgLinuxTray* tray = handle;
    if (!tray || !tray->conn) return NG_ERROR_INVALID_PARAMETER;
    GError* error = NULL;
    GVariant* reply = g_dbus_connection_call_sync(
        tray->conn, "org.freedesktop.Notifications", "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications", "Notify",
        g_variant_new("(susss@as@a{sv}i)", g_get_prgname() ? g_get_prgname() : "aurea", 0u, "",
                      title ? title : "", body ? body : "", g_variant_new_strv(NULL, 0),
                      g_variant_new_array(G_VARIANT_TYPE("{sv}"), NULL, 0), -1),
        G_VARIANT_TYPE("(u)"), G_DBUS_CALL_FLAGS_NONE, -1, NULL, &error);
    if (!reply) {
        g_clear_error(&error);
        return NG_ERROR_PLATFORM_SPECIFIC;
    }
    g_variant_unref(reply);
    return NG_SUCCESS;
}
