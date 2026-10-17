//! Legacy module, code should be either removed or moved to different modules

use miniquad::*;

pub use miniquad::{FilterMode, UniformDesc};

use crate::{color::Color, logging::warn, telemetry, texture::Texture2D, tobytes::ToBytes, Error};

use std::collections::BTreeMap;

pub(crate) use crate::models::Vertex;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawMode {
    Triangles,
    Lines,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlPipeline(usize);

struct DrawCall {
    vertices_count: usize,
    indices_count: usize,
    vertices_start: usize,
    indices_start: usize,

    clip: Option<(i32, i32, i32, i32)>,
    viewport: Option<(i32, i32, i32, i32)>,
    texture: Option<Texture2D>,

    model: glam::Mat4,

    draw_mode: DrawMode,
    pipeline: GlPipeline,
    uniforms: Option<Vec<u8>>,
    render_pass: Option<crate::texture::RenderPass>,
    capture: bool,
}

impl DrawCall {
    const fn new(
        texture: Option<Texture2D>,
        model: glam::Mat4,
        draw_mode: DrawMode,
        pipeline: GlPipeline,
        uniforms: Option<Vec<u8>>,
        render_pass: Option<crate::texture::RenderPass>,
    ) -> DrawCall {
        DrawCall {
            vertices_start: 0,
            indices_start: 0,
            vertices_count: 0,
            indices_count: 0,
            viewport: None,
            clip: None,
            texture,
            model,
            draw_mode,
            pipeline,
            uniforms,
            render_pass,
            capture: false,
        }
    }
}

struct MagicSnapshotter {
    pipeline: Pipeline,
    shader: ShaderId,
    placeholder: TextureId,
    bindings: Bindings,
    pass: Option<RenderPass>,

    screen_texture: Option<miniquad::TextureId>,
}

mod snapshotter_shader {
    use miniquad::{ShaderMeta, UniformBlockLayout};

    pub const VERTEX: &str = r#"#version 100
    attribute vec2 position;
    attribute vec2 texcoord;

    varying lowp vec2 uv;

    void main() {
        gl_Position = vec4(position, 0, 1);
        uv = texcoord;
    }"#;

    pub const FRAGMENT: &str = r#"#version 100
    varying lowp vec2 uv;

    uniform sampler2D Texture;

    void main() {
        gl_FragColor = texture2D(Texture, uv);
    }"#;

    pub const METAL: &str = r#"#include <metal_stdlib>
    using namespace metal;

    struct Vertex
    {
        float2 position    [[attribute(0)]];
        float2 texcoord    [[attribute(1)]];
    };

    struct RasterizerData
    {
        float4 position [[position]];
        float2 uv [[user(locn1)]];
    };

    vertex RasterizerData vertexShader(Vertex v [[stage_in]])
    {
        RasterizerData out;

        out.position = float4(v.position, 0, 1);
        out.uv = v.texcoord;

        return out;
    }

    fragment float4 fragmentShader(RasterizerData in [[stage_in]], texture2d<float> tex [[texture(0)]], sampler texSmplr [[sampler(0)]])
    {
        return tex.sample(texSmplr, in.uv);
    }"#;

    pub fn meta() -> ShaderMeta {
        ShaderMeta {
            images: vec!["Texture".to_string()],
            uniforms: UniformBlockLayout { uniforms: vec![] },
        }
    }
}

impl MagicSnapshotter {
    fn new(ctx: &mut dyn RenderingBackend) -> MagicSnapshotter {
        let shader = ctx
            .new_shader(
                match ctx.info().backend {
                    Backend::OpenGl => ShaderSource::Glsl {
                        vertex: snapshotter_shader::VERTEX,
                        fragment: snapshotter_shader::FRAGMENT,
                    },
                    Backend::Metal => ShaderSource::Msl {
                        program: snapshotter_shader::METAL,
                    },
                },
                snapshotter_shader::meta(),
            )
            .unwrap_or_else(|e| panic!("Failed to load shader: {e}"));

        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("position", VertexFormat::Float2),
                VertexAttribute::new("texcoord", VertexFormat::Float2),
            ],
            shader,
            PipelineParams::default(),
        );

        #[rustfmt::skip]
        let vertices: [f32; 16] = [
             -1.0, -1.0, 0., 0.,
             1.0, -1.0, 1., 0. ,
             1.0,  1.0, 1., 1. ,
            -1.0,  1.0, 0., 1. ,
        ];
        let vertex_buffer = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&vertices),
        );

        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let index_buffer = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );

        let placeholder = ctx.new_texture_from_rgba8(1, 1, &[0, 0, 0, 0]);
        let bindings = Bindings {
            vertex_buffers: vec![vertex_buffer],
            index_buffer,
            images: vec![placeholder],
        };

        MagicSnapshotter {
            pipeline,
            shader,
            placeholder,
            bindings,
            pass: None,
            screen_texture: None,
        }
    }

    fn snapshot(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        camera: Option<&crate::texture::RenderPass>,
    ) {
        let (width, height) = camera.map_or_else(
            || {
                let (w, h) = miniquad::window::screen_size();
                (w as u32, h as u32)
            },
            |pass| ctx.texture_size(pass.color_texture.raw_miniquad_id()),
        );
        crate::texture::validate_target_dimensions(width, height);
        let stale = self
            .screen_texture
            .is_none_or(|texture| ctx.texture_size(texture) != (width, height))
            || self.pass.is_some() != camera.is_some();
        if stale {
            if let Some(pass) = self.pass.take() {
                ctx.delete_render_pass(pass);
            }
            if let Some(texture) = self.screen_texture.take() {
                ctx.delete_texture(texture);
            }
            let texture = ctx.new_render_texture(TextureParams {
                width,
                height,
                ..Default::default()
            });
            self.screen_texture = Some(texture);
            if camera.is_some() {
                self.pass = Some(ctx.new_render_pass(texture, None));
            }
        }
        if let Some(camera) = camera {
            self.bindings.images[0] = camera.color_texture.raw_miniquad_id();
            ctx.begin_pass(self.pass, PassAction::clear_color(1., 0., 1., 1.));
            ctx.apply_pipeline(&self.pipeline);
            ctx.apply_bindings(&self.bindings);
            ctx.draw(0, 6, 1);
            ctx.end_render_pass();
        } else {
            crate::texture::copy_screen_to_texture(ctx, self.screen_texture.unwrap());
        }
    }
}

