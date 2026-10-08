// The sky dome: a gradient from the horizon to the zenith that follows the time of day, the
// game's cloud noise drifting across it, the sun with its halo, the moon, and the stars at night.

#import bevy_pbr::mesh_view_bindings::view

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{VertexOutput, FragmentOutput}
#else
#import bevy_pbr::forward_io::{VertexOutput, FragmentOutput}
#endif

struct Sky {
    // xyz: direction towards the sun, w: daylight (0 night .. 1 noon)
    sun: vec4<f32>,
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    // rgb: sun colour, w: twilight
    sun_color: vec4<f32>,
    // x: time (s), y: night (0..1), z: cloud cover, w: unused
    params: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> sky: Sky;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var cloud_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var cloud_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var star_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var star_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var halo_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var halo_samp: sampler;

// A disc-shaped texture (sun or moon halo) centred on `towards`, `size` radians across.
fn billboard(dir: vec3<f32>, towards: vec3<f32>, size: f32) -> vec4<f32> {
    let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(towards.y) > 0.99);
    let right = normalize(cross(up, towards));
    let top = cross(towards, right);
    let d = dot(dir, towards);
    if (d <= 0.0) {
        return vec4<f32>(0.0);
    }
    let p = vec2<f32>(dot(dir, right), dot(dir, top)) / d / size + 0.5;
    if (any(p < vec2<f32>(0.0)) || any(p > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    return textureSample(halo_tex, halo_samp, p);
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
#ifndef PREPASS_PIPELINE
    let dir = normalize(in.world_position.xyz - view.world_position);
    let t = sky.params.x;
    let night = sky.params.y;
    let day = sky.sun.w;
    let sun_dir = normalize(sky.sun.xyz);

    // The gradient, with a little haze at the horizon.
    let h = max(dir.y, 0.0);
    var col = mix(sky.horizon.rgb, sky.zenith.rgb, pow(h, 0.5));
    if (dir.y < 0.0) {
        col = sky.horizon.rgb * (1.0 + dir.y * 0.6);
    }

    // The sun: a bright core and the game's halo around it.
    let sd = max(dot(dir, sun_dir), 0.0);
    let glow = pow(sd, 6.0) * 0.25 + pow(sd, 64.0) * 0.5;
    col += sky.sun_color.rgb * glow * (0.35 + 0.65 * day) * (1.0 - night);
    let halo = billboard(dir, sun_dir, 0.22);
    col += sky.sun_color.rgb * halo.rgb * halo.a * 0.8 * (1.0 - night);
    col += sky.sun_color.rgb * smoothstep(0.99975, 0.9999, sd) * 3.0 * (1.0 - night);

    // The moon, across the sky from the sun.
    let moon_dir = normalize(vec3<f32>(-sun_dir.x, max(-sun_dir.y, 0.15), -sun_dir.z));
    let md = max(dot(dir, moon_dir), 0.0);
    let moon = billboard(dir, moon_dir, 0.16);
    col += vec3<f32>(0.75, 0.82, 1.0) * (moon.rgb * moon.a * 0.6 + smoothstep(0.99985, 0.99995, md) * 1.5) * night;

    // Stars, away from the horizon.
    let star_uv = dir.xz / (dir.y + 1.0) * 1.6;
    let stars = textureSample(star_tex, star_samp, star_uv).rgb;
    col += stars * night * night * smoothstep(0.05, 0.35, dir.y) * 0.9;

    // Clouds: two layers of the game's noise, drifting, lit by the sun.
    let p = dir.xz / max(dir.y + 0.08, 0.04) * 0.09;
    let n = textureSample(cloud_tex, cloud_samp, p + vec2<f32>(t * 0.0012, t * 0.0005)).r * 0.65
        + textureSample(cloud_tex, cloud_samp, p * 2.7 + vec2<f32>(-t * 0.0008, t * 0.0011)).r * 0.35;
    let cover = sky.params.z;
    let c = smoothstep(0.42 - cover * 0.2, 0.62 - cover * 0.1, n) * smoothstep(0.0, 0.12, dir.y);
    // (Grey and heavy under an overcast sky.)
    let lit = mix(vec3<f32>(0.10, 0.11, 0.16), mix(sky.horizon.rgb * 1.1, vec3<f32>(1.0, 0.99, 0.97), day), 1.0 - night) * (1.0 - 0.4 * sky.params.w);
    let sunlit = sky.sun_color.rgb * pow(sd, 4.0) * 0.4 * (1.0 - night);
    col = mix(col, lit + sunlit, c * 0.85);

    out.color = vec4<f32>(col, 1.0);
#endif
    return out;
}
