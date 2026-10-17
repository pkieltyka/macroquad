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
    draw_calls_count: usize,
    // Shared GPU buffers, one pair per u16-addressable flush segment. They are
    // sized by actual use and reused across flushes instead of allocating a
    // full-capacity pair for every draw call slot.
    flush_buffers: Vec<FlushBuffers>,
    flush_plan: FlushPlan,
    // Uniform values are program state; built-in pipelines share one program.
    uniform_cache: Vec<(ShaderId, Vec<u32>)>,
    state: GlState,
    start_time: f64,

    pub(crate) white_texture: miniquad::TextureId,
    max_vertices: usize,
    max_indices: usize,
    vertex_limit: usize,
    // Release builds warn once per distinct rejection reason.
    geometry_rejections_warned: Vec<&'static str>,

    batch_vertex_buffer: Vec<Vertex>,
    batch_index_buffer: Vec<u16>,
}

fn drawcall_vertex_limit(gl_version: &str) -> usize {
    // WebGL2 reserves 0xffff for fixed primitive restart, even for triangles/lines.
    // https://registry.khronos.org/webgl/specs/latest/2.0/#NO_PRIMITIVE_RESTART_FIXED_INDEX
    if gl_version.starts_with("WebGL 2.") {
        u16::MAX as usize
    } else {
        u16::MAX as usize + 1
    }
}

struct FlushBuffers {
    bindings: Bindings,
    vertex_capacity: usize,
    index_capacity: usize,
}

/// A contiguous run of draw calls sharing one vertex/index buffer pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FlushSegment {
    first_vertex: usize,
    vertices: usize,
    first_index: usize,
    indices: usize,
}

#[derive(Default)]
struct FlushPlan {
    segments: Vec<FlushSegment>,
    /// Per draw call: its segment and first element in that segment's indices.
    placements: Vec<(usize, usize)>,
    /// Segment-relative indices of every draw call, in submission order.
    indices: Vec<u16>,
}

impl FlushPlan {
    /// Groups consecutive draw calls so every segment-relative index stays below
    /// the backend vertex limit, and rebases each call's local indices.
    fn build(
        &mut self,
        draw_calls: &[DrawCall],
        batch_indices: &[u16],
        vertex_limit: usize,
        index_limit: usize,
        segment_per_call: bool,
    ) {
        self.segments.clear();
        self.placements.clear();
        self.indices.clear();
        for dc in draw_calls {
            let fits = !segment_per_call
                && self.segments.last().is_some_and(|segment| {
                    segment.vertices + dc.vertices_count <= vertex_limit
                        && segment.indices + dc.indices_count <= index_limit
                });
            if !fits {
                self.segments.push(FlushSegment {
                    first_vertex: dc.vertices_start,
                    vertices: 0,
                    first_index: self.indices.len(),
                    indices: 0,
                });
            }
            let segment_index = self.segments.len() - 1;
            let segment = &mut self.segments[segment_index];
            // geometry() only ever appends to the newest draw call.
            assert_eq!(
                segment.first_vertex + segment.vertices,
                dc.vertices_start,
                "draw call vertices must be contiguous"
            );
            let offset = segment.vertices;
            self.placements.push((segment_index, segment.indices));
            self.indices.extend(
                batch_indices[dc.indices_start..dc.indices_start + dc.indices_count]
                    .iter()
                    .map(|&index| {
                        u16::try_from(usize::from(index) + offset)
                            .expect("segment-relative index must fit u16")
                    }),
            );
            segment.vertices += dc.vertices_count;
            segment.indices += dc.indices_count;
        }
    }
}

// Index *count* has a separate GLsizei and byte-size limit from index values.
pub(crate) fn drawcall_capacity(max_vertices: usize, max_indices: usize) -> (usize, usize) {
    let max_vertices = max_vertices.min(u16::MAX as usize + 1);
    assert!(
        max_indices <= i32::MAX as usize
            && max_indices
                .checked_mul(std::mem::size_of::<u16>())
                .is_some_and(|bytes| bytes <= isize::MAX as usize),
        "draw call index capacity exceeds backend buffer or draw limits"
    );
    (max_vertices, max_indices)
}

