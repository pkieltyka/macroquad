//! Deliberately exercises the backend directly; no Macroquad ownership API is exposed.
use miniquad::{graphics::raw_gl::*, *};

// This standard query enum is not exported by Miniquad 0.4.11.
const GL_TEXTURE_BINDING_2D: u32 = 0x8069;

struct Regression {
    backend: Box<dyn RenderingBackend>,
    pipeline: Pipeline,
    bindings: Bindings,
    ran: bool,
}

impl Regression {
    fn new() -> Self {
        let mut backend = window::new_rendering_backend();
        let shader = backend.new_shader(
            ShaderSource::Glsl {
                vertex: "#version 100\nattribute vec2 position; varying vec2 uv; void main() { uv = position * 0.5 + 0.5; gl_Position = vec4(position, 0., 1.); }",
                fragment: "#version 100\nprecision mediump float; uniform sampler2D image; varying vec2 uv; void main() { gl_FragColor = texture2D(image, uv); }",
            },
            ShaderMeta { images: vec!["image".into()], uniforms: UniformBlockLayout { uniforms: vec![] } },
        ).unwrap();
        let pipeline = backend.new_pipeline(
            &[BufferLayout::default()],
            &[VertexAttribute::new("position", VertexFormat::Float2)],
            shader,
            PipelineParams::default(),
        );
        let vertices = [[-1_f32, -1.], [1., -1.], [1., 1.], [-1., 1.]];
        let indices = [0_u16, 1, 2, 0, 2, 3];
        let bindings = Bindings {
            vertex_buffers: vec![backend.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&vertices),
            )],
            index_buffer: backend.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&indices),
            ),
            images: vec![],
        };
        Self {
            backend,
            pipeline,
            bindings,
            ran: false,
        }
    }

    fn check_pixel(&mut self, texture: TextureId, expected: [u8; 4]) {
        self.bindings.images.clear();
        self.bindings.images.push(texture);
        self.backend.apply_bindings(&self.bindings);
        self.backend.draw(0, 6, 1);
        let mut actual = [0_u8; 4];
        // SAFETY: read one RGBA8 pixel into four initialized bytes from a live pass.
        unsafe {
            glReadPixels(
                16,
                16,
                1,
                1,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                actual.as_mut_ptr().cast(),
            )
        };
        assert_eq!(actual, expected);
        assert_eq!(unsafe { glGetError() }, GL_NO_ERROR);
    }
}

impl EventHandler for Regression {
    fn update(&mut self) {}

    fn draw(&mut self) {
        if self.ran {
            return;
        }
        self.ran = true;
        self.backend
            .begin_default_pass(PassAction::clear_color(0., 0., 0., 1.));
        self.backend.apply_pipeline(&self.pipeline);
        for round in 0..32_u8 {
            let first = self.backend.new_texture_from_rgba8(1, 1, &[255, 0, 0, 255]);
            let live = self.backend.new_texture_from_rgba8(1, 1, &[0, 0, 255, 255]);
            self.check_pixel(first, [255, 0, 0, 255]);
            self.backend.delete_texture(first);
            // This live-only update used to restore the deleted first texture
            // from GlCache. No application handle is reused after deletion.
            let expected = [round, 255 - round, 0, 255];
            self.backend.texture_update(live, &expected);
            let mut restored_binding = -1;
            // The previously bound texture was deleted, so a live upload must
            // restore binding zero rather than resurrect its deleted GL name.
            unsafe { glGetIntegerv(GL_TEXTURE_BINDING_2D, &mut restored_binding) };
            assert_eq!(restored_binding, 0);
            self.check_pixel(live, expected);
            self.backend
                .texture_update_part(live, 0, 0, 1, 1, &[0, 0, 255, 255]);
            self.check_pixel(live, [0, 0, 255, 255]);
            self.backend.delete_texture(live);
            let replacement = self
                .backend
                .new_texture_from_rgba8(1, 1, &[255, round, 255, 255]);
            self.check_pixel(replacement, [255, round, 255, 255]);
            self.backend.delete_texture(replacement);
        }
        self.backend.end_render_pass();
        miniquad::info!(
            "Texture cache regression passed: 32 churn rounds, 128 correct pixels, GL error 0"
        );
        window::request_quit();
    }
}

fn main() {
    let mut config = conf::Conf {
        window_title: "Texture cache regression".into(),
        window_width: 64,
        window_height: 64,
        ..Default::default()
    };
    config.platform.webgl_version = if cfg!(feature = "webgl2") {
        conf::WebGLVersion::WebGL2
    } else {
        conf::WebGLVersion::WebGL1
    };
    start(config, || Box::new(Regression::new()));
}
