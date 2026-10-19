use super::*;
use crate::recognition::{Match, Progress};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct RecognitionState {
    pub(super) busy: bool,
    status: String,
    results: Vec<Match>,
    history: Vec<Match>,
    cancel: Option<Arc<AtomicBool>>,
    progress: Arc<Progress>,
    generation: u64,
    microphone: bool,
    resume_playback: bool,
    resume_identity: Option<(u64, String)>,
    history_error: Option<String>,
    scroll: ScrollHandle,
}
impl RecognitionState {
    pub(super) fn load(store: Option<&Storage>) -> Self {
        let loaded = store
            .map(|store| {
                mio_core::recognition_history::Snapshot::read(store)
                    .and_then(|snapshot| snapshot.rows().map(<[Match]>::to_vec))
                    .map_err(|_| "无法读取识别历史，原文件已保留；清空后可重新保存".to_owned())
            })
            .unwrap_or_else(|| Ok(Vec::new()));
        let (history, history_error) = match loaded {
            Ok(rows) => (rows, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        Self {
            busy: false,
            status: String::new(),
            results: Vec::new(),
            history,
            cancel: None,
            progress: Arc::default(),
            generation: 0,
            microphone: false,
            resume_playback: false,
            resume_identity: None,
            history_error,
            scroll: ScrollHandle::new(),
        }
    }
}
impl Drop for RecognitionState {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Mio {
    pub(super) fn cancel_recognition(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = &self.recognition.cancel {
            cancel.store(true, Ordering::Relaxed);
            self.recognition.status = if self.english {
                "Stopping...".into()
            } else {
                "正在停止...".into()
            };
        }
        self.recognition.progress.level.store(0, Ordering::Relaxed);
        // Remain busy until the worker releases its resources and acknowledges cancellation.
        cx.notify();
    }
    pub(super) fn suppress_recognition_resume(&mut self) {
        self.recognition.resume_playback = false;
    }
    fn resume_after_recognition(&mut self, cx: &mut Context<Self>) {
        let same_song = self
            .recognition
            .resume_identity
            .take()
            .is_some_and(|(generation, id)| {
                generation == self.player.generation
                    && self.player.song().is_some_and(|song| song.id == id)
            });
        if std::mem::take(&mut self.recognition.resume_playback)
            && same_song
            && self.player.status == Status::Paused
        {
            self.toggle_play(cx);
        }
    }
    fn begin_recognition(&mut self, file: Option<std::path::PathBuf>, cx: &mut Context<Self>) {
        if self.recognition.busy {
            return;
        }
        let microphone = file.is_none();
        if microphone && self.player.status == Status::Playing {
            self.toggle_play(cx);
            self.recognition.resume_playback = true;
            self.recognition.resume_identity = self
                .player
                .song()
                .map(|song| (self.player.generation, song.id.clone()));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Progress::default());
        self.recognition.cancel = Some(cancelled.clone());
        self.recognition.progress = progress.clone();
        self.recognition.busy = true;
        self.recognition.microphone = microphone;
        self.recognition.results.clear();
        self.recognition.status = self.t("music.recognize.recognizing");
        self.recognition.generation = self.recognition.generation.wrapping_add(1);
        let generation = self.recognition.generation;
        let task = cx.background_executor().spawn(async move {
            if let Some(file) = file {
                crate::recognition::recognize_file(&file, &cancelled)
            } else {
                crate::recognition::recognize_microphone(cancelled, progress)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.recognition.generation != generation {
                    return;
                }
                this.recognition.busy = false;
                let cancelled = this
                    .recognition
                    .cancel
                    .take()
                    .is_some_and(|cancel| cancel.load(Ordering::Relaxed));
                let result = if cancelled {
                    Err(if this.english {
                        "Recognition cancelled".into()
                    } else {
                        "识别已取消".into()
                    })
                } else {
                    result
                };
                match result {
                    Ok(results) => {
                        this.recognition.results = results.clone();
                        this.recognition.history.splice(0..0, results);
                        this.recognition.history.truncate(20);
                        this.recognition.status = if this.english {
                            "Songs recognized".into()
                        } else {
                            "已识别到歌曲".into()
                        };
                        if this.recognition.history_error.is_none()
                            && let Some(store) = &this.store
                            && store
                                .save("recognition-history.json", &this.recognition.history)
                                .is_err()
                        {
                            this.recognition.history_error =
                                Some("无法保存识别历史；原文件已保留".into());
                        }
                    }
                    Err(error) => this.recognition.status = error,
                }
                this.resume_after_recognition(cx);
                cx.notify();
            });
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let active = this
                    .update(cx, |this, cx| {
                        if !this.recognition.busy || this.recognition.generation != generation {
                            return false;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
    fn pick_recognition_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.recognition.busy {
            return;
        }
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.t("music.recognize.orUpload").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = selection.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| {
                    this.begin_recognition(Some(path), cx);
                });
            }
        })
        .detach();
    }
    fn play_recognition(
        &mut self,
        item: mio_music_sdk::MusicItem,
        start_time_ms: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(online) = self.online.clone() else {
            self.recognition.status = "请先在设置中启用支持网易云的音乐源".into();
            cx.notify();
            return;
        };
        if !online.sources().iter().any(|source| source.id == "wy") {
            self.recognition.status = "当前音乐源不支持网易云歌曲，可使用搜索".into();
            cx.notify();
            return;
        }
        let keep = self
            .player
            .queue
            .iter()
            .map(|song| song.id.clone())
            .collect();
        let Ok(row) = serde_json::to_value(&item) else {
            return;
        };
        if !online.register(vec![row], &keep) {
            self.recognition.status = "无法保存歌曲信息".into();
            cx.notify();
            return;
        }
        let raw = item
            .songmid
            .as_str()
            .map(str::to_owned)
            .or_else(|| item.songmid.as_u64().map(|v| v.to_string()))
            .unwrap_or_default();
        let song = Song {
            id: crate::online::song_id("wy", &raw),
            title: item.name,
            artist: item.singer,
            album: item.album_name,
            duration: 0,
            cover_art: (!item.img.is_empty()).then_some(item.img),
        };
        let index = self.enqueue_player_first(song);
        self.play_index(index, cx);
        if start_time_ms > 0 {
            self.player.seek(start_time_ms as f64 / 1000.);
        }
    }
    pub(super) fn recognize_page(
        &self,
        narrow: bool,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mode = self.ui_colors();
        let busy = self.recognition.busy;
        let seconds = self.recognition.progress.seconds.load(Ordering::Relaxed);
        div()
            .id("recognize-page")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.recognition.scroll)
            .px(px(if narrow {
                (width * 0.04).clamp(16., 24.)
            } else {
                28.
            }))
            .py(px(if narrow {
                (width * 0.035).clamp(14., 22.)
            } else {
                28.
            }))
            .flex()
            .flex_col()
            .gap(px(if narrow { 14. } else { 20. }))
            .text_color(color(mode, "--td-text-color-primary"))
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(if narrow { 8. } else { 24. }))
                    .pb(px(if narrow { 12. } else { 16. }))
                    .child(
                        div()
                            .relative()
                            .size(px(280.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .mb(px(if narrow { 18. } else { 24. }))
                            .when(busy && self.recognition.microphone, |stage| {
                                stage
                                    .child(
                                        div()
                                            .absolute()
                                            .inset_0()
                                            .rounded_full()
                                            .border(px(3.))
                                            .border_color(
                                                color(mode, "--td-brand-color").opacity(0.15),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .absolute()
                                            .inset(px(16.))
                                            .rounded_full()
                                            .border(px(2.))
                                            .border_color(
                                                color(mode, "--td-brand-color").opacity(0.25),
                                            ),
                                    )
                            })
                            .child(
                                gpui_kit::base::Button::new("recognize-center")
                                    .accessibility_label(self.t(if busy {
                                        "music.recognize.stopRecognize"
                                    } else {
                                        "music.recognize.startRecognize"
                                    }))
                                    .size(px(120.))
                                    .rounded_full()
                                    .bg(color(mode, "--td-brand-color"))
                                    .text_color(white())
                                    .shadow_md()
                                    .disabled(busy && !self.recognition.microphone)
                                    .child(if busy {
                                        icon("TD-StopCircle").size(px(48.)).into_any_element()
                                    } else {
                                        icon("TD-Microphone").size(px(48.)).into_any_element()
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if busy {
                                            this.cancel_recognition(cx);
                                        } else {
                                            this.begin_recognition(None, cx);
                                        }
                                    })),
                            ),
                    )
                    .when(busy, |stage| {
                        stage
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.t("music.recognize.recognizing")),
                            )
                            .child(
                                div()
                                    .mt(px(8.))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .child(self.t("music.recognize.recognizingTip")),
                            )
                            .child(
                                div()
                                    .w(px(220.))
                                    .h(px(6.))
                                    .mt(px(12.))
                                    .rounded_full()
                                    .bg(color(mode, "--td-bg-color-secondarycontainer"))
                                    .child(
                                        div()
                                            .w(px(220. * (seconds as f32 / 15.).clamp(0., 1.)))
                                            .h_full()
                                            .rounded_full()
                                            .bg(color(mode, "--td-brand-color")),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .mt(px(8.))
                                    .child(format!("{seconds}s / 15s")),
                            )
                    })
                    .when(!busy && self.recognition.results.is_empty(), |stage| {
                        stage
                            .child(
                                div()
                                    .text_size(px(if narrow {
                                        (width * 0.07).clamp(24., 32.)
                                    } else {
                                        24.
                                    }))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(self.t("music.recognize.tapToStart")),
                            )
                            .child(
                                div()
                                    .mt(px(8.))
                                    .text_size(px(14.))
                                    .text_color(color(mode, "--td-text-color-secondary"))
                                    .child(self.t("music.recognize.recordTip")),
                            )
                            .child(
                                Button::new("recognize-upload")
                                    .ghost()
                                    .mt(px(16.))
                                    .label(self.t("music.recognize.orUpload"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.pick_recognition_file(window, cx)
                                    })),
                            )
                    })
                    .when(!self.recognition.status.is_empty() && !busy, |stage| {
                        stage.child(
                            div()
                                .mt(px(12.))
                                .text_size(px(13.))
                                .child(self.recognition.status.clone()),
                        )
                    }),
            )
            .when(!self.recognition.results.is_empty(), |el| {
                el.children(
                    self.recognition
                        .results
                        .iter()
                        .enumerate()
                        .map(|(i, row)| self.recognition_row("result", i, row, cx)),
                )
                .child(
                    Button::new("recognize-continue")
                        .ghost()
                        .label(self.t("music.recognize.continueRecognize"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.recognition.results.clear();
                            this.recognition.status.clear();
                            cx.notify();
                        })),
                )
            })
            .when(
                !self.recognition.history.is_empty() || self.recognition.history_error.is_some(),
                |el| {
                    el.child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .child(self.t("music.recognize.history"))
                            .child(
                                Button::new("recognize-clear")
                                    .label(self.t("music.recognize.clearHistory"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(store) = &this.store
                                            && store
                                                .save(
                                                    "recognition-history.json",
                                                    &Vec::<Match>::new(),
                                                )
                                                .is_err()
                                        {
                                            this.recognition.status = "无法清除识别历史".into();
                                            cx.notify();
                                            return;
                                        }
                                        this.recognition.history.clear();
                                        this.recognition.history_error = None;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .children(
                        self.recognition
                            .history
                            .iter()
                            .enumerate()
                            .map(|(i, row)| self.recognition_row("history", i, row, cx)),
                    )
                },
            )
    }
    fn recognition_row(
        &self,
        group: &'static str,
        index: usize,
        row: &Match,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = row.item.clone();
        let start_time_ms = if group == "result" {
            row.start_time_ms
        } else {
            0
        };
        let query = format!("{} {}", item.name, item.singer);
        let timestamp = chrono::DateTime::from_timestamp_millis(row.timestamp)
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%m/%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let cover = if item.img.starts_with("https://") {
            ImageSource::from(item.img.clone())
        } else {
            ImageSource::from("images/default-cover.png")
        };
        let history_item = item.clone();
        div()
            .id((group, index))
            .flex()
            .items_center()
            .gap(px(12.))
            .child(
                img(cover)
                    .size(px(44.))
                    .rounded(px(6.))
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(|| {
                        img(ImageSource::from("images/default-cover.png"))
                            .size(px(44.))
                            .object_fit(ObjectFit::Cover)
                            .into_any_element()
                    }),
            )
            .when(group == "history", |el| {
                el.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.play_recognition(history_item.clone(), 0, cx)
                    }))
            })
            .py(px(8.))
            .border_b_1()
            .border_color(color(self.ui_colors(), "--td-border-level-1-color"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(item.name.clone()))
                    .child(
                        div()
                            .text_size(px(12.))
                            .truncate()
                            .child(format!("{} · {}", item.singer, item.album_name)),
                    )
                    .when(group == "history", |el| {
                        el.child(div().text_size(px(11.)).child(timestamp))
                    }),
            )
            .when(group == "result", |el| {
                el.child(
                    Button::new((group, index * 2))
                        .label(self.t("common.play"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.play_recognition(item.clone(), start_time_ms, cx)
                        })),
                )
                .child(
                    Button::new((group, index * 2 + 1))
                        .label(self.t("common.search"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.search
                                .update(cx, |input, cx| input.set_value(query.clone(), window, cx));
                            this.song_view.search_online = true;
                            this.song_view.search_playlists = false;
                            this.search(cx);
                        })),
                )
            })
            .into_any_element()
    }
}
