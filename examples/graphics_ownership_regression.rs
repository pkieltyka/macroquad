//! Finite native/browser ownership regression; no external assets.
use macroquad::{prelude::*, telemetry, ui::root_ui};

#[derive(Default)]
struct InputCallback {
    touches: usize,
    mouse: usize,
}
impl macroquad::miniquad::EventHandler for InputCallback {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn touch_event(&mut self, _: macroquad::miniquad::TouchPhase, _: u64, _: f32, _: f32) {
        let _ = screen_width();
        // Growing the subscriber store must not invalidate the events currently
        // dispatched to this callback.
        let _ = macroquad::input::utils::register_input_subscriber();
        let task = macroquad::experimental::coroutines::start_coroutine(async {});
        macroquad::experimental::coroutines::stop_coroutine(task);
        self.touches += 1;
    }
    fn mouse_button_down_event(&mut self, _: MouseButton, _: f32, _: f32) {
        draw_rectangle(0., 0., 1., 1., WHITE);
        self.mouse += 1;
    }
}

struct CallbackCamera;
impl Camera for CallbackCamera {
    fn matrix(&self) -> Mat4 {
        // Camera trait callbacks execute outside the graphics scope.
        let _ = mouse_position();
        Camera2D::from_display_rect(Rect::new(0., 0., screen_width(), screen_height())).matrix()
    }
    fn depth_enabled(&self) -> bool {
        let _ = get_frame_time();
        false
    }
    fn render_pass(&self) -> Option<RenderPass> {
        None
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}

#[repr(transparent)]
struct ReentrantUniform(f32);
// SAFETY: repr(transparent) f32 has an initialized padding-free representation.
unsafe impl macroquad::material::ToBytes for ReentrantUniform {
    fn to_bytes(&self) -> &[u8] {
        let _ = screen_width();
        macroquad::material::ToBytes::to_bytes(&self.0)
    }
}
impl Drop for ReentrantUniform {
    fn drop(&mut self) {
        let _ = get_frame_time();
    }
}

const VERTEX: &str = "#version 100\nattribute vec3 position;attribute vec2 texcoord;attribute vec4 color0;uniform mat4 Model;uniform mat4 Projection;varying lowp vec4 color;void main(){gl_Position=Projection*Model*vec4(position,1.);color=color0/255.;}";
const FRAGMENT: &str = "#version 100\nprecision lowp float;varying lowp vec4 color;uniform float Value;void main(){gl_FragColor=color*Value;}";

#[macroquad::main("Graphics ownership regression")]
async fn main() {
    clear_background(BLACK);
    next_frame().await;
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(|| {
        assert!(std::panic::catch_unwind(|| screen_width()).is_err());
        assert!(std::panic::catch_unwind(|| macroquad::Window::new(
            "Rejected second window",
            async {}
        ))
        .is_err());
    })
    .join()
    .unwrap();
    let subscriber = macroquad::input::utils::register_input_subscriber();
    let mut input = InputCallback::default();
    info!("graphics ownership regression: callbacks ready");
    #[cfg(target_arch = "wasm32")]
    {
        for _ in 0..120 {
            macroquad::input::utils::repeat_all_miniquad_input(&mut input, subscriber);
            if input.touches > 0 && input.mouse > 0 {
                break;
            }
            next_frame().await;
        }
        assert!(
            input.touches > 0 && input.mouse > 0,
            "Browser must dispatch genuine touch/mouse handlers"
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    macroquad::input::utils::repeat_all_miniquad_input(&mut input, subscriber);
    let baseline = telemetry::textures_count();
    info!("graphics ownership regression: baseline");

    // Independent UI ownership permits graphics calls while UI is active.
    root_ui(|ui| {
        ui.label(None, "Scoped UI");
        draw_rectangle(2., 2., 4., 4., WHITE);
        let _ = screen_width();
    });
    set_camera(&CallbackCamera);
    draw_cube(vec3(0., 0., 0.), vec3(1., 1., 1.), None, WHITE);
    set_default_camera();
    let material = load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: FRAGMENT,
        },
        MaterialParams {
            uniforms: vec![UniformDesc::new("Value", UniformType::Float1)],
            ..Default::default()
        },
    )
    .unwrap();
    material.set_uniform("Value", ReentrantUniform(1.));
    gl_use_material(&material);
    draw_rectangle(10., 10., 8., 8., WHITE);
    drop(material); // selected/queued draws retain the pipeline until flushed
    gl_use_default_material();
    next_frame().await;

    for _ in 0..12 {
        let target = render_target_ex(
            8,
            8,
            RenderTargetParams {
                depth: true,
                ..Default::default()
            },
        );
        let camera = Camera2D {
            render_target: Some(target.clone()),
            ..Camera2D::from_display_rect(Rect::new(0., 0., 8., 8.))
        };
        set_camera(&camera);
        clear_background(Color::new(1., 0., 0., 1.));
        draw_rectangle(0., 0., 8., 8., Color::new(1., 0., 0., 1.));
        set_default_camera();
        let texture = target.texture.clone();
        drop((camera, target));
        next_frame().await;
        let pixels = texture.get_texture_data();
        assert!(
            pixels
                .bytes()
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255]),
            "Unexpected target pixels: {:?}",
            &pixels.bytes()[..16]
        );
        draw_texture(&texture, 20., 20., WHITE);
        drop(texture);
        next_frame().await;
        assert_eq!(telemetry::textures_count(), baseline);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let target = render_target_ex(
            8,
            8,
            RenderTargetParams {
                depth: true,
                sample_count: 4,
            },
        );
        let camera = Camera2D {
            render_target: Some(target.clone()),
            ..Camera2D::from_display_rect(Rect::new(0., 0., 8., 8.))
        };
        set_camera(&camera);
        clear_background(Color::new(0., 0., 1., 1.));
        set_default_camera();
        let color = target.texture.clone();
        drop((target, camera));
        next_frame().await;
        assert!(color
            .get_texture_data()
            .bytes()
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 255, 255]));
        drop(color);
        next_frame().await;
        assert_eq!(telemetry::textures_count(), baseline);
    }

    // Discarded draw-call slots must release their old texture leases too.
    let texture = Texture2D::from_rgba8(1, 1, &[0, 255, 0, 255]);
    draw_texture(&texture, 30., 30., WHITE);
    drop(texture);
    clear_background(BLACK);
    draw_rectangle(0., 0., 1., 1., WHITE);
    next_frame().await;
    assert_eq!(telemetry::textures_count(), baseline);

    telemetry::capture_frame();
    next_frame().await;
    draw_rectangle(0., 0., 4., 4., WHITE);
    next_frame().await;
    let original = telemetry::drawcalls();
    assert!(!original.is_empty());
    let retained = original.clone();
    drop(original);
    telemetry::capture_frame();
    next_frame().await;
    draw_rectangle(0., 0., 4., 4., RED);
    next_frame().await;
    assert!(!retained.is_empty());
    drop(retained);
    telemetry::capture_frame();
    next_frame().await;
    next_frame().await;
    next_frame().await;
    assert_eq!(telemetry::textures_count(), baseline);
    info!("graphics ownership regression: passed");
}
