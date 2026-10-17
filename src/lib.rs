//!
//! `macroquad` is a simple and easy to use game library for Rust programming language.
//!
//! `macroquad` attempts to avoid any rust-specific programming concepts like lifetimes/borrowing, making it very friendly for rust beginners.
//!
//! ## Supported platforms
//!
//! * Primary: HTML5 browser games using WASM/WebGL1/2 and the included matching runtime.
//!   Chrome/Firefox target capable modern OS environments, including Windows, macOS and Linux.
//!   Actual browser fixtures run on Linux; Safari/mobile and every OS/device are not validated.
//! * Secondary development/test path: native Linux X11 with OpenGL.
//!
//! ## Features
//!
//! * Same code for all supported platforms, no platform dependent defines required
//! * Efficient 2D rendering with automatic geometry batching
//! * Minimal amount of dependencies: build after `cargo clean` takes only 16s on x230(~6years old laptop)
//! * Immediate mode UI library included
//! * HTML5 deployment with WASM, WebGL and the included matching JavaScript runtime
//! # Example
//! ```no_run
//! use macroquad::prelude::*;
//!
//! #[macroquad::main("BasicShapes")]
//! async fn main() {
//!     loop {
//!         clear_background(RED);
//!
//!         draw_line(40.0, 40.0, 100.0, 200.0, 15.0, BLUE);
//!         draw_rectangle(screen_width() / 2.0 - 60.0, 100.0, 120.0, 60.0, GREEN);
//!         draw_circle(screen_width() - 30.0, screen_height() - 30.0, 15.0, YELLOW);
//!         draw_text("HELLO", 20.0, 20.0, 20.0, DARKGRAY);
//!
//!         next_frame().await
//!     }
//! }
//!```

use ::miniquad::*;

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;

mod exec;
mod ownership;
mod quad_gl;
#[cfg(test)]
mod test_backend;
mod tobytes;

pub mod audio;
pub mod camera;
pub mod color;
pub mod file;
pub mod input;
pub mod material;
pub mod math;
pub mod models;
pub mod shapes;
pub mod text;
pub mod texture;
pub mod time;
pub mod ui;
pub mod window;

pub mod experimental;

pub mod prelude;

pub mod telemetry;

mod error;

pub use error::Error;

/// Macroquad entry point.
///
/// ```skip
/// #[main("Window name")]
/// async fn main() {
/// }
/// ```
///
/// ```skip
/// fn window_conf() -> Conf {
///     Conf {
///         window_title: "Window name".to_owned(),
///         fullscreen: true,
///         ..Default::default()
///     }
/// }
/// #[macroquad::main(window_conf)]
/// async fn main() {
/// }
/// ```
///
/// ## Error handling
///
/// `async fn main()` can have the same signature as a normal `main` in Rust.
/// The most typical use cases are:
/// * `async fn main() {}`
/// * `async fn main() -> Result<(), Error> {}` (note that `Error` should implement `Debug`)
///
/// When a lot of third party crates are involved and very different errors may happens, `anyhow` crate may help:
/// * `async fn main() -> anyhow::Result<()> {}`
///
/// For better control over game errors custom error type may be introduced:
/// ```skip
/// #[derive(Debug)]
/// enum GameError {
///     FileError(macroquad::FileError),
///     SomeThirdPartyCrateError(somecrate::Error)
/// }
/// impl From<macroquad::file::FileError> for GameError {
///     fn from(error: macroquad::file::FileError) -> GameError {
///         GameError::FileError(error)
///     }
/// }
/// impl From<somecrate::Error> for GameError {
///     fn from(error: somecrate::Error) -> GameError {
///         GameError::SomeThirdPartyCrateError(error)
///     }
/// }
/// ```
pub use macroquad_macro::main;

/// #[macroquad::test] fn test() {}
///
/// Very similar to macroquad::main
/// Right now it will still spawn a window, just like ::main, therefore
/// is not really useful for anything than developping macroquad itself
#[doc(hidden)]
pub use macroquad_macro::test;

/// Cross platform random generator.
pub mod rand {
    pub use quad_rand::*;
}

