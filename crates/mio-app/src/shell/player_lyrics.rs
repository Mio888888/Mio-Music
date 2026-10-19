//! Native lyric layout and effects. Timing stays in song milliseconds, including LRC offset.
use super::*;
use crate::lyrics::Line;
use std::cell::RefCell;

#[derive(Clone, Default)]
pub(super) struct LayoutCache(
    Rc<RefCell<Option<(u32, u32, SharedString, u32, TextAlign, Rc<Layout>)>>>,
);

pub(super) struct Motion {
    pub visible: bool,
    #[cfg(feature = "parity")]
    pub delivery_lag: f64,
    #[cfg(feature = "parity")]
    trace: Option<(std::path::PathBuf, Instant, Vec<[f64; 7]>)>,
    pub geometry: Rc<Cell<Option<(Bounds<Pixels>, Option<Bounds<Pixels>>, Point<Pixels>)>>>,
    pub lines: Vec<Arc<Line>>,
    pub layouts: Vec<[LayoutCache; 3]>,
    scales: Vec<(f32, f32)>,
    alphas: Vec<[f32; 2]>,
    rows: Vec<RowMotion>,
    scroll_target: f32,
    scroll_params: (f32, f32),
    scale_frame: Instant,
    scale_step: f32,
    snap: bool,
    cache_revision: u64,
    frame: Instant,
    revision: u64,
    position: f64,
    velocity: f32,
    pub manual_until: Option<Instant>,
    pub settling: bool,
    pub changed: Instant,
    pub previous: Option<usize>,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            visible: false,
            #[cfg(feature = "parity")]
            delivery_lag: 0.,
            #[cfg(feature = "parity")]
            trace: std::env::var_os("MIO_GPUI_LYRIC_TRACE")
                .map(|p| (p.into(), Instant::now(), Vec::new())),
            geometry: Default::default(),
            lines: Vec::new(),
            layouts: Vec::new(),
            scales: Vec::new(),
            alphas: Vec::new(),
            rows: Vec::new(),
            scroll_target: 0.,
            scroll_params: (170., 170_f32.sqrt() * 2.2),
            scale_frame: Instant::now(),
            scale_step: 0.,
            snap: true,
            cache_revision: u64::MAX,
            frame: Instant::now(),
            revision: u64::MAX,
            position: 0.,
            velocity: 0.,
            manual_until: None,
            settling: false,
            changed: Instant::now(),
            previous: None,
        }
    }
}
#[cfg(feature = "parity")]
impl Drop for Motion {
    fn drop(&mut self) {
        if let Some((path, _, samples)) = &self.trace {
            if let Ok(bytes) = serde_json::to_vec(samples) {
                let _ = std::fs::write(path, bytes);
            }
        }
    }
}
impl Motion {
    #[cfg(feature = "parity")]
    pub fn trace_frame(
        &mut self,
        position: f64,
        sample_age: f64,
        render_start: Instant,
        playing: bool,
    ) {
        if let Some((path, origin, samples)) = &mut self.trace {
            if samples.len() < 36_000 {
                samples.push([
                    origin.elapsed().as_secs_f64(),
                    position,
                    sample_age,
                    render_start.elapsed().as_secs_f64(),
                    f64::from(self.velocity),
                    self.delivery_lag,
                    if playing { 1. } else { 0. },
                ]);
                if samples.len() % 600 == 0 {
                    if let Ok(bytes) = serde_json::to_vec(samples) {
                        let _ = std::fs::write(path, bytes);
                    }
                }
            }
        }
    }

