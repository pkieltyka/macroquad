//! Generated, fixed-pose browser workload; no application assets or GPU timing.
use macroquad::{miniquad::conf::WebGLVersion, prelude::*, telemetry};

const SEED: u32 = 0x5eed_0040;
const WARMUP: usize = 6;
const FRAMES: usize = 18;
const TRIALS: usize = 3;
const GLYPHS: [[u8; 7]; 8] = [
    [14, 17, 19, 21, 25, 17, 14],
    [4, 12, 4, 4, 4, 4, 14],
    [14, 17, 1, 2, 4, 8, 31],
    [30, 1, 1, 14, 1, 1, 30],
    [2, 6, 10, 18, 31, 2, 2],
    [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14],
    [31, 1, 2, 4, 8, 8, 8],
];

fn config() -> Conf {
    let mut conf = Conf {
        window_title: "Generated browser scene regression".into(),
        window_width: 640,
        window_height: 480,
        high_dpi: false,
        sample_count: 1,
        ..Default::default()
    };
    conf.platform.webgl_version = if cfg!(feature = "webgl2") {
        WebGLVersion::WebGL2
    } else {
        WebGLVersion::WebGL1
    };
    conf
}

fn atlas(width: u16, height: u16, pixel: impl Fn(usize, usize) -> [u8; 4]) -> Texture2D {
    let mut bytes = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        for x in 0..width as usize {
            bytes.extend_from_slice(&pixel(x, y));
        }
    }
    let texture = Texture2D::from_rgba8(width, height, &bytes);
    texture.set_filter(FilterMode::Nearest);
    texture
}

struct Scene {
    tiles: Texture2D,
    sprites: Texture2D,
    glyphs: Texture2D,
    ship_cells: [usize; 64],
    camera: Camera2D,
}

impl Scene {
    fn new() -> Self {
        let tile_colors = [
            [24, 40, 64, 255],
            [32, 64, 48, 255],
            [64, 40, 32, 255],
            [48, 32, 64, 255],
        ];
        let tiles = atlas(64, 16, |x, y| {
            if x % 16 == 0 || y == 0 {
                [8, 12, 20, 255]
            } else {
                tile_colors[x / 16]
            }
        });
        let sprite_colors = [
            [64, 208, 240, 255],
            [240, 176, 64, 255],
            [208, 96, 224, 255],
            [240, 80, 64, 255],
        ];
        let sprites = atlas(64, 16, |x, y| {
            if !(2..14).contains(&(x % 16)) || !(2..14).contains(&y) {
                [0; 4]
            } else {
                sprite_colors[x / 16]
            }
        });
        let glyphs = atlas(64, 8, |x, y| {
            let column = x % 8;
            if (1..=5).contains(&column)
                && (1..=7).contains(&y)
                && GLYPHS[x / 8][y - 1] & (1 << (5 - column)) != 0
            {
                [240, 224, 96, 255]
            } else {
                [0; 4]
            }
        });
        let mut state = SEED;
        let mut ship_cells = [0; 64];
        for cell in &mut ship_cells {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *cell = (state % 3) as usize;
        }
        ship_cells[0] = 0;
        ship_cells[63] = 2;
        Self {
            tiles,
            sprites,
            glyphs,
            ship_cells,
            camera: Camera2D {
                target: vec2(420., 290.),
                zoom: vec2(2. / 640., 2. / 480.),
                ..Default::default()
            },
        }
    }

    fn glyph(&self, cell: usize, position: Vec2) {
        // Like a bitmap font, sheet width is CPU metadata, not a GL query.
        let columns = (self.glyphs.width() / 8.) as usize;
        draw_texture_ex(
            &self.glyphs,
            position.x,
            position.y,
            WHITE,
            DrawTextureParams {
                source: Some(Rect::new((cell % columns) as f32 * 8., 0., 8., 8.)),
                ..Default::default()
            },
        );
    }

