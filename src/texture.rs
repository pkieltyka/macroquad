//! Loading and rendering textures. Also render textures, per-pixel image manipulations.

use crate::{
    color::Color, file::load_file, get_context, get_quad_context, math::Rect,
    text::atlas::SpriteKey, Error,
};

pub use crate::quad_gl::FilterMode;
use crate::quad_gl::{DrawMode, Vertex};
use glam::{vec2, Vec2};
use slotmap::{TextureIdSlotMap, TextureSlotId};
use std::sync::Arc;

mod slotmap;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextureSlotGuarded(pub TextureSlotId);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TextureHandle {
    // texture that belongs to macroquad and follows normal garbage collection rules
    Managed(Arc<TextureSlotGuarded>),
    ManagedWeak(TextureSlotId),
    // raw miniquad texture, there are no guarantees that this texture is not yet deleted
    Unmanaged(miniquad::TextureId),
}

pub(crate) struct TexturesContext {
    textures: TextureIdSlotMap,
    removed: Vec<TextureSlotId>,
    removed_render_passes: Vec<miniquad::RenderPass>,
}
impl TexturesContext {
    pub fn new() -> TexturesContext {
        TexturesContext {
            textures: TextureIdSlotMap::new(),
            removed: Vec::with_capacity(200),
            removed_render_passes: Vec::with_capacity(10),
        }
    }
    fn schedule_removed(&mut self, texture: TextureSlotId) {
        self.removed.push(texture);
    }
    fn schedule_render_pass_removed(&mut self, pass: miniquad::RenderPass) {
        self.removed_render_passes.push(pass);
    }
    fn store_texture(&mut self, texture: miniquad::TextureId) -> TextureHandle {
        TextureHandle::Managed(Arc::new(TextureSlotGuarded(self.textures.insert(texture))))
    }
    pub fn texture(&self, texture: TextureSlotId) -> Option<miniquad::TextureId> {
        self.textures.get(texture)
    }
    // fn remove(&mut self, texture: TextureSlotId) {
    //     self.textures.remove(texture);
    // }
    pub const fn len(&self) -> usize {
        self.textures.len()
    }
    pub fn garbage_collect(&mut self, ctx: &mut miniquad::Context) {
        // Delete RenderPasses first, then textures (safer for attachments/FBOs)
        for pass in self.removed_render_passes.drain(0..) {
            ctx.delete_render_pass(pass);
        }

        for texture in self.removed.drain(0..) {
            if let Some(texture) = self.textures.get(texture) {
                ctx.delete_texture(texture);
            }
            self.textures.remove(texture);
        }
    }
}

/// Image, data stored in CPU memory
///
/// Dimensions and storage are validated together by constructors.
/// ```compile_fail
/// let mut image = macroquad::texture::Image::empty();
/// image.width = 1;
/// ```
#[derive(Clone)]
pub struct Image {
    pub(crate) bytes: Vec<u8>,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes.len()", &self.bytes.len())
            .finish()
    }
}

/// Checked RGBA layout shared by CPU images and Miniquad transfers.
/// Miniquad computes format sizes in u32, and Rust slices are limited to isize::MAX.
fn rgba_len(width: u32, height: u32) -> Result<usize, Error> {
    let length = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(Error::UnknownError("RGBA dimensions overflow"))?;
    if length > u32::MAX as u64 || length > isize::MAX as u64 {
        return Err(Error::UnknownError(
            "RGBA buffer exceeds supported slice size",
        ));
    }
    Ok(length as usize)
}

fn image_dimensions(width: u32, height: u32) -> Result<(u16, u16, usize), Error> {
    let width = u16::try_from(width).map_err(|_| Error::UnknownError("Image width exceeds u16"))?;
    let height =
        u16::try_from(height).map_err(|_| Error::UnknownError("Image height exceeds u16"))?;
    Ok((width, height, rgba_len(width as u32, height as u32)?))
}

fn validate_rgba(width: u32, height: u32, bytes: &[u8]) {
    assert_eq!(
        rgba_len(width, height).expect("Invalid RGBA dimensions"),
        bytes.len(),
        "RGBA buffer length does not match dimensions"
    );
}

fn validate_region(
    texture_width: u32,
    texture_height: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    bytes: &[u8],
) {
    assert!(
        x >= 0 && y >= 0 && width >= 0 && height >= 0,
        "Negative texture region"
    );
    let (x, y, width, height) = (x as u32, y as u32, width as u32, height as u32);
    assert!(
        x.checked_add(width).is_some_and(|end| end <= texture_width),
        "Texture region exceeds width"
    );
    assert!(
        y.checked_add(height)
            .is_some_and(|end| end <= texture_height),
        "Texture region exceeds height"
    );
    validate_rgba(width, height, bytes);
}

