#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::view,
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

// x: world size (m), y: 1 / layer tile size, z: layer count, w: flags (1 overview, 2 lightmap)
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> terrain_params: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var layer_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var layer_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var weight_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var weight_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var overview_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var overview_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var light_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var light_samp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var<uniform> layer_avg: array<vec4<f32>, 16>;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var<uniform> terrain_night: vec4<f32>;

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
    var avg = col;
    if (n > 0) {
        col = textureSampleGrad(layer_tex, layer_samp, tuv, 0, ddx_t, ddy_t).rgb;
        avg = layer_avg[0].rgb;
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
                avg = mix(avg, layer_avg[li].rgb, w);
            }
        }
    }
    let flags = u32(terrain_params.w);
    if ((flags & 1u) != 0u) {
        // Colour comes from the game's own pre-composited terrain map (paint, lot ground, tree
        // shadows); the paint layers only add their texture detail on top of it.
        let ov = textureSample(overview_tex, overview_samp, wuv).rgb;
        let detail = clamp(col / max(avg, vec3<f32>(0.01)), vec3<f32>(0.0), vec3<f32>(2.5));
        let dist = distance(view.world_position.xyz, in.world_position.xyz);
        col = ov * mix(detail, vec3<f32>(1.0), smoothstep(150.0, 450.0, dist));
    } else if ((flags & 2u) != 0u) {
        let shade = textureSample(light_tex, light_samp, wuv).a;
        col = col * mix(0.55, 1.0, shade);
    }
    pbr_input.material.base_color = vec4<f32>(col, 1.0);
    if ((flags & 2u) != 0u && terrain_night.x > 0.01) {
        // Pools of street light baked into the world's light map.
        let glow = textureSample(light_tex, light_samp, wuv).rgb;
        pbr_input.material.emissive = vec4<f32>(glow * glow * terrain_night.x * 2.5, 1.0);
    }

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
