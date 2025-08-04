struct RelativisticUniforms {
    viw: vec4<f32>,
    strt_time: f32,

    // OLD / global:
    // vpc: vec4<f32>,
    // player_offset: vec4<f32>,
    // spd_of_light: f32,
    // wrld_time: f32,
    // color_shift: u32,
};


struct RelativisticGlobalsUniform {
    /// velocity of player
    vpc: vec4<f32>,
    player_offset: vec4<f32>,
    spd_of_light: f32,
    wrld_time: f32,
    color_shift: u32,
    // xy ratio
    xyr: f32,
    // x scale (tangent of half FOV)
    xs: f32,
}
