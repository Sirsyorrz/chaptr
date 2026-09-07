#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// WebKitGTK's DMA-BUF renderer fails against several drivers, most visibly
/// NVIDIA under Wayland: the window never appears and the process exits after
/// "Failed to create GBM buffer". Disabling it costs a little compositing
/// performance and is the difference between the app running and not.
///
/// Must happen before the webview starts, and stays overridable.
#[cfg(target_os = "linux")]
fn appease_webkit() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

#[cfg(not(target_os = "linux"))]
fn appease_webkit() {}

fn main() {
    appease_webkit();
    chaptr::run()
}
