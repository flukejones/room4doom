// Quad corners (u, v) in order TL, TR, BR, BL as two triangles (6 verts).
// u: 0 = left side, 1 = right side. v: 0 = top, 1 = bottom. Shared by the world
// sprite and weapon psprite shaders.
fn corner_uv(vertex: u32) -> vec2<f32> {
    switch vertex {
        case 0u: { return vec2<f32>(0.0, 0.0); } // TL
        case 1u: { return vec2<f32>(1.0, 0.0); } // TR
        case 2u: { return vec2<f32>(1.0, 1.0); } // BR
        case 3u: { return vec2<f32>(0.0, 0.0); } // TL
        case 4u: { return vec2<f32>(1.0, 1.0); } // BR
        default: { return vec2<f32>(0.0, 1.0); } // BL
    }
}
