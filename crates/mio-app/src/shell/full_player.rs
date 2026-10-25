use super::*;

impl Mio {
    fn full_player_title(
        &mut self,
        title: String,
        width: f32,
        font_size: f32,
        narrow: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = (title.clone(), self.cover_layout, width.to_bits());
        let clock = &mut self.player_motion.title_clock;
        if clock.as_ref().is_none_or(|(text, layout, width, _)| {
            (text, *layout, *width) != (&key.0, key.1, key.2)
        }) {
            *clock = Some((key.0, key.1, key.2, Instant::now()));
        }
        let started = clock.as_ref().unwrap().3;
        // Cover, title and artist share one centre line in both layouts.
        let centered = true;
        let family: SharedString = ".SystemUIFont".into();
        canvas(
            move |bounds, window, cx| {
                let text: SharedString = title.into();
                let run = TextRun {
                    len: text.len(),
                    font: Font {
                        family,
                        weight: FontWeight::EXTRA_BOLD,
                        ..Font::default()
                    },
                    color: white().opacity(0.95),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line = window
                    .text_system()
                    .shape_line(text, px(font_size), &[run], None);
                let stride = f32::from(line.width()) + 2. * f32::from(window.rem_size());
                let overflow = (if narrow {
                    f32::from(line.width())
                } else {
                    stride
                }) > f32::from(bounds.size.width);
                let offset = if overflow && !cx.reduce_motion() {
                    if window.is_window_active() {
                        window.request_animation_frame();
                    }
                    ((started.elapsed().as_secs_f32() % 15. - 3.75) / 11.25).max(0.) * stride
                } else {
                    0.
                };
                (line, stride, overflow, offset)
            },
            move |bounds, (line, stride, overflow, offset), window, cx| {
                let inset = if centered && !overflow {
                    // Centre the text itself; the scroll gap is not part of it.
                    (f32::from(bounds.size.width) - f32::from(line.width())) * 0.5
                } else {
                    0.
                };
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    let origin = bounds.origin + point(px(inset - offset), px(0.));
                    let _ = line.paint(
                        origin,
                        px(font_size * 1.2),
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                    if overflow && !cx.reduce_motion() {
                        let _ = line.paint(
                            origin + point(px(stride), px(0.)),
                            px(font_size * 1.2),
                            TextAlign::Left,
                            None,
                            window,
                            cx,
                        );
                    }
                });
            },
        )
        .w_full()
        .h(px(font_size * 1.2))
        .into_any_element()
    }

    fn compact_lyric_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut center = 0;
        let mut lines = vec![String::new(); 3];
        if let Remote::Ready(tracks) = &self.lyrics
            && let Some(lyrics) = tracks.first()
        {
            let active = lyrics.active_line(self.lyric_position()).unwrap_or(0);
            let indices: Vec<_> = lyrics
                .lines
                .iter()
                .enumerate()
                .filter(|(_, line)| !line.background && !line.text.trim().is_empty())
                .map(|(i, _)| i)
                .collect();
            center = indices.iter().rposition(|i| *i <= active).unwrap_or(0);
            for (slot, text) in lines.iter_mut().enumerate() {
                if let Some(index) = (center + slot).checked_sub(1).and_then(|i| indices.get(i)) {
                    *text = lyrics.lines[*index].text.clone();
                }
            }
        }
        let message = match &self.lyrics {
            Remote::Idle | Remote::Loading => {
                if self.english {
                    "Loading lyrics…"
                } else {
                    "歌词加载中…"
                }
            }
            Remote::Failed(_) => {
                if self.english {
                    "Unable to load lyrics"
                } else {
                    "歌词加载失败"
                }
            }
            _ => {
                if self.english {
                    "No lyrics"
                } else {
                    "暂无歌词"
                }
            }
        };
        if lines[1].is_empty() {
            lines[1] = message.to_owned();
        }
        let motion = !cx.reduce_motion();
        let content = div()
            .w_full()
            .h(px(100.))
            .flex()
            .flex_col()
            .justify_center()
            .children(lines.into_iter().enumerate().map(|(i, text)| {
                div()
                    .w_full()
                    .h(px(28.))
                    .truncate()
                    .text_center()
                    .text_size(px(13.))
                    .line_height(px(28.))
                    .text_color(white().opacity(if i == 1 { 0.9 } else { 0.18 }))
                    .child(text)
            }));
        let content = if motion {
            content
                .with_animation(
                    SharedString::from(format!("compact-lyrics-{}-{center}", self.lyrics_revision)),
                    Animation::new(Duration::from_millis(380)),
                    |view, amount| {
                        let eased = 1. - (1. - amount).powi(3);
                        view.relative().top(px(12. * (1. - eased))).opacity(eased)
                    },
                )
                .into_any_element()
        } else {
            content.into_any_element()
        };
        gpui_kit::base::Button::new("compact-lyric-preview")
            .accessibility_label(if self.english {
                "Expand lyrics"
            } else {
                "展开歌词"
            })
            .w_full()
            .h(px(100.))
            .px(px(16.))
            .overflow_hidden()
            .child(content)
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.mobile_lyrics = true;
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn toggle_full_player(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.player.song().is_none() {
            return;
        }
        self.full_player_open = !self.full_player_open;
        self.player_settings_open = false;
        self.player_idle.hidden = false;
        self.player_idle.manual = false;
        if self.full_player_open {
            self.player_focus.focus(window, cx);
            self.wake_player(cx);
        }
        if self.full_player_open
            && (matches!(self.lyrics, Remote::Idle | Remote::Loading)
                || (self.cover.is_none()
                    && self
                        .player
                        .song()
                        .is_some_and(|song| song.cover_art.is_some())))
        {
            if matches!(self.lyrics, Remote::Idle) {
                self.lyrics = Remote::Loading;
            }
            self.load_lyrics(cx);
        }
        cx.notify();
    }

