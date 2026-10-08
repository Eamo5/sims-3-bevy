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

// x: overlay present, y: opacity present
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> road_params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var overlay_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var overlay_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var opacity_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var opacity_samp: sampler;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    var c = pbr_input.material.base_color.rgb;
    var a = 1.0;
#ifdef VERTEX_UVS_B
    let uv1 = in.uv_b;
    let ov = textureSample(overlay_tex, overlay_samp, uv1);
    let op = textureSample(opacity_tex, opacity_samp, uv1).r;
    if (road_params.x > 0.5) {
        c = mix(c, ov.rgb, ov.a);
    }
    if (road_params.y > 0.5) {
        a = op;
    }
#endif
    // (The seasons: z snow on the road, w how wet.)
    c = mix(c, vec3<f32>(0.84, 0.87, 0.92), clamp(road_params.z * 1.6 - 0.4, 0.0, 0.85));
    c = c * (1.0 - 0.35 * road_params.w);
    if (road_params.w > 0.001) {
        pbr_input.material.perceptual_roughness = mix(pbr_input.material.perceptual_roughness, 0.25, road_params.w);
    }
    pbr_input.material.base_color = vec4<f32>(c, a);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