impl Image {
    /// Constructs an image from exactly width * height RGBA pixels.
    pub fn from_rgba8(width: u16, height: u16, bytes: Vec<u8>) -> Result<Image, Error> {
        let expected = rgba_len(width as u32, height as u32)?;
        if bytes.len() != expected {
            return Err(Error::UnknownError(
                "RGBA buffer length does not match dimensions",
            ));
        }
        Ok(Image {
            width,
            height,
            bytes,
        })
    }

    /// Returns the RGBA bytes. Dimensions cannot be changed independently.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns mutable RGBA bytes without permitting resizing.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    /// Creates an empty Image.
    ///
    /// ```
    /// # use macroquad::prelude::*;
    /// let image = Image::empty();
    /// ```
    pub const fn empty() -> Image {
        Image {
            width: 0,
            height: 0,
            bytes: vec![],
        }
    }

    /// Creates an Image from a slice of bytes that contains an encoded image.
    ///
    /// If `format` is None, it will make an educated guess on the
    /// [ImageFormat][image::ImageFormat].
    ///
    /// # Example
    ///
    /// ```
    /// # use macroquad::prelude::*;
    /// let icon = Image::from_file_with_format(
    ///     include_bytes!("../examples/rust.png"),
    ///     Some(ImageFormat::Png),
    ///     );
    /// ```
    pub fn from_file_with_format(
        bytes: &[u8],
        format: Option<image::ImageFormat>,
    ) -> Result<Image, Error> {
        let img = if let Some(fmt) = format {
            image::load_from_memory_with_format(bytes, fmt)?.to_rgba8()
        } else {
            image::load_from_memory(bytes)?.to_rgba8()
        };
        let (width, height, _) = image_dimensions(img.width(), img.height())?;
        Self::from_rgba8(width, height, img.into_raw())
    }

    /// Creates an Image filled with the provided [Color].
    pub fn gen_image_color(width: u16, height: u16, color: Color) -> Image {
        let mut bytes =
            vec![0; rgba_len(width as u32, height as u32).expect("Invalid image dimensions")];
        for i in 0..width as usize * height as usize {
            bytes[i * 4 + 0] = (color.r * 255.) as u8;
            bytes[i * 4 + 1] = (color.g * 255.) as u8;
            bytes[i * 4 + 2] = (color.b * 255.) as u8;
            bytes[i * 4 + 3] = (color.a * 255.) as u8;
        }
        Image {
            width,
            height,
            bytes,
        }
    }

    /// Updates this image from a slice of [Color]s.
    pub fn update(&mut self, colors: &[Color]) {
        assert!(self.width as usize * self.height as usize == colors.len());

        for i in 0..colors.len() {
            self.bytes[i * 4] = (colors[i].r * 255.) as u8;
            self.bytes[i * 4 + 1] = (colors[i].g * 255.) as u8;
            self.bytes[i * 4 + 2] = (colors[i].b * 255.) as u8;
            self.bytes[i * 4 + 3] = (colors[i].a * 255.) as u8;
        }
    }

    /// Returns the width of this image.
    pub const fn width(&self) -> usize {
        self.width as usize
    }

    /// Returns the height of this image.
    pub const fn height(&self) -> usize {
        self.height as usize
    }

    /// Returns this image's data as a slice of 4-byte arrays.
    pub fn get_image_data(&self) -> &[[u8; 4]] {
        validate_rgba(self.width as u32, self.height as u32, &self.bytes);
        self.bytes.as_chunks::<4>().0
    }

    /// Returns this image's data as a mutable slice of 4-byte arrays.
    pub fn get_image_data_mut(&mut self) -> &mut [[u8; 4]] {
        validate_rgba(self.width as u32, self.height as u32, &self.bytes);
        self.bytes.as_chunks_mut::<4>().0
    }

    /// Modifies a pixel [Color] in this image.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        assert!(x < self.width as u32);
        assert!(y < self.height as u32);

        let width = self.width;

