//! Finite native/WASM regression for UI rendering, uniform uploads, and material reset.
use macroquad::{prelude::*, ui::root_ui};

const VERTEX: &str = r#"#version 100
attribute vec3 position;
uniform mat4 Model;
uniform mat4 Projection;
void main() { gl_Position = Projection * Model * vec4(position, 1.0); }
"#;
const FRAGMENT: &str = r#"#version 100
precision mediump float;
uniform vec4 tint;
uniform vec4 colors[2];
void main() { gl_FragColor = tint * colors[1]; }
"#;

fn assert_center(expected: [u8; 4]) {
    let image = get_screen_data();
    let x = image.width() as usize / 2;
    let y = image.height() as usize / 2;
    let offset = (y * image.width() as usize + x) * 4;
    assert_eq!(&image.bytes()[offset..offset + 4], &expected);
}

fn solid_rect() {
    draw_rectangle(
        0.,
        0.,
        screen_width(),
        screen_height(),
        Color::new(1., 0., 0., 1.),
    );
}

#[macroquad::main("UI material regression")]
async fn main() {
    // The browser harness counts shader compilation in this phase.
    for _ in 0..3 {
        clear_background(BLACK);
        solid_rect();
        assert_center([255, 0, 0, 255]);
        next_frame().await;
    }
    info!("regression: empty UI frames passed");

    let material = load_material(
        ShaderSource::Glsl {
            vertex: VERTEX,
            fragment: FRAGMENT,
        },
        MaterialParams {
            uniforms: vec![
                UniformDesc::new("tint", UniformType::Float4),
                UniformDesc::new("colors", UniformType::Float4).array(2),
            ],
            ..Default::default()
        },
    )
    .unwrap();
    material.set_uniform("tint", vec4(0., 1., 0., 1.));
    material.set_uniform_array("colors", &[[0_f32; 4], [1_f32; 4]]);
    gl_use_material(&material);
    solid_rect();
    assert_center([0, 255, 0, 255]);
    // Leave the custom material active: empty UI must still restore the default.
    next_frame().await;
    clear_background(BLACK);
    solid_rect();
    assert_center([255, 0, 0, 255]);
    info!("regression: uniforms and empty UI reset passed");

    let ui_texture = Texture2D::from_rgba8(1, 1, &[0, 0, 255, 255]);
    for _ in 0..2 {
        clear_background(BLACK);
        root_ui(|ui| ui.label(Some(vec2(10., 10.)), "UI material regression"));
        root_ui(|ui| ui.button(Some(vec2(10., 40.)), "button"));
        root_ui(|ui| {
            ui.window(1, vec2(10., 100.), vec2(160., 100.), |ui| {
                ui.label(None, "window label");
            })
        });
        root_ui(|ui| {
            ui.canvas()
                .image(Rect::new(200., 10., 32., 32.), &ui_texture)
        });
        // Filled UI must restore the default after drawing with a custom material.
        gl_use_material(&material);
        next_frame().await;
        solid_rect();
        assert_center([255, 0, 0, 255]);
    }
    info!("regression: filled UI reset passed");
    next_frame().await;
    info!("UI material regression passed");
}