    fn label(&self, index: usize) {
        let world = vec2(
            124. + (index % 4) as f32 * 152.,
            82. + (index / 4) as f32 * 48.,
        );
        let screen = self.camera.world_to_screen(world);
        for glyph in 0..8 {
            self.glyph(glyph, screen + vec2(glyph as f32 * 8., 0.));
        }
    }

    fn draw(&self, per_label_camera: bool) {
        clear_background(BLACK);
        set_camera(&self.camera);
        for row in 0..30 {
            for col in 0..40 {
                let cell = (col + row * 3 + (SEED & 3) as usize) % 4;
                draw_texture_ex(
                    &self.tiles,
                    100. + col as f32 * 16.,
                    50. + row as f32 * 16.,
                    WHITE,
                    DrawTextureParams {
                        source: Some(Rect::new(cell as f32 * 16., 0., 16., 16.)),
                        ..Default::default()
                    },
                );
            }
        }
        for i in 0..64 {
            let position = if i == 0 {
                vec2(180., 130.)
            } else {
                vec2(164. + (i % 16) as f32 * 32., 114. + (i / 16) as f32 * 48.)
            };
            draw_texture_ex(
                &self.sprites,
                position.x,
                position.y,
                WHITE,
                DrawTextureParams {
                    source: Some(Rect::new(self.ship_cells[i] as f32 * 16., 0., 16., 16.)),
                    ..Default::default()
                },
            );
        }
        for i in 0..512 {
            draw_texture_ex(
                &self.sprites,
                108. + (i % 32) as f32 * 19.,
                300. + (i / 32) as f32 * 8.,
                WHITE,
                DrawTextureParams {
                    source: Some(Rect::new(52., 4., 8., 8.)),
                    dest_size: Some(vec2(3., 3.)),
                    ..Default::default()
                },
            );
        }
        if per_label_camera {
            for label in 0..32 {
                set_default_camera();
                self.label(label);
                set_camera(&self.camera);
            }
        } else {
            set_default_camera();
            for label in 0..32 {
                self.label(label);
            }
            set_camera(&self.camera);
        }
        set_default_camera();
        draw_rectangle(0., 432., 640., 48., Color::from_rgba(8, 16, 32, 255));
        for glyph in 0..16 {
            self.glyph(glyph % 8, vec2(16. + glyph as f32 * 8., 448.));
        }
    }
}

#[macroquad::main(config)]
async fn main() {
    next_frame().await;
    assert_eq!((screen_width(), screen_height()), (640., 480.));
    let baseline = telemetry::textures_count();
    info!("SCENE_BASELINE {}", baseline);
    let scene = Scene::new();
    assert_eq!(telemetry::textures_count(), baseline + 3);
    info!("SCENE_ASSETS_READY");
    for trial in 0..TRIALS {
        for per_label in if trial % 2 == 0 {
            [true, false]
        } else {
            [false, true]
        } {
            let variant = if per_label { "camera" } else { "screen" };
            for _ in 0..WARMUP {
                scene.draw(per_label);
                next_frame().await;
            }
            let mut cpu_ms = Vec::with_capacity(FRAMES);
            info!("SCENE_START {} {}", trial, variant);
            for _ in 0..FRAMES {
                let start = get_time();
                scene.draw(per_label);
                cpu_ms.push((get_time() - start) * 1000.);
                next_frame().await;
            }
            info!("SCENE_END {} {} {:?}", trial, variant, cpu_ms);
            // Readback/GL diagnostics happen only after the measured window.
            scene.draw(per_label);
            set_default_camera();
            info!("SCENE_VERIFY {} {}", trial, variant);
            next_frame().await;
        }
    }
    drop(scene);
    next_frame().await;
    next_frame().await;
    assert_eq!(telemetry::textures_count(), baseline);
    info!("SCENE_CLEANUP {}", telemetry::textures_count());
    info!("browser scene regression: passed");
}