impl QuadGl {
    pub fn new(
        ctx: &mut dyn miniquad::RenderingBackend,
        max_vertices: usize,
        max_indices: usize,
    ) -> QuadGl {
        let (max_vertices, max_indices) = drawcall_capacity(max_vertices, max_indices);
        let vertex_limit = drawcall_vertex_limit(&ctx.info().gl_version_string);
        let max_vertices = max_vertices.min(vertex_limit);
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
            draw_calls_count: 0,
            flush_buffers: Vec::new(),
            flush_plan: FlushPlan::default(),
            uniform_cache: Vec::new(),
            start_time: miniquad::date::now(),

            white_texture,
            batch_vertex_buffer: Vec::with_capacity(max_vertices),
            batch_index_buffer: Vec::with_capacity(max_indices),
            max_vertices,
            max_indices,
            vertex_limit,
            geometry_rejections_warned: Vec::new(),
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
        let count = self.draw_calls_count;
        if count == 0 {
            self.batch_index_buffer.clear();
            self.batch_vertex_buffer.clear();
            return;
        }

        // Upload each segment once, then draw its calls at index offsets.
        // Miniquad's Metal backend only draws from element 0 and binds uniforms
        // per render encoder, so it keeps one call per segment and pass state.
        let metal = ctx.info().backend == Backend::Metal;
        let index_limit = self.max_indices.max(3 * (u16::MAX as usize + 1));
        self.flush_plan.build(
            &self.draw_calls[..count],
            &self.batch_index_buffer,
            self.vertex_limit,
            index_limit,
            metal,
        );
        for (slot, segment) in self.flush_plan.segments.iter().enumerate() {
            let reusable = self.flush_buffers.get(slot).is_some_and(|buffers| {
                buffers.vertex_capacity >= segment.vertices
                    && buffers.index_capacity >= segment.indices
            });
            if !reusable {
                let (previous_vertices, previous_indices) =
                    self.flush_buffers.get(slot).map_or((0, 0), |buffers| {
                        (buffers.vertex_capacity, buffers.index_capacity)
                    });
                // Grow geometrically within the u16-addressable segment limits.
                let vertex_capacity = segment
                    .vertices
                    .max(self.max_vertices)
                    .max(previous_vertices.saturating_mul(2))
                    .min(self.vertex_limit);
                let index_capacity = segment
                    .indices
                    .max(self.max_indices)
                    .max(previous_indices.saturating_mul(2))
                    .min(index_limit);
                let bindings = Bindings {
                    vertex_buffers: vec![ctx.new_buffer(
                        BufferType::VertexBuffer,
                        BufferUsage::Stream,
                        BufferSource::empty::<Vertex>(vertex_capacity),
                    )],
                    index_buffer: ctx.new_buffer(
                        BufferType::IndexBuffer,
                        BufferUsage::Stream,
                        BufferSource::empty::<u16>(index_capacity),
                    ),
                    images: vec![white_texture, white_texture],
                };
                let buffers = FlushBuffers {
                    bindings,
                    vertex_capacity,
                    index_capacity,
                };
                if slot < self.flush_buffers.len() {
                    let old = std::mem::replace(&mut self.flush_buffers[slot], buffers);
                    ctx.delete_buffer(old.bindings.vertex_buffers[0]);
                    ctx.delete_buffer(old.bindings.index_buffer);
                } else {
                    self.flush_buffers.push(buffers);
                }
            }
            let bindings = &self.flush_buffers[slot].bindings;
            ctx.buffer_update(
                bindings.vertex_buffers[0],
                BufferSource::slice(
                    &self.batch_vertex_buffer
                        [segment.first_vertex..segment.first_vertex + segment.vertices],
                ),
            );
            ctx.buffer_update(
                bindings.index_buffer,
                BufferSource::slice(
                    &self.flush_plan.indices
                        [segment.first_index..segment.first_index + segment.indices],
                ),
            );
        }

        let (screen_width, screen_height) = miniquad::window::screen_size();
        let time = (miniquad::date::now() - self.start_time) as f32;
        let time = glam::vec4(time, time.sin(), time.cos(), 0.);

        // Consecutive calls into one target share a pass. State already applied
        // inside the open pass is not re-sent; a new pass resets that tracking.
        let mut open_pass: Option<Option<miniquad::RenderPass>> = None;
        let mut applied_pipeline = None;
        let mut applied_viewport = None;
        let mut applied_scissor = None;
        let mut applied_bindings: Option<(GlPipeline, usize)> = None;
        // Uniform values are per-program GL state and persist across passes.
        self.uniform_cache.clear();

        for (dc, &(slot, base_element)) in self.draw_calls[..count]
            .iter_mut()
            .zip(&self.flush_plan.placements)
        {
            let pipeline = self.pipelines.get_quad_pipeline_mut(dc.pipeline);
            let target = dc.render_pass.as_ref().map(|pass| pass.raw_miniquad_id());

            let (width, height) = if let Some(render_pass) = target {
                let render_texture = ctx.render_pass_texture(render_pass);
                ctx.texture_size(render_texture)
            } else {
                (screen_width as u32, screen_height as u32)
            };

            if pipeline.wants_screen_texture {
                // The snapshot must observe every earlier call and runs its own pass.
                if open_pass.take().is_some() {
                    ctx.end_render_pass();
                }
                self.state
                    .snapshotter
                    .snapshot(ctx, dc.render_pass.as_ref());
            }

            // An MSAA target's texture is its resolve copy, updated when its pass
            // ends. A call sampling its own target must see the earlier calls.
            if open_pass == Some(target) {
                if let Some(pass) = dc.render_pass.as_ref() {
                    let own = Some(&pass.color_texture);
                    if dc.texture.as_ref() == own
                        || pipeline
                            .textures_data
                            .values()
                            .any(|texture| Some(texture) == own)
                    {
                        ctx.end_render_pass();
                        open_pass = None;
                    }
                }
            }

            if open_pass != Some(target) {
                if open_pass.is_some() {
                    ctx.end_render_pass();
                }
                if metal {
                    self.uniform_cache.clear();
                }
                ctx.begin_pass(target, PassAction::Nothing);
                open_pass = Some(target);
                applied_pipeline = None;
                applied_viewport = None;
                applied_scissor = None;
                // Ending a pass resets the cached buffer bindings.
                applied_bindings = None;
            }

            let bindings = &mut self.flush_buffers[slot].bindings;
            let previous_images = bindings.images.len();
            let previous_texture = bindings.images[0];
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

            if applied_pipeline != Some(dc.pipeline) {
                ctx.apply_pipeline(&pipeline.pipeline);
                applied_pipeline = Some(dc.pipeline);
                applied_bindings = None;
            }
            let viewport = dc.viewport.unwrap_or((0, 0, width as i32, height as i32));
            if applied_viewport != Some(viewport) {
                ctx.apply_viewport(viewport.0, viewport.1, viewport.2, viewport.3);
                applied_viewport = Some(viewport);
            }
            let scissor = dc.clip.map_or((0, 0, width as i32, height as i32), |clip| {
                (clip.0, height as i32 - (clip.1 + clip.3), clip.2, clip.3)
            });
            if applied_scissor != Some(scissor) {
                ctx.apply_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                applied_scissor = Some(scissor);
            }
            // Only built-in pipelines (no extra textures) may reuse bindings; the
            // screen texture slot stays fixed within one pass.
            if applied_bindings != Some((dc.pipeline, slot))
                || previous_texture != bindings.images[0]
                || previous_images != 2
                || !pipeline.textures.is_empty()
            {
                ctx.apply_bindings(bindings);
                applied_bindings = Some((dc.pipeline, slot));
            }

            if let Some(ref uniforms) = dc.uniforms {
                for i in 0..uniforms.len() {
                    pipeline.uniforms_data[i] = uniforms[i];
                }
            }
            pipeline.set_uniform("Projection", projection);
            pipeline.set_uniform("Model", dc.model);
            pipeline.set_uniform("_Time", time);
            let uniforms = &pipeline.uniforms_data.0;
            match self
                .uniform_cache
                .iter_mut()
                .find(|(cached, _)| *cached == pipeline.shader)
            {
                Some((_, applied)) if applied == uniforms => {}
                cached => {
                    ctx.apply_uniforms_from_bytes(
                        pipeline.uniforms_data.as_ptr(),
                        pipeline.uniforms_data.len(),
                    );
                    match cached {
                        Some((_, applied)) => applied.clone_from(uniforms),
                        None => self.uniform_cache.push((pipeline.shader, uniforms.clone())),
                    }
                }
            }
            ctx.draw(
                i32::try_from(base_element).expect("segment index offset must fit GLint"),
                i32::try_from(dc.indices_count).expect("validated index count must fit GLsizei"),
                1,
            );

            if dc.capture {
                // Capture replays the call into its own pass. End ours first so
                // its MSAA resolve runs before another pass becomes current.
                ctx.end_render_pass();
                open_pass = None;
                telemetry::track_drawcall(
                    ctx,
                    &pipeline.pipeline,
                    bindings,
                    base_element,
                    dc.indices_count,
                );
            }

            dc.texture = None;
            dc.render_pass = None;
            dc.vertices_count = 0;
            dc.indices_count = 0;
            dc.vertices_start = 0;
            dc.indices_start = 0;
        }
        if open_pass.is_some() {
            ctx.end_render_pass();
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
        self.release_flush_buffers(ctx);
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
        if indices.is_empty() {
            return;
        }
        if vertices.len() > self.max_vertices || indices.len() > self.max_indices {
            self.reject_geometry(
                vertices.len(),
                indices.len(),
                "geometry exceeds draw call capacity",
            );
            return;
        }
        if indices
            .iter()
            .any(|&index| usize::from(index) >= vertices.len())
        {
            self.reject_geometry(
                vertices.len(),
                indices.len(),
                "geometry index is outside its vertex slice",
            );
            return;
        }

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
                || draw_call.vertices_count > self.max_vertices - vertices.len()
                || draw_call.indices_count > self.max_indices - indices.len()
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
            self.draw_calls[self.draw_calls_count].draw_mode = self.state.draw_mode;
            self.draw_calls[self.draw_calls_count].render_pass = self.state.render_pass.clone();
            self.draw_calls[self.draw_calls_count].capture = self.state.capture;
            self.draw_calls[self.draw_calls_count].indices_start = self.batch_index_buffer.len();
            self.draw_calls[self.draw_calls_count].vertices_start = self.batch_vertex_buffer.len();

            self.draw_calls_count += 1;
            self.state.break_batching = false;
        };
        let dc = &mut self.draw_calls[self.draw_calls_count - 1];

        self.batch_vertex_buffer.extend(vertices);
        self.batch_index_buffer.extend(indices.iter().map(|&index| {
            u16::try_from(usize::from(index) + dc.vertices_count)
                .expect("validated draw call index must fit u16")
        }));

        dc.vertices_count += vertices.len();
        dc.indices_count += indices.len();

        dc.texture = self.state.texture.clone();
    }