    /// Lyrics of the current online song from its source, parsed to lines.
    fn load_online_lyrics(&mut self, song: Song, cx: &mut Context<Self>) {
        if self.lyrics_busy {
            return;
        }
        let item = self
            .online
            .as_ref()
            .and_then(|online| online.item_json(&song.id))
            .and_then(|json| serde_json::from_value::<mio_music_sdk::MusicItem>(json).ok());
        let Some(item) = item else {
            self.lyrics = Remote::Failed(if self.english {
                "Song details are no longer available; search for it again".into()
            } else {
                "缺少歌曲信息，请重新搜索".into()
            });
            return;
        };
        self.lyrics_busy = true;
        self.lyrics_active = None;
        self.lyric_motion = Default::default();
        self.lyric_scroll.set_offset(point(px(0.), px(0.)));
        let revision = self.lyrics_revision;
        let task = cx.background_executor().spawn(async move {
            // Songs whose plugin offers TTML ask the TTML library first (word timings, background
            // vocals, translations); everything else, and a miss, uses the source's own answer.
            crate::lyrics::fetch_online(&item)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.lyrics_busy = false;
                if this.lyrics_revision == revision {
                    this.lyrics = match result {
                        // A source with no lyrics is "no lyrics", not a failure.
                        Ok(lyrics) => Remote::Ready(lyrics.into_iter().collect()),
                        Err(error) => Remote::Failed(error),
                    };
                } else if this.full_player_open && matches!(this.lyrics, Remote::Loading) {
                    this.load_lyrics(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn load_lyrics(&mut self, cx: &mut Context<Self>) {
        if let Some(song) = self
            .player
            .song()
            .filter(|song| crate::local_music::is_local(song))
            .cloned()
        {
            if self.lyrics_busy {
                return;
            }
            let Some(path) = self
                .local_music
                .library
                .as_ref()
                .and_then(|library| library.path(&song.id))
            else {
                self.lyrics = Remote::Ready(vec![]);
                return;
            };
            self.lyrics_busy = true;
            self.lyrics_active = None;
            self.lyric_motion = Default::default();
            self.lyric_scroll.set_offset(point(px(0.), px(0.)));
            let revision = self.lyrics_revision;
            let task = cx
                .background_executor()
                .spawn(async move { crate::local_music::lyrics(&path) });
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.lyrics_busy = false;
                    if this.lyrics_revision == revision {
                        this.lyrics = match result {
                            Ok(lyrics) => Remote::Ready(lyrics.into_iter().collect()),
                            Err(error) => Remote::Failed(error),
                        };
                    } else if matches!(this.lyrics, Remote::Loading) {
                        this.load_lyrics(cx);
                    }
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        if let Some(song) = self
            .player
            .song()
            .filter(|song| crate::online::is_online(song))
            .cloned()
        {
            self.load_online_lyrics(song, cx);
        }
    }

    /// Keep lyric masks, interludes and the scrolling spring moving while the view is visible.
    pub(super) fn lyrics_animating(&self) -> bool {
        self.full_player_open
            && self.lyric_motion.visible
            && matches!(&self.lyrics, Remote::Ready(tracks) if !tracks.is_empty())
            && ((self.player.status == Status::Playing && !self.player.buffering)
                || self.lyric_motion.settling
                || self.lyric_motion.manual_until.is_some()
                || self.lyric_motion.changed.elapsed() < Duration::from_millis(400))
    }

    /// Playback position for lyrics, in seconds. The engine reports every 200 ms with its sampling instant; between
    /// reports the position is advanced by the time since that instant (at most 0.6 s, so a
    /// stalled engine cannot run the lyrics away).
    pub(super) fn lyric_position(&self) -> f64 {
        let playing = self.player.status == Status::Playing && !self.player.buffering;
        let seen = (self.player.position, playing, self.player.generation);
        let clock = self.lyric_clock.get();
        let clock = if (clock.0, clock.1, clock.2) != seen {
            let reset = (seen.0, seen.1, seen.2, Instant::now());
            self.lyric_clock.set(reset);
            reset
        } else {
            clock
        };
        if playing {
            seen.0 + clock.3.elapsed().as_secs_f64().min(0.6)
        } else {
            seen.0
        }
    }

    fn lyric_view(
        &mut self,
        font_size: f32,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(feature = "parity")]
        let render_start = Instant::now();
        // `lyricFontWeight` / `lyricFontFamily` of the settings page; the default family is
        // the player's own font.
        let weight = self.app_settings.lyric_font_weight.clamp(100, 900) as f32;
        let family: Option<SharedString> = match self
            .app_settings
            .lyric_font_family
            .split(',')
            .map(str::trim)
            .find(|f| !f.is_empty())
        {
            None | Some("lyricfont") => Some("Alimama FangYuanTi VF".into()),
            Some(other) => Some(other.to_string().into()),
        };
        let position = self.lyric_position();
        let ready = match &self.lyrics {
            Remote::Ready(tracks) => tracks.first(),
            _ => None,
        };
        if let Some(lyrics) = ready {
            let effective_ms = position * 1000. + lyrics.offset as f64;
            let started = effective_ms >= 0.;
            let now_ms = effective_ms.max(0.) as u64;
            self.lyric_motion
                .prepare(&lyrics.lines, self.lyrics_revision);
            // A background vocal belongs to the preceding foreground row, never the scroll anchor.
            let groups = self.lyric_motion.groups.clone();
            let active = lyrics.active_line(position);
            if active != self.lyrics_active {
                self.lyric_motion.previous = self.lyrics_active;
                self.lyric_motion.changed = Instant::now();
                self.lyrics_active = active;
            }
            let playing = self.player.status == Status::Playing;
            let word_motion = !cx.reduce_motion();
            let motion = self.app_settings.jump_lyric && word_motion;
            let mut sung_end = Some(0);
            let gap = groups.iter().enumerate().find_map(|(g, i)| {
                let end = sung_end;
                let last = groups.get(g + 1).copied().unwrap_or(lyrics.lines.len());
                sung_end = lyrics.lines[*i..last]
                    .iter()
                    .filter_map(|line| line.end)
                    .max()
                    .map(|end| end.max(sung_end.unwrap_or(0)));
                let start = lyrics.lines[*i].start?;
                let end = end?;
                (started
                    && start.saturating_sub(250).saturating_sub(end) >= 4000
                    && now_ms.saturating_add(20) > end
                    && now_ms.saturating_add(20) < start.saturating_sub(250))
                .then_some((g, end, start.saturating_sub(250)))
            });
            let anchor = gap.map(|(g, _, _)| g).unwrap_or_else(|| {
                active
                    .and_then(|i| groups.iter().position(|g| *g == i))
                    .unwrap_or(0)
            });
            let align_position = if !self.cover_layout && self.show_left_panel {
                0.5
            } else {
                0.34
            };
            if lyrics.synced {
                self.lyric_motion.scroll(
                    &self.lyric_scroll,
                    anchor,
                    self.lyrics_revision,
                    position,
                    self.player.seeking(),
                    align_position,
                    motion,
                    if gap.is_some() {
                        None
                    } else {
                        groups
                            .get(anchor)
                            .and_then(|i| lyrics.lines[*i].start)
                            .map(|start| {
                                start.saturating_sub(
                                    anchor
                                        .checked_sub(1)
                                        .and_then(|g| groups.get(g))
                                        .and_then(|i| lyrics.lines[*i].start)
                                        .unwrap_or(0),
                                )
                            })
                    },
                );
            }
            let manual = self.lyric_motion.manual();
            let mut color = white();
            if self.immersive_lyric_color
                && let Some(palette) = self.theme_palette()
            {
                let rgb = palette.main.to_rgb();
                color = Rgba {
                    r: rgb.r * 0.2 + 0.8,
                    g: rgb.g * 0.2 + 0.8,
                    b: rgb.b * 0.2 + 0.8,
                    a: 1.,
                }
                .into();
            }
            let offset = lyrics.offset;
            let synced = lyrics.synced;
            let font = Font {
                family: family.unwrap_or_else(|| ".SystemUIFont".into()),
                weight: FontWeight(weight),
                ..Font::default()
            };
            let mut rows = Vec::new();
            let mut row_delay = 0.;
            let mut delay_step = 0.05;
            for (group, &index) in groups.iter().enumerate() {
                let last = groups.get(group + 1).copied().unwrap_or(lyrics.lines.len());
                let row_offset = self
                    .lyric_motion
                    .row_offset(group, row_delay, &self.lyric_scroll);
                let row_top = self.lyric_motion.row_target_top(group, &self.lyric_scroll);
                if row_top >= 0. {
                    row_delay += delay_step;
                    if group >= anchor {
                        delay_step /= 1.05;
                    }
                }
                let mut row = div().w_full().flex_col();
                if let Some((g, end, start)) = gap
                    && g == group
                {
                    row = row.child(
                        div()
                            .w_full()
                            .relative()
                            .top(px(row_offset))
                            .flex()
                            .when(!self.show_left_panel, |dots| dots.justify_center())
                            .when(lyrics.lines[index].duet, |dots| dots.justify_end())
                            .px(px(16.))
                            .py(px(font_size * 0.5))
                            .child(player_lyrics::interlude(
                                end, start, now_ms, color, motion, font_size,
                            )),
                    );
                }
                for (i, line) in lyrics.lines.iter().enumerate().take(last).skip(index) {
                    let sung = started
                        && line.start.is_some_and(|start| now_ms >= start)
                        && line.end.is_some_and(|end| now_ms < end);
                    let current = !synced || sung || (active == Some(i) && line.end.is_none());
                    if line.background && playing && !current {
                        continue;
                    }
                    let seconds = line
                        .start
                        .map(|start| ((start as f64 - offset as f64) / 1000.).max(0.));
                    let scale = self.lyric_motion.scale(i, current, motion, playing);
                    let mask_alpha = self.lyric_motion.mask_alpha(i, scale, current, motion);
                    let blur = if self.blur_lyrics && !manual && !current && synced {
                        (1. + group.abs_diff(anchor) as f32 + if group < anchor { 1. } else { 0. })
                            .mul_add(if viewport_width <= 1024. { 0.8 } else { 1. }, 0.)
                            .min(5.)
                    } else {
                        0.
                    };
                    let align = if line.duet {
                        TextAlign::Right
                    } else if !self.show_left_panel {
                        TextAlign::Center
                    } else {
                        TextAlign::Left
                    };
                    let size = font_size * if line.background { 0.7 } else { 1. };
                    let text = player_lyrics::LyricText {
                        line: self.lyric_motion.lines[i].clone(),
                        cache: self.lyric_motion.layouts[i][0].clone(),
                        font: font.clone(),
                        font_size: size,
                        scale,
                        color,
                        now_ms,
                        current,
                        mask_alpha,
                        subline: false,
                        motion: word_motion,
                        blur,
                        align,
                        edge: Cell::new(1.),
                    };
                    let mut button =
                        gpui_kit::base::Button::new(SharedString::from(format!("lyric-line-{i}")))
                            .accessibility_label(line.text.clone())
                            .relative()
                            .top(px(row_offset))
                            .w_full()
                            .py(px(size * 0.5))
                            .px(px(if viewport_width <= 500. {
                                20.
                            } else {
                                font_size
                            }))
                            .flex_col()
                            .gap(px(4.))
                            .items_start()
                            .hover(|button| button.bg(white().opacity(0.05)).rounded(px(8.)))
                            .child(text);
                    for (kind, sub) in self.lyric_motion.sublines[i]
                        .iter()
                        .enumerate()
                        .filter_map(|(kind, line)| line.as_ref().map(|line| (kind, line)))
                    {
                        button = button.child(player_lyrics::LyricText {
                            line: sub.clone(),
                            cache: self.lyric_motion.layouts[i][kind + 1].clone(),
                            font: font.clone(),
                            font_size: size * 0.5,
                            scale: 1.,
                            color: color.opacity(0.38),
                            now_ms,
                            current,
                            mask_alpha,
                            subline: true,
                            motion: false,
                            blur,
                            align,
                            edge: Cell::new(1.),
                        });
                    }
                    row = row.child(button.on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if synced
                            && this.player.status != Status::Loading
                            && let Some(seconds) = seconds
                        {
                            this.lyric_motion.manual_until = None;
                            this.suppress_recognition_resume();
                            this.player.seek(seconds);
                            this.update_progress(window, cx);
                            this.save_session();
                            cx.notify();
                        }
                    })));
                }
                rows.push(row);
            }
            #[cfg(feature = "parity")]
            self.lyric_motion.trace_frame(
                position,
                self.lyric_clock.get().3.elapsed().as_secs_f64(),
                render_start,
                playing && !self.player.buffering,
            );
            let scroll = self.lyric_scroll.clone();
            let geometry = self.lyric_motion.geometry.clone();
            let animating = self.lyrics_animating();
            return div()
                .on_children_prepainted(move |_, window, _| {
                    let measured = (
                        scroll.bounds(),
                        scroll.bounds_for_item(anchor),
                        scroll.max_offset(),
                    );
                    // A paused view still needs one fresh frame after wrapping/resizing changes
                    // the scroll geometry; the render phase only had the previous layout.
                    if geometry.replace(Some(measured)) != Some(measured)
                        || (animating && window.is_window_active())
                    {
                        window.request_animation_frame();
                    }
                })
                .id("full-player-lyrics")
                .test_support()
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.lyric_scroll)
                // Keep total padding at one viewport; larger self-sized padding grows on every frame.
                .pt(px(f32::from(self.lyric_scroll.bounds().size.height)
                    .max(400.)
                    * align_position))
                .pb(px(f32::from(self.lyric_scroll.bounds().size.height)
                    .max(400.)
                    * (1. - align_position)))
                .on_scroll_wheel(cx.listener(move |this, _, _, cx| {
                    if synced {
                        this.lyric_motion.manual_until =
                            Some(Instant::now() + Duration::from_secs(5));
                        cx.notify();
                    }
                }))
                .children(rows)
                .into_any_element();
        }
        let message = match &self.lyrics {
            Remote::Idle | Remote::Loading => {
                if self.english {
                    "Loading lyrics…"
                } else {
                    "歌词加载中…"
                }
            }
            Remote::Failed(_) => {
                if self.english {
                    "Unable to load lyrics"
                } else {
                    "歌词加载失败"
                }
            }
            Remote::Ready(_) => {
                if self.english {
                    "No lyrics"
                } else {
                    "暂无歌词"
                }
            }
        };
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(16.))
            .text_color(white().opacity(0.4))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(16.))
                    .child(message)
                    .when(matches!(self.lyrics, Remote::Failed(_)), |view| {
                        view.child(
                            Button::new("retry-lyrics")
                                .label(if self.english { "Retry" } else { "重试" })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.lyrics = Remote::Loading;
                                    this.load_lyrics(cx);
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(super) fn full_player(
        &mut self,
        width: f32,
        height: f32,
        fullscreen: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let cover = self
            .cover
            .clone()
            .map(ImageSource::from)
            .unwrap_or_else(|| ImageSource::from("images/default-cover.png"));
        let (title, artist, album) = self
            .player
            .song()
            .map(|song| (song.title.clone(), song.artist.clone(), song.album.clone()))
            .unwrap_or_default();
        let artist_singer = self
            .player
            .song()
            .and_then(|song| self.singer_of_song(song));
        let narrow = width <= 768.;
        let header_button = if narrow { 44. } else { 45. };
        let header_radius = if narrow { 22. } else { 10. };
        let header_idle = if narrow {
            0.
        } else {
            self.player_motion.idle_visibility
        };
        let bottom = mio_tokens::player_height(height);
        let disc_size = (width * 0.3)
            .min(700.)
            .min(height - bottom - 250.)
            .max(200.);
        let cover_mode = self.cover_layout;
        let short = narrow && height <= 700.;
        let mobile_gutter = (width * 0.04).clamp(16., 24.);
        let mobile_top = (width * 0.035).clamp(14., 22.);
        let compact_height = height - bottom - mobile_top - 56. - 18.;
        let compact_art = (width * 0.68)
            .min(if cover_mode { 300. } else { 280. })
            .min(((compact_height - 200.) / if cover_mode { 1. } else { 1.24 }).max(96.));
        let disc_size = if narrow { compact_art } else { disc_size };
        let padding = if narrow {
            mobile_gutter
        } else if cover_mode && self.show_left_panel {
            (width * 0.045).min(100.)
        } else {
            width * 0.1
        };
        let left_width = if narrow {
            width - 2. * mobile_gutter
        } else {
            (width - padding * 2.) * if cover_mode { 0.35 } else { 0.4 }
        };
        let title_width = if narrow {
            left_width
        } else if cover_mode {
            left_width - width * 0.06
        } else {
            left_width.min(520.)
        };
        let title_font = if narrow {
            (width * 0.066).clamp(23.2, 30.)
        } else {
            (width * 0.03).min(42.)
        };
        let title_view = self.full_player_title(title, title_width, title_font, narrow, cx);
        let texture_time = self.player_motion.texture_time;
        let smooth = |period: f32| {
            let phase = texture_time % period / period;
            player_motion::easing(
                if phase <= 0.5 {
                    phase * 2.
                } else {
                    (1. - phase) * 2.
                },
                [0.42, 0., 0.58, 1.],
            )
        };
        let record_ease = smooth(6.);
        let record_shine = 0.1 + 0.1 * record_ease;
        let record_half = texture_time % 6. / 3.;
        let record_rotation = (record_half.floor()
            + player_motion::easing(record_half.fract(), [0.42, 0., 0.58, 1.]))
            * std::f32::consts::PI;
        let use_black_text = self.background_config.enabled
            && self.cover_background.is_some()
            && self
                .theme_palette()
                .is_some_and(|palette| palette.use_black_text);
        let control =
            |id: &'static str, name: &'static str, label: String, mode: Mode, cx: &App| {
                crate::skin::control(id, name, label, mode, cx)
                    .size(px(36.))
                    .icon(match name {
                        "Settings" => glyph("shezhi").size(px(18.)),
                        "WindowMinimize" => glyph("zuixiaohua").size(px(18.)),
                        "WindowMaximize" => glyph("a-tingzhiwukuang").size(px(18.)),
                        "WindowClose" => glyph("a-quxiaoguanbi").size(px(18.)),
                        _ => icon(name),
                    })
                    .custom(
                        gpui_kit::component::button::ButtonCustomVariant::new(cx)
                            .foreground(if use_black_text {
                                black().opacity(0.6)
                            } else {
                                white().opacity(0.6)
                            })
                            .hover(if use_black_text {
                                black().opacity(0.2)
                            } else {
                                white().opacity(0.3)
                            }),
                    )
            };
        let label_phase = texture_time % 8. / 8.;
        let label_points = [0.05, 0.15, 0.1, 0.15, 0.05];
        let segment = (label_phase * 4.).floor() as usize;
        let local = label_phase * 4. - segment as f32;
        let ease = player_motion::easing(local, [0.42, 0., 0.58, 1.]);
        let label_shine =
            label_points[segment] + (label_points[segment + 1] - label_points[segment]) * ease;
        self.lyric_motion.visible = !narrow || self.mobile_lyrics;
        let lyrics = self.lyric_view(
            if narrow {
                (width * 0.07).clamp(24., 34.)
            } else {
                (width * 0.025).clamp(30., 50.).min(height * 0.05)
                    * self.app_settings.lyric_font_rate
            },
            width,
            cx,
        );
        let animated = self
            .player_motion
            .frame
            .as_ref()
            .filter(|frame| frame.generation == self.player.generation);
        let record_angle = animated.map(|frame| frame.angle).unwrap_or(0.);
        let rotating_cover = animated
            .and_then(|frame| frame.cover.clone())
            .map(ImageSource::from)
            .unwrap_or_else(|| cover.clone());
        let pointer = if let Some(frame) = animated {
            let scale = disc_size * 0.3 / frame.pointer_width;
            img(frame.pointer.clone())
                .absolute()
                .left(px(disc_size * 0.52 + frame.pointer_offset.0 * scale))
                .top(px(-disc_size * 0.24 + frame.pointer_offset.1 * scale))
                .w(px(frame.pointer_size.0 * scale))
                .h(px(frame.pointer_size.1 * scale))
                .into_any_element()
        } else {
            img("images/pointer.png")
                .absolute()
                .left(relative(0.52))
                .top(relative(-0.24))
                .w(relative(0.3))
                .into_any_element()
        };
        let cover_size = if narrow {
            compact_art
        } else {
            (left_width - width * 0.06)
                .min(480.)
                .min(height * 0.45)
                .max(120.)
        };
        // Only macOS paints its own background surface over this one.
        #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
        let mut background_view = self
            .cover_background
            .clone()
            .filter(|_| self.background_config.enabled)
            .map(|image| {
                img(image)
                    .absolute()
                    .w_full()
                    .h(px(height))
                    .object_fit(ObjectFit::Cover)
                    .into_any_element()
            });
        #[cfg(target_os = "macos")]
        if self.background_config.enabled
            && let Some(frame) = self.background.frame()
        {
            background_view = Some(
                surface(frame)
                    .absolute()
                    .w_full()
                    .h(px(height))
                    .object_fit(ObjectFit::Fill)
                    .into_any_element(),
            );
        }
        let disc = div()
            .relative()
            .size(px(disc_size))
            .when(narrow, |disc| disc.mt(px(disc_size * 0.24)))
            .shadow_lg()
            .child(img(vinyl_image()).size_full())
            .children(self.vinyl_grooves.as_ref().map(|image| {
                let image = image.clone();
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        // Crop a fixed 700px texture instead of stretching its 8px rings.
                        let image_bounds = Bounds::new(
                            bounds.center() - point(px(350.), px(350.)),
                            size(px(700.), px(700.)),
                        );
                        let _ = window.paint_image(
                            bounds,
                            image_bounds,
                            Corners::all(bounds.size.width / 2.),
                            image.clone(),
                            0,
                            false,
                        );
                    },
                )
                .absolute()
                .left(px(0.))
                .top(px(0.))
                .size_full()
            }))
            .child(
                svg()
                    .path("images/vinyl-texture.svg")
                    .absolute()
                    .left(px(0.))
                    .top(px(0.))
                    .size_full()
                    .text_color(white())
                    .with_transformation(Transformation::rotate(radians(record_angle))),
            )
            .child(
                svg()
                    .data(record_shine_mask(1. + 0.1 * record_ease))
                    .absolute()
                    .left(px((disc_size - 400.) * 0.5))
                    .top(px((disc_size - 400.) * 0.5))
                    .size(px(400.))
                    .text_color(white().opacity(record_shine))
                    .with_transformation(
                        Transformation::rotate(radians(record_angle + record_rotation))
                            .with_scaling(size(disc_size / 400., disc_size / 400.)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(relative(0.2))
                    .top(relative(0.2))
                    .size(relative(0.6))
                    .rounded_full()
                    .child(img(vinyl_label_image()).absolute().size_full())
                    .child(
                        img(rotating_cover)
                            .absolute()
                            .left(relative(0.025))
                            .top(relative(0.025))
                            .size(relative(0.95))
                            .rounded_full()
                            .object_fit(ObjectFit::Cover),
                    )
                    // Monochrome gradient masks rotate on the GPU; no raster jobs.
                    .child(
                        svg()
                            .path("images/label-shine.svg")
                            .absolute()
                            .left(px(0.))
                            .top(px(0.))
                            .size_full()
                            .text_color(white().opacity(label_shine))
                            .with_transformation(Transformation::rotate(radians(
                                record_angle
                                    + ((label_phase * 2.).floor()
                                        + player_motion::easing(
                                            (label_phase * 2.).fract(),
                                            [0.42, 0., 0.58, 1.],
                                        ))
                                        * std::f32::consts::PI,
                            ))),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(relative(0.46))
                    .top(relative(0.46))
                    .size(relative(0.08))
                    .rounded_full()
                    .shadow_sm()
                    .child(img(center_hole_image()).size_full()),
            )
            .child(pointer);
        div()
            .id("full-player")
            .size_full()
            .when(narrow, |view| view.h(px(height)))
            .min_h_0()
            .relative()
            .font_family(".SystemUIFont")
            .overflow_hidden()
            .bg(rgb(0x19232b))
            .children(background_view)
            .child(div().absolute().size_full().bg(black().opacity(0.256)))
            .child(
                div()
                    .id("full-player-main")
                    .relative()
                    .size_full()
                    .flex()
                    .px(px(padding))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if narrow && this.mobile_lyrics {
                            this.mobile_lyrics = false;
                            cx.notify();
                        }
                    }))
                    .when(narrow, |main| {
                        main.flex_col()
                            .items_center()
                            .justify_center()
                            .overflow_y_scroll()
                            .pt(px(mobile_top + 44. + if short { 8. } else { 12. }))
                            .pb(px(bottom + 18.))
                    })
                    .child(
                        div()
                            .w(px(left_width))
                            .min_w_0()
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap(px(if narrow {
                                16.
                            } else if cover_mode {
                                40.
                            } else if short {
                                10.4
                            } else {
                                36.
                            }))
                            .when(narrow, |left| left.min_h_0().mt(px(8.)))
                            .when(cover_mode && !narrow, |left| left.px(px(width * 0.03)))
                            .child(if cover_mode {
                                div()
                                    .mt(px(if narrow { 0. } else { bottom * 0.5 }))
                                    .size(px(cover_size))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        img(cover)
                                            .size(px(cover_size * self.player_motion.cover_scale))
                                            .flex_shrink_0()
                                            .rounded(px(24.))
                                            .object_fit(ObjectFit::Cover),
                                    )
                                    .into_any_element()
                            } else {
                                disc.into_any_element()
                            })
                            .child(
                                div()
                                    .w_full()
                                    .when(!cover_mode, |info| info.max_w(px(520.)))
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(if narrow { 8. } else { 12. }))
                                    .child(title_view)
                                    .child(
                                        div()
                                            .w_full()
                                            .flex()
                                            .when(!narrow, |meta| meta.flex_wrap())
                                            .items_center()
                                            .justify_center()
                                            .gap(px(8.))
                                            .text_size(px(if narrow {
                                                13.
                                            } else {
                                                (width * 0.015).min(20.)
                                            }))
                                            .when(narrow, |meta| meta.line_height(relative(1.35)))
                                            .font_weight(if narrow {
                                                FontWeight::NORMAL
                                            } else {
                                                FontWeight::SEMIBOLD
                                            })
                                            .text_color(white().opacity(0.33))
                                            .child(
                                                div()
                                                    .id("full-player-artist")
                                                    .max_w_full()
                                                    .when(narrow, |artist| artist.py(px(10.)))
                                                    .when(narrow, |artist| {
                                                        artist
                                                            .max_w(relative(0.46))
                                                            .min_w_0()
                                                            .truncate()
                                                            .text_right()
                                                    })
                                                    .text_color(
                                                        self.theme_palette()
                                                            .map(|palette| palette.artist)
                                                            .unwrap_or(white().opacity(0.495)),
                                                    )
                                                    // `.artist.singer-link`: brighter on hover.
                                                    .when_some(artist_singer, |el, singer| {
                                                        el.cursor_pointer()
                                                            .hover(|el| el.text_color(white()))
                                                            .on_click(cx.listener(
                                                                move |this, _, window, cx| {
                                                                    cx.stop_propagation();
                                                                    this.open_singer(
                                                                        singer.clone(),
                                                                        window,
                                                                        cx,
                                                                    );
                                                                },
                                                            ))
                                                    })
                                                    .child(artist.clone()),
                                            )
                                            .when(
                                                narrow || (!artist.is_empty() && !album.is_empty()),
                                                |meta| {
                                                    meta.child(
                                                        div()
                                                            .text_color(white().opacity(0.132))
                                                            .child("/"),
                                                    )
                                                },
                                            )
                                            .child(
                                                div()
                                                    .max_w_full()
                                                    .when(narrow, |album| {
                                                        album
                                                            .max_w(relative(0.46))
                                                            .min_w_0()
                                                            .truncate()
                                                    })
                                                    .child(album),
                                            ),
                                    ),
                            )
                            .when(narrow, |left| left.child(self.compact_lyric_preview(cx)))
                            .when(
                                (!narrow && !self.show_left_panel)
                                    || (narrow && self.mobile_lyrics),
                                |left| left.hidden(),
                            ),
                    )
                    .when(!narrow || self.mobile_lyrics, |main| {
                        main.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .min_h_0()
                                .h(px(height - bottom))
                                .px(px(20.))
                                .when(narrow, |right| {
                                    right.w_full().h(px(height - bottom - mobile_top - 62.))
                                })
                                .when(cover_mode, |right| right.pl(px(width * 0.03)))
                                .when(!self.show_left_panel, |right| right.px(px(width * 0.1)))
                                .child(lyrics),
                        )
                    }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(0.))
                    .left(px(0.))
                    .w_full()
                    .h(px(80.))
                    .when(self.player_idle.hidden, |header| header.invisible())
                    .on_mouse_down(MouseButton::Left, |event, window, _| {
                        if !window.default_prevented() {
                            if event.click_count == 2 {
                                window.titlebar_double_click();
                            } else {
                                window.start_window_move();
                            }
                        }
                    }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(if narrow {
                        (width * 0.035).clamp(14., 22.)
                    } else {
                        25.
                    } - 45. * header_idle))
                    .opacity((1. - header_idle).clamp(0., 1.))
                    .when(header_idle >= 1., |header| header.invisible())
                    .left(px(if narrow {
                        (width * 0.04).clamp(16., 24.)
                    } else {
                        30.
                    }))
                    .flex()
                    .gap(px(15.))
                    .when(cfg!(target_os = "macos"), |header| {
                        #[cfg(target_os = "macos")]
                        {
                            // Siblings precede the button fills and icons.
                            header.children(
                                (if narrow { vec![0.] } else { vec![0., 60.] })
                                    .into_iter()
                                    .map(|left| {
                                        canvas(
                                            |_, _, _| (),
                                            move |bounds, _, window, _| {
                                                window.paint_backdrop(
                                                    bounds,
                                                    Corners::all(px(header_radius)),
                                                    if narrow { 12. } else { 6. },
                                                    if narrow { 1.8 } else { 1. },
                                                );
                                            },
                                        )
                                        .absolute()
                                        .left(px(left))
                                        .top_0()
                                        .size(px(header_button))
                                    }),
                            )
                        }
                        #[cfg(not(target_os = "macos"))]
                        {
                            header
                        }
                    })
                    .child(
                        control(
                            "full-player-fullscreen",
                            if fullscreen {
                                "TD-FullscreenExit"
                            } else {
                                "TD-Fullscreen"
                            },
                            if self.english {
                                if fullscreen {
                                    "Exit fullscreen"
                                } else {
                                    "Fullscreen"
                                }
                                .into()
                            } else {
                                if fullscreen { "退出全屏" } else { "全屏" }.into()
                            },
                            Mode::Dark,
                            cx,
                        )
                        .size(px(45.))
                        .icon(
                            icon(if fullscreen {
                                "TD-FullscreenExit"
                            } else {
                                "TD-Fullscreen"
                            })
                            .size(px(25.)),
                        )
                        .disabled(!narrow && self.player_idle.hidden)
                        .rounded(px(10.))
                        .bg(if use_black_text {
                            black().opacity(0.1)
                        } else {
                            white().opacity(0.1)
                        })
                        .when(narrow, |button| button.hidden())
                        .on_click(|_, window, _| window.toggle_fullscreen()),
                    )
                    .child(
                        control(
                            "collapse-full-player",
                            "TD-Down",
                            if self.english {
                                "Collapse player".into()
                            } else {
                                "收起播放器".into()
                            },
                            Mode::Dark,
                            cx,
                        )
                        .size(px(header_button))
                        .icon(icon("TD-Down").size(px(if narrow { 22. } else { 25. })))
                        .disabled(!narrow && self.player_idle.hidden)
                        .rounded(px(header_radius))
                        .bg(if narrow {
                            black().opacity(0.2)
                        } else if use_black_text {
                            black().opacity(0.1)
                        } else {
                            white().opacity(0.1)
                        })
                        .when(narrow, |button| {
                            button
                                .border_t(px(0.5))
                                .border_b(px(0.5))
                                .border_l(px(0.5))
                                .border_r(px(0.5))
                                .border_color(white().opacity(0.2))
                                .shadow(vec![
                                    BoxShadow::new(px(0.), px(10.), black().opacity(0.18))
                                        .blur_radius(px(26.)),
                                ])
                                .custom(
                                    gpui_kit::component::button::ButtonCustomVariant::new(cx)
                                        .foreground(white().opacity(0.9))
                                        .hover(black().opacity(0.2)),
                                )
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.full_player_open = false;
                            this.player_settings_open = false;
                            cx.notify();
                        })),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top(px(30. - 70. * self.player_motion.idle_visibility))
                    .opacity((1. - self.player_motion.idle_visibility).clamp(0., 1.))
                    .when(self.player_motion.idle_visibility >= 1., |header| {
                        header.invisible()
                    })
                    .right(px(30.))
                    .flex()
                    .when(narrow, |header| header.hidden())
                    .gap(px(2.))
                    .child(
                        control(
                            "full-player-settings",
                            "Settings",
                            self.t("common.titleBar.settings"),
                            Mode::Dark,
                            cx,
                        )
                        .disabled(self.player_idle.hidden)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.full_player_open = false;
                            this.player_settings_open = false;
                            this.go(Page::Appearance, cx);
                        })),
                    )
                    .child(
                        div()
                            .w(px(1.))
                            .h(px(16.))
                            .my_auto()
                            .mx(px(3.))
                            .bg(if use_black_text { black() } else { white() }),
                    )
                    .child(
                        control(
                            "full-player-minimize",
                            "WindowMinimize",
                            self.t("common.titleBar.minimize"),
                            Mode::Dark,
                            cx,
                        )
                        .disabled(self.player_idle.hidden)
                        .on_click(|_, window, _| window.minimize_window()),
                    )
                    .child(
                        control(
                            "full-player-maximize",
                            "WindowMaximize",
                            self.t("common.titleBar.maximize"),
                            Mode::Dark,
                            cx,
                        )
                        .disabled(self.player_idle.hidden)
                        .on_click(|_, window, _| window.zoom_window()),
                    )
                    .child(
                        control(
                            "full-player-close",
                            "WindowClose",
                            self.t("common.close"),
                            Mode::Dark,
                            cx,
                        )
                        .disabled(self.player_idle.hidden)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.request_close(window, cx)),
                        ),
                    ),
            )
            .child(self.player_spectrum(width, height))
            .child(self.player_settings(width, height, cx))
            .when(
                self.background_warning
                    .is_some_and(|started| started.elapsed() < Duration::from_secs(5)),
                |view| {
                    view.child(
                        div()
                            .absolute()
                            .top(px(80.))
                            .left(px((width - 600.).max(48.) * 0.5))
                            .w(px((width - 48.).min(600.)))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.))
                            .p(px(12.))
                            .rounded(px(8.))
                            .bg(rgb(0xff9800).opacity(0.95))
                            .text_color(white())
                            .child(self.t("play.performanceWarning"))
                            .child(
                                Button::new("dismiss-background-warning")
                                    .label("×")
                                    .accessibility_label(if self.english {
                                        "Dismiss performance warning"
                                    } else {
                                        "关闭性能提示"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.background_warning = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                },
            )
    }
}