#[cfg(not(feature = "log-rs"))]
/// Logging macros, available with miniquad "log-impl" feature.
pub mod logging {
    pub use miniquad::{debug, error, info, trace, warn};
}
#[cfg(feature = "log-rs")]
// Use logging facade
pub use ::log as logging;
/// Supported Miniquad value types and clock. Raw rendering backends are private
/// because their safe pointer/buffer APIs do not uphold Rust's safety contract.
///
/// ```compile_fail
/// use macroquad::miniquad::RenderingBackend;
/// ```
/// ```compile_fail
/// macroquad::miniquad::window::new_rendering_backend();
/// ```
/// ```compile_fail
/// use macroquad::window::get_internal_gl;
/// ```
/// ```compile_fail
/// use macroquad::experimental::{scene, collections, state_machine};
/// ```
/// ```compile_fail
/// use macroquad::experimental::coroutines::tweens;
/// ```
/// ```compile_fail
/// use macroquad::prelude::QuadGl;
/// ```
pub mod miniquad {
    pub(crate) use ::miniquad::*;
    pub use ::miniquad::{
        conf, date, Backend, BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation,
        EventHandler, FilterMode, FrontFaceOrder, KeyCode, KeyMods, MouseButton, PipelineParams,
        ShaderError, ShaderSource, StencilFaceState, StencilOp, StencilState, TouchPhase,
        UniformDesc, UniformType,
    };
}

use crate::{
    color::{colors::*, Color},
    quad_gl::QuadGl,
};

use glam::{vec2, Mat4, Vec2};

pub(crate) mod thread_assert {
    pub fn same_thread() {
        super::with_context(|_| ());
    }
}

impl Context {
    fn record_mouse_down(&mut self, btn: MouseButton, x: f32, y: f32) {
        self.mouse_down.insert(btn);
        self.mouse_pressed.insert(btn);

        self.input_events
            .iter_mut()
            .for_each(|arr| arr.push(MiniquadInputEvent::MouseButtonDown { x, y, btn }));

        if !self.cursor_grabbed {
            self.mouse_position = Vec2::new(x, y);
        }

        if self.update_on.mouse_down {
            ::miniquad::window::schedule_update();
        }
    }
    fn record_mouse_up(&mut self, btn: MouseButton, x: f32, y: f32) {
        self.mouse_down.remove(&btn);
        self.mouse_released.insert(btn);

        self.input_events
            .iter_mut()
            .for_each(|arr| arr.push(MiniquadInputEvent::MouseButtonUp { x, y, btn }));

        if !self.cursor_grabbed {
            self.mouse_position = Vec2::new(x, y);
        }
        if self.update_on.mouse_up {
            ::miniquad::window::schedule_update();
        }
    }
}

struct Context {
    screen_width: f32,
    screen_height: f32,

    simulate_mouse_with_touch: bool,
    touch_mouse: input::TouchMouseSimulation,
    physical_left_mouse_down: bool,

    keys_down: HashSet<KeyCode>,
    keys_pressed: HashSet<KeyCode>,
    keys_released: HashSet<KeyCode>,
    mouse_down: HashSet<MouseButton>,
    mouse_pressed: HashSet<MouseButton>,
    mouse_released: HashSet<MouseButton>,
    touches: HashMap<u64, input::Touch>,
    chars_pressed_queue: VecDeque<char>,
    chars_pressed_ui_queue: VecDeque<char>,
    mouse_position: Vec2,
    last_mouse_position: Option<Vec2>,
    mouse_wheel: Vec2,

    prevent_quit_event: bool,
    quit_requested: bool,

    cursor_grabbed: bool,

    input_events: Vec<Vec<MiniquadInputEvent>>,

    gl: QuadGl,
    camera_matrix: Option<Mat4>,
    active_material: Option<material::Material>,
    retained_materials: Vec<material::Material>,

    fonts_storage: text::FontsStorage,

    pc_assets_folder: Option<String>,

    start_time: f64,
    last_frame_time: f64,
    frame_time: f64,

    #[cfg(one_screenshot)]
    counter: usize,

    camera_stack: Vec<camera::CameraState>,
    texture_batcher: texture::Batcher,
    unwind: bool,
    recovery_factory: Option<Rc<dyn Fn(String, String) -> Pin<Box<dyn Future<Output = ()>>>>>,

    gpu: std::rc::Rc<GraphicsOwner>,

    default_filter_mode: crate::quad_gl::FilterMode,
    textures: crate::texture::TexturesContext,

    update_on: conf::UpdateTrigger,

    dropped_files: Vec<DroppedFile>,
}

#[derive(Clone)]
enum MiniquadInputEvent {
    MouseMotion {
        x: f32,
        y: f32,
    },
    MouseWheel {
        x: f32,
        y: f32,
    },
    MouseButtonDown {
        x: f32,
        y: f32,
        btn: MouseButton,
    },
    MouseButtonUp {
        x: f32,
        y: f32,
        btn: MouseButton,
    },
    Char {
        character: char,
        modifiers: KeyMods,
        repeat: bool,
    },
    KeyDown {
        keycode: KeyCode,
        modifiers: KeyMods,
        repeat: bool,
    },
    KeyUp {
        keycode: KeyCode,
        modifiers: KeyMods,
    },
    Touch {
        phase: TouchPhase,
        id: u64,
        x: f32,
        y: f32,
    },
    WindowMinimized,
    WindowRestored,
}

