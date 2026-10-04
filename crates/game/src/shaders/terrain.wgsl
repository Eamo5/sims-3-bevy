#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
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

// x: world size (m), y: 1 / layer tile size, z: layer count
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> terrain_params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var layer_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var layer_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var weight_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var weight_samp: sampler;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let wp = in.world_position.xz;
    let wuv = wp / terrain_params.x;
    let tuv = wp * terrain_params.y;
    let ddx_t = dpdx(tuv);
    let ddy_t = dpdy(tuv);
    let n = i32(terrain_params.z);

    var col = vec3<f32>(0.35, 0.45, 0.25);
    if (n > 0) {
        col = textureSampleGrad(layer_tex, layer_samp, tuv, 0, ddx_t, ddy_t).rgb;
    }
    // Paint layers are composited in order over the base layer.
    for (var g = 0; g < 4; g = g + 1) {
        var w4 = textureSampleLevel(weight_tex, weight_samp, wuv, g, 0.0);
        for (var c = 0; c < 4; c = c + 1) {
            let li = g * 4 + c;
            if (li == 0 || li >= n) {
                continue;
            }
            let w = w4[c];
            if (w > 0.003) {
                let s = textureSampleGrad(layer_tex, layer_samp, tuv, li, ddx_t, ddy_t).rgb;
                col = mix(col, s, w);
            }
        }
    }
    pbr_input.material.base_color = vec4<f32>(col, 1.0);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
