// The text filter: turns the rendered 3D scene into characters. The screen is
// cut into cells; each cell averages the scene under it, picks a character by
// how bright that is (denser characters for brighter cells), and draws it from
// the Dungeon Mode font in the scene's color.

struct FilterParams {
    // x, y: cells across and down; z: 1 draws text, 0 shows the scene as is
    grid: vec4<f32>,
};

// Group 2 is Bevy's 2D material bind group (MATERIAL_2D_BIND_GROUP_INDEX)
@group(2) @binding(0) var<uniform> params: FilterParams;
@group(2) @binding(1) var scene: texture_2d<f32>;
@group(2) @binding(2) var scene_sampler: sampler;
@group(2) @binding(3) var font: texture_2d<f32>;
@group(2) @binding(4) var font_sampler: sampler;

// Samples taken across each cell, per side
const SAMPLES: i32 = 4;

@fragment
fn fragment(@location(2) uv: vec2<f32>) -> @location(0) vec4<f32> {
    // Code page 437 slots, darkest to brightest: space . : - = + * # % @
    var ramp = array<u32, 10>(32u, 46u, 58u, 45u, 61u, 43u, 42u, 35u, 37u, 64u);

    if params.grid.z < 0.5 {
        return vec4<f32>(textureSampleLevel(scene, scene_sampler, uv, 0.0).rgb, 1.0);
    }

    let grid = params.grid.xy;
    let cell = floor(uv * grid);
    let inside = fract(uv * grid);

    // Average the scene over the cell
    var sum = vec3<f32>(0.0);
    for (var y = 0; y < SAMPLES; y++) {
        for (var x = 0; x < SAMPLES; x++) {
            let offset = (vec2<f32>(f32(x), f32(y)) + 0.5) / f32(SAMPLES);
            sum += textureSampleLevel(scene, scene_sampler, (cell + offset) / grid, 0.0).rgb;
        }
    }
    let color = sum / f32(SAMPLES * SAMPLES);

    // Brightness picks the character; sqrt spreads out the dark end
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    let index = u32(clamp(sqrt(luma) * 9.0 + 0.5, 0.0, 9.0));
    let slot = ramp[index];

    // The font sheet is 16 x 16 glyphs; alpha is the ink
    let glyph_uv = (vec2<f32>(f32(slot % 16u), f32(slot / 16u)) + inside) / 16.0;
    let ink = textureSampleLevel(font, font_sampler, glyph_uv, 0.0).a;

    // The character carries the hue at full strength; its density carries the brightness
    let hue = color / max(max(color.r, max(color.g, color.b)), 0.0001);
    return vec4<f32>(hue * ink, 1.0);
}