impl MiniquadInputEvent {
    fn repeat<T: ::miniquad::EventHandler>(&self, t: &mut T) {
        use crate::MiniquadInputEvent::*;
        match self {
            MouseMotion { x, y } => t.mouse_motion_event(*x, *y),
            MouseWheel { x, y } => t.mouse_wheel_event(*x, *y),
            MouseButtonDown { x, y, btn } => t.mouse_button_down_event(*btn, *x, *y),
            MouseButtonUp { x, y, btn } => t.mouse_button_up_event(*btn, *x, *y),
            Char {
                character,
                modifiers,
                repeat,
            } => t.char_event(*character, *modifiers, *repeat),
            KeyDown {
                keycode,
                modifiers,
                repeat,
            } => t.key_down_event(*keycode, *modifiers, *repeat),
            KeyUp { keycode, modifiers } => t.key_up_event(*keycode, *modifiers),
            Touch { phase, id, x, y } => t.touch_event(*phase, *id, *x, *y),
            WindowMinimized => t.window_minimized_event(),
            WindowRestored => t.window_restored_event(),
        }
    }
}

impl Context {
    const DEFAULT_BG_COLOR: Color = BLACK;

    fn new(
        update_on: conf::UpdateTrigger,
        default_filter_mode: crate::FilterMode,
        draw_call_vertex_capacity: usize,
        draw_call_index_capacity: usize,
    ) -> Context {
        let mut ctx: Box<dyn ::miniquad::RenderingBackend> =
            ::miniquad::window::new_rendering_backend();
        let (screen_width, screen_height) = ::miniquad::window::screen_size();

        let gpu = GraphicsOwner::new(ctx);
        let gl =
            gpu.with(|ctx| QuadGl::new(ctx, draw_call_vertex_capacity, draw_call_index_capacity));
        gpu.white.set(Some(gl.white_texture));
        Context {
            screen_width,
            screen_height,

            simulate_mouse_with_touch: true,
            touch_mouse: input::TouchMouseSimulation::default(),
            physical_left_mouse_down: false,

            keys_down: HashSet::new(),
            keys_pressed: HashSet::new(),
            keys_released: HashSet::new(),
            chars_pressed_queue: VecDeque::new(),
            chars_pressed_ui_queue: VecDeque::new(),
            mouse_down: HashSet::new(),
            mouse_pressed: HashSet::new(),
            mouse_released: HashSet::new(),
            touches: HashMap::new(),
            mouse_position: vec2(0., 0.),
            last_mouse_position: None,
            mouse_wheel: vec2(0., 0.),

            prevent_quit_event: false,
            quit_requested: false,

            cursor_grabbed: false,

            input_events: Vec::new(),

            camera_matrix: None,
            active_material: None,
            retained_materials: vec![],
            gl,

            fonts_storage: gpu.with(|ctx| text::FontsStorage::new(ctx, gpu.clone())),
            texture_batcher: gpu.with(|ctx| texture::Batcher::new(ctx, gpu.clone())),
            camera_stack: vec![],

            pc_assets_folder: None,

            start_time: ::miniquad::date::now(),
            last_frame_time: ::miniquad::date::now(),
            frame_time: 1. / 60.,

            #[cfg(one_screenshot)]
            counter: 0,
            unwind: false,
            recovery_factory: None,

            gpu: gpu.clone(),

            default_filter_mode,
            textures: crate::texture::TexturesContext::new(gpu.clone()),
            update_on,

            dropped_files: Vec::new(),
        }
    }

    /// Returns the files which have been dropped onto the window.
    pub fn dropped_files(&mut self) -> Vec<DroppedFile> {
        std::mem::take(&mut self.dropped_files)
    }

    fn begin_frame(&mut self) {
        let color = Self::DEFAULT_BG_COLOR;

        self.gpu
            .with(|backend| backend.clear(Some((color.r, color.g, color.b, color.a)), None, None));
        self.gl.reset();
    }

    fn end_frame(&mut self) {
        // Stage already submitted game geometry with its retained camera.
        // UI queued afterwards uses screen space without changing that camera.
        let screen_mat = self.pixel_perfect_projection_matrix();
        self.gpu.with(|backend| self.gl.draw(backend, screen_mat));
        self.retained_materials.clear();

        self.gpu.with(|backend| backend.commit_frame());

        self.mouse_wheel = Vec2::new(0., 0.);
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.mouse_pressed.clear();
        self.mouse_released.clear();
        self.last_mouse_position = Some(
            self.mouse_position / vec2(self.screen_width, self.screen_height) * 2. - Vec2::ONE,
        );

        self.quit_requested = false;

        self.collect_resources();

        // remove all touches that were Ended or Cancelled
        self.touches.retain(|_, touch| {
            touch.phase != input::TouchPhase::Ended && touch.phase != input::TouchPhase::Cancelled
        });

        // change all Started or Moved touches to Stationary
        for touch in self.touches.values_mut() {
            if touch.phase == input::TouchPhase::Started || touch.phase == input::TouchPhase::Moved
            {
                touch.phase = input::TouchPhase::Stationary;
            }
        }

        self.dropped_files.clear();
    }