    pub fn prepare(&mut self, lines: &[Line], revision: u64) {
        let now = Instant::now();
        self.scale_step = now.duration_since(self.scale_frame).as_secs_f32().min(0.1);
        self.scale_frame = now;
        if self.cache_revision != revision || self.lines.len() != lines.len() {
            self.lines = lines.iter().cloned().map(Arc::new).collect();
            self.layouts = (0..lines.len())
                .map(|_| std::array::from_fn(|_| LayoutCache::default()))
                .collect();
            self.cache_revision = revision;
            self.scales = vec![(1., 0.); lines.len()];
            self.alphas = vec![[0.4, 1.]; lines.len()];
            self.rows = (0..lines.len()).map(|_| RowMotion::default()).collect();
        }
    }
    pub fn scale(&mut self, index: usize, current: bool, enabled: bool, playing: bool) -> f32 {
        let target = if !enabled || !playing || current {
            1.
        } else {
            0.97
        };
        let (value, velocity) = &mut self.scales[index];
        if self.snap || !enabled {
            *value = target;
            *velocity = 0.;
            return target;
        }
        // AMLL foreground scale spring: mass 2, damping 25, stiffness 100.
        let dt = self.scale_step;
        let frequency = (800_f32 - 625.).sqrt();
        let delta = target - *value;
        let leftover = (25. * delta - 4. * *velocity) / frequency;
        let angle = frequency / 4. * dt;
        let decay = (-25. / 4. * dt).exp();
        let displacement = angle.cos() * delta + angle.sin() * leftover;
        *value = target - displacement * decay;
        *velocity = (25. / 4. * displacement
            + frequency / 4. * (angle.sin() * delta - angle.cos() * leftover))
            * decay;
        if (target - *value).abs() < 0.00001 && velocity.abs() < 0.00001 {
            *value = target;
            *velocity = 0.;
        } else {
            self.settling = true;
        }
        *value
    }
    pub fn mask_alpha(
        &mut self,
        index: usize,
        scale: f32,
        current: bool,
        enabled: bool,
    ) -> [f32; 2] {
        let factor = ((scale - 0.97) / 0.03).clamp(0., 1.);
        let dark = 0.2 + factor * 0.2;
        let targets = [dark, if current { 0.2 + factor * 0.8 } else { dark }];
        let alpha = &mut self.alphas[index];
        for (value, target) in alpha.iter_mut().zip(targets) {
            if self.snap || !enabled || (*value - target).abs() < 0.001 {
                *value = target;
            } else {
                let speed = if target > *value { 50. } else { 7. };
                *value += (target - *value) * (1. - (-speed * self.scale_step).exp());
                self.settling = true;
            }
        }
        *alpha
    }
    pub fn row_target_top(&self, index: usize, scroll: &ScrollHandle) -> f32 {
        scroll
            .bounds_for_item(index)
            .map(|row| f32::from(row.top() - scroll.bounds().top()) + self.scroll_target)
            .unwrap_or(0.)
    }
    pub fn row_offset(&mut self, index: usize, delay: f32, scroll: &ScrollHandle) -> f32 {
        let row = &mut self.rows[index];
        let now = Instant::now();
        let actual = f32::from(scroll.offset().y);
        if self.snap || self.manual_until.is_some_and(|until| now < until) {
            row.position = actual;
            row.target = actual;
            row.pending = None;
            row.velocity = 0.;
            return 0.;
        }
        let queued = row.pending.map_or(row.target, |(_, target)| target);
        if (queued - self.scroll_target).abs() > 0.01 {
            row.pending = Some((now + Duration::from_secs_f32(delay), self.scroll_target));
        }
        let mut dt = self.scale_step;
        if let Some((due, target)) = row.pending
            && now >= due
        {
            row.target = target;
            row.pending = None;
            dt = dt.min(now.duration_since(due).as_secs_f32());
        }
        let (stiffness, damping) = self.scroll_params;
        spring_step(
            &mut row.position,
            &mut row.velocity,
            row.target,
            stiffness,
            damping,
            dt,
        );
        if (row.target - row.position).abs() > 0.2
            || row.velocity.abs() > 0.2
            || row.pending.is_some()
        {
            self.settling = true;
        } else {
            row.position = row.target;
            row.velocity = 0.;
        }
        row.position - actual
    }
    pub fn manual(&self) -> bool {
        self.manual_until
            .is_some_and(|until| Instant::now() < until)
    }
    pub fn scroll(
        &mut self,
        scroll: &ScrollHandle,
        active: usize,
        revision: u64,
        position: f64,
        seeking: bool,
        align: f32,
        spring: bool,
        interval_ms: Option<u64>,
    ) {
        let now = Instant::now();
        let dt = now.duration_since(self.frame).as_secs_f32().min(0.1);
        let seek = seeking || revision != self.revision || (position - self.position).abs() > 0.8;
        self.snap = seek || !spring || self.manual();
        self.frame = now;
        self.position = position;
        self.revision = revision;
        if seek {
            self.manual_until = None;
            self.velocity = 0.;
        }
        if self.manual_until.is_some_and(|until| now >= until) {
            self.manual_until = None;
        }
        if self.manual() {
            self.velocity = 0.;
            self.settling = false;
            return;
        }
        let viewport = scroll.bounds();
        let Some(row) = scroll.bounds_for_item(active) else {
            self.settling = true;
            return;
        };
        if viewport.size.height <= px(0.) {
            self.settling = true;
            return;
        }
        let target = f32::from(
            viewport.top() + viewport.size.height * align - row.top() - row.size.height * 0.5,
        )
        .clamp(-f32::from(scroll.max_offset().y).max(0.), 0.);
        let mut y = f32::from(scroll.offset().y);
        self.scroll_target = target;
        self.scroll_params = interval_ms.map_or((90., 15.), |interval| {
            let ratio = (1. - (interval.clamp(100, 800) as f32 - 100.) / 700.).powf(0.2);
            let stiffness = 170. + 50. * ratio;
            (stiffness, stiffness.sqrt() * 2.2)
        });
        if seek || !spring {
            y = target;
            self.velocity = 0.;
        } else {
            spring_step(
                &mut y,
                &mut self.velocity,
                target,
                self.scroll_params.0,
                self.scroll_params.1,
                dt,
            );
        }
        self.settling = (target - y).abs() > 0.2 || self.velocity.abs() > 0.2;
        if !self.settling {
            y = target;
            self.velocity = 0.;
        }
        scroll.set_offset(point(px(0.), px(y)));
    }
}