fn vinyl_image() -> Arc<Image> {
    static VINYL: std::sync::OnceLock<Arc<Image>> = std::sync::OnceLock::new();
    VINYL
        .get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../../assets/images/vinyl.svg").to_vec(),
            ))
        })
        .clone()
}

fn vinyl_label_image() -> Arc<Image> {
    static LABEL: std::sync::OnceLock<Arc<Image>> = std::sync::OnceLock::new();
    LABEL
        .get_or_init(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("../../assets/images/vinyl-label.svg").to_vec(),
            ))
        })
        .clone()
}

fn center_hole_image() -> Arc<Image> {
    static HOLE: std::sync::OnceLock<Arc<Image>> = std::sync::OnceLock::new();
    HOLE.get_or_init(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            include_bytes!("../../assets/images/vinyl-hole.svg").to_vec(),
        ))
    })
    .clone()
}

pub(super) fn vinyl_grooves() -> Arc<RenderImage> {
    static GROOVES: std::sync::OnceLock<Arc<RenderImage>> = std::sync::OnceLock::new();
    GROOVES
        .get_or_init(|| {
            // Two samples per logical pixel; one shared image for every disc size.
            let pixels = image::RgbaImage::from_fn(1400, 1400, |x, y| {
                let radius = ((x as f32 + 0.5 - 700.).hypot(y as f32 + 0.5 - 700.)) / 2.;
                let phase = radius % 8.;
                let strength = if phase <= 1. {
                    phase
                } else {
                    (2. - phase).max(0.)
                };
                image::Rgba([255, 255, 255, (strength * 0.03 * 255.).round() as u8])
            });
            Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))
        })
        .clone()
}

