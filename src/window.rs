//! Window and associated to window rendering context related functions.

use crate::color::Color;

pub use crate::miniquad;

pub use miniquad::conf::Conf;

/// Block execution until the next frame.
#[must_use = "use `next_frame().await` to advance to the next frame"]
pub fn next_frame() -> crate::exec::FrameFuture {
    crate::thread_assert::same_thread();
    crate::exec::FrameFuture::default()
}

/// Fill window background with solid color.
/// Note: even when "clear_background" is not called explicitly,
/// the screen will be cleared at the beginning of the frame.
pub fn clear_background(color: Color) {
    crate::with_context(|context| context.gpu.with(|backend| context.gl.clear(backend, color)));
}

/// Change per-draw buffer capacities. Vertex capacity is limited to 65,535
/// on WebGL2 (which reserves index 65,535 for primitive restart), otherwise 65,536;
/// index capacity counts indices, independently of their u16 values.
///
/// # Panics
/// Panics before changing buffers if geometry is queued or the index capacity
/// exceeds backend buffer/draw limits. Call `set_default_camera()` to flush first,
/// or call this at the beginning of a frame before drawing.
#[doc(hidden)]
pub fn gl_set_drawcall_buffer_capacity(max_vertices: usize, max_indices: usize) {
    crate::with_context(|context| {
        context.gpu.with(|backend| {
            context
                .gl
                .update_drawcall_capacity(backend, max_vertices, max_indices)
        })
    });
}

/// Returns the selected renderer without exposing its raw backend.
pub fn rendering_backend() -> miniquad::Backend {
    crate::with_backend(|backend| backend.info().backend)
}

pub fn screen_width() -> f32 {
    crate::with_context(|context| context.screen_width / miniquad::window::dpi_scale())
}

pub fn screen_height() -> f32 {
    crate::with_context(|context| context.screen_height / miniquad::window::dpi_scale())
}

pub fn screen_dpi_scale() -> f32 {
    crate::thread_assert::same_thread();
    miniquad::window::dpi_scale()
}

/// Request the window size to be the given value. This takes DPI into account.
///
/// Note that the OS might decide to give a different size. Additionally, the size in macroquad won't be updated until the next `next_frame().await`.
pub fn request_new_screen_size(width: f32, height: f32) {
    crate::thread_assert::same_thread();
    miniquad::window::set_window_size(
        (width * miniquad::window::dpi_scale()) as u32,
        (height * miniquad::window::dpi_scale()) as u32,
    );
    // We do not set the context.screen_width and context.screen_height here.
    // After `set_window_size` is called, EventHandlerFree::resize will be invoked, setting the size correctly.
    // Because the OS might decide to give a different screen dimension, setting the context.screen_* here would be confusing.
}

/// Toggle whether the window is fullscreen.
pub fn set_fullscreen(fullscreen: bool) {
    crate::thread_assert::same_thread();
    miniquad::window::set_fullscreen(fullscreen);
}

/// With `set_panic_handler` set to a handler code, macroquad will use
/// `std::panic::catch_unwind` on user code to catch some panics.
///
/// Sometimes it is nice to let player send a bug report with a screenshot of an
/// error. It is way easier to ask for a screenshot than ask to connect to a phone
/// with adb and post a log.
///
/// For this very case "set_panic_handler" exists.
/// ```ignore
/// set_panic_handler(|msg, backtrace| async move {
///     loop {
///         clear_background(RED);
///         ui::root_ui(|ui| ui.label(None, &msg));
///         for line in backtrace.split('\n') {
///             ui::root_ui(|ui| ui.label(None, line));
///         }
///         next_frame().await;
///      }
/// });
/// ```
///
/// `set_panic_handler` acts as a second app entry-point, that will be used
/// after a panic in user code will happen. Macroquad will also try to catch some OS
/// panics, but not all of them - some compatibility bugs may end up crashing the app.
///
/// Without `set_panic_handler` macroquad will not use `catch_unwind` at all,
/// therefore `panic_handler` is completely optional.
/// NOTE: only with "backtrace" macroquad feature `backtrace` string will contain an
/// actual backtrace. Otherwise only panic location and message will be available.
/// NOTE: on android, even with "backtrace" nice backtrace is available only if the game is compiled with sdk >= 21.
/// To use sdk >= 21 add "min_sdk_version = 21" to Cargo.toml
pub fn set_panic_handler<T, F>(future: F)
where
    T: std::future::Future<Output = ()> + 'static,
    F: Fn(String, String) -> T + Send + Sync + 'static,
{
    let factory: std::rc::Rc<
        dyn Fn(String, String) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>,
    > = std::rc::Rc::new(move |message, backtrace| Box::pin(future(message, backtrace)));
    let old = crate::with_context(|context| {
        context.unwind = true;
        context.recovery_factory.replace(factory)
    });
    drop(old);
    std::panic::set_hook(Box::new(move |info| {
        let message = format!("{info:?}");
        #[cfg(feature = "backtrace")]
        let backtrace_string = format!("{:?}", backtrace::Backtrace::new());
        #[cfg(not(feature = "backtrace"))]
        let backtrace_string = format!("Macroquad compiled without \"backtrace\" feature");
        crate::logging::error!("{}", message);
        crate::logging::error!("{}", backtrace_string);

        let _ = PANIC_MESSAGE.try_with(|pending| {
            if let Ok(mut pending) = pending.try_borrow_mut() {
                *pending = Some((message, backtrace_string));
            }
        });
    }));
}

thread_local! { static PANIC_MESSAGE: std::cell::RefCell<Option<(String, String)>> = const { std::cell::RefCell::new(None) }; }
pub(crate) fn take_panic_message() -> Option<(String, String)> {
    PANIC_MESSAGE.with(|pending| pending.borrow_mut().take())
}
