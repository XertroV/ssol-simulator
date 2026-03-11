// Source: Mostly ported from OpenRelativity
// Original Author: MITGameLab
// License: MIT License.

#import bevy_pbr::{
    mesh_functions::{get_world_from_local, get_local_from_world, mesh_position_local_to_clip, mesh_position_local_to_world},
    view_transformations::position_world_to_clip,
}
#import "shaders/relativistic_math.wgsl"::{UV_START, UV_RANGE, IR_START, IR_RANGE, RGBToXYZC, weightFromXYZCurves, getXFromCurve, getYFromCurve, getZFromCurve, XYZToRGBC, constrainRGB}
#import "shaders/rel_structs.wgsl"::{RelativisticUniforms}

fn rgb_to_hsv(color: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = select(
        vec4<f32>(color.b, color.g, k.w, k.z),
        vec4<f32>(color.g, color.b, k.x, k.y),
        color.g >= color.b,
    );
    let q = select(
        vec4<f32>(p.x, p.y, p.w, color.r),
        vec4<f32>(color.r, p.y, p.z, p.x),
        color.r >= p.x,
    );
    let d = q.x - min(q.w, q.y);
    let e = 1.0e-10;
    return vec3<f32>(
        abs(q.z + (q.w - q.y) / (6.0 * d + e)),
        d / (q.x + e),
        q.x,
    );
}

fn hsv_to_rgb(hsv: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let base = vec3<f32>(k.x, k.x, k.x);
    let p = abs(fract(vec3<f32>(hsv.x, hsv.x, hsv.x) + k.xyz) * 6.0 - vec3<f32>(k.w, k.w, k.w));
    let rgb_target = clamp(p - base, vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0));
    return hsv.z * (base + (rgb_target - base) * hsv.y);
}

@group(3) @binding(0) var<uniform> sky_color: vec4<f32>;
@group(3) @binding(1) var<uniform> material: RelativisticUniforms;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // @location(1) normal: vec3<f32>,
    // @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_pos: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) vr: vec4<f32>, // velocity relative
    @location(3) svc: f32, // lorentz factor
    @location(4) draw: f32, // whether to draw this vertex (0 or 1)
    // @location(5) pos: vec4<f32>, // final position in local space
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    // out.uv = vertex.uv;
    out.draw = 1;

    let world_from_local = get_world_from_local(vertex.instance_index);
    var pos_world = world_from_local * vec4<f32>(vertex.position, 1.0);
    out.clip_position = position_world_to_clip(pos_world.xyz);
    pos_world -= material.player_offset;

    let vr = material.vpc - material.viw;
    let speed_sq = dot(vr, vr);
    let speed = sqrt(speed_sq);
    out.vr = vr;
    out.svc = sqrt(1.0 - speed_sq);
    out.world_pos = pos_world;

    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let vr = in.vr;
    let svc = in.svc;
    let pos = in.world_pos.xyz;

    var shift = 1.0f;
    if material.color_shift > 0u {
        let shift_numerator = 1.0 - (dot(pos, vr.xyz) / length(pos));
        shift = shift_numerator / svc;
    }

    var data = sky_color;
    let uv_val = data.r * 4;
    let ir_val = data.r * 4;

    let xyz = RGBToXYZC(data.r, data.g, data.b);
    let weights = weightFromXYZCurves(xyz);

    var rParam = vec3(weights.x, 615.0, 8.0);
    var gParam = vec3(weights.y, 550.0, 4.0);
    var bParam = vec3(weights.z, 463.0, 5.0);
    var uVParam = vec3(0.1, UV_START + UV_RANGE * uv_val, 1.0);
    var iRParam = vec3(0.1, IR_START + IR_RANGE * ir_val, 1.0);

    let inv_shift_cubed = pow(1.0 / shift, 3.0);

    let xf = inv_shift_cubed * (getXFromCurve(rParam, shift) + getXFromCurve(gParam, shift) + getXFromCurve(bParam, shift) + getXFromCurve(iRParam, shift) + getXFromCurve(uVParam, shift));
    let yf = inv_shift_cubed * (getYFromCurve(rParam, shift) + getYFromCurve(gParam, shift) + getYFromCurve(bParam, shift) + getYFromCurve(iRParam, shift) + getYFromCurve(uVParam, shift));
    let zf = inv_shift_cubed * (getZFromCurve(rParam, shift) + getZFromCurve(gParam, shift) + getZFromCurve(bParam, shift) + getZFromCurve(iRParam, shift) + getZFromCurve(uVParam, shift));

    let rgbFinal = XYZToRGBC(xf, yf, zf);
    let constrained = constrainRGB(rgbFinal.r, rgbFinal.g, rgbFinal.b);

    var final_rgb = constrained;
    if material.desaturation_enabled > 0u {
        let hsv = rgb_to_hsv(final_rgb);
        final_rgb = hsv_to_rgb(vec3<f32>(hsv.x, hsv.y * 0.5, hsv.z));
    }

    let final_col = vec4<f32>(final_rgb, data.a);

    return final_col;
}
