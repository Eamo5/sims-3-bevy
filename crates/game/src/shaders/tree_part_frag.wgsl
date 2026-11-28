// A grown tree's bark, fronds and leaves lit as the game's standard materials, through the
// seasons as the billboards are (`tree_frag.wgsl`): in fall the leaves turn orange, gold and
// red, then drop a cluster at a time, leaving the branches and twigs (the fronds) bare through
// winter; evergreens keep theirs; snow settles on what faces up.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}
#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif

struct TreePart {
    params: vec4<f32>,
    // x: fall colour (0..1), y: leaves gone (0..1), z: snow (0..1), w: 1 for an evergreen
    season: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> part: TreePart;

fn hash3(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var c = pbr_input.material.base_color;
    // (The fronds are twigs, as the game's composite maps have them: bare through winter.)
    let foliage = part.params.x > 1.5;
    let evergreen = part.season.w > 0.5;
    if (foliage && !evergreen) {
        // (Clusters of leaves go at a time, by where they hang.)
        let cluster = floor(in.world_position.xyz * 0.9);
        if (part.season.y > 0.0 && hash3(cluster) < part.season.y) {
            discard;
        }
        if (part.season.x > 0.0) {
            let l = dot(c.rgb, vec3<f32>(0.3, 0.59, 0.11));
            let h = hash3(floor(in.world_position.xyz * 1.7));
            let turned = mix(mix(vec3<f32>(0.72, 0.22, 0.05), vec3<f32>(0.82, 0.45, 0.06), h), vec3<f32>(0.85, 0.66, 0.12), h * h) * (l * 2.6);
            c = vec4<f32>(mix(c.rgb, turned, part.season.x), c.a);
        }
    }
    if (part.season.z > 0.0) {
        // (Settled on what faces up.)
        let up = clamp(pbr_input.N.y, 0.0, 1.0);
        let l = dot(c.rgb, vec3<f32>(0.3, 0.59, 0.11));
        let k = select(0.2 + 0.6 * up, 0.18 + 0.32 * smoothstep(0.1, 0.45, l), foliage);
        c = vec4<f32>(mix(c.rgb, vec3<f32>(0.88, 0.9, 0.95), part.season.z * k), c.a);
    }
    pbr_input.material.base_color = alpha_discard(pbr_input.material, c);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