struct GlState {
    texture: Option<Texture2D>,
    draw_mode: DrawMode,
    clip: Option<(i32, i32, i32, i32)>,
    viewport: Option<(i32, i32, i32, i32)>,
    model_stack: Vec<glam::Mat4>,
    pipeline: Option<GlPipeline>,
    depth_test_enable: bool,

    break_batching: bool,
    snapshotter: MagicSnapshotter,

    render_pass: Option<crate::texture::RenderPass>,
    capture: bool,
}

impl GlState {
    fn model(&self) -> glam::Mat4 {
        *self.model_stack.last().unwrap()
    }
}

#[derive(Clone, Debug)]
struct Uniform {
    name: String,
    uniform_type: UniformType,
    byte_offset: usize,
    byte_size: usize,
}

// Miniquad forwards the pointer as f32/i32 pointers to OpenGL. A Vec<u8>
// allocation only promises byte alignment, including under custom allocators.
#[derive(Clone)]
struct UniformBuffer(Vec<u32>);

impl UniformBuffer {
    fn new(byte_len: usize) -> Self {
        assert_eq!(byte_len % 4, 0);
        Self(vec![0; byte_len / 4])
    }
}

impl std::ops::Deref for UniformBuffer {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        // SAFETY: every u32 bit pattern is initialized and has no padding.
        // The byte view borrows the storage and preserves its four-byte alignment.
        unsafe { std::slice::from_raw_parts(self.0.as_ptr().cast(), self.0.len() * 4) }
    }
}

impl std::ops::DerefMut for UniformBuffer {
    fn deref_mut(&mut self) -> &mut [u8] {
        // SAFETY: all byte writes produce valid u32 values; the exclusive borrow
        // prevents access through the word view while the byte view is live.
        unsafe { std::slice::from_raw_parts_mut(self.0.as_mut_ptr().cast(), self.0.len() * 4) }
    }
}

#[derive(Clone)]
struct PipelineExt {
    pipeline: miniquad::Pipeline,
    wants_screen_texture: bool,
    uniforms: Vec<Uniform>,
    uniforms_data: UniformBuffer,
    textures: Vec<String>,
    textures_data: BTreeMap<String, Texture2D>,
    shader: ShaderId,
}

impl Uniform {
    fn set_scalar<T: ToBytes>(&self, destination: &mut [u8], uniform: T) {
        if size_of::<T>() != self.uniform_type.size() || self.byte_size != self.uniform_type.size()
        {
            warn!("Uniform {} requires a matching scalar value", self.name);
            return;
        }
        self.set_bytes(destination, uniform.to_bytes());
    }

    fn set_bytes(&self, destination: &mut [u8], bytes: &[u8]) {
        if bytes.len() != self.byte_size {
            warn!(
                "Trying to set uniform {} sized {} bytes with {} bytes",
                self.name,
                self.byte_size,
                bytes.len()
            );
            return;
        }
        destination[self.byte_offset..self.byte_offset + self.byte_size].copy_from_slice(bytes);
    }
}

fn uniform_layout(descriptions: &[UniformDesc]) -> Result<(Vec<Uniform>, usize), Error> {
    let mut offset = 0_usize;
    let mut uniforms = Vec::with_capacity(descriptions.len());
    for description in descriptions {
        // Miniquad casts the count to a GLsizei (i32) and computes the same
        // offsets unchecked. Validate before handing metadata to the backend.
        if description.array_count == 0 || description.array_count > i32::MAX as usize {
            return Err("Uniform array count must fit a positive i32".into());
        }
        let byte_size = description
            .uniform_type
            .size()
            .checked_mul(description.array_count)
            .ok_or("Uniform array byte size overflow")?;
        let end = offset
            .checked_add(byte_size)
            .filter(|end| *end <= isize::MAX as usize && *end <= i32::MAX as usize)
            .ok_or("Uniform buffer byte size overflow")?;
        uniforms.push(Uniform {
            name: description.name.clone(),
            uniform_type: description.uniform_type,
            byte_offset: offset,
            byte_size,
        });
        offset = end;
    }
    Ok((uniforms, offset))
}