        self.get_image_data_mut()[(y * width as u32 + x) as usize] = color.into();
    }

    /// Returns a pixel [Color] from this image.
    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        assert!(
            x < self.width as u32 && y < self.height as u32,
            "Pixel outside image"
        );
        self.get_image_data()[(y * self.width as u32 + x) as usize].into()
    }

    /// Returns an Image from a rect inside this image.
    pub fn sub_image(&self, rect: Rect) -> Image {
        assert!(
            [rect.x, rect.y, rect.w, rect.h]
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0 && value.fract() == 0.0),
            "Image region must contain nonnegative integer coordinates"
        );
        assert!(
            rect.x <= self.width as f32 && rect.y <= self.height as f32,
            "Image region starts outside image"
        );
        assert!(
            rect.w <= self.width as f32 - rect.x && rect.h <= self.height as f32 - rect.y,
            "Image region exceeds image"
        );
        let (x, y, width, height) = (
            rect.x as usize,
            rect.y as usize,
            rect.w as usize,
            rect.h as usize,
        );
        let mut bytes = Vec::with_capacity(
            rgba_len(width as u32, height as u32).expect("Invalid image region"),
        );
        for row in y..y + height {
            let start = (row * self.width as usize + x) * 4;
            bytes.extend_from_slice(&self.bytes[start..start + width * 4]);
        }
        Image::from_rgba8(width as u16, height as u16, bytes).expect("Invalid image region")
    }

    /// Blends this image with another image (of identical dimensions)
    /// Inspired by  OpenCV saturated blending
    pub fn blend(&mut self, other: &Image) {
        assert!(
            self.width as usize * self.height as usize
                == other.width as usize * other.height as usize
        );

        for i in 0..self.bytes.len() / 4 {
            let c1: Color = Color {
                r: self.bytes[i * 4] as f32 / 255.,
                g: self.bytes[i * 4 + 1] as f32 / 255.,
                b: self.bytes[i * 4 + 2] as f32 / 255.,
                a: self.bytes[i * 4 + 3] as f32 / 255.,
            };
            let c2: Color = Color {
                r: other.bytes[i * 4] as f32 / 255.,
                g: other.bytes[i * 4 + 1] as f32 / 255.,
                b: other.bytes[i * 4 + 2] as f32 / 255.,
                a: other.bytes[i * 4 + 3] as f32 / 255.,
            };
            let new_color: Color = Color {
                r: f32::min(c1.r * c1.a + c2.r * c2.a, 1.),
                g: f32::min(c1.g * c1.a + c2.g * c2.a, 1.),
                b: f32::min(c1.b * c1.a + c2.b * c2.a, 1.),
                a: f32::max(c1.a, c2.a) + (1. - f32::max(c1.a, c2.a)) * f32::min(c1.a, c2.a),
            };
            self.bytes[i * 4] = (new_color.r * 255.) as u8;
            self.bytes[i * 4 + 1] = (new_color.g * 255.) as u8;
            self.bytes[i * 4 + 2] = (new_color.b * 255.) as u8;
            self.bytes[i * 4 + 3] = (new_color.a * 255.) as u8;
        }
    }

    /// Overlays an image on top of this one.
    /// Slightly different from blending two images,
    /// overlaying a completely transparent image has no effect
    /// on the original image, though blending them would.
    pub fn overlay(&mut self, other: &Image) {
        assert!(
            self.width as usize * self.height as usize
                == other.width as usize * other.height as usize
        );

        for i in 0..self.bytes.len() / 4 {
            let c1: Color = Color {
                r: self.bytes[i * 4] as f32 / 255.,
                g: self.bytes[i * 4 + 1] as f32 / 255.,
                b: self.bytes[i * 4 + 2] as f32 / 255.,
                a: self.bytes[i * 4 + 3] as f32 / 255.,
            };
            let c2: Color = Color {
                r: other.bytes[i * 4] as f32 / 255.,
                g: other.bytes[i * 4 + 1] as f32 / 255.,
                b: other.bytes[i * 4 + 2] as f32 / 255.,
                a: other.bytes[i * 4 + 3] as f32 / 255.,
            };
            let new_color: Color = Color {
                r: f32::min(c1.r * (1. - c2.a) + c2.r * c2.a, 1.),
                g: f32::min(c1.g * (1. - c2.a) + c2.g * c2.a, 1.),
                b: f32::min(c1.b * (1. - c2.a) + c2.b * c2.a, 1.),
                a: f32::min(c1.a + c2.a, 1.),
            };

            self.bytes[i * 4] = (new_color.r * 255.) as u8;
            self.bytes[i * 4 + 1] = (new_color.g * 255.) as u8;
            self.bytes[i * 4 + 2] = (new_color.b * 255.) as u8;
            self.bytes[i * 4 + 3] = (new_color.a * 255.) as u8;
        }
    }

    /// Saves this image as a PNG file.
    /// This method is not supported on web and will panic.
    pub fn export_png(&self, path: &str) {
        let mut bytes = vec![
            0;
            rgba_len(self.width as u32, self.height as u32)
                .expect("Invalid image dimensions")
        ];

        // flip the image before saving
        for y in 0..self.height as usize {
            for x in 0..self.width as usize * 4 {
                bytes[y * self.width as usize * 4 + x] =
                    self.bytes[(self.height as usize - y - 1) * self.width as usize * 4 + x];
            }
        }

        image::save_buffer(
            path,
            &bytes[..],
            self.width as _,
            self.height as _,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

/// Loads an [Image] from a file into CPU memory.
pub async fn load_image(path: &str) -> Result<Image, Error> {
    let bytes = load_file(path).await?;

    Image::from_file_with_format(&bytes, None)
}

/// Loads a [Texture2D] from a file into GPU memory.
pub async fn load_texture(path: &str) -> Result<Texture2D, Error> {
    let bytes = load_file(path).await?;

    Ok(Texture2D::from_file_with_format(&bytes[..], None))
}

#[derive(Debug, Clone)]
pub struct RenderPass {
    pub color_texture: Texture2D,
    pub depth_texture: Option<Texture2D>,
    pub(crate) render_pass: Arc<miniquad::RenderPass>,
}

#[derive(Debug, Clone)]
pub struct RenderTargetParams {
    /// 1 means no multi sampling.
    /// Note that sample_count > 1 is not supported on GL2, GLES2 and WebGL1
    pub sample_count: i32,

    /// depth: true creates a depth render target attachment and allows
    /// such a render target being used for a depth-testing cameras
    pub depth: bool,
}
impl Default for RenderTargetParams {
    fn default() -> RenderTargetParams {
        RenderTargetParams {
            sample_count: 1,
            depth: false,
        }
    }
}

impl RenderPass {
    /// Returns the miniquad handle for this render pass.
    pub(crate) fn raw_miniquad_id(&self) -> miniquad::RenderPass {
        *self.render_pass
    }
}

impl Drop for RenderPass {
    fn drop(&mut self) {
        // Safety: if strong_count < 2, this is the last strong ref.
        // No new strong references can be created after this point.
        if Arc::strong_count(&self.render_pass) < 2 {
            let ctx = get_context();
            ctx.textures.schedule_render_pass_removed(*self.render_pass);
        }
    }
}

#[derive(Clone, Debug)]
pub struct RenderTarget {
    pub texture: Texture2D,
    pub render_pass: RenderPass,
}

/// A shortcut to create a render target with sample_count: 1 and no depth buffer
pub fn render_target(width: u32, height: u32) -> RenderTarget {
    render_target_ex(width, height, RenderTargetParams::default())
}

/// A shortcut to create a render target with no depth buffer and `sample_count: 4`
pub fn render_target_msaa(width: u32, height: u32) -> RenderTarget {
    render_target_ex(
        width,
        height,
        RenderTargetParams {
            sample_count: 4,
            ..Default::default()
        },
    )
}

pub fn render_target_ex(width: u32, height: u32, params: RenderTargetParams) -> RenderTarget {
    image_dimensions(width, height).expect("Invalid render target dimensions");
    assert!(width > 0 && height > 0, "Empty render target");
    assert!(params.sample_count >= 1, "Invalid sample count");
    let context = get_context();

    let color_texture = get_quad_context().new_render_texture(miniquad::TextureParams {
        width,
        height,
        sample_count: params.sample_count,
        ..Default::default()
    });
    let depth_texture = if params.depth {
        Some(
            get_quad_context().new_render_texture(miniquad::TextureParams {
                width,
                height,
                format: miniquad::TextureFormat::Depth,
                sample_count: params.sample_count,
                ..Default::default()
            }),
        )
    } else {
        None
    };
    let render_pass;
    let texture;
    if params.sample_count > 1 {
        let color_resolve_texture =
            get_quad_context().new_render_texture(miniquad::TextureParams {
                width,
                height,
                ..Default::default()
            });
        render_pass = get_quad_context().new_render_pass_mrt(
            &[color_texture],
            Some(&[color_resolve_texture]),
            depth_texture,
        );
        texture = color_resolve_texture;
    } else {
        render_pass = get_quad_context().new_render_pass_mrt(&[color_texture], None, depth_texture);
        texture = color_texture;
    }

    let texture = Texture2D {
        texture: context.textures.store_texture(texture),
    };

    let render_pass = RenderPass {
        color_texture: texture.clone(),
        depth_texture: None,
        render_pass: Arc::new(render_pass),
    };
    RenderTarget {
        texture,
        render_pass,
    }
}

#[derive(Debug, Clone)]
pub struct DrawTextureParams {
    pub dest_size: Option<Vec2>,

    /// Part of texture to draw. If None - draw the whole texture.
    /// Good use example: drawing an image from texture atlas.
    /// Is None by default
    pub source: Option<Rect>,

    /// Rotation in radians
    pub rotation: f32,

    /// Mirror on the X axis
    pub flip_x: bool,

    /// Mirror on the Y axis
    pub flip_y: bool,

    /// Rotate around this point.
    /// When `None`, rotate around the texture's center.
    /// When `Some`, the coordinates are in screen-space.
    /// E.g. pivot (0,0) rotates around the top left corner of the screen, not of the
    /// texture.
    pub pivot: Option<Vec2>,
}

impl Default for DrawTextureParams {
    fn default() -> DrawTextureParams {
        DrawTextureParams {
            dest_size: None,
            source: None,
            rotation: 0.,
            pivot: None,
            flip_x: false,
            flip_y: false,
        }
    }
}

pub fn draw_texture(texture: &Texture2D, x: f32, y: f32, color: Color) {
    draw_texture_ex(texture, x, y, color, Default::default());
}

pub fn draw_texture_ex(
    texture: &Texture2D,
    x: f32,
    y: f32,
    color: Color,
    params: DrawTextureParams,
) {
    let context = get_context();

    let [mut width, mut height] = texture.size().to_array();

    let Rect {
        x: mut sx,
        y: mut sy,
        w: mut sw,
        h: mut sh,
    } = params.source.unwrap_or(Rect {
        x: 0.,
        y: 0.,
        w: width,
        h: height,
    });

    let texture_opt = context
        .texture_batcher
        .get(texture)
        .map(|(batched_texture, uv)| {
            let [batched_width, batched_height] = batched_texture.size().to_array();
            sx = ((sx / width) * uv.w + uv.x) * batched_width;
            sy = ((sy / height) * uv.h + uv.y) * batched_height;
            sw = (sw / width) * uv.w * batched_width;
            sh = (sh / height) * uv.h * batched_height;

            width = batched_width;
            height = batched_height;

            batched_texture
        });
    let texture = texture_opt.as_ref().unwrap_or(texture);

    let (mut w, mut h) = match params.dest_size {
        Some(dst) => (dst.x, dst.y),
        _ => (sw, sh),
    };
    let mut x = x;
    let mut y = y;
    if params.flip_x {
        x += w;
        w = -w;
    }
    if params.flip_y {
        y += h;
        h = -h;
    }

    let pivot = params.pivot.unwrap_or(vec2(x + w / 2., y + h / 2.));
    let m = pivot;
    let p = [
        vec2(x, y) - pivot,
        vec2(x + w, y) - pivot,
        vec2(x + w, y + h) - pivot,
        vec2(x, y + h) - pivot,
    ];
    let r = params.rotation;
    let p = [
        vec2(
            p[0].x * r.cos() - p[0].y * r.sin(),
            p[0].x * r.sin() + p[0].y * r.cos(),
        ) + m,
        vec2(
            p[1].x * r.cos() - p[1].y * r.sin(),
            p[1].x * r.sin() + p[1].y * r.cos(),
        ) + m,
        vec2(
            p[2].x * r.cos() - p[2].y * r.sin(),
            p[2].x * r.sin() + p[2].y * r.cos(),
        ) + m,
        vec2(
            p[3].x * r.cos() - p[3].y * r.sin(),
            p[3].x * r.sin() + p[3].y * r.cos(),
        ) + m,
    ];
    #[rustfmt::skip]
    let vertices = [
        Vertex::new(p[0].x, p[0].y, 0.,  sx      /width,  sy      /height, color),
        Vertex::new(p[1].x, p[1].y, 0., (sx + sw)/width,  sy      /height, color),
        Vertex::new(p[2].x, p[2].y, 0., (sx + sw)/width, (sy + sh)/height, color),
        Vertex::new(p[3].x, p[3].y, 0.,  sx      /width, (sy + sh)/height, color),
    ];
    let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

    context.gl.texture(Some(texture));
    context.gl.draw_mode(DrawMode::Triangles);
    context.gl.geometry(&vertices, &indices);
}

/// Get pixel data from screen buffer and return an Image (screenshot)
pub fn get_screen_data() -> Image {
    get_context().perform_render_passes();

    let context = get_context();
    image_dimensions(context.screen_width as u32, context.screen_height as u32)
        .expect("Invalid screenshot dimensions");

    let texture_id = get_quad_context().new_render_texture(miniquad::TextureParams {
        width: context.screen_width as _,
        height: context.screen_height as _,
        ..Default::default()
    });

    let texture = Texture2D {
        texture: context.textures.store_texture(texture_id),
    };

    texture.grab_screen();

    texture.get_texture_data()
}

/// Texture, data stored in GPU memory
#[derive(Clone, Debug, PartialEq)]
pub struct Texture2D {
    pub(crate) texture: TextureHandle,
}

impl Drop for TextureSlotGuarded {
    fn drop(&mut self) {
        let ctx = get_context();
        ctx.textures.schedule_removed(self.0);
    }
}

impl Texture2D {
    pub fn weak_clone(&self) -> Texture2D {
        match &self.texture {
            TextureHandle::Unmanaged(id) => Texture2D::unmanaged(*id),
            TextureHandle::Managed(t) => Texture2D {
                texture: TextureHandle::ManagedWeak(t.0),
            },
            TextureHandle::ManagedWeak(t) => Texture2D {
                texture: TextureHandle::ManagedWeak(*t),
            },
        }
    }
    pub(crate) const fn unmanaged(texture: miniquad::TextureId) -> Texture2D {
        Texture2D {
            texture: TextureHandle::Unmanaged(texture),
        }
    }
    /// Creates an empty Texture2D.
    ///
    /// # Example
    /// ```
    /// # use macroquad::prelude::*;
    /// # #[macroquad::main("test")]
    /// # async fn main() {
    /// let texture = Texture2D::empty();
    /// # }
    /// ```
    pub fn empty() -> Texture2D {
        let ctx = get_context();

        Texture2D::unmanaged(ctx.gl.white_texture)
    }

    /// Creates a Texture2D from a slice of bytes that contains an encoded image.
    ///
    /// If `format` is None, it will make an educated guess on the
    /// [ImageFormat][image::ImageFormat].
    ///
    /// # Example
    /// ```
    /// # use macroquad::prelude::*;
    /// # #[macroquad::main("test")]
    /// # async fn main() {
    /// let texture = Texture2D::from_file_with_format(
    ///     include_bytes!("../examples/rust.png"),
    ///     None,
    ///     );
    /// # }
    /// ```
    pub fn from_file_with_format(bytes: &[u8], format: Option<image::ImageFormat>) -> Texture2D {
        let img = if let Some(fmt) = format {
            image::load_from_memory_with_format(bytes, fmt)
                .unwrap_or_else(|e| panic!("{}", e))
                .to_rgba8()
        } else {
            image::load_from_memory(bytes)
                .unwrap_or_else(|e| panic!("{}", e))
                .to_rgba8()
        };
        let (width, height, _) =
            image_dimensions(img.width(), img.height()).expect("Invalid decoded image dimensions");
        Self::from_rgba8(width, height, &img.into_raw())
    }

    /// Creates a Texture2D from an [Image].
    pub fn from_image(image: &Image) -> Texture2D {
        Texture2D::from_rgba8(image.width, image.height, &image.bytes)
    }

    /// Creates a Texture2D from a slice of bytes in an R,G,B,A sequence,
    /// with the given width and height.
    ///
    /// # Example
    ///
    /// ```
    /// # use macroquad::prelude::*;
    /// # #[macroquad::main("test")]
    /// # async fn main() {
    /// // Create a 2x2 texture from a byte slice with 4 rgba pixels
    /// let bytes: Vec<u8> = vec![255, 0, 0, 192, 0, 255, 0, 192, 0, 0, 255, 192, 255, 255, 255, 192];
    /// let texture = Texture2D::from_rgba8(2, 2, &bytes);
    /// # }
    /// ```
    pub fn from_rgba8(width: u16, height: u16, bytes: &[u8]) -> Texture2D {
        validate_rgba(width as u32, height as u32, bytes);
        let texture = get_quad_context().new_texture_from_rgba8(width, height, bytes);
        let ctx = get_context();
        let texture = ctx.textures.store_texture(texture);
        let texture = Texture2D { texture };
        texture.set_filter(ctx.default_filter_mode);

        ctx.texture_batcher.add_unbatched(&texture);

        texture
    }

    /// Uploads [Image] data to this texture.
    pub fn update(&self, image: &Image) {
        validate_rgba(image.width as u32, image.height as u32, &image.bytes);
        let ctx = get_quad_context();
        assert_eq!(
            ctx.texture_params(self.raw_miniquad_id()).format,
            miniquad::TextureFormat::RGBA8,
            "Only RGBA8 updates are supported"
        );
        let (width, height) = ctx.texture_size(self.raw_miniquad_id());

        assert_eq!(width, image.width as u32);
        assert_eq!(height, image.height as u32);

        ctx.texture_update(self.raw_miniquad_id(), &image.bytes);
    }

    // Updates the texture from an array of bytes.
    pub fn update_from_bytes(&self, width: u32, height: u32, bytes: &[u8]) {
        validate_rgba(width, height, bytes);
        let ctx = get_quad_context();
        assert_eq!(
            ctx.texture_params(self.raw_miniquad_id()).format,
            miniquad::TextureFormat::RGBA8,
            "Only RGBA8 updates are supported"
        );
        let (texture_width, texture_height) = ctx.texture_size(self.raw_miniquad_id());

        assert_eq!(texture_width, width);
        assert_eq!(texture_height, height);

        ctx.texture_update(self.raw_miniquad_id(), bytes);
    }

    /// Uploads [Image] data to part of this texture.
    pub fn update_part(
        &self,
        image: &Image,
        x_offset: i32,
        y_offset: i32,
        width: i32,
        height: i32,
    ) {
        let ctx = get_quad_context();

        let params = ctx.texture_params(self.raw_miniquad_id());
        assert_eq!(
            params.format,
            miniquad::TextureFormat::RGBA8,
            "Only RGBA8 updates are supported"
        );
        validate_region(
            params.width,
            params.height,
            x_offset,
            y_offset,
            width,
            height,
            &image.bytes,
        );
        ctx.texture_update_part(
            self.raw_miniquad_id(),
            x_offset,
            y_offset,
            width,
            height,
            &image.bytes,
        );
    }

    /// Returns the width of this texture.
    pub fn width(&self) -> f32 {
        let ctx = get_quad_context();
        let (width, _) = ctx.texture_size(self.raw_miniquad_id());

        width as f32
    }

    /// Returns the height of this texture.
    pub fn height(&self) -> f32 {
        let ctx = get_quad_context();
        let (_, height) = ctx.texture_size(self.raw_miniquad_id());

        height as f32
    }

    pub fn size(&self) -> Vec2 {
        let ctx = get_quad_context();
        let (width, height) = ctx.texture_size(self.raw_miniquad_id());

        vec2(width as f32, height as f32)
    }

    /// Sets the [FilterMode] of this texture.
    ///
    /// Use Nearest if you need integer-ratio scaling for pixel art, for example.
    ///
    /// # Example
    /// ```
    /// # use macroquad::prelude::*;
    /// # #[macroquad::main("test")]
    /// # async fn main() {
    /// let texture = Texture2D::empty();
    /// texture.set_filter(FilterMode::Linear);
    /// # }
    /// ```
    pub fn set_filter(&self, filter_mode: FilterMode) {
        let ctx = get_quad_context();

        ctx.texture_set_filter(
            self.raw_miniquad_id(),
            filter_mode,
            miniquad::MipmapFilterMode::None,
        );
    }

    /// Returns the handle for this texture.
    pub(crate) fn raw_miniquad_id(&self) -> miniquad::TextureId {
        let ctx = get_context();

        ctx.raw_miniquad_id(&self.texture)
    }

    /// Updates this texture from the screen.
    #[allow(unreachable_patterns)]
    pub fn grab_screen(&self) {
        use miniquad::*;
        let texture = self.raw_miniquad_id();
        let ctx = get_quad_context();
        let params = ctx.texture_params(texture);
        let raw_id = match unsafe { ctx.texture_raw_id(texture) } {
            miniquad::RawId::OpenGl(id) => id,
            _ => unimplemented!(),
        };
        let internal_format = match params.format {
            TextureFormat::RGB8 => miniquad::gl::GL_RGB,
            TextureFormat::RGBA8 => miniquad::gl::GL_RGBA,
            TextureFormat::RGBA16F => miniquad::gl::GL_RGBA,
            TextureFormat::Depth => miniquad::gl::GL_DEPTH_COMPONENT,
            TextureFormat::Depth32 => miniquad::gl::GL_DEPTH_COMPONENT,
            #[cfg(target_arch = "wasm32")]
            TextureFormat::Alpha => miniquad::gl::GL_ALPHA,
            #[cfg(not(target_arch = "wasm32"))]
            TextureFormat::Alpha => miniquad::gl::GL_R8,
        };
        unsafe {
            gl::glBindTexture(gl::GL_TEXTURE_2D, raw_id);
            gl::glCopyTexImage2D(
                gl::GL_TEXTURE_2D,
                0,
                internal_format,
                0,
                0,
                params.width as _,
                params.height as _,
                0,
            );
        }
    }

    /// Returns an [Image] from the pixel data in this texture.
    ///
    /// This operation can be expensive.
    pub fn get_texture_data(&self) -> Image {
        let ctx = get_quad_context();
        let id = self.raw_miniquad_id();
        let params = ctx.texture_params(id);
        assert_eq!(
            params.format,
            miniquad::TextureFormat::RGBA8,
            "Only RGBA8 readback is supported"
        );
        assert_eq!(
            params.sample_count, 1,
            "Multisampled readback is unsupported"
        );
        let (width, height, length) = image_dimensions(params.width, params.height)
            .expect("Invalid texture readback dimensions");
        let mut image = Image::from_rgba8(width, height, vec![0; length]).unwrap();
        ctx.texture_read_pixels(id, &mut image.bytes);
        image
    }
}

pub(crate) struct Batcher {
    unbatched: Vec<Texture2D>,
    atlas: crate::text::atlas::Atlas,
}

impl Batcher {
    pub fn new(ctx: &mut dyn miniquad::RenderingBackend) -> Batcher {
        Batcher {
            unbatched: vec![],
            atlas: crate::text::atlas::Atlas::new(ctx, miniquad::FilterMode::Linear),
        }
    }

    pub fn add_unbatched(&mut self, texture: &Texture2D) {
        self.unbatched.push(texture.weak_clone());
    }

    pub fn get(&mut self, texture: &Texture2D) -> Option<(Texture2D, Rect)> {
        let id = SpriteKey::Texture(texture.raw_miniquad_id());
        let uv_rect = self.atlas.get_uv_rect(id)?;
        Some((Texture2D::unmanaged(self.atlas.texture()), uv_rect))
    }
}

/// Build an atlas out of all currently loaded texture
/// Later on all draw_texture calls with texture available in the atlas will use
/// the one from the atlas
/// NOTE: the GPU memory and texture itself in Texture2D will still be allocated
/// and Texture->Image conversions will work with Texture2D content, not the atlas
pub fn build_textures_atlas() {
    let context = get_context();

    for texture in context.texture_batcher.unbatched.drain(0..) {
        let sprite: Image = texture.get_texture_data();
        let id = SpriteKey::Texture(texture.raw_miniquad_id());

        context.texture_batcher.atlas.cache_sprite(id, sprite);
    }

    let texture = context.texture_batcher.atlas.texture();
    let (w, h) = get_quad_context().texture_size(texture);
    crate::telemetry::log_string(&format!("Atlas: {w} {h}"));
}

#[doc(hidden)]
/// Macroquad do not have track of all loaded fonts.
/// Fonts store their characters as ID's in the atlas.
/// There fore resetting the atlas will render all fonts unusable.
pub unsafe fn reset_textures_atlas() {
    let context = get_context();
    context.fonts_storage = crate::text::FontsStorage::new(&mut *context.quad_context);
    context.texture_batcher = Batcher::new(&mut *context.quad_context);
}

pub fn set_default_filter_mode(filter: FilterMode) {
    let context = get_context();

    context.default_filter_mode = filter;
    context.texture_batcher.atlas.set_filter(filter);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_constructor_rejects_short_and_long_storage() {
        assert!(Image::from_rgba8(1, 1, vec![]).is_err());
        assert!(Image::from_rgba8(1, 1, vec![0; 5]).is_err());
        assert!(Image::from_rgba8(0, 1, vec![0; 4]).is_err());
        assert!(Image::from_rgba8(0, 0, vec![]).is_ok());
    }

    #[test]
    fn pixels_and_byte_access_preserve_layout() {
        let mut image = Image::from_rgba8(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!(image.get_image_data(), &[[1, 2, 3, 4], [5, 6, 7, 8]]);
        image.get_image_data_mut()[1] = [9, 10, 11, 12];
        image.bytes_mut()[0] = 13;
        assert_eq!(image.bytes(), &[13, 2, 3, 4, 9, 10, 11, 12]);
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 1);
    }

    #[test]
    fn dimensions_reject_backend_and_slice_overflow_without_allocating() {
        assert!(rgba_len(u32::MAX, u32::MAX).is_err());
        assert!(rgba_len(u16::MAX as u32, u16::MAX as u32).is_err());
        assert!(image_dimensions(65536, 1).is_err());
        assert_eq!(rgba_len(100, 200).unwrap(), 80_000);
        // This fits u32 but exceeds the Rust slice bound on 32-bit WASM.
        assert_eq!(rgba_len(32768, 16384).is_err(), usize::BITS == 32);
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "PNG codec integration is covered by native tests; checked dimensions are tested directly under Miri"
    )]
    fn decoded_dimensions_are_not_truncated() {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(65536, 1)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        assert!(Image::from_file_with_format(encoded.get_ref(), None).is_err());
    }

    #[test]
    fn sub_image_copies_rows_and_accepts_empty_edge() {
        let image = Image::from_rgba8(2, 2, (0..16).collect()).unwrap();
        assert_eq!(
            image.sub_image(Rect::new(1., 0., 1., 2.)).bytes(),
            &[4, 5, 6, 7, 12, 13, 14, 15]
        );
        assert!(image
            .sub_image(Rect::new(2., 2., 0., 0.))
            .bytes()
            .is_empty());
    }

    #[test]
    fn invalid_sub_images_fail_before_conversion_or_indexing() {
        let image = Image::gen_image_color(2, 2, crate::color::WHITE);
        for rect in [
            Rect::new(-1., 0., 1., 1.),
            Rect::new(0.5, 0., 1., 1.),
            Rect::new(0., 0., 3., 1.),
            Rect::new(0., 0., f32::INFINITY, 1.),
            Rect::new(f32::NAN, 0., 1., 1.),
            Rect::new(f32::MAX, 0., 1., 1.),
        ] {
            assert!(std::panic::catch_unwind(|| image.sub_image(rect)).is_err());
        }
    }

    #[test]
    fn texture_regions_validate_signed_bounds_and_exact_byte_count() {
        validate_region(4, 4, 3, 3, 1, 1, &[0; 4]);
        for (x, y, width, height, length) in [
            (-1, 0, 1, 1, 4),
            (0, -1, 1, 1, 4),
            (0, 0, -1, 1, 4),
            (0, 0, 1, -1, 4),
            (3, 3, 2, 1, 8),
            (3, 3, 1, 2, 8),
            (i32::MAX, 0, i32::MAX, 1, 4),
            (0, 0, 1, 1, 0),
        ] {
            assert!(std::panic::catch_unwind(|| validate_region(
                4,
                4,
                x,
                y,
                width,
                height,
                &vec![0; length]
            ))
            .is_err());
        }
    }

    #[test]
    #[should_panic(expected = "RGBA buffer length does not match dimensions")]
    fn malformed_upload_is_rejected_before_accessing_graphics() {
        Texture2D::from_rgba8(1, 1, &[]);
    }

    #[test]
    #[should_panic(expected = "RGBA buffer length does not match dimensions")]
    fn malformed_internal_image_cannot_construct_an_out_of_bounds_slice() {
        let image = Image {
            width: 1,
            height: 1,
            bytes: vec![],
        };
        let _ = image.get_image_data();
    }
}