    fn shutdown(self, release_gpu: bool) {
        let Context {
            mut gl,
            gpu,
            fonts_storage,
            texture_batcher,
            camera_stack,
            active_material,
            retained_materials,
            recovery_factory,
            ..
        } = self;
        drop((
            fonts_storage,
            texture_batcher,
            camera_stack,
            active_material,
            retained_materials,
            recovery_factory,
        ));
        if release_gpu {
            gpu.with(|backend| gl.shutdown(backend));
        }
        drop(gl);
        let pending = gpu.pending.take();
        if release_gpu {
            gpu.with(|backend| {
                for resource in pending {
                    match resource {
                        ResourceDelete::Texture(id) => backend.delete_texture(id),
                        ResourceDelete::RenderPass(id) => backend.delete_render_pass(id),
                        ResourceDelete::Pipeline(_) => (), // all renderer pipelines already released
                    }
                }
            });
        }
    }

    fn collect_resources(&mut self) {
        let removed = self.gpu.pending.take();
        self.gpu.with(|backend| {
            // Frame draw calls are flushed before any queued GPU deletion.
            for resource in removed {
                match resource {
                    ResourceDelete::Texture(id) => backend.delete_texture(id),
                    ResourceDelete::RenderPass(id) => backend.delete_render_pass(id),
                    ResourceDelete::Pipeline(id) => self.gl.delete_pipeline(backend, id),
                }
            }
        });
    }

    pub(crate) fn pixel_perfect_projection_matrix(&self) -> glam::Mat4 {
        let (width, height) = ::miniquad::window::screen_size();

        let dpi = ::miniquad::window::dpi_scale();

        glam::Mat4::orthographic_rh_gl(0., width / dpi, height / dpi, 0., -1., 1.)
    }

    pub(crate) fn projection_matrix(&self) -> glam::Mat4 {
        if let Some(matrix) = self.camera_matrix {
            matrix
        } else {
            self.pixel_perfect_projection_matrix()
        }
    }

    pub(crate) fn perform_render_passes(&mut self) {
        let matrix = self.projection_matrix();

        self.gpu.with(|backend| self.gl.draw(backend, matrix));
        self.retained_materials.clear();
    }
}

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
thread_local! {
    static CONTEXT: RefCell<Option<Rc<ownership::Scoped<Context>>>> = const { RefCell::new(None) };
    static GRAPHICS: RefCell<Option<Rc<GraphicsOwner>>> = const { RefCell::new(None) };
}

pub(crate) enum ResourceDelete {
    Texture(::miniquad::TextureId),
    RenderPass(::miniquad::RenderPass),
    Pipeline(quad_gl::GlPipeline),
}
impl std::fmt::Debug for GraphicsOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GraphicsOwner")
    }
}
pub(crate) struct GraphicsOwner {
    backend: ownership::Scoped<Box<dyn ::miniquad::RenderingBackend>>,
    pending: RefCell<Vec<ResourceDelete>>,
    active: Cell<bool>,
    textures: Cell<usize>,
    white: Cell<Option<::miniquad::TextureId>>,
    next_texture_key: Cell<u64>,
}
impl GraphicsOwner {
    fn new(backend: Box<dyn ::miniquad::RenderingBackend>) -> Rc<Self> {
        Rc::new(Self {
            backend: ownership::Scoped::new(backend),
            pending: RefCell::new(vec![]),
            active: Cell::new(true),
            textures: Cell::new(0),
            white: Cell::new(None),
            next_texture_key: Cell::new(0),
        })
    }
    pub(crate) fn with<R>(&self, f: impl FnOnce(&mut dyn ::miniquad::RenderingBackend) -> R) -> R {
        assert!(self.active.get(), "Graphics owner has shut down");
        self.backend.with(|backend| f(&mut **backend))
    }
    pub(crate) fn assert_current(&self) {
        assert!(self.active.get(), "Graphics owner has shut down");
        let current = graphics_owner();
        assert!(
            std::ptr::eq(self, &*current),
            "Resource belongs to another graphics owner"
        );
    }
    pub(crate) fn defer(&self, resource: ResourceDelete) {
        self.pending.borrow_mut().push(resource);
    }
}
pub(crate) fn graphics_owner() -> Rc<GraphicsOwner> {
    GRAPHICS
        .with(|owner| owner.borrow().as_ref().cloned())
        .expect("Graphics are not initialized on this thread")
}
fn with_context<R>(f: impl FnOnce(&mut Context) -> R) -> R {
    let owner = CONTEXT
        .with(|owner| owner.borrow().as_ref().cloned())
        .expect("Macroquad is not initialized on this thread");
    owner.with(f)
}
fn with_backend<R>(f: impl FnOnce(&mut dyn ::miniquad::RenderingBackend) -> R) -> R {
    graphics_owner().with(f)
}

