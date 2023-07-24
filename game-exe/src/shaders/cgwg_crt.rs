use std::f32::consts::FRAC_PI_4;

use gameplay::glam::{Mat4, Vec3};
use golem::{Dimension::*, *};

use super::{Drawer, GL_QUAD, GL_QUAD_INDICES};

/// CRT shader
/// ```
/// /*  CRT shader
///  *
///  *  Copyright (C) 2010-2012 cgwg, Themaister and DOLLS
///  *
///  *  This program is free software; you can redistribute it and/or modify it
///  *  under the terms of the GNU General Public License as published by the Free
///  *  Software Foundation; either version 2 of the License, or (at your option)
///  *  any later version.
///  */
/// ```
pub struct Cgwgcrt {
    _quad: [f32; 16],
    indices: [u32; 6],
    crt_shader: ShaderProgram,
    crt_width: u32,
    crt_height: u32,
    projection: Mat4,
    look_at: Mat4,
    texture: Texture,
    vb: VertexBuffer,
    eb: ElementBuffer,
}

impl Cgwgcrt {
    pub fn new(ctx: &Context, crt_width: u32, crt_height: u32) -> Self {
        let crt = ShaderProgram::new(
            ctx,
            ShaderDescription {
                uniforms: &[
                    // Standard view stuff
                    Uniform::new("projMat", UniformType::Matrix(D4)),
                    Uniform::new("viewMat", UniformType::Matrix(D4)),
                    Uniform::new("modelMat", UniformType::Matrix(D4)),
                    //
                    Uniform::new("inputSize", UniformType::Vector(NumberType::Float, D2)),
                    Uniform::new("outputSize", UniformType::Vector(NumberType::Float, D2)),
                    Uniform::new("textureSize", UniformType::Vector(NumberType::Float, D2)),
                    //
                    Uniform::new("CRTgamma", UniformType::Scalar(NumberType::Float)),
                    Uniform::new("monitorgamma", UniformType::Scalar(NumberType::Float)),
                    Uniform::new("cornersize", UniformType::Scalar(NumberType::Float)),
                    Uniform::new("cornersmooth", UniformType::Scalar(NumberType::Float)),
                    Uniform::new("d", UniformType::Scalar(NumberType::Float)),
                    Uniform::new("R", UniformType::Scalar(NumberType::Float)),
                    //
                    Uniform::new("overscan", UniformType::Vector(NumberType::Float, D2)),
                    Uniform::new("aspect", UniformType::Vector(NumberType::Float, D2)),
                    // The SDL bytes
                    Uniform::new("image", UniformType::Sampler2D),
                ],
                vertex_input: &[
                    Attribute::new("position", AttributeType::Vector(D2)),
                    Attribute::new("vert_uv", AttributeType::Vector(D2)),
                ],
                vertex_shader: VERT,
                fragment_input: &[
                    Attribute::new("texCoord", AttributeType::Vector(D2)),
                    Attribute::new("one", AttributeType::Vector(D2)),
                    Attribute::new("mod_factor", AttributeType::Scalar),
                    Attribute::new("ilfac", AttributeType::Vector(D2)),
                    Attribute::new("stretch", AttributeType::Vector(D3)),
                    Attribute::new("sinangle", AttributeType::Vector(D2)),
                    Attribute::new("cosangle", AttributeType::Vector(D2)),
                ],
                fragment_shader: FRAG,
            },
        )
        .map_err(|e| format!("{}", e))
        .unwrap();

        let projection = Mat4::perspective_rh_gl(FRAC_PI_4, 1.0, 0.1, 50.0);
        let look_at = Mat4::look_at_rh(
            Vec3::new(0.0, 0.0, 2.42),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        );

        let mut vb = VertexBuffer::new(ctx).unwrap();
        let mut eb = ElementBuffer::new(ctx).unwrap();
        vb.set_data(&GL_QUAD);
        eb.set_data(&GL_QUAD_INDICES);

        Self {
            _quad: GL_QUAD,
            indices: GL_QUAD_INDICES,
            crt_shader: crt,
            crt_width,
            crt_height,
            projection,
            look_at,
            texture: Texture::new(ctx).unwrap(),
            vb,
            eb,
        }
    }
}

impl Drawer for Cgwgcrt {
    fn set_tex_filter(&self) -> Result<(), GolemError> {
        self.texture.set_minification(TextureFilter::Nearest)?;
        self.texture.set_magnification(TextureFilter::Nearest)
    }

    fn set_image_data(&mut self, input: &[u8], input_size: (u32, u32)) {
        self.texture
            .set_image(Some(input), input_size.0, input_size.1, ColorFormat::RGB);
    }