// SVG sprite transforms have no circular clip. Twenty-one fixed circle masks
// sample only the gradient scale (0.005 steps), retaining one atlas size across resizes.
fn record_shine_mask(scale: f32) -> &'static [u8] {
    static MASKS: std::sync::OnceLock<Vec<Vec<u8>>> = std::sync::OnceLock::new();
    let masks = MASKS.get_or_init(|| (0..=20).map(|index| {
        let scale = 1. + index as f32 * 0.005;
        let center = 200. - 80. * scale;
        let radius = 280_f32.hypot(280.) * scale;
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="400" viewBox="0 0 400 400"><defs><radialGradient id="shine" gradientUnits="userSpaceOnUse" cx="{center}" cy="{center}" r="{radius}"><stop stop-color="white" stop-opacity="0.08"/><stop offset="0.25" stop-color="white" stop-opacity="0.04"/><stop offset="0.5" stop-color="white" stop-opacity="0.02"/><stop offset="0.75" stop-color="white" stop-opacity="0.01"/><stop offset="1" stop-color="white" stop-opacity="0"/></radialGradient></defs><circle cx="200" cy="200" r="200" fill="url(#shine)"/></svg>"#).into_bytes()
    }).collect());
    &masks[((scale - 1.) / 0.005).round().clamp(0., 20.) as usize]
}

