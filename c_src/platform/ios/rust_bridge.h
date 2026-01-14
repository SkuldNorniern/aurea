#ifndef NATIVE_GUI_IOS_RUST_BRIDGE_H
#define NATIVE_GUI_IOS_RUST_BRIDGE_H

#ifdef __cplusplus
extern "C" {
#endif

// Function to be called from Rust to set up the iOS UI
// This should be called from the app delegate after the window is created
void ng_ios_setup_ui(void);

#ifdef __cplusplus
}
#endif

#endif // NATIVE_GUI_IOS_RUST_BRIDGE_H




