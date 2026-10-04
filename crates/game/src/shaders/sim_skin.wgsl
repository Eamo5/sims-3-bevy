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

// rgb: skin tint, w: number of clothing layers
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> skin_params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var layer0_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var layer0_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var layer1_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var layer1_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var layer2_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var layer2_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var layer3_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var layer3_samp: sampler;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var c = pbr_input.material.base_color.rgb * skin_params.rgb;
#ifdef VERTEX_UVS_A
    let uv = in.uv;
    let l0 = textureSample(layer0_tex, layer0_samp, uv);
    let l1 = textureSample(layer1_tex, layer1_samp, uv);
    let l2 = textureSample(layer2_tex, layer2_samp, uv);
    let l3 = textureSample(layer3_tex, layer3_samp, uv);
    let n = skin_params.w;
    if (n > 0.5) { c = mix(c, l0.rgb, l0.a); }
    if (n > 1.5) { c = mix(c, l1.rgb, l1.a); }
    if (n > 2.5) { c = mix(c, l2.rgb, l2.a); }
    if (n > 3.5) { c = mix(c, l3.rgb, l3.a); }
#endif
    pbr_input.material.base_color = vec4<f32>(c, 1.0);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