pub(super) const LYRIC_DIM: f32 = 0.35;
pub(super) const LYRIC_LIT: f32 = 0.95;

/// The current word-timed line with its sung part lit: words before `now_ms` are lit, the word
/// being sung is lit up to its fraction, and the rest stays dim. The fill moves in whole
/// characters with the one under the sweep faded in, which stands in for AMLL's soft-edged
/// gradient mask over the word.
///
/// GPUI composites a highlight colour over the text colour and keeps the text colour's alpha
/// unless the highlight is opaque, so a translucent white could never lighten dim text. Every
/// character therefore gets an opaque white with a `fade_out` that sets the brightness.
pub(super) fn word_highlight(
    line: &crate::lyrics::Line,
    now_ms: u64,
    base: Hsla,
    dim: f32,
    lit: f32,
) -> StyledText {
    // Brightness steps, so neighbouring characters share one run.
    const STEPS: f32 = 24.;
    let opaque = Hsla { a: 1., ..base };
    let mut runs: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut offset = 0;
    let mut push = |start: usize, end: usize, alpha: f32| {
        if let Some((range, style)) = runs.last_mut()
            && range.end == start
            && style.fade_out == Some((1. - alpha).clamp(0., 1.))
        {
            range.end = end;
            return;
        }
        runs.push((
            start..end,
            HighlightStyle {
                color: Some(opaque),
                fade_out: Some((1. - alpha * base.a).clamp(0., 1.)),
                ..HighlightStyle::default()
            },
        ));
    };
    for word in &line.words {
        let span = word.end.saturating_sub(word.start).max(1) as f32;
        let fraction = if now_ms >= word.end {
            1.
        } else if now_ms <= word.start {
            0.
        } else {
            (now_ms - word.start) as f32 / span
        };
        let count = word.text.chars().count().max(1) as f32;
        let filled = fraction * count;
        for (index, ch) in word.text.chars().enumerate() {
            let covered = ((filled - index as f32).clamp(0., 1.) * STEPS).round() / STEPS;
            let alpha = dim + (lit - dim) * covered;
            let end = offset + ch.len_utf8();
            push(offset, end, alpha);
            offset = end;
        }
    }
    StyledText::new(line.text.clone()).with_highlights(runs)
}