    fn draw(&mut self) -> Result<(), GolemError> {
        // Set the image to use
        let bind_point = std::num::NonZeroU32::new(1).unwrap();
        self.texture.set_active(bind_point);

        self.crt_shader.bind();
        self.crt_shader.prepare_draw(&self.vb, &self.eb)?;

        self.crt_shader.set_uniform("image", UniformValue::Int(1))?;

        self.crt_shader.set_uniform(
            "projMat",
            UniformValue::Matrix4(self.projection.to_cols_array()),
        )?;
        self.crt_shader.set_uniform(
            "viewMat",
            UniformValue::Matrix4(self.look_at.to_cols_array()),
        )?;
        self.crt_shader.set_uniform(
            "modelMat",
            UniformValue::Matrix4(Mat4::IDENTITY.to_cols_array()),
        )?;

        self.crt_shader.set_uniform(
            "inputSize",
            UniformValue::Vector2([self.texture.width() as f32, self.texture.height() as f32]),
        )?;

        self.crt_shader.set_uniform(
            "outputSize",
            UniformValue::Vector2([self.crt_width as f32, self.crt_height as f32]),
        )?;
        self.crt_shader.set_uniform(
            "textureSize",
            UniformValue::Vector2([self.texture.width() as f32, self.texture.height() as f32]),
        )?;

        self.crt_shader
            .set_uniform("CRTgamma", UniformValue::Float(1.9))?;
        self.crt_shader
            .set_uniform("monitorgamma", UniformValue::Float(2.4))?;
        // distance from viewer
        self.crt_shader.set_uniform("d", UniformValue::Float(1.5))?;
        // radius of curvature - 2.0 to 3.0?
        self.crt_shader.set_uniform("R", UniformValue::Float(3.3))?;
        self.crt_shader
            .set_uniform("cornersize", UniformValue::Float(0.02))?;
        // border smoothness parameter
        self.crt_shader
            .set_uniform("cornersmooth", UniformValue::Float(80.0))?;

        self.crt_shader
            .set_uniform("overscan", UniformValue::Vector2([0.99, 0.99]))?;
        self.crt_shader
            .set_uniform("aspect", UniformValue::Vector2([0.8, 1.0]))?;

        unsafe {
            self.crt_shader
                .draw_prepared(0..self.indices.len(), GeometryMode::Triangles);
        }
        Ok(())
    }
}

const FRAG: &str = r#"
// Comment the next line to disable interpolation in linear gamma (and gain speed).
#define LINEAR_PROCESSING

// Enable screen curvature.
#define CURVATURE

// Enable 3x oversampling of the beam profile
// #define OVERSAMPLE

// Use the older, purely gaussian beam profile
// #define USEGAUSSIAN

#pragma optimize (on)
#pragma debug (off)

// Macros.
#define FIX(c) max(abs(c), 1e-5);
#define PI 3.141592653589

#ifdef LINEAR_PROCESSING
#       define TEX2D(c) pow(texture2D(image, (c)), vec4(CRTgamma))
#else
#       define TEX2D(c) texture2D(image, (c))
#endif

float intersect(vec2 xy)
{
  float A = dot(xy,xy)+d*d;
  float B = 2.0*(R*(dot(xy,sinangle)-d*cosangle.x*cosangle.y)-d*d);
  float C = d*d + 2.0*R*d*cosangle.x*cosangle.y;
  return (-B-sqrt(B*B-4.0*A*C))/(2.0*A);
}

vec2 bkwtrans(vec2 xy)
{
  float c = intersect(xy);
  vec2 point = vec2(c)*xy;
  point -= vec2(-R)*sinangle;
  point /= vec2(R);
  vec2 tang = sinangle/cosangle;
  vec2 poc = point/cosangle;
  float A = dot(tang,tang)+1.0;
  float B = -2.0*dot(poc,tang);
  float C = dot(poc,poc)-1.0;
  float a = (-B+sqrt(B*B-4.0*A*C))/(2.0*A);
  vec2 uv = (point-a*sinangle)/cosangle;
  float r = FIX(R*acos(a));
  return uv*r/sin(r/R);
}

vec2 transform(vec2 coord)
{
  coord *= textureSize / inputSize;
  coord = (coord-vec2(0.5))*aspect*stretch.z+stretch.xy;
  return (bkwtrans(coord)/overscan/aspect+vec2(0.5)) * inputSize / textureSize;
}

