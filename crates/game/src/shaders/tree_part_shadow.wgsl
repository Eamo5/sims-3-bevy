// A grown tree's shadow: its leaves cut out as they're drawn, and gone through winter as they
// are (`tree_part_frag.wgsl`), so the bare tree's shadow is bare too.

#import bevy_pbr::{pbr_prepass_functions, prepass_io}

struct TreePart {
    params: vec4<f32>,
    season: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> part: TreePart;

fn hash3(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

fn cut(in: prepass_io::VertexOutput) {
    pbr_prepass_functions::prepass_alpha_discard(in);
    if (part.params.x > 1.5 && part.season.w < 0.5 && part.season.y > 0.0 && hash3(floor(in.world_position.xyz * 0.9)) < part.season.y) {
        discard;
    }
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