impl PipelineExt {
    fn set_uniform<T: ToBytes>(&mut self, name: &str, uniform: T) {
        if let Some(meta) = self.uniforms.iter().find(|meta| meta.name == name) {
            meta.set_scalar(&mut self.uniforms_data, uniform);
        } else {
            warn!("Trying to set non-existing uniform: {}", name);
        }
    }

    fn set_uniform_array<T: ToBytes>(&mut self, name: &str, uniform: &[T]) {
        if let Some(meta) = self.uniforms.iter().find(|meta| meta.name == name) {
            meta.set_bytes(&mut self.uniforms_data, uniform.to_bytes());
        } else {
            warn!("Trying to set non-existing uniform: {}", name);
        }
    }
}

struct PipelinesStorage {
    pipelines: [Option<PipelineExt>; Self::MAX_PIPELINES],
    pipelines_amount: usize,
}

impl PipelinesStorage {
    const MAX_PIPELINES: usize = 32;
    const TRIANGLES_PIPELINE: GlPipeline = GlPipeline(0);
    const LINES_PIPELINE: GlPipeline = GlPipeline(1);
    const TRIANGLES_DEPTH_PIPELINE: GlPipeline = GlPipeline(2);
    const LINES_DEPTH_PIPELINE: GlPipeline = GlPipeline(3);

    fn new(ctx: &mut dyn RenderingBackend) -> PipelinesStorage {
        let shader = ctx
            .new_shader(
                match ctx.info().backend {
                    Backend::OpenGl => ShaderSource::Glsl {
                        vertex: shader::VERTEX,
                        fragment: shader::FRAGMENT,
                    },
                    Backend::Metal => ShaderSource::Msl {
                        program: shader::METAL,
                    },
                },
                shader::meta(),
            )
            .unwrap_or_else(|e| panic!("Failed to load shader: {e}"));

        let params = PipelineParams {
            color_blend: Some(BlendState::new(
                Equation::Add,
                BlendFactor::Value(BlendValue::SourceAlpha),
                BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            )),
            ..Default::default()
        };

        let mut storage = PipelinesStorage {
            pipelines: Default::default(),
            pipelines_amount: 0,
        };

        let triangles_pipeline = storage.make_pipeline(
            ctx,
            shader,
            PipelineParams {
                primitive_type: PrimitiveType::Triangles,
                ..params
            },
            false,
            vec![],
            vec![],
        );
        assert_eq!(triangles_pipeline, Self::TRIANGLES_PIPELINE);

        let lines_pipeline = storage.make_pipeline(
            ctx,
            shader,
            PipelineParams {
                primitive_type: PrimitiveType::Lines,
                ..params
            },
            false,
            vec![],
            vec![],
        );
        assert_eq!(lines_pipeline, Self::LINES_PIPELINE);

        let triangles_depth_pipeline = storage.make_pipeline(
            ctx,
            shader,
            PipelineParams {
                depth_write: true,
                depth_test: Comparison::LessOrEqual,
                primitive_type: PrimitiveType::Triangles,
                ..params
            },
            false,
            vec![],
            vec![],
        );
        assert_eq!(triangles_depth_pipeline, Self::TRIANGLES_DEPTH_PIPELINE);

        let lines_depth_pipeline = storage.make_pipeline(
            ctx,
            shader,
            PipelineParams {
                depth_write: true,
                depth_test: Comparison::LessOrEqual,
                primitive_type: PrimitiveType::Lines,
                ..params
            },
            false,
            vec![],
            vec![],
        );
        assert_eq!(lines_depth_pipeline, Self::LINES_DEPTH_PIPELINE);

        storage
    }

    fn make_pipeline(
        &mut self,
        ctx: &mut dyn RenderingBackend,
        shader: ShaderId,
        params: PipelineParams,
        wants_screen_texture: bool,
        mut uniforms: Vec<UniformDesc>,
        textures: Vec<String>,
    ) -> GlPipeline {
        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("position", VertexFormat::Float3),
                VertexAttribute::new("texcoord", VertexFormat::Float2),
                VertexAttribute::new("color0", VertexFormat::Byte4),
                VertexAttribute::new("normal", VertexFormat::Float4),
            ],
            shader,
            params,
        );

        let id = self
            .pipelines
            .iter()
            .position(|p| p.is_none())
            .unwrap_or_else(|| panic!("Pipelines amount exceeded"));

        for (name, kind) in shader::uniforms().into_iter().rev() {
            uniforms.insert(0, UniformDesc::new(name, kind));
        }
        let (uniforms, max_offset) = uniform_layout(&uniforms)
            .expect("uniform metadata must be validated before creating the pipeline");

        self.pipelines[id] = Some(PipelineExt {
            pipeline,
            shader,
            wants_screen_texture,
            uniforms,
            uniforms_data: UniformBuffer::new(max_offset),
            textures,
            textures_data: BTreeMap::new(),
        });
        self.pipelines_amount += 1;

        GlPipeline(id)
    }

    const fn get(&self, draw_mode: DrawMode, depth_enabled: bool) -> GlPipeline {
        match (draw_mode, depth_enabled) {
            (DrawMode::Triangles, false) => Self::TRIANGLES_PIPELINE,
            (DrawMode::Triangles, true) => Self::TRIANGLES_DEPTH_PIPELINE,
            (DrawMode::Lines, false) => Self::LINES_PIPELINE,
            (DrawMode::Lines, true) => Self::LINES_DEPTH_PIPELINE,
        }
    }

    fn get_quad_pipeline_mut(&mut self, pip: GlPipeline) -> &mut PipelineExt {
        self.pipelines[pip.0].as_mut().unwrap()
    }

    fn delete_pipeline(&mut self, ctx: &mut dyn RenderingBackend, pip: GlPipeline) {
        if let Some(pipeline) = self.pipelines[pip.0].take() {
            ctx.delete_pipeline(pipeline.pipeline);
            ctx.delete_shader(pipeline.shader);
            self.pipelines_amount -= 1;
        }
    }
}

