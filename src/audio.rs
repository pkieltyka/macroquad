//! Loading and playing sounds on the window thread.
//!
//! Sounds retain their originating audio owner. Final-handle destruction and
//! pending-load cancellation release that owner's source without borrowing the
//! graphics context or coroutine scheduler.

use crate::{file::load_file, Error};
use std::{cell::RefCell, rc::Rc};

#[cfg(feature = "audio")]
use quad_snd::AudioContext as QuadSndContext;
#[cfg(feature = "audio")]
pub use quad_snd::PlaySoundParams;
#[cfg(all(feature = "audio", not(target_arch = "wasm32")))]
use quad_snd::Sound as QuadSndSound;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Native audio is ready synchronously; WASM uses all states.
enum LoadState {
    Pending,
    Ready,
    Failed,
    Removed,
    Invalid,
}

#[cfg(all(feature = "audio", target_arch = "wasm32"))]
mod wasm_sound {
    use super::{LoadState, PlaySoundParams, QuadSndContext};

    // The paired runtime provides audio_source_state extension version 1 while
    // retaining quad-snd's existing macroquad_audio plugin ABI version 1.
    #[link(wasm_import_module = "env")]
    extern "C" {
        fn audio_add_buffer(content: *const u8, content_len: u32) -> u32;
        fn audio_source_state(source: u32) -> i32;
        fn audio_play_buffer(source: u32, volume: f32, repeat: bool) -> u32;
        fn audio_source_set_volume(source: u32, volume: f32);
        fn audio_source_stop(source: u32);
        fn audio_source_delete(source: u32);
    }

    pub(super) struct Sound(u32);
    impl Sound {
        pub fn load(_: &mut QuadSndContext, data: &[u8]) -> Self {
            let length = u32::try_from(data.len()).expect("Audio data exceeds WASM import length");
            // JS copies the complete input before returning, and returns an
            // owned source ID. No Rust borrow crosses its asynchronous decode.
            Self(unsafe { audio_add_buffer(data.as_ptr(), length) })
        }
        pub fn state(&self) -> LoadState {
            match unsafe { audio_source_state(self.0) } {
                0 => LoadState::Pending,
                1 => LoadState::Ready,
                -1 => LoadState::Failed,
                -2 => LoadState::Removed,
                _ => LoadState::Invalid,
            }
        }
        pub fn play(&self, _: &mut QuadSndContext, params: PlaySoundParams) {
            unsafe {
                audio_play_buffer(self.0, params.volume, params.looped);
            }
        }
        pub fn stop(&self, _: &mut QuadSndContext) {
            unsafe {
                audio_source_stop(self.0);
            }
        }
        pub fn set_volume(&self, _: &mut QuadSndContext, volume: f32) {
            unsafe {
                audio_source_set_volume(self.0, volume);
            }
        }
        pub fn delete(&self, _: &QuadSndContext) {
            unsafe {
                audio_source_delete(self.0);
            }
        }
    }
}
#[cfg(all(feature = "audio", target_arch = "wasm32"))]
use wasm_sound::Sound as QuadSndSound;

#[cfg(not(feature = "audio"))]
mod dummy_audio {
    use super::{LoadState, PlaySoundParams};
    #[cfg(test)]
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    #[cfg(test)]
    thread_local! {
        pub(super) static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
        pub(super) static NEXT_STATE: Cell<LoadState> = const { Cell::new(LoadState::Ready) };
        pub(super) static LAST_STATE: RefCell<Option<Rc<Cell<LoadState>>>> = const { RefCell::new(None) };
        static NEXT_CONTEXT: Cell<u64> = const { Cell::new(0) };
    }
    fn event(_name: &'static str) {
        #[cfg(test)]
        let _ = EVENTS.try_with(|events| events.borrow_mut().push(_name));
    }