    pub(crate) fn reject_geometry(
        &mut self,
        vertices: usize,
        indices: usize,
        reason: &'static str,
    ) {
        if cfg!(debug_assertions) || !self.geometry_rejections_warned.contains(&reason) {
            let message = format!(
                "{reason}; submitted {vertices} vertices/{indices} indices, effective capacity {} vertices/{} indices. Entire call rejected; submit smaller meshes or increase capacity and use valid local indices",
                self.max_vertices, self.max_indices,
            );
            if cfg!(debug_assertions) {
                panic!("{message}");
            }
            self.geometry_rejections_warned.push(reason);
            warn!("{}", message);
        }
    }

    pub(crate) fn builtin_geometry_supported(&mut self, vertices: usize, indices: usize) -> bool {
        if (vertices <= self.max_vertices && indices <= self.max_indices)
            || (self.state.pipeline.is_none()
                && self.state.draw_mode == DrawMode::Triangles
                && self.max_vertices >= 3
                && self.max_indices >= 3)
        {
            return true;
        }
        self.reject_geometry(vertices, indices, "geometry exceeds draw call capacity");
        false
    }

    pub(crate) fn geometry_builtin_triangles(&mut self, vertices: &[Vertex], indices: &[u16]) {
        if indices.is_empty()
            || (vertices.len() <= self.max_vertices && indices.len() <= self.max_indices)
        {
            // All custom pipelines keep their original indices and draw boundary.
            // They may interpret indices as points/lines, use gl_VertexID or sample
            // _ScreenTexture; repartitioning those calls changes their semantics.
            self.geometry(vertices, indices);
            return;
        }
        if !self.builtin_geometry_supported(vertices.len(), indices.len()) {
            return;
        }
        if indices.len() % 3 != 0
            || indices
                .iter()
                .any(|&index| usize::from(index) >= vertices.len())
        {
            self.reject_geometry(
                vertices.len(),
                indices.len(),
                "built-in geometry requires complete triangles with valid local indices",
            );
            return;
        }
        crate::ui::submit_geometry_chunks(
            vertices,
            indices,
            self.max_vertices,
            self.max_indices,
            |vertices, indices| self.geometry(vertices, indices),
        );
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

    pub(crate) fn drawcall_capacity(&self) -> (usize, usize) {
        (self.max_vertices, self.max_indices)
    }

    pub(crate) fn update_drawcall_capacity(
        &mut self,
        ctx: &mut dyn miniquad::RenderingBackend,
        max_vertices: usize,
        max_indices: usize,
    ) {
        let (max_vertices, max_indices) = drawcall_capacity(max_vertices, max_indices);
        assert_eq!(
            self.draw_calls_count, 0,
            "cannot change draw call capacity with queued geometry; flush with set_default_camera or next_frame first"
        );
        self.max_vertices = max_vertices.min(self.vertex_limit);
        self.max_indices = max_indices;

        for draw_call in &mut self.draw_calls {
            draw_call.indices_start = 0;
            draw_call.vertices_start = 0;
        }
        // Shared buffers are sized by use; reallocate them for the new limits.
        self.release_flush_buffers(ctx);
    }

    fn release_flush_buffers(&mut self, ctx: &mut dyn miniquad::RenderingBackend) {
        for buffers in self.flush_buffers.drain(..) {
            for buffer in buffers.bindings.vertex_buffers {
                ctx.delete_buffer(buffer);
            }
            ctx.delete_buffer(buffers.bindings.index_buffer);
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

#[cfg(test)]
mod flush_plan_tests {
    use super::*;

    // Lay out draw calls exactly as geometry() does: contiguous, local indices.
    fn calls(sizes: &[(usize, usize)], salt: u32) -> (Vec<DrawCall>, Vec<u16>) {
        let mut draw_calls = Vec::new();
        let mut indices = Vec::new();
        let mut vertices = 0;
        let mut state = salt | 1;
        for &(vertex_count, index_count) in sizes {
            let mut dc = DrawCall::new(
                None,
                glam::Mat4::IDENTITY,
                DrawMode::Triangles,
                GlPipeline(0),
                None,
                None,
            );
            dc.vertices_start = vertices;
            dc.indices_start = indices.len();
            dc.vertices_count = vertex_count;
            dc.indices_count = index_count;
            for i in 0..index_count {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                // Always include the call's last vertex to exercise the bound.
                let local = if i == 0 {
                    vertex_count - 1
                } else {
                    state as usize % vertex_count
                };
                indices.push(local as u16);
            }
            vertices += vertex_count;
            draw_calls.push(dc);
        }
        (draw_calls, indices)
    }

    fn check(
        sizes: &[(usize, usize)],
        vertex_limit: usize,
        index_limit: usize,
        salt: u32,
    ) -> FlushPlan {
        let (draw_calls, batch) = calls(sizes, salt);
        let mut plan = FlushPlan::default();
        plan.build(&draw_calls, &batch, vertex_limit, index_limit, false);
        assert_eq!(plan.placements.len(), draw_calls.len());
        assert_eq!(plan.indices.len(), batch.len());
        let (mut next_vertex, mut next_index) = (0, 0);
        for segment in &plan.segments {
            // Segments tile the batch in order and stay addressable.
            assert_eq!(
                (segment.first_vertex, segment.first_index),
                (next_vertex, next_index)
            );
            assert!(segment.vertices <= vertex_limit && segment.indices <= index_limit);
            next_vertex += segment.vertices;
            next_index += segment.indices;
        }
        assert_eq!(
            (next_vertex, next_index),
            (
                draw_calls.iter().map(|dc| dc.vertices_count).sum(),
                batch.len()
            )
        );
        for (dc, &(slot, base)) in draw_calls.iter().zip(&plan.placements) {
            let segment = plan.segments[slot];
            let rebased = &plan.indices[segment.first_index + base..][..dc.indices_count];
            for (&local, &global) in batch[dc.indices_start..][..dc.indices_count]
                .iter()
                .zip(rebased)
            {
                // Each rebased index names the same batch vertex as the local one.
                assert_eq!(
                    segment.first_vertex + usize::from(global),
                    dc.vertices_start + usize::from(local)
                );
                assert!(usize::from(global) < segment.vertices);
            }
        }
        plan
    }

    #[test]
    fn segments_never_exceed_the_webgl2_restart_limit() {
        let plan = check(&[(65_534, 6), (1, 3), (1, 3)], 65_535, 196_608, 1);
        assert_eq!(plan.placements, [(0, 0), (0, 6), (1, 0)]);
        assert_eq!(plan.indices.iter().max(), Some(&65_534));
        let plan = check(&[(65_536, 3), (1, 3)], 65_536, 196_608, 2);
        assert_eq!(plan.placements, [(0, 0), (1, 0)]);
        assert_eq!(plan.indices[0], 65_535);
    }

    #[test]
    fn index_count_also_bounds_a_segment() {
        let plan = check(&[(3, 6), (3, 6), (3, 6)], 65_535, 12, 3);
        assert_eq!(plan.placements, [(0, 0), (0, 6), (1, 0)]);
    }

    #[test]
    fn mixed_call_sequences_rebase_exactly() {
        let mut state = 0x5eed_u32;
        for round in 0..if cfg!(miri) { 6 } else { 64 } {
            let sizes: Vec<_> = (0..1 + round % 17)
                .map(|_| {
                    state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    let vertices = 1 + (state >> 8) as usize % 40_000;
                    (vertices, 1 + (state >> 4) as usize % 300)
                })
                .collect();
            check(
                &sizes,
                if round % 2 == 0 { 65_535 } else { 65_536 },
                196_608,
                round + 7,
            );
        }
    }

    #[test]
    fn metal_keeps_one_call_per_segment() {
        // Metal can only draw from element 0: every call gets its own upload.
        let (draw_calls, batch) = calls(&[(3, 3), (4, 6), (3, 3)], 9);
        let mut plan = FlushPlan::default();
        plan.build(&draw_calls, &batch, 65_535, 196_608, true);
        assert_eq!(plan.placements, [(0, 0), (1, 0), (2, 0)]);
        assert_eq!(plan.indices, batch);
    }

    #[test]
    #[should_panic(expected = "draw call vertices must be contiguous")]
    fn non_contiguous_calls_are_rejected() {
        let (mut draw_calls, batch) = calls(&[(3, 3), (3, 3)], 4);
        draw_calls[1].vertices_start += 1;
        FlushPlan::default().build(&draw_calls, &batch, 65_535, 196_608, false);
    }
}

#[cfg(test)]
mod capacity_validation_tests {
    use super::*;

    #[test]
    fn actual_webgl2_reserves_the_fixed_restart_index() {
        assert_eq!(
            drawcall_vertex_limit("WebGL 2.0 (OpenGL ES 3.0 Chromium)"),
            65_535
        );
        assert_eq!(drawcall_vertex_limit("WebGL 2.0"), 65_535);
        for version in ["WebGL 1.0 (OpenGL ES 2.0 Chromium)", "4.6 Mesa", ""] {
            assert_eq!(drawcall_vertex_limit(version), 65_536);
        }
    }

    #[test]
    fn vertex_domain_and_index_count_have_independent_limits() {
        assert_eq!(drawcall_capacity(0, 0), (0, 0));
        assert_eq!(drawcall_capacity(1, 1), (1, 1));
        assert_eq!(drawcall_capacity(usize::MAX, 70_000), (65_536, 70_000));
        let max_indices = (i32::MAX as usize).min(isize::MAX as usize / 2);
        assert_eq!(
            drawcall_capacity(65_536, max_indices),
            (65_536, max_indices)
        );
        crate::test_backend::assert_panics_with(
            || {
                drawcall_capacity(4, max_indices + 1);
            },
            "draw call index capacity exceeds backend buffer or draw limits",
        );
        crate::test_backend::assert_panics_with(
            || {
                drawcall_capacity(4, usize::MAX);
            },
            "draw call index capacity exceeds backend buffer or draw limits",
        );
    }

    #[test]
    #[should_panic(expected = "draw call index capacity exceeds backend buffer or draw limits")]
    fn invalid_capacity_is_rejected_before_backend_use() {
        let mut backend = crate::test_backend::Recorder {
            deleted: Default::default(),
        };
        QuadGl::new(&mut backend, 4, usize::MAX);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod capacity_tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::{Arc, Mutex};

    struct Finished;
    impl miniquad::EventHandler for Finished {
        fn update(&mut self) {}
        fn draw(&mut self) {}
    }

    fn vertices(count: usize) -> Vec<Vertex> {
        vec![Vertex::new(0., 0., 0., 0., 0., crate::color::WHITE); count]
    }

    fn geometry_rejected(call: impl FnOnce(), message: &str) {
        if cfg!(debug_assertions) {
            crate::test_backend::assert_panics_with(call, message);
        } else {
            call();
        }
    }

    fn case(
        ctx: &mut dyn RenderingBackend,
        failures: &mut Vec<&'static str>,
        name: &'static str,
        capacity: (usize, usize),
        test: impl FnOnce(&mut QuadGl, &mut dyn RenderingBackend),
    ) {
        let mut gl = QuadGl::new(ctx, capacity.0, capacity.1);
        if catch_unwind(AssertUnwindSafe(|| test(&mut gl, ctx))).is_err() {
            failures.push(name);
        }
        gl.shutdown(ctx);
    }

    #[test]
    #[ignore = "requires a graphics display; run this exact test under Xvfb"]
    fn capacity_regressions() {
        // Miniquad permits one display per process. Isolate this test even when
        // a developer runs all ignored tests alongside the live UI test.
        if std::env::var_os("MACROQUAD_CAPACITY_TEST_CHILD").is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "quad_gl::capacity_tests::capacity_regressions",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("MACROQUAD_CAPACITY_TEST_CHILD", "1")
                .status()
                .unwrap();
            assert!(
                status.success(),
                "isolated capacity regression failed: {status}"
            );
            return;
        }
        let failures = Arc::new(Mutex::new(Vec::new()));
        let output = failures.clone();
        miniquad::start(
            miniquad::conf::Conf {
                window_title: "draw capacity regressions".into(),
                window_width: 64,
                window_height: 64,
                ..Default::default()
            },
            move || {
                let mut ctx = miniquad::window::new_rendering_backend();
                let mut failures = output.lock().unwrap();
                case(
                    &mut *ctx,
                    &mut failures,
                    "exact fit and one over",
                    (8, 12),
                    |gl, _| {
                        for _ in 0..2 {
                            gl.geometry(&vertices(4), &[0, 1, 2, 0, 2, 3]);
                        }
                        assert_eq!(gl.draw_calls_count, 1);
                        assert_eq!(gl.batch_index_buffer, [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
                        gl.geometry(&vertices(4), &[0, 1, 2, 0, 2, 3]);
                        assert_eq!(gl.draw_calls_count, 2);
                        assert_eq!(gl.draw_calls[1].vertices_count, 4);
                    },
                );
                case(&mut *ctx, &mut failures, "line fit", (4, 4), |gl, ctx| {
                    gl.geometry(&vertices(3), &[0, 1, 2]);
                    gl.draw(ctx, glam::Mat4::IDENTITY);
                    gl.draw_mode(DrawMode::Lines);
                    gl.geometry(&vertices(2), &[0, 1]);
                    gl.geometry(&vertices(2), &[0, 1]);
                    assert_eq!(gl.draw_calls_count, 1);
                    assert_eq!(gl.batch_index_buffer, [0, 1, 2, 3]);
                });
                case(
                    &mut *ctx,
                    &mut failures,
                    "oversized is atomic",
                    (4, 6),
                    |gl, _| {
                        gl.geometry(&vertices(3), &[0, 1, 2]);
                        for (v, i) in [(vertices(5), vec![0, 1, 4]), (vertices(3), vec![0; 7])] {
                            geometry_rejected(
                                || gl.geometry(&v, &i),
                                "geometry exceeds draw call capacity",
                            );
                            assert_eq!(
                                (
                                    gl.draw_calls_count,
                                    gl.batch_vertex_buffer.len(),
                                    gl.batch_index_buffer.len()
                                ),
                                (1, 3, 3)
                            );
                        }
                    },
                );
                case(
                    &mut *ctx,
                    &mut failures,
                    "invalid local index is atomic",
                    (8, 12),
                    |gl, _| {
                        gl.geometry(&vertices(3), &[0, 1, 2]);
                        geometry_rejected(
                            || gl.geometry(&vertices(3), &[0, 1, 3]),
                            "geometry index is outside its vertex slice",
                        );
                        assert_eq!(
                            (
                                gl.draw_calls_count,
                                gl.batch_vertex_buffer.len(),
                                gl.batch_index_buffer.len()
                            ),
                            (1, 3, 3)
                        );
                    },
                );
                case(
                    &mut *ctx,
                    &mut failures,
                    "full u16 domain and rebase split",
                    (100_000, 100_000),
                    |gl, ctx| {
                        gl.geometry(&vertices(65_532), &[0, 1, 65_531]);
                        gl.geometry(&vertices(4), &[0, 1, 2, 0, 2, 3]);
                        assert_eq!(gl.draw_calls_count, 1);
                        assert_eq!(
                            &gl.batch_index_buffer[3..],
                            [65_532, 65_533, 65_534, 65_532, 65_534, 65_535]
                        );
                        gl.geometry(&vertices(3), &[0, 1, 2]);
                        assert_eq!(gl.draw_calls_count, 2);
                        assert_eq!(&gl.batch_index_buffer[9..], [0, 1, 2]);
                        gl.draw(ctx, glam::Mat4::IDENTITY);
                        // A full u16 domain fills one shared segment; the next call
                        // starts another rather than wrapping its rebased indices.
                        assert_eq!(
                            gl.flush_plan.segments,
                            [
                                FlushSegment {
                                    first_vertex: 0,
                                    vertices: 65_536,
                                    first_index: 0,
                                    indices: 9
                                },
                                FlushSegment {
                                    first_vertex: 65_536,
                                    vertices: 3,
                                    first_index: 9,
                                    indices: 3
                                },
                            ]
                        );
                        assert_eq!(gl.flush_plan.placements, [(0, 0), (1, 0)]);
                        assert_eq!(&gl.flush_plan.indices[9..], [0, 1, 2]);
                        assert_eq!(gl.flush_buffers.len(), 2);
                        for buffers in &gl.flush_buffers {
                            assert_eq!(
                                ctx.buffer_size(buffers.bindings.vertex_buffers[0]),
                                65_536 * std::mem::size_of::<Vertex>()
                            );
                        }
                    },
                );
                case(
                    &mut *ctx,
                    &mut failures,
                    "index count exceeds u16 domain",
                    (3, 70_000),
                    |gl, ctx| {
                        gl.geometry(&vertices(3), &vec![0; 69_999]);
                        assert_eq!(gl.draw_calls[0].indices_count, 69_999);
                        gl.draw(ctx, glam::Mat4::IDENTITY);
                        assert_eq!(
                            ctx.buffer_size(gl.flush_buffers[0].bindings.index_buffer),
                            140_000
                        );
                    },
                );
                case(
                    &mut *ctx,
                    &mut failures,
                    "zero empty and tiny capacities",
                    (0, 0),
                    |gl, ctx| {
                        gl.geometry(&[], &[]);
                        gl.geometry(&vertices(3), &[]);
                        assert_eq!(gl.draw_calls_count, 0);
                        geometry_rejected(
                            || gl.geometry(&[], &[0]),
                            "geometry exceeds draw call capacity",
                        );
                        gl.update_drawcall_capacity(ctx, 1, 1);
                        gl.geometry(&vertices(1), &[0]);
                        assert_eq!(gl.batch_index_buffer, [0]);
                    },
                );
                case(
                    &mut *ctx,
                    &mut failures,
                    "queued setter is atomic and later succeeds",
                    (8, 12),
                    |gl, ctx| {
                        gl.geometry(&vertices(3), &[0, 1, 2]);
                        crate::test_backend::assert_panics_with(
                            || gl.update_drawcall_capacity(ctx, 4, 6),
                            "cannot change draw call capacity with queued geometry",
                        );
                        assert_eq!(
                            (
                                gl.max_vertices,
                                gl.max_indices,
                                gl.draw_calls_count,
                                gl.batch_vertex_buffer.len()
                            ),
                            (8, 12, 1, 3)
                        );
                        gl.draw(ctx, glam::Mat4::IDENTITY);
                        gl.update_drawcall_capacity(ctx, 4, 6);
                        gl.geometry(&vertices(4), &[0, 1, 2, 0, 2, 3]);
                        gl.draw(ctx, glam::Mat4::IDENTITY);
                        assert_eq!(
                            ctx.buffer_size(gl.flush_buffers[0].bindings.vertex_buffers[0]),
                            4 * std::mem::size_of::<Vertex>()
                        );
                    },
                );
                case(
                    &mut *ctx,
                    &mut failures,
                    "unrepresentable index capacity is atomic",
                    (4, 6),
                    |gl, ctx| {
                        for count in [i32::MAX as usize + 1, usize::MAX] {
                            crate::test_backend::assert_panics_with(
                                || gl.update_drawcall_capacity(ctx, 4, count),
                                "draw call index capacity exceeds backend buffer or draw limits",
                            );
                            assert_eq!((gl.max_vertices, gl.max_indices), (4, 6));
                        }
                    },
                );
                miniquad::window::quit();
                Box::new(Finished)
            },
        );
        let failures = failures.lock().unwrap();
        assert!(failures.is_empty(), "failed capacity cases: {failures:?}");
    }

    #[test]
    #[ignore = "requires a graphics display; run this exact test under Xvfb"]
    fn public_capacity_regressions() {
        crate::test::run(
            concat!(module_path!(), "::public_capacity_regressions"),
            || {
                crate::Window::from_config(
                    miniquad::conf::Conf {
                        window_title: "Public capacity regressions".into(),
                        window_width: 64,
                        window_height: 64,
                        ..Default::default()
                    },
                    async {
                        use crate::{
                            camera::{set_camera, Camera, Camera2D},
                            color::WHITE,
                            math::Rect,
                            shapes::{draw_arc, draw_rectangle},
                            texture::render_target,
                            window::gl_set_drawcall_buffer_capacity,
                        };
                        let mut failed = Vec::new();
                        for (name, arc, capacity) in [
                            ("default large arc", 1177_f32, (10_000, 5_000)),
                            (
                                "arc beyond source u16 domain",
                                24_001_f32,
                                (100_000, 100_000),
                            ),
                        ] {
                            gl_set_drawcall_buffer_capacity(capacity.0, capacity.1);
                            if catch_unwind(AssertUnwindSafe(|| {
                                draw_arc(0., 0., 255, 8., 0., 2., arc, WHITE);
                                crate::with_context(|context| {
                                    let gl = &context.gl;
                                    let part = arc.to_radians();
                                    let segments =
                                        (255_f32 * part / std::f32::consts::TAU).ceil() as usize;
                                    let span = part / segments as f32;
                                    assert_eq!(gl.batch_index_buffer.len(), segments * 6);
                                    assert!(gl.draw_calls_count >= 2);
                                    let mut emitted = 0;
                                    for dc in &gl.draw_calls[..gl.draw_calls_count] {
                                        for &index in &gl.batch_index_buffer
                                            [dc.indices_start..dc.indices_start + dc.indices_count]
                                        {
                                            assert!(usize::from(index) < dc.vertices_count);
                                            let segment = emitted / 6;
                                            let corner = [0, 1, 2, 2, 1, 3][emitted % 6];
                                            let start_angle = segment as f32 * span;
                                            let angle = if corner < 2 {
                                                start_angle
                                            } else {
                                                start_angle + span
                                            };
                                            let radius = if corner % 2 == 0 { 8. } else { 10. };
                                            let expected = glam::Vec2::from_angle(angle) * radius;
                                            let actual = gl.batch_vertex_buffer
                                                [dc.vertices_start + usize::from(index)]
                                            .position;
                                            assert!(
                                                (actual.truncate() - expected).length() < 0.0001
                                            );
                                            emitted += 1;
                                        }
                                    }
                                    assert_eq!(emitted, segments * 6);
                                });
                            }))
                            .is_err()
                            {
                                failed.push(name);
                            }
                            crate::with_context(|context| context.gl.clear_draw_calls());
                        }
                        gl_set_drawcall_buffer_capacity(3, 3);
                        if catch_unwind(AssertUnwindSafe(|| {
                            crate::with_context(|context| context.gl.capture(true));
                            draw_rectangle(0., 0., 16., 16., WHITE);
                            crate::with_context(|context| {
                                assert_eq!(context.gl.batch_index_buffer.len(), 6);
                                assert_eq!(context.gl.draw_calls_count, 2);
                                assert!(context.gl.draw_calls[..2].iter().all(|dc| dc.capture));
                            });
                            crate::with_context(|context| context.perform_render_passes());
                            let captured = crate::telemetry::drawcalls();
                            assert_eq!(captured.len(), 2);
                            assert!(captured.iter().all(|draw| draw.indices_count == 3));
                        }))
                        .is_err()
                        {
                            failed.push("tiny default built-in with capture");
                        }
                        crate::with_context(|context| {
                            context.gl.clear_draw_calls();
                            context.gl.capture(false);
                        });
                        for primitive in [
                            PrimitiveType::Triangles,
                            PrimitiveType::Lines,
                            PrimitiveType::Points,
                        ] {
                            gl_set_drawcall_buffer_capacity(4, 6);
                            let material = crate::material::load_material(
                                ShaderSource::Glsl {
                                    vertex: "#version 100\nattribute vec3 position; uniform mat4 Projection; uniform mat4 Model; void main(){gl_Position=Projection*Model*vec4(position,1.);gl_PointSize=1.;}",
                                    fragment: "#version 100\nprecision mediump float; uniform sampler2D _ScreenTexture; void main(){gl_FragColor=texture2D(_ScreenTexture,vec2(0.5));}",
                                },
                                crate::material::MaterialParams {pipeline_params: PipelineParams {primitive_type: primitive, ..Default::default()}, ..Default::default()},
                            ).unwrap();
                            crate::material::gl_use_material(&material);
                            draw_rectangle(0., 0., 16., 16., WHITE);
                            geometry_rejected(
                                || draw_arc(0., 0., 255, 8., 0., 2., 1177., WHITE),
                                "geometry exceeds draw call capacity",
                            );
                            crate::with_context(|context| {
                                assert_eq!(context.gl.draw_calls_count, 1);
                                assert_eq!(context.gl.batch_vertex_buffer.len(), 4);
                                assert_eq!(context.gl.batch_index_buffer, [0, 1, 2, 0, 2, 3]);
                                assert!(context.gl.state.pipeline.is_some());
                                // Repeated custom-material rejections warn once.
                                assert_eq!(
                                    context.gl.geometry_rejections_warned,
                                    if cfg!(debug_assertions) {
                                        vec![]
                                    } else {
                                        vec!["geometry exceeds draw call capacity"]
                                    }
                                );
                                context.gl.clear_draw_calls();
                            });
                            crate::material::gl_use_default_material();
                        }
                        gl_set_drawcall_buffer_capacity(8, 12);
                        let target = render_target(64, 64);
                        let mut camera = Camera2D::from_display_rect(Rect::new(10., 20., 32., 32.));
                        camera.render_target = Some(target);
                        camera.viewport = Some((8, 8, 32, 32));
                        set_camera(&camera);
                        let model = glam::Mat4::from_translation(glam::vec3(2., 3., 0.));
                        crate::with_context(|context| {
                            context.gl.push_model_matrix(model);
                            context.gl.scissor(Some((4, 5, 40, 41)));
                        });
                        draw_rectangle(10., 20., 16., 16., WHITE);
                        crate::test_backend::assert_panics_with(
                            || gl_set_drawcall_buffer_capacity(4, usize::MAX),
                            "draw call index capacity exceeds backend buffer or draw limits",
                        );
                        crate::with_context(|context| {
                            assert_eq!(context.gl.draw_calls_count, 1);
                            assert_eq!(context.gl.drawcall_capacity(), (8, 12));
                        });
                        if catch_unwind(AssertUnwindSafe(|| {
                            gl_set_drawcall_buffer_capacity(4, 6);
                            crate::with_context(|context| {
                                assert_eq!(context.gl.draw_calls_count, 0);
                                assert_eq!(context.gl.drawcall_capacity(), (4, 6));
                                assert_eq!(context.camera_matrix, Some(camera.matrix()));
                                assert_eq!(context.gl.state.viewport, camera.viewport);
                                assert_eq!(context.gl.state.render_pass, camera.render_pass());
                                assert_eq!(context.gl.state.model(), model);
                                assert_eq!(context.gl.state.clip, Some((4, 5, 40, 41)));
                            });
                        }))
                        .is_err()
                        {
                            failed.push("queued public setter preserves state");
                        }
                        crate::with_context(|context| context.gl.clear_draw_calls());
                        // An MSAA target's texture is its resolve copy. Drawing it into
                        // the same target within one flush must see earlier calls.
                        gl_set_drawcall_buffer_capacity(10_000, 5_000);
                        // Drop the model matrix and scissor left by the case above.
                        crate::with_context(|context| context.gl.reset());
                        if catch_unwind(AssertUnwindSafe(|| {
                            let red = crate::color::Color::new(1., 0., 0., 1.);
                            let msaa = crate::texture::render_target_ex(
                                8,
                                8,
                                crate::texture::RenderTargetParams {
                                    sample_count: 4,
                                    depth: false,
                                },
                            );
                            let mut camera = Camera2D::from_display_rect(Rect::new(0., 0., 8., 8.));
                            camera.render_target = Some(msaa.clone());
                            set_camera(&camera);
                            crate::window::clear_background(crate::color::BLACK);
                            draw_rectangle(0., 0., 4., 8., red);
                            crate::texture::draw_texture_ex(
                                &msaa.texture,
                                4.,
                                0.,
                                WHITE,
                                crate::texture::DrawTextureParams {
                                    source: Some(Rect::new(0., 0., 4., 8.)),
                                    ..Default::default()
                                },
                            );
                            crate::camera::set_default_camera();
                            let image = msaa.texture.get_texture_data();
                            for y in 0..8 {
                                for x in 0..8 {
                                    let pixel = image.get_pixel(x, y);
                                    assert_eq!(
                                        (pixel.r, pixel.g, pixel.b),
                                        (1., 0., 0.),
                                        "pixel {x},{y}"
                                    );
                                }
                            }
                        }))
                        .is_err()
                        {
                            failed.push("msaa target samples its own earlier calls");
                        }
                        assert!(
                            failed.is_empty(),
                            "failed public capacity cases: {failed:?}"
                        );
                    },
                );
            },
        );
    }
}