pub struct QuadGl {
    pipelines: PipelinesStorage,

    draw_calls: Vec<DrawCall>,
    draw_calls_bindings: Vec<Bindings>,
    draw_calls_count: usize,
    state: GlState,
    start_time: f64,

    pub(crate) white_texture: miniquad::TextureId,
    max_vertices: usize,
    max_indices: usize,

    batch_vertex_buffer: Vec<Vertex>,
    batch_index_buffer: Vec<u16>,
}

impl QuadGl {
    pub fn new(
        ctx: &mut dyn miniquad::RenderingBackend,
        max_vertices: usize,
        max_indices: usize,
    ) -> QuadGl {
        let white_texture = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);

        QuadGl {
            pipelines: PipelinesStorage::new(ctx),
            state: GlState {
                clip: None,
                viewport: None,
                texture: None,
                model_stack: vec![glam::Mat4::IDENTITY],
                draw_mode: DrawMode::Triangles,
                pipeline: None,
                break_batching: false,
                depth_test_enable: false,
                snapshotter: MagicSnapshotter::new(ctx),
                render_pass: None,
                capture: false,
            },
            draw_calls: Vec::with_capacity(200),
            draw_calls_bindings: Vec::with_capacity(200),
            draw_calls_count: 0,
            start_time: miniquad::date::now(),

            white_texture,
            batch_vertex_buffer: Vec::with_capacity(max_vertices),
            batch_index_buffer: Vec::with_capacity(max_indices),
            max_vertices,
            max_indices,
        }
    }

    pub fn make_pipeline(
        &mut self,
        ctx: &mut dyn miniquad::RenderingBackend,
        shader: miniquad::ShaderSource,
        params: PipelineParams,
        uniforms: Vec<UniformDesc>,
        textures: Vec<String>,
    ) -> Result<GlPipeline, Error> {
        let mut shader_meta: ShaderMeta = shader::meta();

        for uniform in &uniforms {
            shader_meta.uniforms.uniforms.push(uniform.clone());
        }

        for texture in &textures {
            if texture == "Texture" {
                panic!(
                    "you can't use name `Texture` for your texture. This name is reserved for the texture that will be drawn with that material"
                );
            }
            if texture == "_ScreenTexture" {
                panic!(
                    "you can't use name `_ScreenTexture` for your texture in shaders. This name is reserved for screen texture"
                );
            }
            shader_meta.images.push(texture.clone());
        }

        let source = match shader {
            ShaderSource::Glsl { fragment, .. } => fragment,
            ShaderSource::Msl { program } => program,
        };
        let wants_screen_texture = source.contains("_ScreenTexture");
        if self.pipelines.pipelines.iter().all(Option::is_some) {
            return Err("Pipelines amount exceeded".into());
        }
        uniform_layout(&shader_meta.uniforms.uniforms)?;
        let shader = ctx.new_shader(shader, shader_meta)?;
        Ok(self.pipelines.make_pipeline(
            ctx,
            shader,
            params,
            wants_screen_texture,
            uniforms,
            textures,
        ))
    }

    pub(crate) fn clear(&mut self, ctx: &mut dyn miniquad::RenderingBackend, color: Color) {
        let clear = PassAction::clear_color(color.r, color.g, color.b, color.a);

        if let Some(current_pass) = self.state.render_pass.as_ref() {
            ctx.begin_pass(Some(current_pass.raw_miniquad_id()), clear);
        } else {
            ctx.begin_default_pass(clear);
        }
        ctx.end_render_pass();

        self.clear_draw_calls();
    }

    /// Reset only draw calls state
    pub fn clear_draw_calls(&mut self) {
        for call in &mut self.draw_calls {
            call.texture = None;
            call.render_pass = None;
            call.uniforms = None;
            call.vertices_count = 0;
            call.indices_count = 0;
        }
        self.draw_calls_count = 0;
        self.batch_index_buffer.clear();
        self.batch_vertex_buffer.clear();
    }

    /// Reset internal state to known default
    pub fn reset(&mut self) {
        self.state.clip = None;
        self.state.texture = None;
        self.state.model_stack = vec![glam::Mat4::IDENTITY];
        self.clear_draw_calls();
    }

    pub fn draw(&mut self, ctx: &mut dyn miniquad::RenderingBackend, projection: glam::Mat4) {
        let white_texture = self.white_texture;

        for _ in 0..self.draw_calls.len() - self.draw_calls_bindings.len() {
            let vertex_buffer = ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<Vertex>(self.max_vertices),
            );
            let index_buffer = ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<u16>(self.max_indices),
            );
            let bindings = Bindings {
                vertex_buffers: vec![vertex_buffer],
                index_buffer,
                images: vec![white_texture, white_texture],
            };

            self.draw_calls_bindings.push(bindings);
        }
        assert_eq!(self.draw_calls_bindings.len(), self.draw_calls.len());

        let (screen_width, screen_height) = miniquad::window::screen_size();
        let time = (miniquad::date::now() - self.start_time) as f32;
        let time = glam::vec4(time, time.sin(), time.cos(), 0.);

        for (dc, bindings) in self.draw_calls[0..self.draw_calls_count]
            .iter_mut()
            .zip(self.draw_calls_bindings.iter_mut())
        {
            let pipeline = self.pipelines.get_quad_pipeline_mut(dc.pipeline);

            let (width, height) = if let Some(render_pass) = dc.render_pass.as_ref() {
                let render_texture = ctx.render_pass_texture(render_pass.raw_miniquad_id());
                let (width, height) = ctx.texture_size(render_texture);
                (width, height)
            } else {
                (screen_width as u32, screen_height as u32)
            };

            if pipeline.wants_screen_texture {
                self.state
                    .snapshotter
                    .snapshot(ctx, dc.render_pass.as_ref());
            }

            if let Some(render_pass) = dc.render_pass.as_ref() {
                ctx.begin_pass(Some(render_pass.raw_miniquad_id()), PassAction::Nothing);
            } else {
                ctx.begin_default_pass(PassAction::Nothing);
            }

            ctx.buffer_update(
                bindings.vertex_buffers[0],
                BufferSource::slice(
                    &self.batch_vertex_buffer
                        [dc.vertices_start..(dc.vertices_start + dc.vertices_count)],
                ),
            );
            ctx.buffer_update(
                bindings.index_buffer,
                BufferSource::slice(
                    &self.batch_index_buffer
                        [dc.indices_start..(dc.indices_start + dc.indices_count)],
                ),
            );

            bindings.images[0] = dc
                .texture
                .as_ref()
                .map_or(white_texture, Texture2D::raw_miniquad_id);
            bindings.images[1] = self
                .state
                .snapshotter
                .screen_texture
                .unwrap_or(white_texture);
            bindings
                .images
                .resize(2 + pipeline.textures.len(), white_texture);

            for (pos, name) in pipeline.textures.iter().enumerate() {
                if let Some(texture) = pipeline.textures_data.get(name) {
                    bindings.images[2 + pos] = texture.raw_miniquad_id();
                }
            }

            ctx.apply_pipeline(&pipeline.pipeline);
            if let Some((x, y, w, h)) = dc.viewport {
                ctx.apply_viewport(x, y, w, h);
            } else {
                ctx.apply_viewport(0, 0, width as i32, height as i32);
            }
            if let Some(clip) = dc.clip {
                ctx.apply_scissor_rect(clip.0, height as i32 - (clip.1 + clip.3), clip.2, clip.3);
            } else {
                ctx.apply_scissor_rect(0, 0, width as i32, height as i32);
            }
            ctx.apply_bindings(bindings);

            if let Some(ref uniforms) = dc.uniforms {
                for i in 0..uniforms.len() {
                    pipeline.uniforms_data[i] = uniforms[i];
                }
            }
            pipeline.set_uniform("Projection", projection);
            pipeline.set_uniform("Model", dc.model);
            pipeline.set_uniform("_Time", time);
            ctx.apply_uniforms_from_bytes(
                pipeline.uniforms_data.as_ptr(),
                pipeline.uniforms_data.len(),
            );
            ctx.draw(0, dc.indices_count as i32, 1);
            ctx.end_render_pass();

            if dc.capture {
                telemetry::track_drawcall(ctx, &pipeline.pipeline, bindings, dc.indices_count);
            }

            dc.texture = None;
            dc.render_pass = None;
            dc.vertices_count = 0;
            dc.indices_count = 0;
            dc.vertices_start = 0;
            dc.indices_start = 0;
        }

        self.draw_calls_count = 0;
        self.batch_index_buffer.clear();
        self.batch_vertex_buffer.clear();
    }

    pub(crate) fn shutdown(&mut self, ctx: &mut dyn RenderingBackend) {
        self.clear_draw_calls();
        self.state.texture = None;
        self.state.render_pass = None;
        self.draw_calls.clear();
        for bindings in self.draw_calls_bindings.drain(..) {
            for buffer in bindings.vertex_buffers {
                ctx.delete_buffer(buffer);
            }
            ctx.delete_buffer(bindings.index_buffer);
        }
        let mut shaders = std::collections::HashSet::new();
        for pipeline in &mut self.pipelines.pipelines {
            if let Some(pipeline) = pipeline.take() {
                ctx.delete_pipeline(pipeline.pipeline);
                shaders.insert(pipeline.shader);
            }
        }
        let snapshot = &mut self.state.snapshotter;
        if let Some(pass) = snapshot.pass.take() {
            ctx.delete_render_pass(pass);
        }
        if let Some(texture) = snapshot.screen_texture.take() {
            ctx.delete_texture(texture);
        }
        for buffer in snapshot.bindings.vertex_buffers.drain(..) {
            ctx.delete_buffer(buffer);
        }
        ctx.delete_buffer(snapshot.bindings.index_buffer);
        ctx.delete_texture(snapshot.placeholder);
        ctx.delete_pipeline(snapshot.pipeline);
        shaders.insert(snapshot.shader);
        for shader in shaders {
            ctx.delete_shader(shader);
        }
        ctx.delete_texture(self.white_texture);
    }

    pub(crate) fn capture(&mut self, capture: bool) {
        self.state.capture = capture;
    }

    pub fn get_active_render_pass(&self) -> Option<crate::texture::RenderPass> {
        self.state.render_pass.clone()
    }

    pub const fn is_depth_test_enabled(&self) -> bool {
        self.state.depth_test_enable
    }

    pub fn render_pass(&mut self, render_pass: Option<crate::texture::RenderPass>) {
        self.state.render_pass = render_pass;
    }

    pub fn depth_test(&mut self, enable: bool) {
        self.state.depth_test_enable = enable;
    }

    pub fn texture(&mut self, texture: Option<&Texture2D>) {
        self.state.texture = texture.cloned();
    }

    pub fn scissor(&mut self, clip: Option<(i32, i32, i32, i32)>) {
        self.state.clip = clip;
    }

    pub fn viewport(&mut self, viewport: Option<(i32, i32, i32, i32)>) {
        self.state.viewport = viewport;
    }

    pub fn get_viewport(&self) -> (i32, i32, i32, i32) {
        self.state.viewport.unwrap_or((
            0,
            0,
            miniquad::window::screen_size().0 as _,
            miniquad::window::screen_size().1 as _,
        ))
    }

    pub fn push_model_matrix(&mut self, matrix: glam::Mat4) {
        self.state.model_stack.push(self.state.model() * matrix);
    }

    pub fn pop_model_matrix(&mut self) {
        if self.state.model_stack.len() > 1 {
            self.state.model_stack.pop();
        }
    }

    pub fn pipeline(&mut self, pipeline: Option<GlPipeline>) {
        if self.state.pipeline == pipeline {
            return;
        }

        self.state.break_batching = true;
        self.state.pipeline = pipeline;
    }

    pub fn draw_mode(&mut self, mode: DrawMode) {
        self.state.draw_mode = mode;
    }

    pub fn geometry(&mut self, vertices: &[Vertex], indices: &[u16]) {
        if vertices.len() >= self.max_vertices || indices.len() >= self.max_indices {
            warn!("geometry() exceeded max drawcall size, clamping");
        }

        let vertices = &vertices[0..self.max_vertices.min(vertices.len())];
        let indices = &indices[0..self.max_indices.min(indices.len())];

        let pip = self.state.pipeline.unwrap_or(
            self.pipelines
                .get(self.state.draw_mode, self.state.depth_test_enable),
        );

        let previous_dc_ix = if self.draw_calls_count == 0 {
            None
        } else {
            Some(self.draw_calls_count - 1)
        };
        let previous_dc = previous_dc_ix.and_then(|ix| self.draw_calls.get(ix));

        if previous_dc.map_or(true, |draw_call| {
            draw_call.texture != self.state.texture
                || draw_call.clip != self.state.clip
                || draw_call.viewport != self.state.viewport
                || draw_call.model != self.state.model()
                || draw_call.pipeline != pip
                || draw_call.render_pass != self.state.render_pass
                || draw_call.draw_mode != self.state.draw_mode
                || draw_call.vertices_count >= self.max_vertices - vertices.len()
                || draw_call.indices_count >= self.max_indices - indices.len()
                || draw_call.capture != self.state.capture
                || self.state.break_batching
        }) {
            let uniforms = self.state.pipeline.map_or(None, |pipeline| {
                Some(
                    self.pipelines
                        .get_quad_pipeline_mut(pipeline)
                        .uniforms_data
                        .to_vec(),
                )
            });

            if self.draw_calls_count >= self.draw_calls.len() {
                self.draw_calls.push(DrawCall::new(
                    self.state.texture.clone(),
                    self.state.model(),
                    self.state.draw_mode,
                    pip,
                    uniforms.clone(),
                    self.state.render_pass.clone(),
                ));
            }
            self.draw_calls[self.draw_calls_count].texture = self.state.texture.clone();
            self.draw_calls[self.draw_calls_count].uniforms = uniforms;
            self.draw_calls[self.draw_calls_count].vertices_count = 0;
            self.draw_calls[self.draw_calls_count].indices_count = 0;
            self.draw_calls[self.draw_calls_count].clip = self.state.clip;
            self.draw_calls[self.draw_calls_count].viewport = self.state.viewport;
            self.draw_calls[self.draw_calls_count].model = self.state.model();
            self.draw_calls[self.draw_calls_count].pipeline = pip;
            self.draw_calls[self.draw_calls_count].render_pass = self.state.render_pass.clone();
            self.draw_calls[self.draw_calls_count].capture = self.state.capture;
            self.draw_calls[self.draw_calls_count].indices_start = self.batch_index_buffer.len();
            self.draw_calls[self.draw_calls_count].vertices_start = self.batch_vertex_buffer.len();

            self.draw_calls_count += 1;
            self.state.break_batching = false;
        };
        let dc = &mut self.draw_calls[self.draw_calls_count - 1];

        self.batch_vertex_buffer.extend(vertices);
        self.batch_index_buffer
            .extend(indices.iter().map(|x| *x + dc.vertices_count as u16));

        dc.vertices_count += vertices.len();
        dc.indices_count += indices.len();

        dc.texture = self.state.texture.clone();
    }

    pub fn delete_pipeline(&mut self, ctx: &mut dyn RenderingBackend, pipeline: GlPipeline) {
        if self.state.pipeline == Some(pipeline) {
            self.state.pipeline = None;
        }
        self.pipelines.delete_pipeline(ctx, pipeline);
    }

    pub(crate) fn set_uniform_bytes(
        &mut self,
        pipeline: GlPipeline,
        name: &str,
        bytes: &[u8],
        scalar: bool,
    ) {
        self.state.break_batching = true;
        let pipeline = self.pipelines.get_quad_pipeline_mut(pipeline);
        if let Some(uniform) = pipeline
            .uniforms
            .iter()
            .find(|uniform| uniform.name == name)
        {
            if scalar && uniform.byte_size != uniform.uniform_type.size() {
                return;
            }
            uniform.set_bytes(&mut pipeline.uniforms_data, bytes);
        }
    }

    pub fn set_uniform<T: ToBytes>(&mut self, pipeline: GlPipeline, name: &str, uniform: T) {
        self.state.break_batching = true;

        self.pipelines
            .get_quad_pipeline_mut(pipeline)
            .set_uniform(name, uniform);
    }
    pub fn set_uniform_array<T: ToBytes>(
        &mut self,
        pipeline: GlPipeline,
        name: &str,
        uniform: &[T],
    ) {
        self.state.break_batching = true;

        self.pipelines
            .get_quad_pipeline_mut(pipeline)
            .set_uniform_array(name, uniform);
    }

    pub fn set_texture(&mut self, pipeline: GlPipeline, name: &str, texture: Texture2D) {
        let pipeline = self.pipelines.get_quad_pipeline_mut(pipeline);
        pipeline
            .textures
            .iter()
            .find(|x| *x == name)
            .unwrap_or_else(|| {
                panic!(
                    "can't find texture with name '{}', there is only this names: {:?}",
                    name, pipeline.textures
                )
            });
        pipeline.textures_data.insert(name.to_owned(), texture);
    }

    pub(crate) fn update_drawcall_capacity(
        &mut self,
        ctx: &mut dyn miniquad::RenderingBackend,
        max_vertices: usize,
        max_indices: usize,
    ) {
        self.max_vertices = max_vertices;
        self.max_indices = max_indices;
        self.draw_calls_count = 0;

        for draw_call in &mut self.draw_calls {
            draw_call.indices_start = 0;
            draw_call.vertices_start = 0;
        }
        for binding in &mut self.draw_calls_bindings {
            ctx.delete_buffer(binding.index_buffer);
            for vertex_buffer in &binding.vertex_buffers {
                ctx.delete_buffer(*vertex_buffer);
            }
            let vertex_buffer = ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<Vertex>(self.max_vertices),
            );
            let index_buffer = ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Stream,
                BufferSource::empty::<u16>(self.max_indices),
            );
            *binding = Bindings {
                vertex_buffers: vec![vertex_buffer],
                index_buffer,
                images: vec![self.white_texture, self.white_texture],
            };
        }
    }
}

