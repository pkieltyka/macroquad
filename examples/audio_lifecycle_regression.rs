//! Finite WASM/audio regression. Requires the paired runtime's audio state ABI.
#[cfg(all(feature = "audio", target_arch = "wasm32"))]
use macroquad::prelude::*;

#[cfg(all(feature = "audio", target_arch = "wasm32"))]
async fn check_audio() {
    use macroquad::audio::{load_sound_from_bytes, play_sound_once, set_sound_volume, stop_sound};
    use macroquad::experimental::coroutines::{start_coroutine, stop_coroutine};
    const WAV: &[u8] = include_bytes!("sound.wav");

    let failed = start_coroutine(async { load_sound_from_bytes(b"invalid audio").await });
    loop {
        if let Some(result) = failed.retrieve() {
            assert!(result.is_err());
            break;
        }
        next_frame().await;
    }

    let mut cancelled = start_coroutine(async { load_sound_from_bytes(WAV).await });
    cancelled.set_manual_poll();
    cancelled.poll(0.);
    assert!(
        !cancelled.is_done(),
        "WebAudio decode should be asynchronous"
    );
    stop_coroutine(cancelled);
    assert!(cancelled.is_done());
    assert!(cancelled.retrieve().is_none());

    let original = load_sound_from_bytes(WAV).await.unwrap();
    let clone = original.clone();
    drop(original);
    play_sound_once(&clone);
    set_sound_volume(&clone, 0.);
    stop_sound(&clone);
    drop(clone);

    let completed = start_coroutine(async { load_sound_from_bytes(WAV).await });
    while !completed.is_done() {
        next_frame().await;
    }
    stop_coroutine(completed);
    assert!(completed.retrieve().is_none());

    // The browser fixture observes a further callback grace period after this
    // marker, independent of background-tab animation-frame throttling.
    info!("audio lifecycle regression: passed");
}

#[macroquad::main("Audio lifecycle regression")]
async fn main() {
    #[cfg(all(feature = "audio", target_arch = "wasm32"))]
    check_audio().await;
    #[cfg(not(all(feature = "audio", target_arch = "wasm32")))]
    println!("Audio lifecycle browser regression requires WASM with the audio feature");
}
