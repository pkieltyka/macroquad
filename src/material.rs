//! Custom materials - shaders, uniforms.

pub use crate::tobytes::ToBytes;
use crate::{quad_gl::GlPipeline, texture::Texture2D, Error};
use miniquad::{PipelineParams, UniformDesc};
use std::rc::Rc;

struct GlPipelineGuarded(GlPipeline, Rc<crate::GraphicsOwner>);

impl PartialEq for GlPipelineGuarded {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && Rc::ptr_eq(&self.1, &other.1)
    }
}
impl Drop for GlPipelineGuarded {
    fn drop(&mut self) {
        self.1.defer(crate::ResourceDelete::Pipeline(self.0));
    }
}

/// Material instance loaded on GPU on its originating window thread.
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<macroquad::material::Material>();
/// ```
#[derive(Clone, PartialEq)]
pub struct Material {
    pipeline: Rc<GlPipelineGuarded>,
}

impl std::fmt::Debug for Material {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Material").finish()
    }
}

impl Material {
    pub(crate) fn pipeline(&self) -> GlPipeline {
        self.pipeline.0
    }
    /// Set GPU uniform value for this material.
    /// "name" should be from "uniforms" list used for material creation.
    /// Otherwise uniform value would be silently ignored.
    /// Values must have an initialized, padding-free representation. Use arrays
    /// or glam vectors instead of tuples, and convert application structs to
    /// those types before uploading them.
    ///
    /// ```compile_fail
    /// # use macroquad::material::Material;
    /// #[repr(C)]
    /// struct Padded { small: u8, large: u32 }
    /// fn upload(material: &Material) {
    ///     material.set_uniform("value", Padded { small: 1, large: 2 });
    /// }
    /// ```
    ///
    /// Implementing the representation contract requires an explicit unsafe impl:
    ///
    /// ```compile_fail
    /// use macroquad::material::ToBytes;
    /// struct Value(f32);
    /// impl ToBytes for Value {
    ///     fn to_bytes(&self) -> &[u8] { &[] }
    /// }
    /// ```
    pub fn set_uniform<T: ToBytes>(&self, name: &str, uniform: T) {
        self.pipeline.1.assert_current();
        let bytes = uniform.to_bytes().to_vec();
        crate::with_context(|context| {
            context
                .gl
                .set_uniform_bytes(self.pipeline.0, name, &bytes, true)
        });
    }

    pub fn set_uniform_array<T: ToBytes>(&self, name: &str, uniform: &[T]) {
        self.pipeline.1.assert_current();
        let bytes = uniform.to_bytes().to_vec();
        crate::with_context(|context| {
            context
                .gl
                .set_uniform_bytes(self.pipeline.0, name, &bytes, false)
        });
    }

    pub fn set_texture(&self, name: &str, texture: Texture2D) {
        self.pipeline.1.assert_current();
        crate::with_context(|context| context.gl.set_texture(self.pipeline.0, name, texture));
    }
}

/// Params used for material loading.
/// It is not possible to change material params at runtime, so this
/// struct is used only once - at "load_material".
#[derive(Default)]
pub struct MaterialParams {
    /// miniquad pipeline configuration for this material.
    /// Things like blending, culling, depth dest
    pub pipeline_params: PipelineParams,

    /// List of custom uniforms used in this material
    pub uniforms: Vec<UniformDesc>,

    /// List of textures used in this material
    pub textures: Vec<String>,
}