mod shader {
    use miniquad::{ShaderMeta, UniformBlockLayout, UniformDesc, UniformType};

    pub const VERTEX: &str = r#"#version 100
    attribute vec3 position;
    attribute vec2 texcoord;
    attribute vec4 color0;
    attribute vec4 normal;

    varying lowp vec2 uv;
    varying lowp vec4 color;

    uniform mat4 Model;
    uniform mat4 Projection;

    void main() {
        gl_Position = Projection * Model * vec4(position, 1);
        color = color0 / 255.0;
        uv = texcoord;
    }"#;

    pub const FRAGMENT: &str = r#"#version 100
    varying lowp vec4 color;
    varying lowp vec2 uv;

    uniform sampler2D Texture;

    void main() {
        gl_FragColor = color * texture2D(Texture, uv) ;
    }"#;

    pub const METAL: &str = r#"
#include <metal_stdlib>
    using namespace metal;

    struct Uniforms
    {
        float4x4 Model;
        float4x4 Projection;
    };

    struct Vertex
    {
        float3 position    [[attribute(0)]];
        float2 texcoord    [[attribute(1)]];
        float4 color0      [[attribute(2)]];
    };

    struct RasterizerData
    {
        float4 position [[position]];
        float4 color [[user(locn0)]];
        float2 uv [[user(locn1)]];
    };

