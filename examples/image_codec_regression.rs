//! Finite native/WASM check of the supported CPU image codecs.
#[path = "../tests/image_codecs.rs"]
mod image_codecs;

#[macroquad::main("Image codec regression")]
async fn main() {
    image_codecs::verify_codec_boundaries();
    macroquad::logging::info!("image codec regression: passed PNG/TGA decode and malformed inputs");
}
