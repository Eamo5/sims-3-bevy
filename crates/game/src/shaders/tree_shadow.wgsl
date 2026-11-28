// A billboard tree's shadow: its outline, its leaves gone from it through winter as they go from
// its picture (`tree_frag.wgsl`), so the bare tree's shadow is bare too.

#import bevy_pbr::{pbr_prepass_functions, prepass_io, pbr_bindings}

struct TreeBillboard {
    views: array<vec4<f32>, 16>,
    params: vec4<f32>,
    season: vec4<f32>,
    near: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> tree: TreeBillboard;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn cut(in: prepass_io::VertexOutput) {
    pbr_prepass_functions::prepass_alpha_discard(in);
#ifdef VERTEX_UVS_A
#ifndef BINDLESS
    if (tree.season.w < 0.5 && tree.season.y > 0.0) {
        let c = textureSampleLevel(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, in.uv, 0.0);
        let leafy = smoothstep(0.005, 0.05, c.g - max(c.r, c.b) * 0.92);
        if (leafy > 0.5 && hash(floor(in.uv * 700.0)) < tree.season.y) {
            discard;
        }
    }
#endif
#endif
}

#ifdef PREPASS_FRAGMENT
@fragment
fn fragment(in: prepass_io::VertexOutput, @builtin(front_facing) is_front: bool) -> prepass_io::FragmentOutput {
    cut(in);
    var out: prepass_io::FragmentOutput;
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.frag_depth = in.unclipped_depth;
#endif
    return out;
}
#else
@fragment
fn fragment(in: prepass_io::VertexOutput) {
    cut(in);
}
#endif
