//! Finite GPU regression for the fork's supported RGBA8 transfer paths.
use macroquad::prelude::*;

#[macroquad::main("Image transfer regression")]
async fn main() {
    let source = Image::from_rgba8(2, 2, (0..16).collect()).unwrap();
    let texture = Texture2D::from_image(&source);
    assert_eq!(texture.get_texture_data().bytes(), source.bytes());

    let replacement = Image::from_rgba8(2, 2, vec![80; 16]).unwrap();
    texture.update(&replacement);
    assert_eq!(texture.get_texture_data().bytes(), replacement.bytes());

    let part = Image::from_rgba8(1, 1, vec![10, 20, 30, 40]).unwrap();
    texture.update_part(&part, 1, 1, 1, 1);
    let mut expected = vec![80; 16];
    expected[12..].copy_from_slice(part.bytes());
    assert_eq!(texture.get_texture_data().bytes(), expected);

    texture.update_from_bytes(2, 2, source.bytes());
    assert_eq!(texture.get_texture_data().bytes(), source.bytes());

    let target = render_target(4, 4);
    set_camera(&Camera2D {
        render_target: Some(target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0., 0., 4., 4.))
    });
    clear_background(Color::from_rgba(255, 0, 0, 255));
    set_default_camera();
    next_frame().await;
    assert!(target
        .texture
        .get_texture_data()
        .get_image_data()
        .iter()
        .all(|pixel| *pixel == [255, 0, 0, 255]));
    println!("image transfer regression: passed");
}
