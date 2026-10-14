// Close up, where the picture's texels are blown up into a blur, the leaves are drawn in: each
// texel's patch of green broken into a few leaves, lit and shaded, with ragged edges, as the
// game's own leaf cards are up close.
//
// Trees through the seasons: in fall the leaves of the picture turn orange, gold and red and
// then drop, clusters at a time, leaving the trunk and branches bare through winter until they
// grow back in spring; evergreens keep theirs. Snow frosts them over.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

struct TreeBillboard {
    views: array<vec4<f32>, 16>,
    params: vec4<f32>,
    // x: fall colour (0..1), y: leaves gone (0..1), z: snow (0..1), w: 1 for an evergreen
    season: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> tree: TreeBillboard;

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var c = pbr_input.material.base_color;
    let evergreen = tree.season.w > 0.5;
#ifdef VERTEX_UVS_A
    let uv = in.uv;
#else
    let uv = vec2<f32>(0.0);
#endif
    // The leaves: the greens of the picture.
    let leafy = smoothstep(0.005, 0.05, c.g - max(c.r, c.b) * 0.92);
    // (How blown up the picture is: screen pixels to a texel.)
    let dims = vec2<f32>(tree.params.w, tree.params.w / max(tree.params.y, 0.01));
    let texel = uv * dims;
    let near = smoothstep(1.6, 4.0, 1.0 / max(length(fwidth(texel)), 1e-4));
    if (near > 0.0 && leafy > 0.0) {
        // A few leaves to a texel: each a lit middle and shaded gaps, a little lighter or
        // darker than its neighbours.
        // (Skewed so the leaves don't line up in rows.)
        let g = vec2<f32>(texel.x * 0.9 + texel.y * 0.35, texel.y * 0.8 - texel.x * 0.3);
        let cell = floor(g);
        var leaf = 0.0;
        for (var j = -1; j <= 1; j++) {
            for (var i = -1; i <= 1; i++) {
                let n = cell + vec2<f32>(f32(i), f32(j));
                let at = n + vec2<f32>(hash(n), hash(n + vec2<f32>(17.3, 5.1)));
                let d = g - at;
                // (An oval leaf, turned its own way.)
                let a = hash(n + vec2<f32>(8.1, 2.9)) * 6.2831853;
                let r = vec2<f32>(d.x * cos(a) + d.y * sin(a), (d.y * cos(a) - d.x * sin(a)) * 1.7);
                leaf = max(leaf, (1.0 - smoothstep(0.15, 0.6, length(r))) * mix(0.75, 1.0, hash(n + vec2<f32>(3.7, 9.2))));
            }
        }
        let shade = mix(0.8, 1.08, leaf);
        let k = near * leafy;
        c = vec4<f32>(c.rgb * mix(1.0, shade, k), clamp(c.a + (leaf - 0.5) * 0.7 * k * (1.0 - c.a) * 2.0, 0.0, 1.0));
    }
    if (!evergreen) {
        // (Whole clusters of leaves go at a time.)
        if (tree.season.y > 0.0 && leafy > 0.5 && hash(floor(uv * 700.0)) < tree.season.y) {
            discard;
        }
        if (tree.season.x > 0.0) {
            let l = dot(c.rgb, vec3<f32>(0.3, 0.59, 0.11));
            let h = hash(floor(uv * 160.0));
            let turned = mix(mix(vec3<f32>(0.72, 0.22, 0.05), vec3<f32>(0.82, 0.45, 0.06), h), vec3<f32>(0.85, 0.66, 0.12), h * h) * (l * 2.6);
            c = vec4<f32>(mix(c.rgb, turned, tree.season.x * leafy), c.a);
        }
    }
    if (tree.season.z > 0.0) {
        // (Settled on the lighter, upper-facing bits.)
        let l = dot(c.rgb, vec3<f32>(0.3, 0.59, 0.11));
        c = vec4<f32>(mix(c.rgb, vec3<f32>(0.88, 0.9, 0.95), tree.season.z * (0.18 + 0.32 * smoothstep(0.1, 0.45, l))), c.a);
    }
    pbr_input.material.base_color = alpha_discard(pbr_input.material, c);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
