#include "utils.h"
#include "common/errors.h"
#include "common/rust_callbacks.h"
#include <gtk/gtk.h>
#include <string.h>

static gboolean gtk_initialized = FALSE;
static void ng_linux_watch_theme(void);

int ng_linux_init(void) {
    if (!gtk_initialized) {
        int argc = 0;
        char **argv = NULL;
        gtk_init(&argc, &argv);
        gtk_initialized = TRUE;
        ng_linux_watch_theme();
    }
    return NG_SUCCESS;
}

void ng_linux_cleanup(void) {
    if (gtk_initialized) {
        gtk_initialized = FALSE;
    }
}

gboolean ng_linux_is_initialized(void) {
    return gtk_initialized;
}

static GSettings* ng_linux_interface_settings(void) {
    static gboolean looked = FALSE;
    static GSettings* settings = NULL;
    if (looked) return settings;
    looked = TRUE;
    GSettingsSchemaSource* source = g_settings_schema_source_get_default();
    if (!source) return NULL;
    GSettingsSchema* schema = g_settings_schema_source_lookup(source, "org.gnome.desktop.interface", TRUE);
    if (!schema) return NULL;
    if (g_settings_schema_has_key(schema, "color-scheme")) {
        settings = g_settings_new("org.gnome.desktop.interface");
    }
    g_settings_schema_unref(schema);
    return settings;
}

void ng_linux_beep(void) {
    GdkDisplay* display = gdk_display_get_default();
    if (display) gdk_display_beep(display);
}

int ng_linux_prefers_dark(void) {
    GSettings* desktop = ng_linux_interface_settings();
    if (desktop) {
        gchar* scheme = g_settings_get_string(desktop, "color-scheme");
        int chosen = g_strcmp0(scheme, "prefer-dark") == 0 ? 1 : g_strcmp0(scheme, "prefer-light") == 0 ? 0 : -1;
        g_free(scheme);
        if (chosen >= 0) return chosen;
    }
    GtkSettings* settings = gtk_settings_get_default();
    if (!settings) return desktop ? 0 : -1;
    gboolean dark = FALSE;
    gchar* theme = NULL;
    g_object_get(settings, "gtk-application-prefer-dark-theme", &dark, "gtk-theme-name", &theme, NULL);
    if (theme) {
        gchar* lower = g_ascii_strdown(theme, -1);
        if (strstr(lower, "dark")) dark = TRUE;
        g_free(lower);
        g_free(theme);
    }
    return dark ? 1 : 0;
}

static int g_theme_dark = -1;

static void ng_linux_theme_check(void) {
    int dark = ng_linux_prefers_dark();
    if (dark < 0 || dark == g_theme_dark) return;
    gboolean known = g_theme_dark >= 0;
    g_theme_dark = dark;
    if (known) ng_invoke_theme_changed(dark);
}

static void ng_linux_on_gtk_setting(GObject* object, GParamSpec* spec, gpointer data) {
    (void)object;
    (void)spec;
    (void)data;
    ng_linux_theme_check();
}

static void ng_linux_on_desktop_setting(GSettings* settings, gchar* key, gpointer data) {
    (void)settings;
    (void)key;
    (void)data;
    ng_linux_theme_check();
}

static void ng_linux_watch_theme(void) {
    GtkSettings* settings = gtk_settings_get_default();
    if (settings) {
        g_signal_connect(settings, "notify::gtk-theme-name", G_CALLBACK(ng_linux_on_gtk_setting), NULL);
        g_signal_connect(settings, "notify::gtk-application-prefer-dark-theme",
                         G_CALLBACK(ng_linux_on_gtk_setting), NULL);
    }
    GSettings* desktop = ng_linux_interface_settings();
    if (desktop) {
        g_signal_connect(desktop, "changed::color-scheme", G_CALLBACK(ng_linux_on_desktop_setting), NULL);
    }
    g_theme_dark = ng_linux_prefers_dark();
}

int ng_linux_reduced_motion(void) {
    GtkSettings* settings = gtk_settings_get_default();
    if (!settings) return 0;
    gboolean enabled = TRUE;
    g_object_get(settings, "gtk-enable-animations", &enabled, NULL);
    return enabled ? 0 : 1;
}
