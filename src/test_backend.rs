//! CPU-only backend recorder for ownership regression tests.
#![allow(unused_variables)]
use miniquad::*;
use std::{cell::RefCell, rc::Rc};
pub(crate) struct Recorder {
    pub deleted: Rc<RefCell<Vec<TextureId>>>,
}
impl RenderingBackend for Recorder {
    unsafe fn texture_raw_id(&self, _: TextureId) -> RawId {
        unimplemented!("Raw GPU access in CPU ownership test")
    }
    fn info(&self) -> ContextInfo {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn new_shader(
        &mut self,
        shader: ShaderSource,
        meta: ShaderMeta,
    ) -> Result<ShaderId, ShaderError> {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn new_texture(
        &mut self,
        access: TextureAccess,
        data: TextureSource,
        params: TextureParams,
    ) -> TextureId {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn texture_params(&self, texture: TextureId) -> TextureParams {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn texture_set_min_filter(
        &mut self,
        texture: TextureId,
        filter: FilterMode,
        mipmap_filter: MipmapFilterMode,
    ) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn texture_set_mag_filter(&mut self, texture: TextureId, filter: FilterMode) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn texture_set_wrap(&mut self, texture: TextureId, wrap_x: TextureWrap, wrap_y: TextureWrap) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn texture_generate_mipmaps(&mut self, texture: TextureId) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn texture_resize(
        &mut self,
        texture: TextureId,
        width: u32,
        height: u32,
        bytes: Option<&[u8]>,
    ) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn texture_read_pixels(&mut self, texture: TextureId, bytes: &mut [u8]) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn texture_update_part(
        &mut self,
        texture: TextureId,
        x_offset: i32,
        y_offset: i32,
        width: i32,
        height: i32,
        bytes: &[u8],
    ) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn new_render_pass_mrt(
        &mut self,
        color_img: &[TextureId],
        resolve_img: Option<&[TextureId]>,
        depth_img: Option<TextureId>,
    ) -> RenderPass {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn render_pass_color_attachments(&self, render_pass: RenderPass) -> &[TextureId] {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn delete_render_pass(&mut self, render_pass: RenderPass) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn new_pipeline(
        &mut self,
        buffer_layout: &[BufferLayout],
        attributes: &[VertexAttribute],
        shader: ShaderId,
        params: PipelineParams,
    ) -> Pipeline {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn apply_pipeline(&mut self, pipeline: &Pipeline) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn delete_pipeline(&mut self, pipeline: Pipeline) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn new_buffer(
        &mut self,
        type_: BufferType,
        usage: BufferUsage,
        data: BufferSource,
    ) -> BufferId {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn buffer_update(&mut self, buffer: BufferId, data: BufferSource) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn buffer_size(&mut self, buffer: BufferId) -> usize {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn delete_buffer(&mut self, buffer: BufferId) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn delete_texture(&mut self, texture: TextureId) {
        self.deleted.borrow_mut().push(texture)
    }
    fn delete_shader(&mut self, program: ShaderId) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn apply_viewport(&mut self, x: i32, y: i32, w: i32, h: i32) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn apply_scissor_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn apply_bindings_from_slice(
        &mut self,
        vertex_buffers: &[BufferId],
        index_buffer: BufferId,
        textures: &[TextureId],
    ) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn apply_uniforms_from_bytes(&mut self, uniform_ptr: *const u8, size: usize) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn clear(
        &mut self,
        color: Option<(f32, f32, f32, f32)>,
        depth: Option<f32>,
        stencil: Option<i32>,
    ) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn begin_default_pass(&mut self, action: PassAction) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn begin_pass(&mut self, pass: Option<RenderPass>, action: PassAction) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn end_render_pass(&mut self) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn commit_frame(&mut self) {
        unimplemented!("GPU operation in CPU ownership test")
    }
    fn draw(&self, base_element: i32, num_elements: i32, num_instances: i32) {
        unimplemented!("GPU operation in CPU ownership test")
    }
}
