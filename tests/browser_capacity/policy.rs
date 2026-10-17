//! Generated public-API capacity policy cases; no application workload/assets.
use macroquad::{
    miniquad::conf::WebGLVersion, prelude::*, window::gl_set_drawcall_buffer_capacity,
};

const RED: Color = Color::new(1., 0., 0., 1.);
const GREEN: Color = Color::new(0., 1., 0., 1.);
const BLUE: Color = Color::new(0., 0., 1., 1.);

fn conf() -> Conf {
    Conf {
        window_title: "Capacity policy regression".into(),
        window_width: 64,
        window_height: 64,
        platform: macroquad::miniquad::conf::Platform {
            webgl_version: if cfg!(feature = "webgl2") {
                WebGLVersion::WebGL2
            } else {
                WebGLVersion::WebGL1
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

fn begin(name: &str, vertices: usize, indices: usize) {
    set_default_camera();
    gl_set_drawcall_buffer_capacity(vertices, indices);
    clear_background(BLACK);
    info!("CAPACITY_BEGIN {}", name);
}

fn end(name: &str) {
    set_default_camera();
    info!("CAPACITY_END {}", name);
}

fn rectangles() {
    draw_rectangle(0., 0., 16., 16., RED);
    draw_rectangle(16., 0., 16., 16., GREEN);
    draw_rectangle(32., 0., 16., 16., BLUE);
}

#[macroquad::main(conf)]
async fn main() {
    if !cfg!(debug_assertions) {
        begin("policy-reject", 8, 12);
        draw_rectangle(0., 0., 16., 16., RED);
        let vertices = vec![Vertex::new(32., 32., 0., 0., 0., WHITE); 9];
        for mesh in [
            Mesh {
                vertices: vertices.clone(),
                indices: vec![0, 1, 8],
                texture: None,
            },
            Mesh {
                vertices: vertices[..3].to_vec(),
                indices: vec![0, 1, 3],
                texture: None,
            },
            Mesh {
                vertices: vertices[..3].to_vec(),
                indices: vec![0; 13],
                texture: None,
            },
        ] {
            draw_mesh(&mesh);
        }
        {
            let material = load_material(
                ShaderSource::Glsl {
                    vertex: "#version 100\nattribute vec3 position; uniform mat4 Projection; uniform mat4 Model; void main(){gl_Position=Projection*Model*vec4(position,1.);gl_PointSize=1.;}",
                    fragment: "#version 100\nprecision mediump float; uniform sampler2D _ScreenTexture; void main(){gl_FragColor=texture2D(_ScreenTexture,vec2(0.5));}",
                },
                MaterialParams { pipeline_params: PipelineParams::default(), ..Default::default() },
            ).unwrap();
            gl_use_material(&material);
            draw_arc(32., 32., 255, 12., 0., 8., 1177., WHITE);
            gl_use_default_material();
        }
        draw_rectangle(16., 0., 16., 16., GREEN);
        draw_rectangle(32., 0., 16., 16., BLUE);
        end("policy-reject");
    }
    for (name, arc, capacity, color) in [
        ("policy-arc-default", 1177_f32, (10_000, 5_000), RED),
        ("policy-arc-wrap", 24_001_f32, (100_000, 100_000), GREEN),
    ] {
        begin(name, capacity.0, capacity.1);
        let segments = (255_f32 * arc.to_radians() / std::f32::consts::TAU).ceil() as usize;
        info!("POLICY_ARC {} {}", arc, segments);
        draw_arc(32., 32., 255, 12.25, 0., 7.9375, arc, color);
        end(name);
    }
    begin("policy-tiny", 3, 3);
    rectangles();
    end("policy-tiny");

    let target = render_target(64, 64);
    target.texture.set_filter(FilterMode::Nearest);
    begin("policy-setter", 8, 12);
    let mut camera = Camera2D::from_display_rect(Rect::new(10., 20., 32., 32.));
    camera.render_target = Some(target.clone());
    camera.viewport = Some((16, 16, 32, 32));
    set_camera(&camera);
    clear_background(BLACK);
    draw_rectangle(10., 20., 16., 32., RED);
    gl_set_drawcall_buffer_capacity(4, 6);
    draw_rectangle(26., 20., 16., 32., GREEN);
    set_default_camera();
    clear_background(BLACK);
    draw_texture(&target.texture, 0., 0., WHITE);
    end("policy-setter");
    info!("browser capacity regression: passed");
}