#[cfg(test)]
mod tests {
    use super::{Mio, Remote, Status};
    use gpui_kit::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, Window, px, size,
    };
    use std::time::{Duration, Instant};

    struct LyricFixture(Entity<Mio>);
    impl Render for LyricFixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0.update(cx, |main, cx| {
                main.lyric_motion.visible = true;
                main.lyric_view(32., 1200., cx)
            })
        }
    }

    #[gpui_kit::test]
    fn lyrics_request_display_frames_and_stop_after_pause_or_close(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let mut main = None;
        let handle = cx.open_window(size(px(800.), px(600.)), |window, cx| {
            let view = cx.new(|cx| Mio::new_in(None, window, cx));
            main = Some(view.clone());
            LyricFixture(view)
        });
        let main = main.unwrap();
        cx.run_until_parked();
        main.update(cx, |this, cx| {
            this.full_player_open = true;
            this.app_settings.jump_lyric = false;
            this.player.status = Status::Playing;
            this.lyrics = Remote::Ready(vec![crate::lyrics::Lyrics {
                synced: true,
                offset: 0,
                word_timed: false,
                lines: vec![crate::lyrics::Line {
                    start: Some(0),
                    text: "Lyric fixture".into(),
                    ..Default::default()
                }],
            }]);
            cx.notify();
        });
        cx.update_window(handle.into(), |_, window, _| {
            window.activate_window();
            window.refresh();
        })
        .unwrap();
        cx.run_until_parked();
        let next_frame = |cx: &mut TestAppContext| {
            let count = cx
                .update_window(handle.into(), |_, window, cx| {
                    window.simulate_next_frame(cx)
                })
                .unwrap();
            cx.run_until_parked();
            count
        };
        for _ in 0..4 {
            assert!(next_frame(cx) > 0);
        }
        main.update(cx, |main, cx| {
            main.player.status = Status::Paused;
            main.lyric_motion.changed = Instant::now() - Duration::from_secs(1);
            cx.notify();
        });
        assert!(next_frame(cx) > 0);
        assert_eq!(next_frame(cx), 0);
        main.update(cx, |main, cx| {
            main.player.status = Status::Playing;
            cx.notify();
        });
        cx.update_window(handle.into(), |_, window, _| window.refresh())
            .unwrap();
        assert!(next_frame(cx) > 0);
        main.update(cx, |main, cx| {
            main.full_player_open = false;
            cx.notify();
        });
        assert!(next_frame(cx) > 0);
        assert_eq!(next_frame(cx), 0);
        cx.update_window(handle.into(), |_, window, _| window.remove_window())
            .unwrap();
        drop(main);
        cx.run_until_parked();
    }
}