#[derive(Default)]
struct RowMotion {
    position: f32,
    velocity: f32,
    target: f32,
    pending: Option<(Instant, f32)>,
}
// Shared analytical solver for the scroll anchor and independently delayed lyric rows.
fn spring_step(
    value: &mut f32,
    velocity: &mut f32,
    target: f32,
    stiffness: f32,
    damping: f32,
    dt: f32,
) {
    let omega = (stiffness / 0.9).sqrt();
    let delta = target - *value;
    if damping / (2. * (stiffness * 0.9).sqrt()) >= 1. {
        let leftover = omega * delta - *velocity;
        let decay = (-omega * dt).exp();
        *value = target - (delta + dt * leftover) * decay;
        *velocity = (omega * (delta + dt * leftover) - leftover) * decay;
    } else {
        let frequency = (4. * 0.9 * stiffness - damping * damping).sqrt();
        let leftover = (damping * delta - 1.8 * *velocity) / frequency;
        let f = frequency / 1.8;
        let decay = (-damping / 1.8 * dt).exp();
        let displacement = (f * dt).cos() * delta + (f * dt).sin() * leftover;
        *velocity = (damping / 1.8 * displacement
            + f * ((f * dt).sin() * delta - (f * dt).cos() * leftover))
            * decay;
        *value = target - displacement * decay;
    }
}