    pub struct AudioContext {
        #[cfg(test)]
        id: u64,
    }
    impl AudioContext {
        pub fn new() -> Self {
            Self {
                #[cfg(test)]
                id: NEXT_CONTEXT.with(|next| {
                    let id = next.get();
                    next.set(id + 1);
                    id
                }),
            }
        }
        #[cfg(target_os = "android")]
        pub fn pause(&mut self) {}
        #[cfg(target_os = "android")]
        pub fn resume(&mut self) {}
    }
    pub struct Sound {
        #[cfg(test)]
        pub(super) state: Rc<Cell<LoadState>>,
        #[cfg(test)]
        origin: u64,
    }
    impl Sound {
        pub fn load(_context: &mut AudioContext, _: &[u8]) -> Self {
            event("load");
            #[cfg(test)]
            let state = Rc::new(Cell::new(NEXT_STATE.with(Cell::get)));
            #[cfg(test)]
            LAST_STATE.with(|last| *last.borrow_mut() = Some(state.clone()));
            Self {
                #[cfg(test)]
                state,
                #[cfg(test)]
                origin: _context.id,
            }
        }
        pub fn play(&self, _: &mut AudioContext, _: PlaySoundParams) {
            event("play");
            eprintln!("warn: macroquad's \"audio\" feature disabled.");
        }
        pub fn stop(&self, _: &mut AudioContext) {
            event("stop");
        }
        pub fn set_volume(&self, _: &mut AudioContext, _: f32) {
            event("volume");
        }
        pub fn state(&self) -> LoadState {
            #[cfg(test)]
            {
                self.state.get()
            }
            #[cfg(not(test))]
            {
                LoadState::Ready
            }
        }
        pub fn delete(&self, _context: &AudioContext) {
            #[cfg(test)]
            assert_eq!(
                self.origin, _context.id,
                "Sound deleted by another audio owner"
            );
            event("delete");
        }
    }
}
#[cfg(not(feature = "audio"))]
use dummy_audio::{AudioContext as QuadSndContext, Sound as QuadSndSound};

#[cfg(not(feature = "audio"))]
pub struct PlaySoundParams {
    pub looped: bool,
    pub volume: f32,
}
#[cfg(not(feature = "audio"))]
impl Default for PlaySoundParams {
    fn default() -> Self {
        Self {
            looped: false,
            volume: 1.,
        }
    }
}

struct AudioOwner {
    context: RefCell<QuadSndContext>,
    pending: RefCell<Vec<QuadSndSound>>,
}
impl AudioOwner {
    fn new() -> Self {
        Self {
            context: RefCell::new(QuadSndContext::new()),
            pending: RefCell::new(Vec::new()),
        }
    }
    fn with_context<R>(&self, f: impl FnOnce(&mut QuadSndContext) -> R) -> R {
        self.collect();
        let result = f(&mut self.context.borrow_mut());
        self.collect();
        result
    }
    fn collect(&self) {
        if let Ok(context) = self.context.try_borrow_mut() {
            let removed = self.pending.take();
            for sound in removed {
                sound.delete(&context);
            }
        }
    }
}
impl Drop for AudioOwner {
    fn drop(&mut self) {
        // Owners cannot be replaced through the public API. Their final drop
        // happens only after every Sound has enqueued its source for deletion.
        for sound in self.pending.get_mut().drain(..) {
            sound.delete(self.context.get_mut());
        }
    }
}

thread_local! { static AUDIO: RefCell<Option<Rc<AudioOwner>>> = const { RefCell::new(None) }; }

pub(crate) fn init_sound() {
    let initialized = AUDIO.with(|audio| audio.borrow().is_some());
    assert!(!initialized, "Audio already initialized on this thread");
    let owner = Rc::new(AudioOwner::new());
    AUDIO.with(|audio| *audio.borrow_mut() = Some(owner));
}
pub(crate) fn shutdown() {
    let owner = AUDIO.with(|audio| audio.borrow_mut().take());
    drop(owner);
}
fn audio_owner() -> Rc<AudioOwner> {
    AUDIO.with(|audio| {
        audio
            .borrow()
            .as_ref()
            .expect("Audio is not initialized on this thread")
            .clone()
    })
}

#[cfg(target_os = "android")]
pub(crate) fn pause() {
    audio_owner().with_context(|ctx| ctx.pause());
}
#[cfg(target_os = "android")]
pub(crate) fn resume() {
    audio_owner().with_context(|ctx| ctx.resume());
}

struct SoundGuard {
    sound: Option<QuadSndSound>,
    owner: Rc<AudioOwner>,
}
impl Drop for SoundGuard {
    fn drop(&mut self) {
        if let Some(sound) = self.sound.take() {
            self.owner.pending.borrow_mut().push(sound);
            self.owner.collect();
        }
    }
}

/// A sound on its originating window thread. Clones share source ownership;
/// dropping the last clone deletes the source, including during a pending load.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<macroquad::audio::Sound>();
/// ```
#[derive(Clone)]
pub struct Sound(Rc<SoundGuard>);
impl std::fmt::Debug for Sound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sound").finish()
    }
}
impl Sound {
    fn raw(&self) -> &QuadSndSound {
        self.0.sound.as_ref().unwrap()
    }
    fn state(&self) -> LoadState {
        #[cfg(all(feature = "audio", not(target_arch = "wasm32")))]
        {
            LoadState::Ready
        }
        #[cfg(any(not(feature = "audio"), target_arch = "wasm32"))]
        {
            self.raw().state()
        }
    }
}