// Each native graphics test needs its own backend process: Miniquad intentionally
// initializes its process-wide native display once. Never reset its globals.
#[doc(hidden)]
pub mod test {
    /// Runs an attributed graphics test in a fresh harness process. The exact
    /// child selection preserves grouped/parallel cargo test invocations.
    pub fn run(full_name: &str, test: impl FnOnce()) {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (full_name, test);
            panic!("macroquad::test requires native process isolation; use finite WASM examples for browser tests");
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            const CHILD: &str = "MACROQUAD_TEST_CHILD";
            let name = full_name
                .split_once("::")
                .map_or(full_name, |(_, name)| name);
            let acknowledgement = format!("macroquad isolated test: {full_name}");
            if std::env::var(CHILD).as_deref() == Ok(name) {
                eprintln!("{acknowledgement}");
                test();
                return;
            }
            let output = std::process::Command::new(
                std::env::current_exe().expect("Test executable unavailable"),
            )
            .args([
                "--exact",
                name,
                "--include-ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(CHILD, name)
            .output()
            .expect("Could not start isolated graphics test");
            assert!(
                output.status.success()
                    && String::from_utf8_lossy(&output.stderr).contains(&acknowledgement),
                "Isolated graphics test {name} failed ({}):\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

struct Stage {
    closed: bool,
    main_future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Stage {
    fn shutdown(&mut self, release_gpu: bool) {
        if self.closed {
            return;
        }
        self.closed = true;
        if !release_gpu {
            graphics_owner().active.set(false);
        }
        // Destroy user futures and recovery captures while graphics are still
        // available and no Context/UI/backend scope is active.
        drop(std::mem::replace(&mut self.main_future, Box::pin(async {})));
        experimental::coroutines::shutdown();
        audio::shutdown();
        let factory = with_context(|context| context.recovery_factory.take());
        drop(factory);
        telemetry::shutdown();
        ui::ui_context::shutdown();
        with_context(|context| {
            context.active_material = None;
            context.retained_materials.clear();
            context.gl.reset();
            if release_gpu {
                context.collect_resources();
            }
        });
        let context = CONTEXT.with(|slot| slot.borrow_mut().take());
        if let Some(context) = context {
            let context = Rc::try_unwrap(context)
                .ok()
                .expect("Active context at shutdown")
                .into_inner();
            context.shutdown(release_gpu);
        }
        let graphics = GRAPHICS.with(|slot| slot.borrow_mut().take());
        if let Some(graphics) = graphics {
            graphics.active.set(false);
        }
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        // Some native backends drop their handler after destroying the GL
        // context. Only live event callbacks may release GPU resources.
        self.shutdown(false);
    }
}

impl EventHandler for Stage {
    fn resize_event(&mut self, width: f32, height: f32) {
        if self.closed {
            return;
        }
        let _z = telemetry::ZoneGuard::new("Event::resize_event");
        with_context(|context| {
            context.screen_width = width;
            context.screen_height = height;
        });

        if ::miniquad::window::blocking_event_loop() {
            ::miniquad::window::schedule_update();
        }
    }

    fn raw_mouse_motion(&mut self, x: f32, y: f32) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            if context.cursor_grabbed {
                context.mouse_position += Vec2::new(x, y);

                let event = MiniquadInputEvent::MouseMotion {
                    x: context.mouse_position.x,
                    y: context.mouse_position.y,
                };
                context
                    .input_events
                    .iter_mut()
                    .for_each(|arr| arr.push(event.clone()));
            }
        })
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            if !context.cursor_grabbed {
                context.mouse_position = Vec2::new(x, y);

                context
                    .input_events
                    .iter_mut()
                    .for_each(|arr| arr.push(MiniquadInputEvent::MouseMotion { x, y }));
            }

            if context.update_on.mouse_motion {
                ::miniquad::window::schedule_update();
            }
        })
    }

    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            context.mouse_wheel.x = x;
            context.mouse_wheel.y = y;

            context
                .input_events
                .iter_mut()
                .for_each(|arr| arr.push(MiniquadInputEvent::MouseWheel { x, y }));

            if context.update_on.mouse_wheel {
                ::miniquad::window::schedule_update();
            }
        })
    }

    fn mouse_button_down_event(&mut self, btn: MouseButton, x: f32, y: f32) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            if btn == MouseButton::Left {
                context.physical_left_mouse_down = true;
            }
            context.record_mouse_down(btn, x, y);
        });
    }

    fn mouse_button_up_event(&mut self, btn: MouseButton, x: f32, y: f32) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            if btn == MouseButton::Left {
                context.physical_left_mouse_down = false;
                if context.touch_mouse.held() {
                    if context.update_on.mouse_up {
                        ::miniquad::window::schedule_update();
                    }
                    return;
                }
            }
            context.record_mouse_up(btn, x, y);
        });
    }

    fn mouse_leave_event(&mut self) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            context.physical_left_mouse_down = false;
            context.mouse_released.extend(context.mouse_down.drain());
        })
    }

    fn mouse_enter_event(&mut self, btn: MouseButton, x: f32, y: f32) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            if !context.cursor_grabbed {
                context.mouse_position = Vec2::new(x, y);
            }

            context
                .input_events
                .iter_mut()
                .for_each(|arr| arr.push(MiniquadInputEvent::MouseButtonUp { x, y, btn }));
            if btn != MouseButton::Unknown {
                if btn == MouseButton::Left {
                    context.physical_left_mouse_down = true;
                }
                context.mouse_down.insert(btn);
                context.mouse_pressed.insert(btn);
            }

            if context.update_on.mouse_up {
                ::miniquad::window::schedule_update();
            }
        })
    }

    fn touch_event(&mut self, phase: TouchPhase, id: u64, x: f32, y: f32) {
        if self.closed {
            return;
        }
        with_context(|context| {
            context.touches.insert(
                id,
                input::Touch {
                    id,
                    phase: phase.into(),
                    position: Vec2::new(x, y),
                },
            );
            if context.simulate_mouse_with_touch {
                match context.touch_mouse.event(phase, id) {
                    Some(TouchPhase::Started) => context.record_mouse_down(MouseButton::Left, x, y),
                    Some(TouchPhase::Ended) => {
                        if !context.physical_left_mouse_down {
                            context.record_mouse_up(MouseButton::Left, x, y);
                        } else if context.update_on.mouse_up {
                            ::miniquad::window::schedule_update();
                        }
                    }
                    _ => (),
                }
            }
        });
        if phase == TouchPhase::Moved && with_context(|context| context.simulate_mouse_with_touch) {
            self.mouse_motion_event(x, y);
        }
        with_context(|context| {
            for events in &mut context.input_events {
                events.push(MiniquadInputEvent::Touch { phase, id, x, y });
            }
            // Preserve the legacy mouse-down wake and honor the independent
            // touch trigger even when mouse simulation also handles the event.
            if context.update_on.touch || context.update_on.mouse_down {
                ::miniquad::window::schedule_update();
            }
        });
    }

    fn char_event(&mut self, character: char, modifiers: KeyMods, repeat: bool) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            context.chars_pressed_queue.push_back(character);
            context.chars_pressed_ui_queue.push_back(character);

            context.input_events.iter_mut().for_each(|arr| {
                arr.push(MiniquadInputEvent::Char {
                    character,
                    modifiers,
                    repeat,
                });
            });
        })
    }

    fn key_down_event(&mut self, keycode: KeyCode, modifiers: KeyMods, repeat: bool) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            context.keys_down.insert(keycode);
            if repeat == false {
                context.keys_pressed.insert(keycode);
            }

            context.input_events.iter_mut().for_each(|arr| {
                arr.push(MiniquadInputEvent::KeyDown {
                    keycode,
                    modifiers,
                    repeat,
                });
            });
            if context
                .update_on
                .specific_key
                .as_ref()
                .map_or(context.update_on.key_down, |keys| keys.contains(&keycode))
            {
                ::miniquad::window::schedule_update();
            }
        })
    }

    fn key_up_event(&mut self, keycode: KeyCode, modifiers: KeyMods) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            context.keys_down.remove(&keycode);
            context.keys_released.insert(keycode);

            context
                .input_events
                .iter_mut()
                .for_each(|arr| arr.push(MiniquadInputEvent::KeyUp { keycode, modifiers }));

            if ::miniquad::window::blocking_event_loop() {
                //::miniquad::window::schedule_update();
            }
        })
    }

    fn update(&mut self) {
        if self.closed {
            return;
        }
        let _z = telemetry::ZoneGuard::new("Event::update");

        // Unless called every frame, cursor will not remain grabbed
        ::miniquad::window::set_cursor_grab(with_context(|context| context.cursor_grabbed));

        #[cfg(not(target_arch = "wasm32"))]
        {
            // TODO: consider making it a part of miniquad?
            std::thread::yield_now();
        }
    }

    fn files_dropped_event(&mut self) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            for i in 0..::miniquad::window::dropped_file_count() {
                context.dropped_files.push(DroppedFile {
                    path: ::miniquad::window::dropped_file_path(i),
                    bytes: ::miniquad::window::dropped_file_bytes(i),
                });
            }
        })
    }

    fn draw(&mut self) {
        if self.closed {
            return;
        }
        {
            let _z = telemetry::ZoneGuard::new("Event::draw");

            use std::panic;

            {
                let _z = telemetry::ZoneGuard::new("Event::draw begin_frame");
                telemetry::begin_gpu_query("GPU");
                ui::ui_context::with(|ui| ui.process_input());
                with_context(|context| context.begin_frame());
            }

            fn maybe_unwind(unwind: bool, f: impl FnOnce() + Sized + panic::UnwindSafe) -> bool {
                if unwind {
                    panic::catch_unwind(f).is_ok()
                } else {
                    f();
                    true
                }
            }

            let mut main_completed = false;
            let result = maybe_unwind(
                with_context(|context| context.unwind),
                AssertUnwindSafe(|| {
                    let _z = telemetry::ZoneGuard::new("Event::draw user code");

                    if exec::resume(&mut self.main_future).is_some() {
                        self.main_future = Box::pin(async move {});
                        main_completed = true;
                        return;
                    }
                    experimental::coroutines::update(crate::time::get_frame_time() as f64);
                }),
            );

            if main_completed {
                self.shutdown(true);
                ::miniquad::window::quit();
                return;
            }
            if result == false {
                if let Some((message, backtrace)) = window::take_panic_message() {
                    let factory = with_context(|context| context.recovery_factory.clone());
                    if let Some(factory) = factory {
                        self.main_future = factory(message, backtrace);
                    }
                }
            }

            {
                let _z = telemetry::ZoneGuard::new("Event::draw end_frame");
                with_context(|context| context.perform_render_passes());
                ui::ui_context::with(|ui| ui.draw());
                with_context(|context| context.end_frame());
                telemetry::end_gpu_query();
            }
            with_context(|context| {
                let now = date::now();
                context.frame_time = now - context.last_frame_time;
                context.last_frame_time = now;
            });
        }

        telemetry::reset();
    }

    fn window_restored_event(&mut self) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            #[cfg(target_os = "android")]
            audio::resume();
            #[cfg(target_os = "android")]
            if ::miniquad::window::blocking_event_loop() {
                ::miniquad::window::schedule_update();
            }

            context
                .input_events
                .iter_mut()
                .for_each(|arr| arr.push(MiniquadInputEvent::WindowRestored));
        })
    }

    fn window_minimized_event(&mut self) {
        if self.closed {
            return;
        }
        crate::with_context(|context| {
            #[cfg(target_os = "android")]
            audio::pause();

            context.touch_mouse.clear();
            context.physical_left_mouse_down = false;
            if context.mouse_down.contains(&MouseButton::Left) {
                let position = context.mouse_position;
                context.record_mouse_up(MouseButton::Left, position.x, position.y);
            }

            // Clear held down keys and button and announce them as released
            context.mouse_released.extend(context.mouse_down.drain());
            context.keys_released.extend(context.keys_down.drain());

            // Announce all touches as released
            for (_, touch) in context.touches.iter_mut() {
                touch.phase = input::TouchPhase::Ended;
            }

            context
                .input_events
                .iter_mut()
                .for_each(|arr| arr.push(MiniquadInputEvent::WindowMinimized));
        })
    }

    fn quit_requested_event(&mut self) {
        if self.closed {
            return;
        }
        let prevented = crate::with_context(|context| {
            if context.prevent_quit_event {
                ::miniquad::window::cancel_quit();
                context.quit_requested = true;
                true
            } else {
                false
            }
        });
        if !prevented {
            self.shutdown(true);
        }
    }
}

