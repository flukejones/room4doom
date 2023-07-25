use std::f32::consts::FRAC_PI_4;

use gameplay::glam::{Mat4, Vec3};
use golem::{Dimension::*, *};

use super::{Drawer, GL_QUAD, GL_QUAD_INDICES};

pub struct LottesCRT {
    _quad: [f32; 16],
    indices: [u32; 6],
    crt_shader: ShaderProgram,
    _projection: Mat4,
    _look_at: Mat4,
    texture: Texture,
    vb: VertexBuffer,
    eb: ElementBuffer,
}

impl LottesCRT {
    pub fn new(ctx: &Context) -> Self {
        let shader = ShaderDescription {
            uniforms: &[
                // Standard view stuff
                Uniform::new("projMat", UniformType::Matrix(D4)),
                Uniform::new("viewMat", UniformType::Matrix(D4)),
                Uniform::new("modelMat", UniformType::Matrix(D4)),
                //
                Uniform::new(
                    "color_texture_sz",
                    UniformType::Vector(NumberType::Float, D2),
                ),
                Uniform::new(
                    "color_texture_pow2_sz",
                    UniformType::Vector(NumberType::Float, D2),
                ),
                //
                Uniform::new("hardScan", UniformType::Scalar(NumberType::Float)),
                Uniform::new("hardPix", UniformType::Scalar(NumberType::Float)),
                Uniform::new("saturation", UniformType::Scalar(NumberType::Float)),
                Uniform::new("tint", UniformType::Scalar(NumberType::Float)),
                Uniform::new("blackClip", UniformType::Scalar(NumberType::Float)),
                Uniform::new("brightMult", UniformType::Scalar(NumberType::Float)),
                Uniform::new("distortion", UniformType::Scalar(NumberType::Float)),
                Uniform::new("cornersize", UniformType::Scalar(NumberType::Float)),
                Uniform::new("cornersmooth", UniformType::Scalar(NumberType::Float)),
                Uniform::new("toSRGB", UniformType::Scalar(NumberType::Float)),
                // The SDL bytes
                Uniform::new("image", UniformType::Sampler2D),
            ],
            vertex_input: &[
                Attribute::new("position", AttributeType::Vector(D2)),
                Attribute::new("vert_uv", AttributeType::Vector(D2)),
            ],
            vertex_shader: VERT,
            fragment_input: &[Attribute::new("texCoord", AttributeType::Vector(D2))],
            fragment_shader: FRAG,
        };
        let mut shader = ShaderProgram::new(ctx, shader).unwrap();
        shader.bind();
        shader.set_uniform("image", UniformValue::Int(1)).unwrap();

        shader
            .set_uniform(
                "modelMat",
                UniformValue::Matrix4(Mat4::IDENTITY.to_cols_array()),
            )
            .unwrap();

        // Hardness of pixels in scanline.
        // -2.0 = soft
        // -4.0 = hard
        shader
            .set_uniform("hardPix", UniformValue::Float(-3.0))
            .unwrap();

        // GAMMA, needs to be increased if SRGB not used
        shader
            .set_uniform("brightMult", UniformValue::Float(0.2))
            .unwrap();
        shader
            .set_uniform("toSRGB", UniformValue::Float(1.0))
            .unwrap();

        // SHAPE
        shader
            .set_uniform("distortion", UniformValue::Float(0.07))
            .unwrap(); // 0.05 to 0.3

        shader
            .set_uniform("cornersize", UniformValue::Float(0.022))
            .unwrap(); // 0.01 to 0.05

        // Edge hardness
        shader
            .set_uniform("cornersmooth", UniformValue::Float(70.0))
            .unwrap(); // 70.0 to 170.0

        let projection = Mat4::perspective_rh_gl(FRAC_PI_4, 1.0, 0.1, 50.0);
        shader
            .set_uniform("projMat", UniformValue::Matrix4(projection.to_cols_array()))
            .unwrap();

        let look_at = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 2.42),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );

        shader
            .set_uniform("viewMat", UniformValue::Matrix4(look_at.to_cols_array()))
            .unwrap();

        let mut vb = VertexBuffer::new(ctx).unwrap();
        let mut eb = ElementBuffer::new(ctx).unwrap();
        vb.set_data(&GL_QUAD);
        eb.set_data(&GL_QUAD_INDICES);

        let texture = Texture::new(ctx).unwrap();
        let bind_point = std::num::NonZeroU32::new(1).unwrap();
        texture.set_active(bind_point);

        Self {
            _quad: GL_QUAD,
            indices: GL_QUAD_INDICES,
            crt_shader: shader,
            _projection: projection,
            _look_at: look_at,
            texture,
            vb,
            eb,
        }
    }
}

impl Drawer for LottesCRT {
    fn set_tex_filter(&self) -> Result<(), GolemError> {
        self.texture.set_minification(TextureFilter::Linear)?;
        self.texture.set_magnification(TextureFilter::Linear)
    }

