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

@group(3) @binding(0) var base_texture: texture_2d<f32>;
@group(3) @binding(1) var base_sampler: sampler;
@group(3) @binding(2) var uv_texture: texture_2d<f32>;
@group(3) @binding(3) var ir_texture: texture_2d<f32>;
@group(3) @binding(4) var<uniform> material: RelativisticUniforms;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    // @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
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
    out.uv = vertex.uv;
    out.draw = 1;

    // Get the vertex position in world space.
    let world_from_local = get_world_from_local(vertex.instance_index);
    let local_from_world = get_local_from_world(vertex.instance_index);
    var pos_world: vec4<f32>;
    // pos_world = mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    pos_world = world_from_local * vec4<f32>(vertex.position, 1.0);

    // Shift to a coordinate system where the player is at the origin.
    pos_world -= material.player_offset;

    // --- Calculate Relative Velocity (vr) and Lorentz Factor (svc) ---
    let vpc = material.vpc.xyz;
    let viw = material.viw.xyz;
    let speed_sq = dot(vpc, vpc);
    let speed = sqrt(speed_sq);

    // This is the relativistic velocity addition formula for vr = (vpc - viw)
    let vu_dot = dot(vpc, viw);
    var uparra = vu_dot / speed_sq * vpc;
    if (speed_sq == 0.0) { // Avoid division by zero
        uparra = vec3<f32>(0.0);
    }
    let uperp = viw - uparra;
    var vr = (vpc - uparra - (sqrt(1.0 - speed_sq)) * uperp) / (1.0 - vu_dot);
    out.vr = vec4<f32>(vr, 0.0);
    // vr *= -1;

    let speed_r_sq = dot(vr, vr);
    let speed_r = sqrt(speed_r_sq);
    out.svc = sqrt(1.0 - speed_r_sq);

    // --- Geometry Warping Logic ---
    // riw = reference in world
    var riw = pos_world; // riw is the final warped position.

    // The rotation logic is complex. We need to rotate the world so that the player's
    // velocity (vpc) is aligned with the Z-axis, perform our calculations, then rotate back.

    if (speed_r != 0.0) {
        // Create the rotation matrix (axis-angle) to align vpc with -Z
        let a = -acos(-vpc.z / speed);
        var ux = 0.0;
        var uy = 0.0;
        var ca = 0.0;
        var sa = 0.0;
        var rotated_viw: vec3<f32>;
        var M: mat3x3<f32>;
        if (speed != 0.0) {
            let cross_len = length(vpc.xy);
            if (cross_len != 0.0) {
                ux = vpc.y / cross_len;
                uy = -vpc.x / cross_len;
            }
            ca = cos(a);
            sa = sin(a);

            M = mat3x3<f32>(
                vec3(ca + ux*ux*(1.0-ca), ux*uy*(1.0-ca), -uy*sa),
                vec3(uy*ux*(1.0-ca), ca + uy*uy*(1.0-ca), ux*sa),
                vec3(uy*sa, -ux*sa, ca)
            );
            // M = mat3x3<f32>(
            //     vec3(ca + ux*ux*(1.0-ca),  uy*ux*(1.0-ca),       uy*sa),
            //     vec3(ux*uy*(1.0-ca),      ca + uy*uy*(1.0-ca),  -ux*sa),
            //     vec3(-uy*sa,              ux*sa,                ca)
            // );
            riw = vec4<f32>(M * riw.xyz, 1.0);
            // let pos = riw.xyz;
            // riw.x = pos.x * (ca + ux*ux*(1-ca)) + pos.y*(ux*uy*(1-ca)) + pos.z*(uy*sa);
            // riw.y = pos.x * (uy*ux*(1-ca)) + pos.y * ( ca + uy*uy*(1-ca)) - pos.z*(ux*sa);
            // riw.z = pos.x * (-uy*sa) + pos.y * (ux*sa) + pos.z*(ca);

            // Rotate our position and the object's velocity into the new frame.
            rotated_viw = M * viw * material.spd_of_light;
            // rotated_viw.x = (viw.x * (ca + ux*ux*(1-ca)) + viw.y*(ux*uy*(1-ca)) + viw.z*(uy*sa)) * material.spd_of_light;
            // rotated_viw.y = (viw.x * (uy*ux*(1-ca)) + viw.y * ( ca + uy*uy*(1-ca)) - viw.z*(ux*sa)) * material.spd_of_light;
            // rotated_viw.z = (viw.x * (-uy*sa) + viw.y * (ux*sa) + viw.z*(ca)) * material.spd_of_light;
        } else {
            rotated_viw = viw * material.spd_of_light;
        }

        // --- Time-of-Flight Calculation ---
        // Solve the quadratic equation: At^2 + Bt + C = 0
        let C = -dot(riw.xyz, riw.xyz);
        let B = -2.0 * dot(riw.xyz, rotated_viw);
        let D = material.spd_of_light * material.spd_of_light - dot(rotated_viw, rotated_viw);

        // tisw is the time it took light to travel from the vertex to the eye.
        let tisw = (-B - sqrt(B*B - 4.0*D*C)) / (2.0 * D);

        // Ensure objects with velocity do not appear before their start time.
        out.draw = f32(i32(material.strt_time == 0.0 || material.wrld_time + tisw > material.strt_time));

        // Calculate the vertex's position in the past, when the light was emitted.
        riw += vec4(rotated_viw * tisw, 0.0);

        // --- Lorentz Transformation ---
        // Apply the transform only along the Z-axis (our direction of motion).
        let lorentz_factor = 1.0 / sqrt(1.0 - speed_sq);
        let delta_z = material.spd_of_light * speed * tisw;
        riw.z = (riw.z + delta_z) * lorentz_factor;

        // riw = vec4<f32>(transpose(M) * riw.xyz, 1.0);

        // Rotate the final position back to world space by multiplying by the inverse
        // of M, which is its transpose since it's a rotation matrix.
        if (speed != 0.0) {
            riw = vec4<f32>(transpose(M) * riw.xyz, 1.0);
            // let trx = riw.x;
            // let trry = riw.y;
            // riw.x = riw.x * (ca + ux*ux*(1-ca)) + riw.y*(ux*uy*(1-ca)) - riw.z*(uy*sa);
            // riw.y = trx * (uy*ux*(1-ca)) + riw.y * ( ca + uy*uy*(1-ca)) + riw.z*(ux*sa);
            // riw.z = trx * (uy*sa) - trry * (ux*sa) + riw.z*(ca);
        }
    }

    // Shift back to the original world coordinate system.
    riw += material.player_offset;

    // The final position to be rendered.

    // unity code:
    // pos1 = mul(unity_WorldToObject*1.0,riw);
    // pos2 = mul(unity_ObjectToWorld, pos1 );
    //
    // let pos1 = inverse(world_from_local) * riw;
    // riw = world_from_local * pos1;

    // let pos = (local_from_world * riw);
    out.world_pos = riw - material.player_offset;
    out.clip_position = position_world_to_clip(riw.xyz);

    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.draw == 0) {
        discard;
        // return vec4(.5, .2, .7, 1.0);
    }
    let vr = in.vr;
    let svc = in.svc;
    let pos = in.world_pos.xyz;

    var shift = 1.0f;
    if material.color_shift > 0u {
        let shift_numerator = 1.0 - (dot(pos, vr.xyz) / length(pos));
        shift = shift_numerator / svc;
    }

    // flip UVs, easier here.
    let uv_orig = in.uv;
    let uv = in.uv * vec2(1.0, -1.0);
    var data = textureSample(base_texture, base_sampler, uv);
    let uv_val = textureSample(uv_texture, base_sampler, uv).r;
    let ir_val = textureSample(ir_texture, base_sampler, uv).r;

    data.a *= in.draw;
    if (data.a < 0.25) {
        discard;
    }

    let xyz = RGBToXYZC(data.r, data.g, data.b);
    let weights = weightFromXYZCurves(xyz);

    var rParam = vec3(weights.x, 615.0, 8.0);
    var gParam = vec3(weights.y, 550.0, 4.0);
    var bParam = vec3(weights.z, 463.0, 5.0);
    var uVParam = vec3(0.02, UV_START + UV_RANGE * uv_val, 5.0);
    var iRParam = vec3(0.02, IR_START + IR_RANGE * ir_val, 5.0);

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