    vertex RasterizerData vertexShader(Vertex v [[stage_in]], constant Uniforms& uniforms [[buffer(0)]])
    {
        RasterizerData out;

        out.position = uniforms.Model * uniforms.Projection * float4(v.position, 1);
        out.color = v.color0 / 255.0;
        out.uv = v.texcoord;

        return out;
    }

    fragment float4 fragmentShader(RasterizerData in [[stage_in]], texture2d<float> tex [[texture(0)]], sampler texSmplr [[sampler(0)]])
    {
        return in.color * tex.sample(texSmplr, in.uv);
    }
    "#;
    pub fn uniforms() -> Vec<(&'static str, UniformType)> {
        vec![
            ("Projection", UniformType::Mat4),
            ("Model", UniformType::Mat4),
            ("_Time", UniformType::Float4),
        ]
    }

    pub fn meta() -> ShaderMeta {
        ShaderMeta {
            images: vec!["Texture".to_string(), "_ScreenTexture".to_string()],
            uniforms: UniformBlockLayout {
                uniforms: uniforms()
                    .into_iter()
                    .map(|(name, kind)| UniformDesc::new(name, kind))
                    .collect(),
            },
        }
    }
}

#[cfg(test)]
mod uniform_tests {
    use super::*;

    #[test]
    fn uploads_preserve_offsets_and_reject_mismatched_lengths() {
        let (uniforms, size) = uniform_layout(&[
            UniformDesc::new("prefix", UniformType::Int1),
            UniformDesc::new("vector", UniformType::Float3),
            UniformDesc::new("array", UniformType::Float2).array(2),
            UniformDesc::new("matrix", UniformType::Mat4),
        ])
        .unwrap();
        let mut bytes = UniformBuffer::new(size);
        bytes.fill(0x55);
        uniforms[0].set_scalar(&mut bytes, 42_i32);
        uniforms[1].set_scalar(&mut bytes, [1_f32, 2., 3.]);
        uniforms[2].set_bytes(&mut bytes, [[4_f32, 5.], [6., 7.]].to_bytes());
        uniforms[3].set_scalar(&mut bytes, glam::Mat4::IDENTITY);
        let expected: Vec<_> = 42_i32
            .to_ne_bytes()
            .into_iter()
            .chain(
                [1_f32, 2., 3., 4., 5., 6., 7.]
                    .iter()
                    .flat_map(|f| f.to_ne_bytes()),
            )
            .chain(
                glam::Mat4::IDENTITY
                    .to_cols_array()
                    .iter()
                    .flat_map(|f| f.to_ne_bytes()),
            )
            .collect();
        assert_eq!(&*bytes, expected);
        uniforms[1].set_scalar(&mut bytes, 1_f32);
        uniforms[2].set_scalar(&mut bytes, [9_f32; 4]);
        uniforms[2].set_bytes(&mut bytes, [9_f32; 2].to_bytes());
        uniforms[2].set_bytes(&mut bytes, [9_f32; 6].to_bytes());
        assert_eq!(&*bytes, expected);
    }

