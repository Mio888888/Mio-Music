#version 460 core
#include <flutter/runtime_effect.glsl>
// Exact 5-tap weights and Pixi arithmetic from shell/background.metal.
// Input dimensions/sampler are populated by ImageFilter.shader (Impeller).
uniform vec2 u_size;
uniform float u_mode;
uniform vec2 u_parameter;
uniform sampler2D u_input;
out vec4 fragColor;
vec4 sampleAt(vec2 uv) {
  uv = clamp(uv, 0.5 / u_size, 1.0 - 0.5 / u_size);
#ifdef IMPELLER_TARGET_OPENGLES
  uv.y = 1.0 - uv.y;
#endif
  return texture(u_input, uv);
}
void main() {
  vec2 uv = FlutterFragCoord().xy / u_size;
  vec4 color;
  if (u_mode < 0.5) {
    vec2 stepSize = u_parameter / u_size;
    color = sampleAt(uv) * 0.250301;
    color += (sampleAt(uv-stepSize) + sampleAt(uv+stepSize)) * 0.221461;
    color += (sampleAt(uv-stepSize*2.0) + sampleAt(uv+stepSize*2.0)) * 0.153388;
  } else if (u_mode < 3.5) {
    color = sampleAt(uv);
    vec3 rgb = color.a > 0.0 ? color.rgb / color.a : vec3(0);
    if (u_mode < 1.5) rgb = rgb * 2.2 - (rgb.r + rgb.g + rgb.b) * 0.4;
    else if (u_mode < 2.5) rgb *= 0.6;
    else rgb = rgb * 1.3 - 0.15 / 255.0;
    color = vec4(rgb * color.a, color.a);
  } else {
    vec2 p = uv * u_size;
    vec2 delta = p - u_parameter * u_size;
    float distance = length(delta);
    float radius = (u_size.x + u_size.y) * 0.5;
    if (distance < radius && distance > 0.0) {
      float percent = distance / radius;
      delta *= mix(1.0, smoothstep(0.0, radius / distance, percent), 0.75);
      p = u_parameter * u_size + delta;
    }
    uv = p / u_size;
    vec2 clamped = clamp(uv, 0.5/u_size, 1.0-0.5/u_size);
    color = sampleAt(clamped) * max(0.0, 1.0 - length(uv-clamped));
  }
  // Desktop intermediates are RGBA8. Preserve each stage's quantization.
  fragColor = floor(clamp(color, 0.0, 1.0) * 255.0 + 0.5) / 255.0;
}