#[derive(Clone)]
pub(super) struct LyricText {
    pub line: Arc<Line>,
    pub cache: LayoutCache,
    pub font: Font,
    pub font_size: f32,
    pub scale: f32,
    pub color: Hsla,
    pub now_ms: u64,
    pub current: bool,
    pub mask_alpha: [f32; 2],
    pub subline: bool,
    pub motion: bool,
    pub blur: f32,
    pub align: TextAlign,
    /// Fade near the viewport's top and bottom edge, refreshed on every paint.
    pub edge: Cell<f32>,
}
struct Piece {
    line: ShapedLine,
    characters: Vec<(ShapedLine, f32, f32)>,
    roman: Option<ShapedLine>,
    x: f32,
    y: f32,
    width: f32,
    start: u64,
    end: u64,
    timed: bool,
    last: bool,
    phonetic: bool,
    /// Long notes of short or CJK words swell while sung; fixed by the text, so shaped once.
    emphasize: bool,
}
pub(super) struct Layout {
    pieces: Vec<Piece>,
    height: f32,
}
impl LyricText {
    fn shape(&self, text: &str, size: f32, window: &Window) -> ShapedLine {
        window.text_system().shape_line(
            text.to_owned().into(),
            px(size),
            &[TextRun {
                len: text.len(),
                font: self.font.clone(),
                color: self.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
    }
    fn layout(&self, width: f32, window: &Window) -> Rc<Layout> {
        let key = (
            width.to_bits(),
            self.font_size.to_bits(),
            self.font.family.clone(),
            self.font.weight.0.to_bits(),
            self.align,
        );
        if let Some((w, s, family, weight, align, layout)) = self.cache.0.borrow().as_ref()
            && (*w, *s, family, *weight, *align) == (key.0, key.1, &key.2, key.3, key.4)
        {
            return layout.clone();
        }
        let layout = Rc::new(self.build_layout(width, window));
        *self.cache.0.borrow_mut() = Some((key.0, key.1, key.2, key.3, key.4, layout.clone()));
        layout
    }
    fn build_layout(&self, width: f32, window: &Window) -> Layout {
        let width = width.max(self.font_size);
        let ruby = self.line.words.iter().any(|w| !w.romanization.is_empty());
        let row_height = self.font_size * if ruby { 1.7 } else { 1.2 };
        let mut pieces = Vec::new();
        let mut fallback_roman = Vec::new();
        let mut x = 0.;
        let mut y = 0.;
        let units: Vec<(&str, &str, u64, u64, bool)> = if self.line.words.is_empty() {
            self.line
                .text
                .split_inclusive(char::is_whitespace)
                .map(|text| (text, "", 0, 0, false))
                .collect()
        } else {
            self.line
                .words
                .iter()
                .map(|w| {
                    (
                        w.text.as_str(),
                        w.romanization.as_str(),
                        w.start,
                        w.end,
                        true,
                    )
                })
                .collect()
        };
        for (text, roman, start, end, timed) in units {
            let shaped = self.shape(text, self.font_size, window);
            let phonetic =
                (!roman.is_empty()).then(|| self.shape(roman, self.font_size * 0.5, window));
            // Ruby never changes the main text advance or wrapping.
            let unit_width = f32::from(shaped.width());
            // Wrap at word boundaries; a single oversized token can still wrap at UTF-8 boundaries.
            let parts: Vec<&str> = if unit_width > width {
                text.char_indices()
                    .map(|(i, c)| &text[i..i + c.len_utf8()])
                    .collect()
            } else {
                vec![text]
            };
            if parts.len() > 1 && !roman.is_empty() {
                fallback_roman.push(roman);
            }
            let mut whole = (parts.len() == 1).then_some(shaped);
            for (part_index, part) in parts.iter().enumerate() {
                // An unsplit unit is the very text already shaped for its width.
                let shaped = whole
                    .take()
                    .unwrap_or_else(|| self.shape(part, self.font_size, window));
                let w = f32::from(shaped.width());
                let phonetic = if parts.len() == 1 {
                    phonetic.as_ref().filter(|_| w > 0.).map(|ruby| {
                        let available = (w - self.font_size * 0.08).max(0.);
                        let ratio = (available / f32::from(ruby.width()).max(1.)).min(1.);
                        if ratio >= 1. {
                            ruby.clone()
                        } else {
                            self.shape(roman, self.font_size * 0.5 * ratio, window)
                        }
                    })
                } else {
                    None
                };
                if x > 0. && x + w > width {
                    x = 0.;
                    y += row_height;
                }
                let span = end.saturating_sub(start);
                let character_count = part.trim().chars().count();
                let characters = if timed
                    && span >= 1000
                    && character_count > 1
                    && (character_count <= 7
                        || part.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c)))
                    && part
                        .chars()
                        .all(|c| c.is_ascii() || ('\u{3400}'..='\u{9fff}').contains(&c))
                {
                    part.char_indices()
                        .map(|(i, c)| {
                            let left = f32::from(shaped.x_for_index(i));
                            let right = f32::from(shaped.x_for_index(i + c.len_utf8()));
                            (
                                self.shape(&part[i..i + c.len_utf8()], self.font_size, window),
                                left,
                                right - left,
                            )
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                let piece_start = start + span * part_index as u64 / parts.len() as u64;
                let piece_end = start + span * (part_index + 1) as u64 / parts.len() as u64;
                let emphasize = piece_end.saturating_sub(piece_start) >= 1000
                    && (part.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
                        || (2..=7).contains(&part.trim().chars().count()));
                pieces.push(Piece {
                    characters,
                    emphasize,
                    line: shaped,
                    roman: phonetic,
                    x,
                    y,
                    width: w,
                    start: piece_start,
                    end: piece_end,
                    timed,
                    last: false,
                    phonetic: false,
                });
                x += w;
            }
        }
        if let Some(last) = pieces.last_mut() {
            last.last = true;
        }
        let mut height = y + row_height;
        if !fallback_roman.is_empty() {
            let text = fallback_roman.join(" ");
            x = 0.;
            y = height;
            for (i, c) in text.char_indices() {
                let shaped = self.shape(&text[i..i + c.len_utf8()], self.font_size * 0.5, window);
                let w = f32::from(shaped.width());
                if x > 0. && x + w > width {
                    x = 0.;
                    y += self.font_size * 0.65;
                }
                pieces.push(Piece {
                    line: shaped,
                    characters: Vec::new(),
                    roman: None,
                    x,
                    y,
                    width: w,
                    start: 0,
                    end: 0,
                    timed: false,
                    last: false,
                    phonetic: true,
                    emphasize: false,
                });
                x += w;
            }
            height = y + self.font_size * 0.65;
        }
        // Keep lyric row geometry fixed during scale animation, including duet/ruby alignment.
        let mut first = 0;
        while first < pieces.len() {
            let end = (first + 1..pieces.len())
                .find(|i| pieces[*i].y != pieces[first].y)
                .unwrap_or(pieces.len());
            let used = pieces[end - 1].x + pieces[end - 1].width;
            let inset = match self.align {
                TextAlign::Right => width - used,
                TextAlign::Center => (width - used) * 0.5,
                _ => 0.,
            };
            for piece in &mut pieces[first..end] {
                piece.x += inset;
            }
            first = end;
        }
        Layout { pieces, height }
    }
    fn paint_text(
        &self,
        shaped: &ShapedLine,
        origin: Point<Pixels>,
        alpha: f32,
        blur: f32,
        window: &mut Window,
        cx: &mut App,
    ) {
        let draw = |offset: Point<Pixels>, opacity: f32, window: &mut Window, cx: &mut App| {
            // A pass below half an 8-bit step blends to nothing: skip it (faded edges, the
            // blur kernel's corner taps).
            if opacity * self.edge.get() < 0.5 / 255. {
                return;
            }
            let _ = LineLayout::paint(
                shaped,
                origin + offset,
                shaped.font_size * 1.2,
                TextAlign::Left,
                None,
                &[DecorationRun {
                    len: shaped.text.len() as u32,
                    color: self.color.opacity(opacity * self.edge.get()),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                window,
                cx,
            );
        };
        if blur < 0.1 {
            draw(point(px(0.), px(0.)), alpha, window, cx);
            return;
        }
        // A normalized separable Gaussian kernel over native glyphs; no image readback/raster jobs.
        let taps = [
            (-1., 0.06136),
            (-0.5, 0.24477),
            (0., 0.38774),
            (0.5, 0.24477),
            (1., 0.06136),
        ];
        for (x, wx) in taps {
            for (y, wy) in taps {
                draw(
                    point(px(x * blur), px(y * blur)),
                    alpha * wx * wy,
                    window,
                    cx,
                );
            }
        }
    }
}
impl IntoElement for LyricText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for LyricText {
    type RequestLayoutState = ();
    type PrepaintState = Rc<Layout>;
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let text = self.clone();
        let style = Style {
            size: size(relative(1.).into(), Length::Auto),
            flex_shrink: 0.,
            ..Style::default()
        };
        let id = window.request_measured_layout(style, move |known, available, window, _| {
            let width = known.width.unwrap_or(match available.width {
                AvailableSpace::Definite(w) => w,
                _ => px(600.),
            });
            let layout = text.layout(f32::from(width), window);
            size(width, px(layout.height))
        });
        (id, ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> Rc<Layout> {
        if !bounds.intersects(&window.content_mask().bounds) {
            return Rc::new(Layout {
                pieces: Vec::new(),
                height: f32::from(bounds.size.height),
            });
        }
        self.layout(f32::from(bounds.size.width), window)
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        layout: &mut Rc<Layout>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let viewport = window.content_mask().bounds;
        if !bounds.intersects(&viewport) {
            return;
        }
        // Lines dissolve towards the scroll viewport's top and bottom edge instead of being
        // cut off, so the column reads as floating over the background.
        let fade = (f32::from(viewport.size.height) * 0.14).clamp(36., 120.);
        let middle = f32::from(bounds.origin.y + bounds.size.height * 0.5);
        let distance =
            (middle - f32::from(viewport.top())).min(f32::from(viewport.bottom()) - middle);
        let t = (distance / fade).clamp(0., 1.);
        self.edge.set(t * t * (3. - 2. * t));
        for piece in &layout.pieces {
            let elapsed = self.now_ms.saturating_sub(piece.start) as f32;
            let duration = piece.end.saturating_sub(piece.start).max(1) as f32;
            // A fixed text advance with per-word duration determines the reveal speed;
            // timing and emphasis must never stretch the layout between characters.
            let progress = if self.now_ms < piece.start {
                0.
            } else {
                (elapsed / duration).min(1.)
            };
            let emphasize = piece.emphasize;
            let lift = if self.motion && piece.timed {
                let hold = if self.now_ms < piece.start {
                    0.
                } else {
                    player_motion::easing(
                        (elapsed / duration.max(1000.)).min(1.),
                        [0., 0., 0.58, 1.],
                    )
                };
                self.font_size * (if self.line.background { 0.1 } else { 0.05 }) * hold
            } else {
                0.
            };
            let characters = if self.motion {
                piece.characters.as_slice()
            } else {
                &[]
            };
            let emphasis_count = if characters.is_empty() {
                usize::from(!piece.line.text.trim().is_empty())
            } else {
                characters
                    .iter()
                    .filter(|(glyph, _, _)| !glyph.text.trim().is_empty())
                    .count()
            }
            .max(1);
            let mut emphasis_index = 0;
            for glyph_index in 0..characters.len().max(1) {
                let (glyph, glyph_x, glyph_width) = match characters.get(glyph_index) {
                    Some((glyph, x, width)) => (glyph, *x, *width),
                    None => (&piece.line, 0., piece.width),
                };
                let glyph_emphasize = emphasize && !glyph.text.trim().is_empty();
                let delay = duration.max(1000.) * if piece.last { 1.2 } else { 1. }
                    / 2.5
                    / emphasis_count as f32
                    * emphasis_index as f32;
                if glyph_emphasize {
                    emphasis_index += 1;
                }
                let emphasis_elapsed = elapsed - delay;
                let mut amount = duration / 2000.;
                amount = if amount > 1. {
                    amount.sqrt()
                } else {
                    amount.powi(3)
                } * 0.6;
                let mut glow = duration / 3000.;
                glow = if glow > 1. { glow.sqrt() } else { glow.powi(3) } * 0.5;
                let mut emphasis_duration = duration.max(1000.);
                if piece.last {
                    amount *= 1.6;
                    glow *= 1.5;
                    emphasis_duration *= 1.2;
                }
                amount = amount.min(1.2);
                glow = glow.min(0.8);
                let pulse = if glyph_emphasize
                    && self.motion
                    && self.now_ms as f32 >= piece.start as f32 + delay
                    && emphasis_elapsed < emphasis_duration
                {
                    let x = emphasis_elapsed / emphasis_duration;
                    if x < 0.5 {
                        player_motion::easing(x * 2., [0.2, 0.4, 0.58, 1.])
                    } else {
                        1. - player_motion::easing((x - 0.5) * 2., [0.3, 0., 0.58, 1.])
                    }
                } else {
                    0.
                };
                let emphasis_lift = if glyph_emphasize
                    && self.motion
                    && self.now_ms as f32 + 400. >= piece.start as f32 + delay
                {
                    let phase = ((self.now_ms as f32 + 400. - piece.start as f32 - delay)
                        / (emphasis_duration * 1.4))
                        .clamp(0., 1.);
                    (phase * std::f32::consts::PI).sin()
                        * self.font_size
                        * if self.line.background { 0.1 } else { 0.05 }
                } else {
                    0.
                };
                // Quantized scale reuses the text cache while sustaining an emphasized long note.
                let emphasized = (pulse > 0. || (self.scale != 1. && !piece.phonetic)).then(|| {
                    // GPUI rasterizes by font size rather than transforming one glyph texture.
                    // Bound spring-generated atlas variants to subpixel size steps shared by words.
                    let paint_size = (self.font_size
                        * self.scale
                        * (1. + (pulse * 32.).round() / 32. * 0.1 * amount)
                        * 32.)
                        .round()
                        / 32.;
                    self.shape(&glyph.text, paint_size, window)
                });
                let shaped = emphasized.as_ref().unwrap_or(glyph);
                let origin = bounds.origin
                    + point(
                        px(piece.x + glyph_x + (glyph_width - f32::from(shaped.width())) * 0.5),
                        px(piece.y
                            - lift
                            - emphasis_lift
                            - pulse * amount * self.font_size * 0.025),
                    );
                let [dark, bright] = self.mask_alpha;
                // AMLL applies .85 opacity to the buffered main line after its masks.
                // Fold it into target alphas before solving the source-over overlay.
                let background = if self.line.background { 0.4 } else { 1. }
                    * if self.current && piece.timed {
                        0.85
                    } else {
                        1.
                    };
                let base = if self.subline {
                    1.
                } else if piece.phonetic {
                    dark * 0.8
                } else if self.current && !piece.timed {
                    0.85
                } else {
                    dark
                } * background;
                let overlay = (bright * background - base).max(0.) / (1. - base).max(0.001);
                self.paint_text(shaped, origin, base, self.blur, window, cx);
                if piece.timed
                    && (self.current || bright > dark + 0.001)
                    && self.now_ms >= piece.start
                {
                    let width = f32::from(shaped.width());
                    let fade = self.font_size * 0.5;
                    let edge = (progress * (piece.width + fade) - glyph_x)
                        * f32::from(shaped.font_size)
                        / self.font_size;
                    let solid = (edge - fade).max(0.).min(width);
                    let mask = |left: f32, w: f32| {
                        Bounds::new(
                            point(
                                origin.x + px(left),
                                bounds.origin.y - px(self.font_size * 0.2),
                            ),
                            size(px(w.max(0.)), bounds.size.height + px(self.font_size * 0.4)),
                        )
                    };
                    if solid > 0. {
                        window.with_content_mask(
                            Some(ContentMask {
                                bounds: mask(0., solid),
                            }),
                            |window| {
                                if pulse > 0. {
                                    self.paint_text(
                                        shaped,
                                        origin,
                                        pulse * glow * background,
                                        self.font_size * (glow * 0.3).min(0.3),
                                        window,
                                        cx,
                                    );
                                }
                                self.paint_text(shaped, origin, overlay, self.blur, window, cx);
                            },
                        );
                    }
                    // Soft mask follows fractional word time, rather than jumping between characters.
                    for slice in 0..16 {
                        let left = (edge - fade + fade * slice as f32 / 16.).max(0.);
                        let right =
                            (edge - fade + fade * (slice + 1) as f32 / 16.).clamp(0., width);
                        if right <= left {
                            continue;
                        }
                        window.with_content_mask(
                            Some(ContentMask {
                                bounds: mask(left, right - left),
                            }),
                            |window| {
                                self.paint_text(
                                    shaped,
                                    origin,
                                    (1. - (slice as f32 + 0.5) / 16.) * overlay,
                                    self.blur,
                                    window,
                                    cx,
                                );
                            },
                        );
                    }
                }
            }
            if let Some(roman) = &piece.roman {
                let [dark, bright] = self.mask_alpha;
                let background = if self.line.background { 0.4 } else { 1. }
                    * if self.current && piece.timed {
                        0.85
                    } else {
                        1.
                    };
                let origin = bounds.origin
                    + point(
                        px(piece.x + (piece.width - f32::from(roman.width())) * 0.5),
                        px(piece.y + self.font_size * 1.2),
                    );
                self.paint_text(
                    roman,
                    origin,
                    (dark + progress * (bright - dark)) * 0.8 * background,
                    self.blur,
                    window,
                    cx,
                );
            }
        }
    }
}

pub(super) fn interlude(
    start: u64,
    end: u64,
    now: u64,
    color: Hsla,
    animate: bool,
    font_size: f32,
) -> AnyElement {
    let dot = font_size * 0.5;
    let gap = font_size * 0.25 + 4.;
    let width = dot * 3. + gap * 2. + font_size * 1.5;
    let height = dot + font_size;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let duration = end.saturating_sub(start).max(1) as f32;
            let elapsed = now.saturating_sub(start) as f32;
            let remaining = duration - elapsed;
            let breathe = duration / (duration / 1500.).ceil();
            let mut scale = if animate {
                (1.5 * std::f32::consts::PI - elapsed / breathe * 2.).sin() / 20. + 1.
            } else {
                1.
            };
            if elapsed < 2000. {
                scale *= 1. - 2_f32.powf(-10. * elapsed / 2000.);
            }
            let mut alpha = ((elapsed - 500.) / 500.).clamp(0., 1.);
            if remaining < 750. {
                // Original easeInOutBack, sampled over the first half for the exit.
                let x = ((750. - remaining) / 750. / 2.).clamp(0., 0.5);
                let c = 1.70158 * 1.525;
                scale *= 1. - (2. * x).powi(2) * ((c + 1.) * 2. * x - c) / 2.;
            }
            if remaining < 375. {
                alpha *= (remaining / 375.).clamp(0., 1.);
            }
            scale = scale.max(0.) * 0.7;
            if elapsed > duration {
                scale = 0.;
            }
            let sweep = (duration - 750.).max(1.);
            for i in 0..3 {
                let lit = ((elapsed - sweep / 3. * i as f32) * 3. / sweep * 0.75).clamp(0.25, 1.);
                let d = dot * scale;
                let x = font_size * 0.75 + i as f32 * (dot + gap) + dot * 0.5;
                let origin = bounds.origin
                    + point(
                        px(width * 0.5 + (x - width * 0.5) * scale - d * 0.5),
                        px((height - d) * 0.5),
                    );
                window.paint_quad(
                    fill(
                        Bounds::new(origin, size(px(d), px(d))),
                        color.opacity(alpha * lit),
                    )
                    .corner_radii(px(d * 0.5)),
                );
            }
        },
    )
    .w(px(width))
    .h(px(height))
    .into_any_element()
}