    fn set_image_data(&mut self, input: &[u8], input_size: (u32, u32)) {
        self.texture
            .set_image(Some(input), input_size.0, input_size.1, ColorFormat::RGB);
    }

    fn draw(&mut self) -> Result<(), GolemError> {
        self.crt_shader.bind();

        // CRT settings
        self.crt_shader
            .set_uniform(
                "color_texture_sz",
                UniformValue::Vector2([self.texture.width() as f32, self.texture.height() as f32]),
            )
            .unwrap();

        // size of color texture rounded up to power of 2
        self.crt_shader
            .set_uniform(
                "color_texture_pow2_sz",
                UniformValue::Vector2([self.texture.width() as f32, self.texture.height() as f32]),
            )
            .unwrap();

        unsafe {
            self.crt_shader.draw(
                &self.vb,
                &self.eb,
                0..self.indices.len(),
                GeometryMode::Triangles,
            )?;
        }
        Ok(())
    }
}

const FRAG: &str = r#"
#pragma optimize (on)
#pragma debug (off)

//An extra per channel gamma adjustment applied at the end.
const vec3 gammaBoost = vec3(1.0/1.2, 1.0/1.2, 1.0/1.2);

// sRGB to Linear.
// Assuing using sRGB typed textures this should not be needed.
float ToLinear1(float c)
{
    return(c <= 0.04045) ? c / 12.92 : pow((c+0.055) / 1.055,2.4);
}
vec3 ToLinear(vec3 c)
{
    return vec3( ToLinear1(c.r), ToLinear1(c.g), ToLinear1(c.b) );
}

// Linear to sRGB.
// Assuming using sRGB typed textures this should not be needed.
float ToSrgb1(float c)
{
    return( c < 0.0031308 ? c * 12.92 : 1.055 * pow(c,0.41666) - 0.055);
}
vec3 ToSrgb(vec3 c)
{
    return vec3(ToSrgb1(c.r), ToSrgb1(c.g), ToSrgb1(c.b));
}

// Nearest emulated sample given floating point position and texel offset.
// Also zero's off screen.
vec3 Fetch(vec2 pos, vec2 off)
{
    pos = (floor(pos * color_texture_pow2_sz + off) + 0.5) / color_texture_pow2_sz;
    if(max(abs(pos.x-0.5),abs(pos.y-0.5))>0.5)return vec3(0.0,0.0,0.0);
    return ToLinear(texture2D(image, pos.xy).rgb);
}

// Distance in emulated pixels to nearest texel.
vec2 Dist(vec2 pos)
{
    pos = pos * color_texture_pow2_sz;
    return -((pos - floor(pos)) - vec2(0.5));
}

// 1D Gaussian.
float Gaus(float pos,float scale)
{
    return exp2(scale * pos * pos);
}

// 3-tap Gaussian filter along horz line.
vec3 Horz3(vec2 pos,float off)
{
    vec3 c = Fetch(pos, vec2( 0.0, off));
    float dst = Dist(pos).x;
    // Convert distance to weight.
    float scale = hardPix;
    float wc = Gaus(dst + 0.0, scale);
    // Return filtered sample.
    return c * wc;
}

///////////////////////////////////////////////////////////////
/// CRT GEOM FUNCTIONS ///
vec2 radialDistortion(vec2 coord) {
    coord *= color_texture_pow2_sz / color_texture_sz;
    vec2 cc = coord - vec2(0.5);
    float dist = dot(cc, cc) * distortion;
    return (coord + cc * (1.0 + dist) * dist) * color_texture_sz / color_texture_pow2_sz;
}

float corner(vec2 coord)
{
    coord *= color_texture_pow2_sz / color_texture_sz;
    coord = (coord - vec2(0.5)) + vec2(0.5);
    coord = min(coord, vec2(1.0)-coord);
    vec2 cdist = vec2(cornersize);
    coord = (cdist - min(coord,cdist));
    float dist = sqrt(dot(coord,coord));
    return clamp((cdist.x-dist)*cornersmooth,0.0, 1.0);
}
///////////////////////////////////////////////////////////////

void main(void)
{
    gl_FragColor.a = 1.0;
    vec2 pos = radialDistortion(texCoord);
    gl_FragColor.rgb = Horz3(pos, 0.0) * vec3(corner(pos));
    gl_FragColor.rgb += brightMult*pow(gl_FragColor.rgb,gammaBoost);

    if (toSRGB == 1.0) {
        gl_FragColor.rgb = ToSrgb(gl_FragColor.rgb);
    }
}"#;

const VERT: &str = r#"
void main() {
    gl_Position = projMat * viewMat * modelMat * vec4(position, 0.0, 1.0);
    texCoord = vert_uv;
}"#;
