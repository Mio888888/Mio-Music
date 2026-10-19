#include <metal_stdlib>
using namespace metal;

// Behavior and parameters from AMLL core 0.4.1 PixiRenderer (package: GPL-3.0).
// Pixi filter arithmetic: MIT, copyright notices in background-LICENSES.txt.
// Input stays on the GPU; no AMLL JavaScript runtime is bundled.
struct Composition {
    float width, height, time, old_time;
    float alpha, old_alpha, has_old, unused;
    float angles[8];
};
constexpr sampler linear_sampler(coord::normalized, address::clamp_to_edge, filter::linear);
float4 cover_stack(texture2d<float, access::sample> cover, float2 p,
                   constant Composition &c, float time, uint base) {
    float max_size = max(c.width, c.height);
    float2 center = float2(c.width, c.height) * 0.5;
    float4 result = 0;
    for (uint i = 0; i < 4; ++i) {
        float2 pos = center;
        float side = max_size * (i == 0 ? sqrt(2.0f) : i == 1 ? 0.8f : i == 2 ? 0.5f : 0.25f);
        if (i == 1) pos = float2(c.width, c.height) / 2.5;
        if (i == 2) pos += c.width / 4.0 * cos(time * 0.00075);
        if (i == 3) pos += c.width / 4.0 * 0.1 + cos(time * 0.0045);
        float angle = c.angles[base+i] + time * (i == 0 || i == 2 ? 0.001f : i == 1 ? -0.002f : -1.0f/750.0f);
        float2 d = p - pos;
        float2 uv = float2(cos(angle)*d.x + sin(angle)*d.y, -sin(angle)*d.x + cos(angle)*d.y) / side + 0.5;
        if (all(uv >= 0) && all(uv <= 1)) {
            float4 color = cover.sample(linear_sampler, uv);
            color.rgb *= color.a;
            result = color + result * (1 - color.a);
        }
    }
    return result;
}
kernel void compose(texture2d<float, access::sample> cover [[texture(0)]],
                    texture2d<float, access::sample> old [[texture(1)]],
                    texture2d<float, access::write> out [[texture(2)]],
                    constant Composition &c [[buffer(0)]], uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= out.get_width() || gid.y >= out.get_height()) return;
    float2 p = float2(gid) + 0.5;
    float4 prior = c.has_old > 0 ? cover_stack(old, p, c, c.old_time, 4) * c.old_alpha : float4(0);
    float4 current = cover_stack(cover, p, c, c.time, 0) * c.alpha;
    float4 color = current + prior * (1 - current.a);
    out.write(color, gid); // Keep premultiplied alpha through Pixi filters; composite over black at output.
}
kernel void blur(texture2d<float, access::sample> src [[texture(0)]],
                 texture2d<float, access::write> out [[texture(1)]],
                 constant float2 &step [[buffer(0)]], uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= out.get_width() || gid.y >= out.get_height()) return;
    float2 uv = (float2(gid) + 0.5) / float2(out.get_width(), out.get_height());
    float4 value = src.sample(linear_sampler, uv) * 0.250301;
    value += (src.sample(linear_sampler, uv-step) + src.sample(linear_sampler, uv+step)) * 0.221461;
    value += (src.sample(linear_sampler, uv-step*2) + src.sample(linear_sampler, uv+step*2)) * 0.153388;
    out.write(value, gid);
}
kernel void tint(texture2d<float, access::read> src [[texture(0)]],
                 texture2d<float, access::write> out [[texture(1)]],
                 constant float &mode [[buffer(0)]], uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= out.get_width() || gid.y >= out.get_height()) return;
    float4 color = src.read(gid);
    float3 rgb = color.a > 0 ? color.rgb / color.a : float3(0);
    // Three distinct Pixi color filters, with RGBA8 quantization between them.
    if (mode == 0) rgb = rgb * 2.2 - (rgb.r + rgb.g + rgb.b) * 0.4;
    else if (mode == 1) rgb *= 0.6;
    else rgb = rgb * 1.3 - 0.15 / 255.0;
    out.write(float4(rgb * color.a, color.a), gid);
}
kernel void bulge(texture2d<float, access::sample> src [[texture(0)]],
                  texture2d<float, access::write> out [[texture(1)]],
                  constant float2 &center [[buffer(0)]], uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= out.get_width() || gid.y >= out.get_height()) return;
    float2 dimensions = float2(out.get_width(), out.get_height());
    float2 p = float2(gid) + 0.5;
    float2 delta = p - center * dimensions;
    float distance = length(delta);
    float radius = (dimensions.x + dimensions.y) * 0.5;
    if (distance < radius && distance > 0) {
        float percent = distance / radius;
        delta *= mix(1.0f, smoothstep(0.0f, radius/distance, percent), 0.75f);
        p = center * dimensions + delta;
    }
    float2 uv = p / dimensions;
    float2 clamped = clamp(uv, 0.5 / dimensions, 1.0 - 0.5 / dimensions);
    float4 color = src.sample(linear_sampler, clamped);
    color *= max(0.0f, 1.0f - length(uv-clamped));
    out.write(color, gid);
}
// GPUI's native Surface accepts only full-range NV12 and decodes using BT.601.
float3 yuv(float3 rgb) {
    float y = dot(rgb, float3(0.299, 0.587, 0.114));
    return float3(y, (rgb.b-y)/1.772+0.5, (rgb.r-y)/1.402+0.5);
}
kernel void surface_output(texture2d<float, access::read> src [[texture(0)]],
                           texture2d<float, access::write> luma [[texture(1)]],
                           texture2d<float, access::write> chroma [[texture(2)]], uint2 gid [[thread_position_in_grid]]) {
    if (gid.x >= chroma.get_width() || gid.y >= chroma.get_height()) return;
    float2 cbcr = 0;
    for (uint y = 0; y < 2; ++y) for (uint x = 0; x < 2; ++x) {
        uint2 p = gid*2 + uint2(x,y);
        float3 color = yuv(src.read(p).rgb);
        luma.write(float4(color.x,0,0,1), p);
        cbcr += color.yz * 0.25;
    }
    chroma.write(float4(cbcr,0,1), gid);
}