/// Creates custom material
///
/// For OpenGL and Metal examples check examples/custom_material.rs.
///
/// # Default variables for OpenGL backend:
/// ## Attributes (order doesn't matter, any could be skipped):
/// ```glsl
/// attribute vec3 position;
/// attribute vec4 color0;
/// attribute vec2 texcoord;
/// ```
/// ## Uniforms (order doesn't matter, any could be skipped):
/// ```glsl
/// uniform mat4 Model;
/// uniform mat4 Projection;
/// uniform float4 _Time;
/// ```
/// ## Textures (order doesn't matter, any could be skipped):
/// ```glsl
/// uniform sampler2D Texture;
/// uniform sampler2D _ScreenTexture; // check examples/screen_texture.rs to see how it works
/// ```
///
/// # Default variables for Metal backend:
/// ## Attributes (order doesn't matter, any could be skipped, should have exact index in attribute()):
/// ```msl
/// struct Vertex
/// {
///     float3 position    [[attribute(0)]];
///     float2 texcoord    [[attribute(1)]];
///     float4 color0      [[attribute(2)]];
/// };
/// ```
/// ## Uniforms (**order matters, all fields before needed one should present**):
/// **All uniforms are in the same buffer, so order matters also for custom additional uniforms**
/// ```msl
/// struct Uniforms
/// {
///     float4x4 Model;
///     float4x4 Projection;
///     float4 _Time;
///     ...
///     additional uniforms
/// };
/// // same for vertexShader
/// // Only buffer(0) is correct here
/// fragment float4 fragmentShader(..., constant Uniforms& u [[buffer(0)]], ...) {...}
/// ```
/// ## Textures (order doesn't matter, any could be skipped, should have exact indices in `texture()` and `sampler()`):
/// ```msl
/// // same for vertexShader
/// fragment float4 fragmentShader(...,
///     texture2d<float> Texture [[texture(0)]],
///     sampler TextureSmplr [[sampler(0)]],
///     _ScreenTexture is not working for metal for now
///     ...
/// ) {...}
/// ```
///
pub fn load_material(
    shader: crate::ShaderSource,
    params: MaterialParams,
) -> Result<Material, Error> {
    crate::with_context(|context| {
        let owner = context.gpu.clone();
        let pipeline = owner.with(|backend| {
            context.gl.make_pipeline(
                backend,
                shader,
                params.pipeline_params,
                params.uniforms,
                params.textures,
            )
        })?;

        Ok(Material {
            pipeline: Rc::new(GlPipelineGuarded(pipeline, context.gpu.clone())),
        })
    })
}

/// All following macroquad rendering calls will use the given material.
pub fn gl_use_material(material: &Material) {
    material.pipeline.1.assert_current();
    crate::with_context(|context| {
        context.gl.pipeline(Some(material.pipeline.0));
        context.retained_materials.push(material.clone());
        context.active_material = Some(material.clone());
    });
}

/// Use default macroquad material.
pub fn gl_use_default_material() {
    crate::with_context(|context| {
        context.gl.pipeline(None);
        context.active_material = None;
    });
}

#[doc(hidden)]
pub mod shaders {
    type IncludeFilename = String;
    type IncludeContent = String;

    #[derive(Debug, Clone)]
    pub struct PreprocessorConfig {
        pub includes: Vec<(IncludeFilename, IncludeContent)>,
    }
    impl Default for PreprocessorConfig {
        fn default() -> PreprocessorConfig {
            PreprocessorConfig { includes: vec![] }
        }
    }

    impl PreprocessorConfig {}

    pub fn preprocess_shader(source: &str, config: &PreprocessorConfig) -> String {
        let mut res = source.chars().collect::<Vec<_>>();

        fn find(data: &[char], n: &mut usize, target: &str) -> bool {
            if *n >= data.len() {
                return false;
            }
            let target = target.chars().collect::<Vec<_>>();

            'outer: for i in *n..data.len() {
                for j in 0..target.len() {
                    if data[i + j] != target[j] {
                        *n += 1;
                        continue 'outer;
                    }
                }
                return true;
            }
            false
        }

        fn skip_character(data: &[char], n: &mut usize, target: char) {
            while *n < data.len() && data[*n] == target {
                *n += 1;
            }
        }

        let mut i = 0;
        while find(&res, &mut i, "#include") {
            let directive_start_ix = i;
            i += "#include".len();
            skip_character(&res, &mut i, ' ');
            assert!(res[i] == '\"');
            i += 1;
            let filename_start_ix = i;
            find(&res, &mut i, "\"");
            let filename_end_ix = i;
            let filename = res[filename_start_ix..filename_end_ix]
                .iter()
                .cloned()
                .collect::<String>();

            let include_content = config
                .includes
                .iter()
                .find(|(name, _)| name == &filename)
                .expect(&format!(
                    "Include file {filename} in not on \"includes\" list"
                ));

            let _ = res
                .splice(
                    directive_start_ix..filename_end_ix + 1,
                    include_content.1.chars(),
                )
                .collect::<Vec<_>>();
        }

        res.into_iter().collect()
    }

    #[test]
    fn preprocessor_test() {
        let shader_string = r#"
#version blah blah

asd
asd

#include "hello.glsl"

qwe
"#;

        let preprocessed = r#"
#version blah blah

asd
asd

iii
jjj

qwe
"#;

        let result = preprocess_shader(
            shader_string,
            &PreprocessorConfig {
                includes: vec![("hello.glsl".to_string(), "iii\njjj".to_string())],
                ..Default::default()
            },
        );

        assert_eq!(result, preprocessed);
    }
}
