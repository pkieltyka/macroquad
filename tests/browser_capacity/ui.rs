//! End-frame UI must remain in screen space under retained world cameras.
use macroquad::{
    prelude::*,
    ui::{root_ui, widgets},
};

fn conf() -> macroquad::conf::Conf {
    let (vertices, indices) = if cfg!(feature = "ui-tiny") {
        (3, 3)
    } else if cfg!(feature = "ui-small") {
        (4, 6)
    } else {
        (10_000, 5_000)
    };
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "UI projection/capacity regression".into(),
            window_width: 64,
            window_height: 64,
            platform: macroquad::miniquad::conf::Platform {
                webgl_version: if cfg!(feature = "webgl2") {
                    macroquad::miniquad::conf::WebGLVersion::WebGL2
                } else {
                    macroquad::miniquad::conf::WebGLVersion::WebGL1
                },
                ..Default::default()
            },
            ..Default::default()
        },
        draw_call_vertex_capacity: vertices,
        draw_call_index_capacity: indices,
        ..Default::default()
    }
}
#[macroquad::main(conf)]
async fn main() {
    let texture = Texture2D::from_rgba8(
        2,
        2,
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ],
    );
    texture.set_filter(FilterMode::Nearest);
    let skin = root_ui(|ui| {
        let mut skin = ui.default_skin();
        skin.window_style = ui
            .style_builder()
            .color(BLANK)
            .color_hovered(BLANK)
            .color_clicked(BLANK)
            .color_inactive(BLANK)
            .build();
        skin.margin = 0.;
        skin
    });
    set_camera(&Camera2D::from_display_rect(Rect::new(
        100., 200., 64., 64.,
    )));
    // Windows become active after their creation frame. Verify two full frames
    // after that normal UI warmup, retaining the same world camera throughout.
    for frame in 0..3 {
        clear_background(BLACK);
        // Complete triangles fit even the minimum supported UI capacities.
        let blue = Color::new(0., 0., 1., 1.);
        draw_triangle(vec2(100., 200.), vec2(164., 200.), vec2(164., 264.), blue);
        draw_triangle(vec2(100., 200.), vec2(164., 264.), vec2(100., 264.), blue);
        root_ui(|ui| {
            ui.push_skin(&skin);
            widgets::Window::new(1, vec2(16., 16.), vec2(32., 32.))
                .titlebar(false)
                .movable(false)
                .ui(ui, |ui| {
                    ui.canvas().image(Rect::new(8., 8., 48., 48.), &texture);
                    ui.canvas().rect(
                        Rect::new(24., 24., 16., 16.),
                        None,
                        Color::new(0., 1., 1., 1.),
                    );
                    ui.canvas().rect(Rect::new(28., 28., 8., 8.), None, WHITE);
                });
            ui.pop_skin();
        });
        if frame > 0 {
            info!("UI_VERIFY {}", frame - 1);
        }
        next_frame().await;
    }
    info!("browser UI regression: passed");
}