pub async fn load_sound(path: &str) -> Result<Sound, Error> {
    load_sound_from_bytes(&load_file(path).await?).await
}

/// Loads encoded sound data. The paired WASM runtime returns terminal decode
/// failures instead of leaving the future pending forever.
pub async fn load_sound_from_bytes(data: &[u8]) -> Result<Sound, Error> {
    let owner = audio_owner();
    let raw = owner.with_context(|ctx| QuadSndSound::load(ctx, data));
    let sound = Sound(Rc::new(SoundGuard {
        sound: Some(raw),
        owner,
    }));
    wait_until_ready(sound).await
}

async fn wait_until_ready(sound: Sound) -> Result<Sound, Error> {
    loop {
        match sound.state() {
            LoadState::Ready => return Ok(sound),
            LoadState::Pending => crate::exec::FrameFuture::default().await,
            LoadState::Failed => return Err(Error::UnknownError("Audio decoding failed")),
            LoadState::Removed => {
                return Err(Error::UnknownError("Audio source was cancelled or removed"))
            }
            LoadState::Invalid => {
                return Err(Error::UnknownError("Unsupported audio source state"))
            }
        }
    }
}

pub fn play_sound_once(sound: &Sound) {
    play_sound(
        sound,
        PlaySoundParams {
            looped: false,
            volume: 1.,
        },
    );
}
pub fn play_sound(sound: &Sound, params: PlaySoundParams) {
    sound.0.owner.with_context(|ctx| {
        sound.raw().play(ctx, params);
    });
}
pub fn stop_sound(sound: &Sound) {
    sound.0.owner.with_context(|ctx| {
        sound.raw().stop(ctx);
    });
}
pub fn set_sound_volume(sound: &Sound, volume: f32) {
    sound.0.owner.with_context(|ctx| {
        sound.raw().set_volume(ctx, volume);
    });
}

#[cfg(all(test, not(feature = "audio")))]
mod tests {
    use super::*;
    use crate::experimental::coroutines::{start_coroutine, stop_coroutine, update};
    use std::{cell::Cell, future::Future, pin::Pin};