pub mod conf {
    #[derive(Default, Debug)]
    pub struct UpdateTrigger {
        pub key_down: bool,
        pub mouse_down: bool,
        pub mouse_up: bool,
        pub mouse_motion: bool,
        pub mouse_wheel: bool,
        pub specific_key: Option<Vec<crate::KeyCode>>,
        pub touch: bool,
    }

    #[derive(Debug)]
    pub struct Conf {
        pub miniquad_conf: ::miniquad::conf::Conf,
        /// With miniquad_conf.platform.blocking_event_loop = true,
        /// next_frame().await will never finish and will wait forever with
        /// zero CPU usage.
        /// update_on will tell macroquad when to proceed with the event loop.
        pub update_on: Option<UpdateTrigger>,
        pub default_filter_mode: crate::FilterMode,
        /// Maximum vertices per draw call. Adjacent compatible geometry batches
        /// until either capacity is full. The effective limit is 65,535 on WebGL2
        /// (index 65,535 is reserved for primitive restart), otherwise 65,536.
        /// Larger configured values use that limit. Defaults to 10,000.
        ///
        /// Individual meshes must fit both capacities and have valid local indices.
        /// With `debug_assertions` (the default dev profile, including a plain
        /// `cargo build` for WASM, where a panic stops the module) oversized or
        /// invalid indexed geometry panics. Otherwise the whole call is rejected
        /// and a warning naming the submitted counts and effective capacities is
        /// logged once per distinct reason; no partial geometry is submitted.
        /// Empty index slices do not submit geometry. Zero and small capacities are
        /// allowed. Default-material 2D shapes (not textures or text) split into
        /// ordered triangles when needed (at least 3/3); custom materials and
        /// public meshes must fit. Splitting can produce multiple draw/capture
        /// entries; custom material index/primitive and screen-texture feedback
        /// semantics stay atomic. Built-in UI splits its private triangle lists to
        /// fit; nonempty UI requires at least 3 vertices and 3 indices and is
        /// rejected under the same policy, before any of it is submitted.
        pub draw_call_vertex_capacity: usize,
        /// Maximum number of indices per draw call, independently of u16 index
        /// values. Defaults to 5,000. Must fit the backend's signed draw count and
        /// buffer byte size; invalid capacities panic before creating GPU resources.
        pub draw_call_index_capacity: usize,
    }