float corner(vec2 coord)
{
  coord *= textureSize / inputSize;
  coord = (coord - vec2(0.5)) * overscan + vec2(0.5);
  coord = min(coord, vec2(1.0)-coord) * aspect;
  vec2 cdist = vec2(cornersize);
  coord = (cdist - min(coord,cdist));
  float dist = sqrt(dot(coord,coord));
  return clamp((cdist.x-dist)*cornersmooth,0.0, 1.0);
}

// Calculate the influence of a scanline on the current pixel.
//
// 'distance' is the distance in texture coordinates from the current
// pixel to the scanline in question.
// 'color' is the colour of the scanline at the horizontal location of
// the current pixel.
vec4 scanlineWeights(float distance, vec4 color)
{
  // "wid" controls the width of the scanline beam, for each RGB channel
  // The "weights" lines basically specify the formula that gives
  // you the profile of the beam, i.e. the intensity as
  // a function of distance from the vertical center of the
  // scanline. In this case, it is gaussian if width=2, and
  // becomes nongaussian for larger widths. Ideally this should
  // be normalized so that the integral across the beam is
  // independent of its width. That is, for a narrower beam
  // "weights" should have a higher peak at the center of the
  // scanline than for a wider beam.
#ifdef USEGAUSSIAN
  vec4 wid = 0.3 + 0.1 * pow(color, vec4(3.0));
  vec4 weights = vec4(distance / wid);
  return 0.4 * exp(-weights * weights) / wid;
#else
  vec4 wid = 2.0 + 2.0 * pow(color, vec4(4.0));
  vec4 weights = vec4(distance / 0.3);
  return 1.4 * exp(-pow(weights * inversesqrt(0.5 * wid), wid)) / (0.6 + 0.2 * wid);
#endif
}

void main()
{
  // Here's a helpful diagram to keep in mind while trying to
  // understand the code:
  //
  //  |      |      |      |      |
  // -------------------------------
  //  |      |      |      |      |
  //  |  01  |  11  |  21  |  31  | <-- current scanline
  //  |      | @    |      |      |
  // -------------------------------
  //  |      |      |      |      |
  //  |  02  |  12  |  22  |  32  | <-- next scanline
  //  |      |      |      |      |
  // -------------------------------
  //  |      |      |      |      |
  //
  // Each character-cell represents a pixel on the output
  // surface, "@" represents the current pixel (always somewhere
  // in the bottom half of the current scan-line, or the top-half
  // of the next scanline). The grid of lines represents the
  // edges of the texels of the underlying texture.

  // Texture coordinates of the texel containing the active pixel.
#ifdef CURVATURE
  vec2 xy = transform(texCoord);
#else
  vec2 xy = texCoord;
#endif
  float cval = corner(xy);

  // Of all the pixels that are mapped onto the texel we are
  // currently rendering, which pixel are we currently rendering?
  vec2 ilvec = vec2(0.0,ilfac.y > 1.5 ? mod(2.0,2.0) : 0.0);
  vec2 ratio_scale = (xy * textureSize - vec2(0.5) + ilvec)/ilfac;
#ifdef OVERSAMPLE
  float filter = fwidth(ratio_scale.y);
#endif
  vec2 uv_ratio = fract(ratio_scale);

  // Snap to the center of the underlying texel.
  xy = (floor(ratio_scale)*ilfac + vec2(0.5) - ilvec) / textureSize;

  // Calculate Lanczos scaling coefficients describing the effect
  // of various neighbour texels in a scanline on the current
  // pixel.
  vec4 coeffs = PI * vec4(1.0 + uv_ratio.x, uv_ratio.x, 1.0 - uv_ratio.x, 2.0 - uv_ratio.x);

  // Prevent division by zero.
  coeffs = FIX(coeffs);

  // Lanczos2 kernel.
  coeffs = 2.0 * sin(coeffs) * sin(coeffs / 2.0) / (coeffs * coeffs);

  // Normalize.
  coeffs /= dot(coeffs, vec4(1.0));

  // Calculate the effective colour of the current and next
  // scanlines at the horizontal location of the current pixel,
  // using the Lanczos coefficients above.
  vec4 col  = clamp(mat4(
              TEX2D(xy + vec2(-one.x, 0.0)),
              TEX2D(xy),
              TEX2D(xy + vec2(one.x, 0.0)),
              TEX2D(xy + vec2(2.0 * one.x, 0.0))) * coeffs,
          0.0, 1.0);
  vec4 col2 = clamp(mat4(
              TEX2D(xy + vec2(-one.x, one.y)),
              TEX2D(xy + vec2(0.0, one.y)),
              TEX2D(xy + one),
              TEX2D(xy + vec2(2.0 * one.x, one.y))) * coeffs,
          0.0, 1.0);

#ifndef LINEAR_PROCESSING
  col  = pow(col , vec4(CRTgamma));
  col2 = pow(col2, vec4(CRTgamma));
#endif

  // Calculate the influence of the current and next scanlines on
  // the current pixel.
  vec4 weights  = scanlineWeights(uv_ratio.y, col);
  vec4 weights2 = scanlineWeights(1.0 - uv_ratio.y, col2);
#ifdef OVERSAMPLE
  uv_ratio.y =uv_ratio.y+1.0/3.0*filter;
  weights = (weights+scanlineWeights(uv_ratio.y, col))/3.0;
  weights2=(weights2+scanlineWeights(abs(1.0-uv_ratio.y), col2))/3.0;
  uv_ratio.y =uv_ratio.y-2.0/3.0*filter;
  weights=weights+scanlineWeights(abs(uv_ratio.y), col)/3.0;
  weights2=weights2+scanlineWeights(abs(1.0-uv_ratio.y), col2)/3.0;
#endif
  vec3 mul_res  = (col * weights + col2 * weights2).rgb * vec3(cval);

  // dot-mask emulation:
  // Output pixels are alternately tinted green and magenta.
  vec3 dotMaskWeights = mix(
          vec3(1.0, 0.7, 1.0),
          vec3(0.7, 1.0, 0.7),
          floor(mod(mod_factor, 2.0))
      );

  mul_res *= dotMaskWeights;

  // Convert the image gamma for display on our output device.
  mul_res = pow(mul_res, vec3(1.0 / monitorgamma));

  // Color the texel.
  gl_FragColor = vec4(mul_res, 1.0);
}
"#;

