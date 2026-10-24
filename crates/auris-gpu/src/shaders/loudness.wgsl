// Whole-buffer loudness reduction: one invocation reduces one contiguous span of samples.
//
// See `analysis.rs` for the host side, which sums the partial results in f64. The output slot
// layout is (minimum, maximum, sum of squares, unused). The host measures true peak with the
// shared FIR over complete channels, independently of GPU dispatch boundaries.

struct Params {
    // Samples uploaded for this dispatch.
    sample_count: u32,
    // First uploaded sample this dispatch reduces.
    first_active: u32,
    // One past the last uploaded sample this dispatch reduces.
    last_active: u32,
    // Samples reduced by one invocation.
    span: u32,
}

@group(0) @binding(0) var<storage, read> samples: array<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let start = params.first_active + id.x * params.span;
    // Invocations past the end of the work have no output slot of their own, so they must
    // return before writing anything.
    if (start >= params.last_active) {
        return;
    }
    let end = min(start + params.span, params.last_active);

    var lowest = samples[start];
    var highest = lowest;
    var sum_squares = 0.0;
    for (var i = start; i < end; i = i + 1u) {
        let value = samples[i];
        lowest = min(lowest, value);
        highest = max(highest, value);
        sum_squares = sum_squares + value * value;
    }

    output[id.x] = vec4<f32>(lowest, highest, sum_squares, 0.0);
}
