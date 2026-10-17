//! Finite generated geometry; no application assets or workload assumptions.
use macroquad::{
    miniquad::conf::WebGLVersion, prelude::*, window::gl_set_drawcall_buffer_capacity,
};

const RED: Color = Color::new(1., 0., 0., 1.);
const GREEN: Color = Color::new(0., 1., 0., 1.);
const BLUE: Color = Color::new(0., 0., 1., 1.);

fn conf() -> Conf {
    Conf {
        window_title: "Draw capacity regression".into(),
        window_width: 64,
        window_height: 64,
        platform: macroquad::miniquad::conf::Platform {
            webgl_version: if cfg!(feature = "auto") {
                WebGLVersion::Auto
            } else if cfg!(feature = "webgl2") {
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

fn mesh(count: usize, color: Color) -> Mesh {
    let mut vertices = vec![Vertex::new(-100., -100., 0., 0., 0., color); count];
    let last = count - 4;
    for (slot, (x, y)) in
        vertices[last..]
            .iter_mut()
            .zip([(0., 0.), (16., 0.), (16., 16.), (0., 16.)])
    {
        *slot = Vertex::new(x, y, 0., 0., 0., color);
    }
    let last = last as u16;
    Mesh {
        vertices,
        indices: vec![last, last + 1, last + 2, last, last + 2, last + 3],
        texture: None,
    }
}

#[macroquad::main(conf)]
async fn main() {
    begin("triangles", 8, 12);
    draw_rectangle(0., 0., 16., 16., RED);
    draw_rectangle(16., 0., 16., 16., GREEN);
    draw_rectangle(32., 0., 16., 16., BLUE);
    end("triangles");

    begin("lines", 4, 4);
    draw_line_3d(vec3(0., 32.5, 0.), vec3(16., 32.5, 0.), RED);
    draw_line_3d(vec3(16., 32.5, 0.), vec3(32., 32.5, 0.), GREEN);
    draw_line_3d(vec3(32., 32.5, 0.), vec3(48., 32.5, 0.), BLUE);
    end("lines");

    begin("u16-domain", 100_000, 100_000);
    draw_mesh(&mesh(65_532, RED));
    draw_rectangle(16., 0., 16., 16., GREEN);
    draw_rectangle(32., 0., 16., 16., BLUE);
    end("u16-domain");

    begin("last-portable-vertex", 100_000, 100_000);
    draw_mesh(&mesh(65_535, RED));
    draw_rectangle(16., 0., 16., 16., GREEN);
    draw_rectangle(32., 0., 16., 16., BLUE);
    end("last-portable-vertex");

    begin("index-count", 4, 70_000);
    let mut repeated = mesh(4, RED);
    repeated.indices = repeated.indices.repeat(11_000);
    draw_mesh(&repeated);
    end("index-count");

    begin("material-order", 8, 12);
    let material = load_material(
        ShaderSource::Glsl {
            vertex: "#version 100\nattribute vec3 position; uniform mat4 Model; uniform mat4 Projection; void main(){gl_Position=Projection*Model*vec4(position,1.);}",
            fragment: "#version 100\nprecision mediump float; uniform vec4 tint; void main(){gl_FragColor=tint;}",
        },
        MaterialParams {
            uniforms: vec![UniformDesc::new("tint", UniformType::Float4)],
            ..Default::default()
        },
    ).unwrap();
    gl_use_material(&material);
    material.set_uniform("tint", vec4(1., 0., 0., 1.));
    draw_rectangle(0., 0., 32., 16., WHITE);
    material.set_uniform("tint", vec4(0., 1., 0., 1.));
    draw_rectangle(16., 0., 32., 16., WHITE);
    gl_use_default_material();
    draw_rectangle(32., 0., 16., 16., BLUE);
    end("material-order");
    info!("browser capacity regression: passed");
}
