// Water: drifting ripples from a tiling slope field, the sky and the sun reflected (more at
// grazing angles), colour deepening with the depth of water over the terrain, clear shallows and
// foam along the shore.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    mesh_view_bindings::view,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct Water {
    // xyz: towards the sun, w: daylight (0 night .. 1 noon)
    sun: vec4<f32>,
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    sun_color: vec4<f32>,
    deep: vec4<f32>,
    shallow: vec4<f32>,
    // x: time (s), y: heightmap size (samples), z: metres per height unit, w: 0 sea, 1 pond
    misc: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: Water;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var height_tex: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var ripple_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var ripple_samp: sampler;

// Terrain height (m) under a point, bilinear between the heightmap's 1 m samples.
fn height_at(p: vec2<f32>) -> f32 {
    let n = i32(water.misc.y);
    let q = clamp(p, vec2<f32>(0.0), vec2<f32>(f32(n - 1)));
    let i = vec2<i32>(floor(q));
    let f = q - floor(q);
    let j = min(i + vec2<i32>(1), vec2<i32>(n - 1));
    let h00 = f32(textureLoad(height_tex, i, 0).r);
    let h10 = f32(textureLoad(height_tex, vec2<i32>(j.x, i.y), 0).r);
    let h01 = f32(textureLoad(height_tex, vec2<i32>(i.x, j.y), 0).r);
    let h11 = f32(textureLoad(height_tex, j, 0).r);
    return mix(mix(h00, h10, f.x), mix(h01, h11, f.x), f.y) * water.misc.z;
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let p = in.world_position.xyz;
    let t = water.misc.x;
    let pond = water.misc.w > 0.5;

    // Ripples: the slope field at three scales, drifting different ways (ponds stiller).
    let s1 = textureSample(ripple_tex, ripple_samp, p.xz / 9.0 + vec2<f32>(t * 0.011, t * 0.007)).rg * 2.0 - 1.0;
    let s2 = textureSample(ripple_tex, ripple_samp, p.xz / 4.3 + vec2<f32>(-t * 0.016, t * 0.012)).rg * 2.0 - 1.0;
    let s3 = textureSample(ripple_tex, ripple_samp, p.xz / 23.0 + vec2<f32>(t * 0.005, -t * 0.006)).rg * 2.0 - 1.0;
    let strength = select(0.55, 0.22, pond);
    let slope = (s1 * 0.5 + s2 * 0.35 + s3 * 0.6) * strength;
    let N = normalize(vec3<f32>(-slope.x, 1.0, -slope.y));
    let V = normalize(view.world_position.xyz - p);

    // Deeper water is darker.
    let depth = max(p.y - height_at(p.xz), 0.0);
    let deepness = 1.0 - exp(-depth * select(0.35, 0.6, pond));
    let body = mix(water.shallow.rgb, water.deep.rgb, deepness);
    pbr_input.material.base_color = vec4<f32>(body, 1.0);
    pbr_input.N = N;
    pbr_input.world_normal = N;

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    let lit = apply_pbr_lighting(pbr_input);
    // The sky reflected, more at grazing angles; the sun's glints.
    let ndv = max(dot(N, V), 0.0);
    let fres = 0.06 + 0.94 * pow(1.0 - ndv, 4.0);
    let r = reflect(-V, N);
    let sky = mix(water.horizon.rgb, water.zenith.rgb, clamp(r.y * 1.6, 0.0, 1.0));
    let to_sun = max(dot(r, water.sun.xyz), 0.0);
    let glint = (pow(to_sun, 700.0) * 8.0 + pow(to_sun, 40.0) * 0.25) * water.sun.w;
    var col = mix(lit.rgb, sky, fres * 0.9) + water.sun_color.rgb * glint;
    // Foam where the water meets the shore.
    let edge = 1.0 - smoothstep(0.0, select(0.45, 0.2, pond), depth);
    let froth = 0.55 + 0.45 * sin(t * 1.3 + (s3.x + s1.y) * 6.0);
    col = mix(col, vec3<f32>(0.85, 0.9, 0.9) * (0.35 + 0.65 * water.sun.w), edge * froth * select(0.7, 0.35, pond));
    // Clearer in the shallows, opaque in the deep and at grazing angles, soft at the very edge.
    let alpha = clamp(mix(0.5, 0.96, smoothstep(0.0, 1.5, depth)) + fres * 0.4 + edge * 0.2, 0.0, 1.0) * smoothstep(0.0, 0.06, depth);
    out.color = vec4<f32>(col, alpha);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
