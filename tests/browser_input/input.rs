//! Finite browser-controlled input fixture with real Macroquad state/callbacks.
use macroquad::{input::utils, prelude::*};
use std::cell::Cell;
thread_local! { static FINISHED: Cell<bool> = const { Cell::new(false) }; }

#[no_mangle]
pub extern "C" fn input_flags() -> u32 {
    u32::from(is_mouse_button_down(MouseButton::Left))
        | (u32::from(is_mouse_button_down(MouseButton::Right)) << 1)
        | (u32::from(is_key_down(KeyCode::A)) << 2)
        | (u32::from(is_mouse_button_released(MouseButton::Left)) << 3)
        | (u32::from(is_simulating_mouse_with_touch()) << 6)
}
#[no_mangle]
pub extern "C" fn input_x() -> f32 {
    mouse_position().0
}
#[no_mangle]
pub extern "C" fn input_y() -> f32 {
    mouse_position().1
}
#[no_mangle]
pub extern "C" fn input_simulate(enabled: bool) {
    simulate_mouse_with_touch(enabled);
    ::miniquad::window::schedule_update();
}
#[no_mangle]
pub extern "C" fn input_grab(enabled: bool) {
    set_cursor_grab(enabled);
    ::miniquad::window::schedule_update();
}
#[no_mangle]
pub extern "C" fn input_update() {
    ::miniquad::window::schedule_update();
}
#[no_mangle]
pub extern "C" fn input_finish() {
    FINISHED.with(|finished| finished.set(true));
    ::miniquad::window::schedule_update();
}
struct Recorder;
impl macroquad::miniquad::EventHandler for Recorder {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn mouse_button_down_event(&mut self, btn: MouseButton, x: f32, y: f32) {
        info!("INPUT_EVENT down {} {} {}", btn as u32, x, y);
    }
    fn mouse_button_up_event(&mut self, btn: MouseButton, x: f32, y: f32) {
        info!("INPUT_EVENT up {} {} {}", btn as u32, x, y);
    }
    fn touch_event(&mut self, phase: macroquad::miniquad::TouchPhase, id: u64, _: f32, _: f32) {
        info!("INPUT_TOUCH {:?} {}", phase, id);
    }
    fn window_minimized_event(&mut self) {
        info!("INPUT_FOCUS false");
    }
    fn window_restored_event(&mut self) {
        info!("INPUT_FOCUS true");
    }
}
fn conf() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "Owned input regression".into(),
            window_width: 160,
            window_height: 120,
            platform: macroquad::miniquad::conf::Platform {
                webgl_version: if cfg!(feature = "webgl2") {
                    macroquad::miniquad::conf::WebGLVersion::WebGL2
                } else {
                    macroquad::miniquad::conf::WebGLVersion::WebGL1
                },
                blocking_event_loop: true,
                ..Default::default()
            },
            ..Default::default()
        },
        update_on: Some(macroquad::conf::UpdateTrigger {
            mouse_up: !cfg!(feature = "touch-wake"),
            touch: cfg!(feature = "touch-wake"),
            ..Default::default()
        }),
        ..Default::default()
    }
}
#[macroquad::main(conf)]
async fn main() {
    let subscriber = utils::register_input_subscriber();
    info!("INPUT_READY");
    let mut frame = 0;
    loop {
        utils::repeat_all_miniquad_input(&mut Recorder, subscriber);
        let (x, y) = mouse_position();
        info!("INPUT_FRAME {} {} {} {}", frame, input_flags(), x, y);
        if FINISHED.with(Cell::get) {
            break;
        }
        frame += 1;
        next_frame().await;
    }
    info!("browser input regression: passed");
}
