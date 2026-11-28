// A tree up close, grown from its SpeedTree parameters: its bark, its fronds, and its leaves,
// cards turned to face the view as SpeedTree's are (each corner's offset across its card in the
// second uv set). All of it sways in the wind, the higher the more, the leaves fluttering; and
// a tree standing between the camera and what it's looking at is taken out of the way, as the
// billboards are (`tree.wgsl`).

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{Vertex, VertexOutput}
// (The prepasses have the time elsewhere.)
#import bevy_render::globals::Globals
@group(0) @binding(1) var<uniform> globals: Globals;
#else
#import bevy_pbr::forward_io::{Vertex, VertexOutput}
#import bevy_pbr::mesh_view_bindings::globals
#endif

struct TreePart {
    // x: 0 bark, 1 fronds, 2 leaves; y: the tree's height (m); z: its crown's reach (m); w: 1 for a
    // shrub or flower, never taken out of the way
    params: vec4<f32>,
    // (The seasons, for the fragment shader.)
    season: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> part: TreePart;

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let model = mesh_functions::get_world_from_local(v.instance_index);
    let origin = model[3].xyz;
    let scale = length(model[1].xyz);
    let h = part.params.y * scale;
    let mode = part.params.x;
    var p = (model * vec4<f32>(v.position, 1.0)).xyz;
#ifdef VERTEX_UVS_B
    if (mode > 1.5) {
        // (The card faces the view: its corner across it, to the view's right and up.)
        let right = normalize(view.world_from_view[0].xyz);
        let up = normalize(view.world_from_view[1].xyz);
        p = p + (right * v.uv_b.x + up * v.uv_b.y) * scale;
    }
#endif
    // The wind: the whole tree swaying, the more the higher up, and the leaves fluttering.
    let rel = clamp((p.y - origin.y) / max(h, 0.1), 0.0, 1.0);
    let t = globals.time;
    let phase = dot(origin.xz, vec2<f32>(0.13, 0.17));
    let sway = sin(t * 0.8 + phase) * 0.7 + sin(t * 1.9 + phase * 1.7) * 0.3;
    var amount = rel * rel * h * 0.01;
    if (mode > 0.5) {
        amount = amount + rel * h * 0.004 * sin(t * 3.1 + dot(p, vec3<f32>(0.9, 0.7, 1.1)));
    }
    p = p + vec3<f32>(1.0, 0.0, 0.6) * sway * amount;
#ifndef PREPASS_PIPELINE
    // Out of the way when it's between the camera and what it's looking at (or the camera's
    // in it). Not in the sun's view: the shadow stays.
    let back = normalize(view.world_from_view[2].xyz);
    let cam = view.world_position;
    let mid = origin + vec3<f32>(0.0, h * 0.55, 0.0);
    let reach = max(part.params.z * scale, h * 0.25);
    var hide = part.params.w < 0.5 && distance(cam, mid) < max(h * 0.7, reach) + 1.0;
    let ahead = -back;
    if (ahead.y < -0.05 && part.params.w < 0.5) {
        let seg = ahead * ((origin.y - cam.y) / ahead.y);
        let s = clamp(dot(mid - cam, seg) / max(dot(seg, seg), 1e-4), 0.0, 1.0);
        hide = hide || (s < 0.92 && distance(mid, cam + seg * s) < reach + 1.0);
    }
    if (hide) {
        out.position = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        return out;
    }
#endif
    out.world_position = vec4<f32>(p, 1.0);
    out.position = position_world_to_clip(p);
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#ifdef VERTEX_UVS_A
    out.uv = v.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = v.uv_b;
#endif
#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.previous_world_position = out.world_position;
#endif
#else
    out.world_normal = mesh_functions::mesh_normal_local_to_world(v.normal, v.instance_index);
    // (Leaves and fronds lit mostly towards the view and up, as SpeedTree's and the billboards'
    // are: the crown's not black on its far side.)
    if (mode > 0.5) {
        out.world_normal = normalize(out.world_normal * 0.4 + normalize(cam - p) * 0.75 + vec3<f32>(0.0, 0.5, 0.0));
    }
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = v.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(v.instance_index, model[3]);
#endif
    return out;
}
