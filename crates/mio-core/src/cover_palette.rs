//! Shared artwork palette. No UI toolkit types or global theme state.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsla {
    pub h: f32,
    pub s: f32,
    pub l: f32,
    pub a: f32,
}
impl From<Rgba> for Hsla {
    fn from(c: Rgba) -> Self {
        let max = c.r.max(c.g.max(c.b));
        let min = c.r.min(c.g.min(c.b));
        let delta = max - min;
        let l = (max + min) / 2.;
        let s = if l == 0. || l == 1. {
            0.
        } else if l < 0.5 {
            delta / (2. * l)
        } else {
            delta / (2. - 2. * l)
        };
        let h = if delta == 0. {
            0.
        } else if max == c.r {
            ((c.g - c.b) / delta).rem_euclid(6.) / 6.
        } else if max == c.g {
            ((c.b - c.r) / delta + 2.) / 6.
        } else {
            ((c.r - c.g) / delta + 4.) / 6.
        };
        Self { h, s, l, a: c.a }
    }
}
impl Hsla {
    pub fn to_rgb(self) -> Rgba {
        let c = (1. - (2. * self.l - 1.).abs()) * self.s;
        let x = c * (1. - ((self.h * 6.) % 2. - 1.).abs());
        let m = self.l - c / 2.;
        let cm = c + m;
        let xm = x + m;
        let (r, g, b) = match (self.h * 6.).floor() as i32 {
            0 | 6 => (cm, xm, m),
            1 => (xm, cm, m),
            2 => (m, cm, xm),
            3 => (m, xm, cm),
            4 => (xm, m, cm),
            _ => (cm, m, xm),
        };
        Rgba {
            r: r.clamp(0., 1.),
            g: g.clamp(0., 1.),
            b: b.clamp(0., 1.),
            a: self.a,
        }
    }
}

impl Hsla {
    fn opacity(self, factor: f32) -> Self {
        Self {
            a: self.a * factor.clamp(0., 1.),
            ..self
        }
    }
}
fn black() -> Hsla {
    Hsla {
        h: 0.,
        s: 0.,
        l: 0.,
        a: 1.,
    }
}
fn white() -> Hsla {
    Hsla {
        h: 0.,
        s: 0.,
        l: 1.,
        a: 1.,
    }
}

// Original 100x100 / five-cluster worker, including distinct random pixel seeds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverPalette {
    pub main: Hsla,
    pub artist: Hsla,
    pub text: Hsla,
    pub hover: Hsla,
    pub use_black_text: bool,
}
pub fn cover_palette(image: &image::DynamicImage) -> CoverPalette {
    let samples: Vec<[f32; 3]> = image
        .resize_exact(100, 100, image::imageops::FilterType::Triangle)
        .to_rgba8()
        .pixels()
        .filter(|pixel| pixel[3] >= 128)
        .map(|pixel| [pixel[0] as f32, pixel[1] as f32, pixel[2] as f32])
        .collect();
    let mut centers = if samples.len() <= 5 {
        samples.clone()
    } else {
        let mut indices = Vec::with_capacity(5);
        while indices.len() < 5 {
            // Reuse the existing v4 entropy source; seeds need no new dependency.
            let index = (uuid::Uuid::new_v4().as_u128() % samples.len() as u128) as usize;
            if !indices.contains(&index) {
                indices.push(index);
            }
        }
        indices.into_iter().map(|index| samples[index]).collect()
    };
    let mut counts = vec![0usize; centers.len()];
    if samples.len() > 5 {
        for _ in 0..10 {
            let mut sums = vec![[0.; 3]; centers.len()];
            counts.fill(0);
            for sample in &samples {
                let nearest = centers
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let distance = |center: &[f32; 3]| {
                            (0..3).map(|c| (sample[c] - center[c]).powi(2)).sum::<f32>()
                        };
                        distance(a).total_cmp(&distance(b))
                    })
                    .map(|(index, _)| index)
                    .unwrap();
                counts[nearest] += 1;
                for c in 0..3 {
                    sums[nearest][c] += sample[c];
                }
            }
            let mut changed = false;
            for (index, center) in centers.iter_mut().enumerate() {
                if counts[index] == 0 {
                    continue;
                }
                let next = sums[index].map(|sum| (sum / counts[index] as f32).round());
                changed |= *center != next;
                *center = next;
            }
            if !changed {
                break;
            }
        }
    }
    let as_color = |rgb: [f32; 3]| {
        Hsla::from(Rgba {
            r: rgb[0] / 255.,
            g: rgb[1] / 255.,
            b: rgb[2] / 255.,
            a: 1.,
        })
    };
    let mut order: Vec<_> = (0..centers.len()).collect();
    order.sort_by_key(|index| std::cmp::Reverse(counts[*index]));
    let mut chosen = order
        .first()
        .map(|index| centers[*index])
        .unwrap_or([76., 116., 206.]);
    let mut saturation = -1.;
    for index in order {
        let center = centers[index];
        if center.iter().all(|value| *value < 30.) || center.iter().all(|value| *value > 225.) {
            continue;
        }
        let candidate = as_color(center).s;
        if candidate > saturation {
            chosen = center;
            saturation = candidate;
        }
    }
    let mut color = as_color(chosen);
    color.s = (color.s * 1.2).min(0.9);
    color.l = if color.l < 0.3 {
        (color.l * 1.5).min(0.5)
    } else if color.l > 0.7 {
        (color.l * 0.8).max(0.5)
    } else {
        color.l
    };
    let linear = |channel: f32| {
        let value = channel / 255.;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = if samples.is_empty() {
        0.5
    } else {
        samples
            .iter()
            .map(|s| 0.2126 * linear(s[0]) + 0.7152 * linear(s[1]) + 0.0722 * linear(s[2]))
            .sum::<f32>()
            / samples.len() as f32
    };
    let use_black_text = luminance >= 0.6;
    let foreground = if use_black_text { black() } else { white() };
    // Preserve the original 8-bit rounding for mainColor and lightMainColor.
    let rgb = color.to_rgb();
    let main = Hsla::from(Rgba {
        r: (rgb.r * 255.).round() / 255.,
        g: (rgb.g * 255.).round() / 255.,
        b: (rgb.b * 255.).round() / 255.,
        a: 1.,
    });
    let light_rgb = main.to_rgb();
    let light_rgb = Rgba {
        r: (light_rgb.r * 255. * 0.2 + 204.).round() / 255.,
        g: (light_rgb.g * 255. * 0.2 + 204.).round() / 255.,
        b: (light_rgb.b * 255. * 0.2 + 204.).round() / 255.,
        a: 0.9,
    };
    CoverPalette {
        main,
        artist: Rgba {
            r: (light_rgb.r * 1.2).min(1.),
            g: (light_rgb.g * 1.2).min(1.),
            b: (light_rgb.b * 1.2).min(1.),
            a: 0.9 * 0.55,
        }
        .into(),
        text: foreground.opacity(0.6),
        hover: foreground,
        use_black_text,
    }
}
