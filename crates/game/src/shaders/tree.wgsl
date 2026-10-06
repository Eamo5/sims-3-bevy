// Trees from the game's 360° billboard pictures: one picture standing at the tree, turned to
// face the view, showing the tree from the side it's seen from (its atlas has it from all the
// way round). In the shadow pass the view is the sun's, so the shadow is the tree's outline.

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{Vertex, VertexOutput}
#else
#import bevy_pbr::forward_io::{Vertex, VertexOutput}
#endif

struct TreeBillboard {
    // The views round the tree, in order: uv rectangles (u0, v0, u1, v1).
    views: array<vec4<f32>, 16>,
    // x: how many views, y: the atlas's width / height, z: the tree's height (metres).
    params: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> tree: TreeBillboard;

@vertex
fn vertex(v: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let model = mesh_functions::get_world_from_local(v.instance_index);
    let origin = model[3].xyz;
    let scale = length(model[1].xyz);

    // Facing the view: upright, leaning back with the view's pitch so the picture is seen whole.
    let back = normalize(view.world_from_view[2].xyz);
    var flat = vec3<f32>(back.x, 0.0, back.z);
    if (dot(flat, flat) < 1e-6) {
        flat = vec3<f32>(0.0, 0.0, 1.0);
    }
    flat = normalize(flat);
    let right = normalize(cross(vec3<f32>(0.0, 1.0, 0.0), flat));
    let up = normalize(cross(back, right));

    // The view of the tree from this side (the tree's own turn taken into account).
    let fwd = normalize(vec2<f32>(model[2].x, model[2].z) + vec2<f32>(0.0, 1e-6));
    let side = normalize(vec2<f32>(model[0].x, model[0].z) + vec2<f32>(1e-6, 0.0));
    let c = vec2<f32>(flat.x, flat.z);
    let angle = atan2(dot(c, side), dot(c, fwd));
    let n = max(tree.params.x, 1.0);
    let i = u32(round(angle / 6.2831853 * n) + n) % u32(n);
    let r = tree.views[min(i, 15u)];

    // Its picture's shape, standing on the ground at the tree.
    let h = tree.params.z * scale;
    let w = h * (r.z - r.x) * tree.params.y / max(r.w - r.y, 0.001);
    let p = origin + right * ((v.uv.x - 0.5) * w) + up * ((1.0 - v.uv.y) * h);

    out.world_position = vec4<f32>(p, 1.0);
    out.position = position_world_to_clip(p);
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#ifdef VERTEX_UVS_A
    out.uv = vec2<f32>(mix(r.x, r.z, v.uv.x), mix(r.y, r.w, v.uv.y));
#endif
    // Lit as a whole crown from above, so every side looks alike.
#ifdef PREPASS_PIPELINE
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = vec3<f32>(0.0, 1.0, 0.0);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.previous_world_position = out.world_position;
#endif
#else
    out.world_normal = vec3<f32>(0.0, 1.0, 0.0);
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = v.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(v.instance_index, model[3]);
#endif
    return out;
}