    #[test]
    fn backend_uniform_pointer_supports_typed_reads() {
        let mut buffer = UniformBuffer::new(16);
        buffer.copy_from_slice([1_i32, -2, 3, -4].to_bytes());
        assert_eq!(buffer.as_ptr().align_offset(std::mem::align_of::<i32>()), 0);
        // SAFETY: this is the typed pointer Miniquad passes to GL. The buffer
        // guarantees alignment, initialized words, and the exact byte extent.
        let words = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<i32>(), 4) };
        assert_eq!(words, [1, -2, 3, -4]);
    }

    #[test]
    fn invalid_uniform_counts_are_rejected_before_backend_use() {
        for count in [0, i32::MAX as usize + 1, usize::MAX] {
            assert!(
                uniform_layout(&[UniformDesc::new("bad", UniformType::Mat4).array(count)]).is_err()
            );
        }
        {
            let count = i32::MAX as usize / 64;
            assert!(uniform_layout(&[
                UniformDesc::new("a", UniformType::Mat4).array(count),
                UniformDesc::new("b", UniformType::Mat4).array(count),
            ])
            .is_err());
            assert!(uniform_layout(&[
                UniformDesc::new("overflow", UniformType::Mat4).array(i32::MAX as usize),
            ])
            .is_err());
        }
        #[cfg(target_pointer_width = "32")]
        {
            // Each count is legal for GLsizei, but the byte length is not legal
            // for Rust slices on wasm32.
            let count = isize::MAX as usize / 64;
            assert!(uniform_layout(&[
                UniformDesc::new("a", UniformType::Mat4).array(count),
                UniformDesc::new("b", UniformType::Mat4).array(count),
            ])
            .is_err());
            assert!(uniform_layout(&[
                UniformDesc::new("overflow", UniformType::Mat4).array(i32::MAX as usize),
            ])
            .is_err());
        }
    }
}
