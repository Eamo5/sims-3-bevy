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

// rgb: skin tint, w: number of clothing layers (plus ten for hair under a hat, which keeps
// its alpha)
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> skin_params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var layer0_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var layer0_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var layer1_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var layer1_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var layer2_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var layer2_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var layer3_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var layer3_samp: sampler;
// (Four more, for a made-up face: sampled with the first layer's sampler.)
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var layer4_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var layer5_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var layer6_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var layer7_tex: texture_2d<f32>;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var c = pbr_input.material.base_color.rgb * skin_params.rgb;
    let hair = skin_params.w > 9.5;
    var n = skin_params.w;
    if (hair) { n = n - 10.0; }
#ifdef VERTEX_UVS_A
    let uv = in.uv;
    let l0 = textureSample(layer0_tex, layer0_samp, uv);
    let l1 = textureSample(layer1_tex, layer1_samp, uv);
    let l2 = textureSample(layer2_tex, layer2_samp, uv);
    let l3 = textureSample(layer3_tex, layer3_samp, uv);
    if (n > 0.5) { c = mix(c, l0.rgb, l0.a); }
    if (n > 1.5) { c = mix(c, l1.rgb, l1.a); }
    if (n > 2.5) { c = mix(c, l2.rgb, l2.a); }
    if (n > 3.5) { c = mix(c, l3.rgb, l3.a); }
    if (n > 4.5) {
        let l4 = textureSample(layer4_tex, layer0_samp, uv);
        let l5 = textureSample(layer5_tex, layer0_samp, uv);
        let l6 = textureSample(layer6_tex, layer0_samp, uv);
        let l7 = textureSample(layer7_tex, layer0_samp, uv);
        c = mix(c, l4.rgb, l4.a);
        if (n > 5.5) { c = mix(c, l5.rgb, l5.a); }
        if (n > 6.5) { c = mix(c, l6.rgb, l6.a); }
        if (n > 7.5) { c = mix(c, l7.rgb, l7.a); }
    }
#endif
    pbr_input.material.base_color = vec4<f32>(c, select(1.0, pbr_input.material.base_color.a, hair));
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
