// Rain, snow and hail: drops in a box round where the camera's looking, each falling (and
// snowflakes drifting) through it and wrapping round, so they stay put in the world as the
// camera moves. Each drop is a quad facing the camera: a thin streak along its fall for rain,
// a soft round flake for snow, a pellet for hail.

#import bevy_pbr::{
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}
#import bevy_pbr::forward_io::{Vertex, VertexOutput, FragmentOutput}

struct Precip {
    // xyz: the box's centre, w: time (s)
    centre: vec4<f32>,
    // x: kind (0 rain, 1 snow, 2 hail), y: how many of the drops fall (0..1), z, w: wind (m/s)
    params: vec4<f32>,
    // x: the box's size (m), y: drop size scale, z: light (0 night .. 1 day), w: unused
    look: vec4<f32>,
    home_to_local: mat4x4<f32>,
    away_to_local: mat4x4<f32>,
    shelter_size: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> precip: Precip;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var shelter: texture_2d_array<f32>;

fn under_shelter(world: vec3<f32>, transform: mat4x4<f32>, size: vec2<f32>, layer: i32) -> bool {
    let p = (transform * vec4<f32>(world, 1.0)).xz;
    if (any(p < vec2<f32>(0.0)) || any(p >= size)) { return false; }
    let d = fract(p) - vec2<f32>(0.5);
    var triangle = 3u;
    if (abs(d.y) > abs(d.x)) {
        triangle = select(2u, 0u, d.y < 0.0);
    } else if (d.x > 0.0) {
        triangle = 1u;
    }
    let ceilings = textureLoad(shelter, vec2<i32>(floor(p)), layer, 0);
    return world.y < ceilings[triangle];
}

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    // (Each drop's four corners share its random place in the box; the normal's x is a random
    // number of its own, which decides whether it falls at this density.)
    let seed = v.position;
    let r = v.normal.x;
    let kind = precip.params.x;
    if (r > precip.params.y) {
        out.position = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        return out;
    }
    let t = precip.centre.w;
    let size = precip.look.x;
    let box_ = vec3<f32>(size, size * 0.6, size);
    var fall = 8.5;
    if (kind > 1.5) {
        fall = 12.0;
    } else if (kind > 0.5) {
        fall = 1.1 + r * 0.6;
    }
    let vel = vec3<f32>(precip.params.z, -fall, precip.params.w);
    var q = seed * box_ + vel * t;
    if (kind > 0.5 && kind < 1.5) {
        // Snowflakes drift and swirl.
        q += vec3<f32>(sin(t * 0.9 + r * 40.0), 0.0, cos(t * 0.7 + r * 31.0)) * 0.6;
    }
    let origin = precip.centre.xyz - box_ * 0.5;
    let p = origin + fract((q - origin) / box_) * box_;

    // Facing the camera; rain stretched out along its fall.
    let to_cam = normalize(view.world_position - p);
    let along = normalize(vel);
    var right = normalize(cross(along, to_cam));
    var up = along;
    var w = 0.012;
    var h = 0.55;
    if (kind > 0.5) {
        right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), to_cam));
        up = cross(to_cam, right);
        w = select(0.05, 0.025, kind > 1.5);
        h = w;
    }
    let s = precip.look.y;
    let corner = p + right * (v.uv.x - 0.5) * w * s + up * (v.uv.y - 0.5) * h * s;
    out.world_position = vec4<f32>(corner, 1.0);
    out.position = position_world_to_clip(corner);
    out.uv = v.uv;
    // (How far out in the box: drops fade towards its edges, so its sides never show.)
    let d = length((p - precip.centre.xyz).xz) / (size * 0.5);
    out.world_normal = vec3<f32>(1.0 - smoothstep(0.6, 1.0, d), r, 0.0);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> FragmentOutput {
    if (under_shelter(in.world_position.xyz, precip.home_to_local, precip.shelter_size.xy, 0)
        || under_shelter(in.world_position.xyz, precip.away_to_local, precip.shelter_size.zw, 1)) {
        discard;
    }
    var out: FragmentOutput;
    let kind = precip.params.x;
    let fade = in.world_normal.x;
    let light = mix(0.18, 1.0, precip.look.z);
    var a: f32;
    var col: vec3<f32>;
    if (kind < 0.5) {
        let across = 1.0 - abs(in.uv.x - 0.5) * 2.0;
        let along = 1.0 - abs(in.uv.y - 0.5) * 2.0;
        a = across * smoothstep(0.0, 0.5, along) * 0.32;
        col = vec3<f32>(0.78, 0.82, 0.9);
    } else {
        let d = length(in.uv - vec2<f32>(0.5)) * 2.0;
        a = (1.0 - smoothstep(0.45, 1.0, d)) * select(0.9, 0.85, kind > 1.5);
        col = select(vec3<f32>(0.97, 0.98, 1.0), vec3<f32>(0.9, 0.93, 0.97), kind > 1.5);
    }
    a *= fade;
    if (a < 0.01) {
        discard;
    }
    out.color = vec4<f32>(col * light, a);
    return out;
}
