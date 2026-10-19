#version 460 core
#include <flutter/runtime_effect.glsl>
// Port of shell/background.metal. AMLL/Pixi notices: assets/background-LICENSES.txt.
uniform vec2 u_size;
uniform float u_time;
uniform float u_oldTime;
uniform float u_alpha;
uniform float u_oldAlpha;
uniform vec4 u_angles;
uniform vec4 u_oldAngles;
uniform float u_bass;
uniform sampler2D u_cover;
uniform sampler2D u_old;
out vec4 fragColor;
vec4 stackCover(vec2 p, float time, vec4 angles, bool old) {
  float maxSize = max(u_size.x, u_size.y);
  vec2 center = u_size * 0.5;
  vec4 result = vec4(0.0);
  for (int i = 0; i < 4; ++i) {
    vec2 pos = center;
    float side = maxSize * (i == 0 ? sqrt(2.0) : i == 1 ? 0.8 : i == 2 ? 0.5 : 0.25);
    if (i == 1) pos = u_size / 2.5;
    if (i == 2) pos += u_size.x / 4.0 * cos(time * 0.00075);
    if (i == 3) pos += u_size.x / 4.0 * 0.1 + cos(time * 0.0045);
    float angle = angles[i] + time * (i == 0 || i == 2 ? 0.001 : i == 1 ? -0.002 : -1.0/750.0);
    // A bounded 8% pulse; static/reduced/inactive paths send zero energy.
    side *= 1.0 + 0.08 * u_bass;
    vec2 d = p - pos;
    vec2 uv = vec2(cos(angle)*d.x + sin(angle)*d.y, -sin(angle)*d.x + cos(angle)*d.y) / side + 0.5;
    if (all(greaterThanEqual(uv, vec2(0))) && all(lessThanEqual(uv, vec2(1)))) {
      // Flutter images are already premultiplied.
      vec4 color = old ? texture(u_old, clamp(uv, vec2(0), vec2(1))) : texture(u_cover, clamp(uv, vec2(0), vec2(1)));
      result = color + result * (1.0 - color.a);
    }
  }
  return result;
}
void main() {
  vec2 p = FlutterFragCoord().xy;
  vec4 prior = stackCover(p, u_oldTime, u_oldAngles, true) * u_oldAlpha;
  vec4 current = stackCover(p, u_time, u_angles, false) * u_alpha;
  fragColor = floor(clamp(current + prior * (1.0 - current.a), 0.0, 1.0) * 255.0 + 0.5) / 255.0;
}