const VERT: &str = r#"
#define FIX(c) max(abs(c), 1e-5);

float intersect(vec2 xy)
{
  float A = dot(xy,xy)+d*d;
  float B = 2.0*(R*(dot(xy,sinangle)-d*cosangle.x*cosangle.y)-d*d);
  float C = d*d + 2.0*R*d*cosangle.x*cosangle.y;
  return (-B-sqrt(B*B-4.0*A*C))/(2.0*A);
}

vec2 bkwtrans(vec2 xy)
{
  float c = intersect(xy);
  vec2 point = vec2(c)*xy;
  point -= vec2(-R)*sinangle;
  point /= vec2(R);
  vec2 tang = sinangle/cosangle;
  vec2 poc = point/cosangle;
  float A = dot(tang,tang)+1.0;
  float B = -2.0*dot(poc,tang);
  float C = dot(poc,poc)-1.0;
  float a = (-B+sqrt(B*B-4.0*A*C))/(2.0*A);
  vec2 uv = (point-a*sinangle)/cosangle;
  float r = R*acos(a);
  return uv*r/sin(r/R);
}

vec2 fwtrans(vec2 uv)
{
  float r = FIX(sqrt(dot(uv,uv)));
  uv *= sin(r/R)/r;
  float x = 1.0-cos(r/R);
  float D = d/R + x*cosangle.x*cosangle.y+dot(uv,sinangle);
  return d*(uv*cosangle-x*sinangle)/D;
}

vec3 maxscale()
{
  vec2 c = bkwtrans(-R * sinangle / (1.0 + R/d*cosangle.x*cosangle.y));
  vec2 a = vec2(0.5,0.5)*aspect;
  vec2 lo = vec2(fwtrans(vec2(-a.x,c.y)).x,
          fwtrans(vec2(c.x,-a.y)).y)/aspect;
  vec2 hi = vec2(fwtrans(vec2(+a.x,c.y)).x,
          fwtrans(vec2(c.x,+a.y)).y)/aspect;
  return vec3((hi+lo)*aspect*0.5,max(hi.x-lo.x,hi.y-lo.y));
}


void main()
{
  // tilt angle in radians
  // (behavior might be a bit wrong if both components are nonzero)
  const vec2 angle = vec2(0.0,-0.0);

  // Precalculate a bunch of useful values we'll need in the fragment
  // shader.
  sinangle = sin(angle);
  cosangle = cos(angle);
  stretch = maxscale();

  // transform the texture coordinates
  gl_Position = projMat * viewMat * modelMat * vec4(position, 0.0, 1.0);
  texCoord = vert_uv;

  ilfac = vec2(1.0,floor(inputSize.y/200.0));

  // Resulting X pixel-coordinate of the pixel we're drawing.
  mod_factor = texCoord.x * textureSize.x * outputSize.x / inputSize.x;
}"#;
