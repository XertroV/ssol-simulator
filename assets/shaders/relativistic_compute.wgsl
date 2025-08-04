// ! UNUSED

#import "shaders/rel_structs.wgsl"::{RelativisticUniforms}

// Bindings for our data
@group(0) @binding(0) var<storage, read> vertices_in: array<vec3<f32>>;
@group(0) @binding(1) var<storage, read_write> vertices_out: array<vec3<f32>>;
@group(0) @binding(2) var<uniform> material: RelativisticUniforms;

// The main compute function, runs once for each vertex.
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let index = global_id.x;
    // Prevent out-of-bounds access
    if (index >= arrayLength(&vertices_in)) {
        return;
    }

    let local_pos = vertices_in[index];
    var pos_world = material.world_matrix * vec4<f32>(local_pos, 1.0);
    pos_world -= material.player_offset;

    // warp geometry based on the player's velocity
    let vpc = material.vpc.xyz;
    let speed = length(vpc);
    var riw = pos_world;

    if (speed != 0.0) {
        let a = acos(vpc.z / speed);
        var ux = 0.0;
        var uy = 0.0;
        let cross_len = length(vec2<f32>(vpc.x, vpc.y));
        if (cross_len != 0.0) {
            ux = -vpc.y / cross_len;
            uy = vpc.x / cross_len;
        }
        let ca = cos(a);
        let sa = sin(a);
        let M = mat4x4<f32>(
            vec4(ca + ux*ux*(1.0-ca),  uy*ux*(1.0-ca),       uy*sa, 0.0),
            vec4(ux*uy*(1.0-ca),      ca + uy*uy*(1.0-ca),  -ux*sa, 0.0),
            vec4(-uy*sa,              ux*sa,                ca, 0.0),
            vec4(0.0, 0.0, 0.0, 1.0)
        );

        riw = M * riw;
        let rotated_viw = (M * vec4<f32>(material.viw.xyz, 0.0)).xyz * material.spd_of_light;

        let C = -dot(riw.xyz, riw.xyz);
        let B = -2.0 * dot(riw.xyz, rotated_viw);
        let D = material.spd_of_light * material.spd_of_light - dot(rotated_viw, rotated_viw);
        let tisw = (-B - sqrt(B*B - 4.0*D*C)) / (2.0 * D);

        riw.xyz += rotated_viw * tisw;

        let lorentz_factor = 1.0 / sqrt(1.0 - speed * speed);
        riw.z = (riw.z - speed * material.spd_of_light * tisw) * lorentz_factor;

        riw = transpose(M) * riw;
    }

    // Write the final, warped LOCAL position to the output buffer.
    // This is the inverse of the first operation we did.
    vertices_out[index] = (inverse(material.world_matrix) * riw).xyz;
}
