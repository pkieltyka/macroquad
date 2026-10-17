//! Cross-platform mouse, keyboard (and gamepads soon) module.

use std::collections::HashSet;

use crate::prelude::screen_height;
use crate::prelude::screen_width;
use crate::DroppedFile;
use crate::Vec2;
pub use miniquad::{KeyCode, MouseButton};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchPhase {
    Started,
    Stationary,
    Moved,
    Ended,
    Cancelled,
}

impl From<miniquad::TouchPhase> for TouchPhase {
    fn from(miniquad_phase: miniquad::TouchPhase) -> TouchPhase {
        match miniquad_phase {
            miniquad::TouchPhase::Started => TouchPhase::Started,
            miniquad::TouchPhase::Moved => TouchPhase::Moved,
            miniquad::TouchPhase::Ended => TouchPhase::Ended,
            miniquad::TouchPhase::Cancelled => TouchPhase::Cancelled,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Touch {
    pub id: u64,
    pub phase: TouchPhase,
    pub position: Vec2,
}

// Keep the legacy all-starts/all-ends mouse simulation, without selecting a
// primary finger. Cancellation ends a held simulation only after its last owner.
#[derive(Default)]
pub(crate) struct TouchMouseSimulation {
    contacts: HashSet<u64>,
    held: bool,
}
impl TouchMouseSimulation {
    pub(crate) fn held(&self) -> bool {
        self.held
    }
    pub(crate) fn clear(&mut self) -> bool {
        self.contacts.clear();
        std::mem::take(&mut self.held)
    }
    pub(crate) fn event(
        &mut self,
        phase: miniquad::TouchPhase,
        id: u64,
    ) -> Option<miniquad::TouchPhase> {
        use miniquad::TouchPhase::*;
        match phase {
            Started => {
                self.contacts.insert(id);
                self.held = true;
                Some(Started)
            }
            Ended if self.contacts.remove(&id) => {
                self.held = false;
                Some(Ended)
            }
            Cancelled if self.contacts.remove(&id) && self.contacts.is_empty() && self.held => {
                self.held = false;
                Some(Ended)
            }
            _ => None,
        }
    }
}

/// Constrain mouse to window
pub fn set_cursor_grab(grab: bool) {
    crate::with_context(|context| {
        context.cursor_grabbed = grab;
        miniquad::window::set_cursor_grab(grab);
    })
}

/// Set mouse cursor visibility
pub fn show_mouse(shown: bool) {
    crate::thread_assert::same_thread();
    miniquad::window::show_mouse(shown);
}

/// Return mouse position in pixels.
pub fn mouse_position() -> (f32, f32) {
    crate::with_context(|context| {
        (
            context.mouse_position.x / miniquad::window::dpi_scale(),
            context.mouse_position.y / miniquad::window::dpi_scale(),
        )
    })
}

/// Return mouse position in range [-1; 1].
pub fn mouse_position_local() -> Vec2 {
    let (pixels_x, pixels_y) = mouse_position();

    convert_to_local(Vec2::new(pixels_x, pixels_y))
}

/// Returns the difference between the current mouse position and the mouse position on the previous frame.
pub fn mouse_delta_position() -> Vec2 {
    let current_position = mouse_position_local();
    crate::with_context(|context| {
        let last_position = context.last_mouse_position.unwrap_or(current_position);

        // Calculate the delta
        last_position - current_position
    })
}

/// This is set to true by default, meaning touches will raise mouse events in addition to raising touch events.
/// If set to false, touches won't affect mouse events.
pub fn is_simulating_mouse_with_touch() -> bool {
    crate::with_context(|context| context.simulate_mouse_with_touch)
}

/// This is set to true by default, meaning touches will raise mouse events in addition to raising touch events.
/// If set to false, touches won't affect mouse events. Disabling simulation
/// terminates a simulated hold, preserving a simultaneously held physical Left.
/// Touch starts/ends retain their legacy mouse mapping; cancellation releases
/// only after the last simulated contact ends. No primary finger is selected.
pub fn simulate_mouse_with_touch(option: bool) {
    crate::with_context(|context| {
        if !option && context.touch_mouse.clear() && !context.physical_left_mouse_down {
            let position = context.mouse_position;
            context.record_mouse_up(MouseButton::Left, position.x, position.y);
        }
        context.simulate_mouse_with_touch = option;
    });
}

/// Return touches with positions in pixels.
pub fn touches() -> Vec<Touch> {
    crate::with_context(|context| context.touches.values().cloned().collect())
}

/// Return touches with positions in range [-1; 1].
pub fn touches_local() -> Vec<Touch> {
    touches()
        .into_iter()
        .map(|mut touch| {
            touch.position = convert_to_local(touch.position);
            touch
        })
        .collect()
}

pub fn mouse_wheel() -> (f32, f32) {
    crate::with_context(|context| (context.mouse_wheel.x, context.mouse_wheel.y))
}

/// Detect if the key has been pressed once
pub fn is_key_pressed(key_code: KeyCode) -> bool {
    crate::with_context(|context| context.keys_pressed.contains(&key_code))
}

/// Detect if the key is being pressed
pub fn is_key_down(key_code: KeyCode) -> bool {
    crate::with_context(|context| context.keys_down.contains(&key_code))
}

/// Detect if the key has been released this frame
pub fn is_key_released(key_code: KeyCode) -> bool {
    crate::with_context(|context| context.keys_released.contains(&key_code))
}

/// Detect if any key is being pressed
pub fn is_any_key_down() -> bool {
    crate::with_context(|context| context.keys_down.len() > 0)
}

/// Return the last pressed char.
/// Each "get_char_pressed" call will consume a character from the input queue.
pub fn get_char_pressed() -> Option<char> {
    crate::with_context(|context| context.chars_pressed_queue.pop_front())
}

pub(crate) fn get_char_pressed_ui() -> Option<char> {
    crate::with_context(|context| context.chars_pressed_ui_queue.pop_front())
}

/// Return the last pressed key.
pub fn get_last_key_pressed() -> Option<KeyCode> {
    crate::with_context(|context| {
        // TODO: this will return a random key from keys_pressed HashMap instead of the last one, fix me later
        context.keys_pressed.iter().next().cloned()
    })
}

pub fn get_keys_pressed() -> HashSet<KeyCode> {
    crate::with_context(|context| context.keys_pressed.clone())
}

pub fn get_keys_down() -> HashSet<KeyCode> {
    crate::with_context(|context| context.keys_down.clone())
}

pub fn get_keys_released() -> HashSet<KeyCode> {
    crate::with_context(|context| context.keys_released.clone())
}

/// Clears input queue
pub fn clear_input_queue() {
    crate::with_context(|context| {
        context.chars_pressed_queue.clear();
        context.chars_pressed_ui_queue.clear();
        context.mouse_pressed.clear();
        context.keys_pressed.clear();
    })
}

/// Detect if the button is being pressed
pub fn is_mouse_button_down(btn: MouseButton) -> bool {
    crate::with_context(|context| context.mouse_down.contains(&btn))
}

/// Detect if the button has been pressed once
pub fn is_mouse_button_pressed(btn: MouseButton) -> bool {
    crate::with_context(|context| context.mouse_pressed.contains(&btn))
}

/// Detect if the button has been released this frame
pub fn is_mouse_button_released(btn: MouseButton) -> bool {
    crate::with_context(|context| context.mouse_released.contains(&btn))
}

/// Convert a position in pixels to a position in the range [-1; 1].
fn convert_to_local(pixel_pos: Vec2) -> Vec2 {
    Vec2::new(pixel_pos.x / screen_width(), pixel_pos.y / screen_height()) * 2.0
        - Vec2::new(1.0, 1.0)
}

/// Prevents quit
pub fn prevent_quit() {
    crate::with_context(|context| context.prevent_quit_event = true);
}

/// Detect if quit has been requested
pub fn is_quit_requested() -> bool {
    crate::with_context(|context| context.quit_requested)
}

/// Gets the files which have been dropped on the window.
pub fn get_dropped_files() -> Vec<DroppedFile> {
    crate::with_context(|context| context.dropped_files())
}

/// Functions for advanced input processing.
///
/// Functions in this module should be used by external tools that uses miniquad system, like different UI libraries. User shouldn't use this function.
pub mod utils {

    /// Register input subscriber. Returns subscriber identifier that must be used in `repeat_all_miniquad_input`.
    pub fn register_input_subscriber() -> usize {
        crate::with_context(|context| {
            context.input_events.push(vec![]);

            context.input_events.len() - 1
        })
    }

    /// Repeats all events that came since last call of this function with current value of `subscriber`. This function must be called at each frame.
    pub fn repeat_all_miniquad_input<T: miniquad::EventHandler>(t: &mut T, subscriber: usize) {
        let events =
            crate::with_context(|context| std::mem::take(&mut context.input_events[subscriber]));
        for event in events {
            event.repeat(t);
        }
    }
}

#[cfg(test)]
mod touch_mouse_tests {
    use super::TouchMouseSimulation;
    use miniquad::TouchPhase::*;

    #[test]
    fn cancellation_terminates_once_without_choosing_a_primary_contact() {
        for order in [[7, 8], [8, 7]] {
            let mut simulation = TouchMouseSimulation::default();
            assert_eq!(simulation.event(Started, 7), Some(Started));
            assert_eq!(simulation.event(Started, 8), Some(Started));
            assert_eq!(simulation.event(Cancelled, 99), None);
            assert_eq!(simulation.event(Cancelled, order[0]), None);
            assert!(simulation.held());
            assert_eq!(simulation.event(Cancelled, order[1]), Some(Ended));
            assert!(!simulation.held());
            assert_eq!(simulation.event(Cancelled, order[1]), None);
            assert_eq!(simulation.event(Ended, order[1]), None);
        }
    }

    #[test]
    fn genuine_ends_retain_legacy_release_with_survivors() {
        let mut simulation = TouchMouseSimulation::default();
        simulation.event(Started, 1);
        simulation.event(Started, 2);
        assert_eq!(simulation.event(Ended, 1), Some(Ended));
        assert!(!simulation.held());
        assert_eq!(simulation.event(Cancelled, 2), None);
        assert_eq!(simulation.event(Ended, 1), None);
        simulation.event(Started, 3);
        simulation.event(Started, 4);
        assert_eq!(simulation.event(Ended, 3), Some(Ended));
        assert_eq!(simulation.event(Ended, 4), Some(Ended));
    }

    #[test]
    fn clearing_ownership_does_not_reanimate_old_contacts() {
        let mut simulation = TouchMouseSimulation::default();
        simulation.event(Started, 1);
        assert!(simulation.clear());
        assert!(!simulation.clear());
        simulation.event(Started, 2);
        assert_eq!(simulation.event(Cancelled, 1), None);
        assert!(simulation.held());
        assert_eq!(simulation.event(Cancelled, 2), Some(Ended));
    }
}