    fn setup(state: LoadState) {
        let owner = Rc::new(AudioOwner::new());
        let previous = AUDIO.with(|audio| audio.replace(Some(owner)));
        drop(previous);
        dummy_audio::EVENTS.with(|events| events.borrow_mut().clear());
        dummy_audio::NEXT_STATE.with(|next| next.set(state));
    }
    fn state() -> Rc<Cell<LoadState>> {
        dummy_audio::LAST_STATE.with(|last| last.borrow().as_ref().unwrap().clone())
    }
    fn events() -> Vec<&'static str> {
        dummy_audio::EVENTS.with(|events| events.borrow().clone())
    }
    fn boxed<T: 'static>(
        future: impl Future<Output = T> + 'static,
    ) -> Pin<Box<dyn Future<Output = T>>> {
        Box::pin(future)
    }

    #[test]
    fn playback_and_last_clone_drop_use_the_originating_owner() {
        setup(LoadState::Ready);
        let sound = crate::exec::resume(&mut boxed(load_sound_from_bytes(&[])))
            .unwrap()
            .unwrap();
        let clone = sound.clone();
        drop(sound);
        assert_eq!(events(), vec!["load"]);
        play_sound_once(&clone);
        set_sound_volume(&clone, 0.5);
        stop_sound(&clone);
        // A different current owner must not receive the old source's deletion.
        let other = Rc::new(AudioOwner::new());
        let previous = AUDIO.with(|audio| audio.replace(Some(other)));
        drop(previous);
        drop(clone);
        assert_eq!(events(), vec!["load", "play", "volume", "stop", "delete"]);
    }

    #[test]
    fn final_handle_drop_during_an_audio_borrow_is_deferred() {
        setup(LoadState::Ready);
        let sound = crate::exec::resume(&mut boxed(load_sound_from_bytes(&[])))
            .unwrap()
            .unwrap();
        let owner = sound.0.owner.clone();
        owner.with_context(|_| {
            drop(sound);
            assert_eq!(events(), vec!["load"]);
            assert_eq!(owner.pending.borrow().len(), 1);
        });
        assert_eq!(events(), vec!["load", "delete"]);
        assert!(owner.pending.borrow().is_empty());
    }

    #[test]
    fn pending_load_retains_its_owner_until_ready() {
        setup(LoadState::Pending);
        let mut future = boxed(load_sound_from_bytes(&[]));
        assert!(crate::exec::resume(&mut future).is_none());
        assert_eq!(events(), vec!["load"]);
        state().set(LoadState::Ready);
        let sound = crate::exec::resume(&mut future).unwrap().unwrap();
        drop(future);
        assert_eq!(events(), vec!["load"]);
        drop(sound);
        assert_eq!(events(), vec!["load", "delete"]);
    }

    #[test]
    fn shutdown_releases_tls_owner_but_preserves_live_sound_owner() {
        setup(LoadState::Ready);
        let mut future = boxed(load_sound_from_bytes(&[]));
        let sound = crate::exec::resume(&mut future).unwrap().unwrap();
        drop(future);
        shutdown();
        assert!(AUDIO.with(|slot| slot.borrow().is_none()));
        assert_eq!(events(), vec!["load"]);
        stop_sound(&sound);
        drop(sound);
        assert_eq!(events(), vec!["load", "stop", "delete"]);
    }

    #[test]
    fn cancellation_releases_the_pending_backend_source_once() {
        setup(LoadState::Pending);
        let mut future = boxed(load_sound_from_bytes(&[]));
        assert!(crate::exec::resume(&mut future).is_none());
        drop(future);
        assert_eq!(events(), vec!["load", "delete"]);
    }

    #[test]
    fn terminal_load_failures_return_errors_and_release_the_source() {
        for terminal in [LoadState::Failed, LoadState::Removed, LoadState::Invalid] {
            setup(LoadState::Pending);
            let mut future = boxed(load_sound_from_bytes(&[]));
            assert!(crate::exec::resume(&mut future).is_none());
            state().set(terminal);
            assert!(crate::exec::resume(&mut future).unwrap().is_err());
            drop(future);
            assert_eq!(events(), vec!["load", "delete"]);
        }
    }

    #[test]
    fn cancelling_an_audio_coroutine_releases_its_pending_source() {
        setup(LoadState::Pending);
        let task = start_coroutine(async { load_sound_from_bytes(&[]).await });
        update(0.01);
        assert_eq!(events(), vec!["load"]);
        assert!(task.retrieve().is_none());
        stop_coroutine(task);
        assert!(task.is_done());
        assert!(task.retrieve().is_none());
        assert_eq!(events(), vec!["load", "delete"]);
    }

    #[test]
    fn cancelling_a_completed_audio_result_releases_its_source() {
        setup(LoadState::Ready);
        let task = start_coroutine(async { load_sound_from_bytes(&[]).await });
        update(0.01);
        assert!(task.is_done());
        assert_eq!(events(), vec!["load"]);
        stop_coroutine(task);
        assert_eq!(events(), vec!["load", "delete"]);
    }

    #[test]
    fn reinitialization_is_rejected_before_replacing_live_audio() {
        setup(LoadState::Ready);
        let owner = audio_owner();
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(init_sound)).is_err());
        assert!(Rc::ptr_eq(&owner, &audio_owner()));
    }

    #[test]
    fn sound_owner_outlives_audio_tls_during_scheduler_shutdown() {
        struct CapturedSound {
            sound: Option<Sound>,
            owner: std::rc::Weak<AudioOwner>,
            sender: std::sync::mpsc::Sender<bool>,
        }
        impl CapturedSound {
            fn keep_alive(&self) {}
        }
        impl Drop for CapturedSound {
            fn drop(&mut self) {
                drop(self.sound.take());
                self.sender.send(self.owner.upgrade().is_none()).unwrap();
            }
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // Initialize the scheduler first: its future drops after audio TLS.
            start_coroutine(async {});
            setup(LoadState::Ready);
            let sound = crate::exec::resume(&mut boxed(load_sound_from_bytes(&[])))
                .unwrap()
                .unwrap();
            let captured = CapturedSound {
                owner: Rc::downgrade(&sound.0.owner),
                sound: Some(sound),
                sender,
            };
            start_coroutine(async move {
                captured.keep_alive();
                std::future::pending::<()>().await;
            });
        })
        .join()
        .unwrap();
        assert!(
            receiver.recv().unwrap(),
            "The last sound must release its original owner after TLS teardown"
        );
    }
}