    impl Default for Conf {
        fn default() -> Self {
            Self {
                miniquad_conf: ::miniquad::conf::Conf::default(),
                update_on: Some(UpdateTrigger::default()),
                default_filter_mode: crate::FilterMode::Linear,
                draw_call_vertex_capacity: 10000,
                draw_call_index_capacity: 5000,
            }
        }
    }
}

impl From<::miniquad::conf::Conf> for conf::Conf {
    fn from(conf: ::miniquad::conf::Conf) -> conf::Conf {
        conf::Conf {
            miniquad_conf: conf,
            update_on: None,
            default_filter_mode: crate::FilterMode::Linear,
            draw_call_vertex_capacity: 10000,
            draw_call_index_capacity: 5000,
        }
    }
}

/// Not meant to be used directly, only from the macro.
#[doc(hidden)]
pub struct Window {}

impl Window {
    pub fn new(label: &str, future: impl Future<Output = ()> + 'static) {
        Window::from_config(
            conf::Conf {
                miniquad_conf: ::miniquad::conf::Conf {
                    window_title: label.to_string(),
                    ..Default::default()
                },
                ..Default::default()
            },
            future,
        );
    }

    pub fn from_config(config: impl Into<conf::Conf>, future: impl Future<Output = ()> + 'static) {
        assert!(
            cfg!(any(target_os = "linux", target_arch = "wasm32")),
            "This fork supports Linux/OpenGL and browser WASM/WebGL"
        );
        static WINDOW_STARTED: ownership::OnceEntry = ownership::OnceEntry::new();
        assert!(
            WINDOW_STARTED.claim(),
            "Macroquad permits one window initialization attempt per process"
        );

        let conf::Conf {
            miniquad_conf,
            update_on,
            default_filter_mode,
            draw_call_vertex_capacity,
            draw_call_index_capacity,
        } = config.into();
        #[cfg(target_os = "linux")]
        assert_eq!(
            miniquad_conf.platform.linux_backend,
            ::miniquad::conf::LinuxBackend::X11Only,
            "This fork supports X11; Wayland backends are not audited"
        );
        ::miniquad::start(miniquad_conf, move || {
            let context = Context::new(
                update_on.unwrap_or_default(),
                default_filter_mode,
                draw_call_vertex_capacity,
                draw_call_index_capacity,
            );
            GRAPHICS.with(|slot| *slot.borrow_mut() = Some(context.gpu.clone()));
            ui::ui_context::init(
                context.gpu.clone(),
                context.screen_width,
                context.screen_height,
            );
            CONTEXT
                .with(|slot| *slot.borrow_mut() = Some(Rc::new(ownership::Scoped::new(context))));
            audio::init_sound();

            Box::new(Stage {
                closed: false,
                main_future: Box::pin(async {
                    future.await;
                    with_context(|ctx| ctx.gl.reset());
                }),
            })
        });
    }
}

/// Information about a dropped file.
pub struct DroppedFile {
    pub path: Option<std::path::PathBuf>,
    pub bytes: Option<Vec<u8>>,
}
