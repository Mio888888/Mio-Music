use super::*;
use crate::library::{self, Playlist};
use base64::{Engine, engine::general_purpose::STANDARD};
use gpui_kit::component::{WindowExt, notification::Notification};
use std::{collections::HashMap, io::Read, path::Path};

const MAX_BYTES: usize = 5 * 1024 * 1024;
const CACHE_BYTES: usize = 128 * 1024 * 1024;
pub(super) type CoverCache = HashMap<String, (String, Option<Arc<RenderImage>>)>;

enum CoverError {
    Type,
    Size,
    Process,
    Save,
}

#[derive(IntoElement)]
pub(super) struct InlineCover {
    pub parent: WeakEntity<Mio>,
    pub id: String,
    pub url: String,
    pub radius: f32,
}
impl RenderOnce for InlineCover {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let image = self.parent.upgrade().and_then(|parent| {
            parent
                .read(cx)
                .local_playlists
                .covers
                .get(&self.id)
                .filter(|(url, _)| url == &self.url)
                .and_then(|(_, image)| image.clone())
        });
        div()
            .relative()
            .size_full()
            .child(
                img(image
                    .map(ImageSource::from)
                    .unwrap_or_else(|| ImageSource::from("images/default-cover.png")))
                .id(SharedString::from(format!(
                    "local-playlist-cover-{}",
                    self.id
                )))
                .size_full()
                .rounded_t(px(self.radius))
                .object_fit(ObjectFit::Cover),
            )
            .child(
                canvas(
                    move |bounds, window, cx| {
                        if bounds.intersects(&window.content_mask().bounds) {
                            let _ = self.parent.update(cx, |this, cx| {
                                this.visible_playlist_cover(self.id, self.url, cx);
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0()
                .size_full(),
            )
    }
}

fn decode(bytes: &[u8], mime: &str, renderer: &SvgRenderer) -> Option<Arc<RenderImage>> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decode_inner(bytes, mime, renderer)
    }))
    .ok()
    .flatten()
}

fn decode_inner(bytes: &[u8], mime: &str, renderer: &SvgRenderer) -> Option<Arc<RenderImage>> {
    if mime == "image/svg+xml" {
        return renderer.render_inline_image(bytes, Some(512)).ok();
    }
    gpui_kit::decode_raster_image(bytes, Some(512))
}

pub(super) fn image_bytes(image: &RenderImage) -> usize {
    (0..image.frame_count())
        .map(|i| image.as_bytes(i).map_or(0, |bytes| bytes.len()))
        .sum()
}

pub(super) fn cache_bytes(cache: &CoverCache) -> usize {
    cache
        .values()
        .filter_map(|(_, image)| image.as_ref())
        .map(|image| image_bytes(image))
        .sum()
}

fn still_frame(image: &RenderImage) -> Option<Arc<RenderImage>> {
    let size = image.size(0);
    let pixels = image::RgbaImage::from_raw(
        size.width.0 as u32,
        size.height.0 as u32,
        image.as_bytes(0)?.to_vec(),
    )?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
}

pub(super) fn decode_url(url: &str, renderer: &SvgRenderer) -> Option<Arc<RenderImage>> {
    // Inline images never enter the HTTP asset loader or its error logs.
    if url.len() > MAX_BYTES.div_ceil(3) * 4 + 128 {
        return None;
    }
    let (mime, encoded) = url.strip_prefix("data:")?.split_once(";base64,")?;
    if !mime.starts_with("image/") {
        return None;
    }
    let bytes = STANDARD.decode(encoded).ok()?;
    if bytes.len() > MAX_BYTES {
        return None;
    }
    decode(&bytes, mime, renderer)
}

pub(super) fn cache_covers(
    playlists: &[Playlist],
    previous: CoverCache,
    renderer: &SvgRenderer,
) -> CoverCache {
    let mut cache = HashMap::new();
    let mut retained = 0;
    for playlist in playlists
        .iter()
        .take(64)
        .filter(|p| p.cover_img_url.starts_with("data:"))
    {
        // Leave room for one maximum-size animation before decoding new data.
        // Remaining covers are loaded when they actually enter the viewport.
        let image = match previous.get(&playlist.id) {
            Some((url, image)) if url == &playlist.cover_img_url => image.clone(),
            _ if retained <= CACHE_BYTES - 64 * 1024 * 1024 => {
                decode_url(&playlist.cover_img_url, renderer)
            }
            _ => continue,
        };
        let bytes = image.as_ref().map_or(0, |image| image_bytes(image));
        if retained + bytes > CACHE_BYTES {
            continue;
        }
        retained += bytes;
        cache.insert(playlist.id.clone(), (playlist.cover_img_url.clone(), image));
    }
    cache
}

fn read_cover(
    file: &Path,
    renderer: &SvgRenderer,
) -> Result<(String, Arc<RenderImage>), CoverError> {
    let mime = match file
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" | "svgz" => "image/svg+xml",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "ico" => "image/x-icon",
        _ => return Err(CoverError::Type),
    };
    let input = std::fs::File::open(file).map_err(|_| CoverError::Process)?;
    if input.metadata().map_err(|_| CoverError::Process)?.len() > MAX_BYTES as u64 {
        return Err(CoverError::Size);
    }
    let mut bytes = Vec::new();
    input
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| CoverError::Process)?;
    if bytes.len() > MAX_BYTES {
        return Err(CoverError::Size);
    }
    let image = decode(&bytes, mime, renderer).ok_or(CoverError::Process)?;
    Ok((
        format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        image,
    ))
}

impl Mio {
    pub(super) fn trim_playlist_cover_cache(&mut self) -> bool {
        let limit = 64.max(self.local_playlists.visible_cover_ids.len());
        let mut changed = false;
        let byte_limit =
            CACHE_BYTES.max(self.local_playlists.visible_cover_ids.len() * 1024 * 1024);
        while self.local_playlists.covers.len() > limit
            || cache_bytes(&self.local_playlists.covers) > byte_limit
        {
            let Some(index) = self
                .local_playlists
                .cover_order
                .iter()
                .position(|old| !self.local_playlists.visible_cover_ids.contains(old))
            else {
                let Some(old) = self
                    .local_playlists
                    .cover_order
                    .iter()
                    .find(|old| {
                        self.local_playlists
                            .covers
                            .get(*old)
                            .and_then(|(_, image)| image.as_ref())
                            .is_some_and(|image| image.frame_count() > 1)
                    })
                    .cloned()
                else {
                    break;
                };
                let (_, image) = self.local_playlists.covers.get_mut(&old).unwrap();
                let animated = image.take().unwrap();
                *image = still_frame(&animated);
                self.local_playlists.retired_covers.push(animated);
                self.local_playlists.degraded_covers.insert(old);
                changed = true;
                continue;
            };
            let old = self.local_playlists.cover_order.remove(index).unwrap();
            if let Some((_, image)) = self.local_playlists.covers.remove(&old) {
                self.local_playlists.retired_covers.extend(image);
                changed = true;
            }
        }
        changed
    }

    fn cache_playlist_cover(
        &mut self,
        id: String,
        url: String,
        mut image: Option<Arc<RenderImage>>,
    ) {
        let limit = 64.max(self.local_playlists.visible_cover_ids.len());
        self.local_playlists.degraded_covers.remove(&id);
        let byte_limit =
            CACHE_BYTES.max(self.local_playlists.visible_cover_ids.len() * 1024 * 1024);
        while (self.local_playlists.covers.len() >= limit
            && !self.local_playlists.covers.contains_key(&id))
            || cache_bytes(&self.local_playlists.covers)
                - self
                    .local_playlists
                    .covers
                    .get(&id)
                    .and_then(|(_, image)| image.as_ref())
                    .map_or(0, |image| image_bytes(image))
                + image.as_ref().map_or(0, |image| image_bytes(image))
                > byte_limit
        {
            let Some(index) = self.local_playlists.cover_order.iter().position(|old| {
                old != &id && !self.local_playlists.visible_cover_ids.contains(old)
            }) else {
                if !self.local_playlists.visible_cover_ids.contains(&id) {
                    return;
                }
                // Preserve every visible cover, freezing animations when all
                // eviction candidates are themselves visible.
                if let Some(animated) = image.as_ref().filter(|image| image.frame_count() > 1) {
                    image = still_frame(animated);
                    self.local_playlists.degraded_covers.insert(id.clone());
                    continue;
                }
                if let Some(old) = self
                    .local_playlists
                    .cover_order
                    .iter()
                    .find(|old| {
                        *old != &id
                            && self
                                .local_playlists
                                .covers
                                .get(*old)
                                .and_then(|(_, image)| image.as_ref())
                                .is_some_and(|image| image.frame_count() > 1)
                    })
                    .cloned()
                {
                    let (_, image) = self.local_playlists.covers.get_mut(&old).unwrap();
                    let animated = image.take().unwrap();
                    *image = still_frame(&animated);
                    self.local_playlists.retired_covers.push(animated);
                    self.local_playlists.degraded_covers.insert(old);
                    continue;
                }
                return;
            };
            let old = self.local_playlists.cover_order.remove(index).unwrap();
            if let Some((_, image)) = self.local_playlists.covers.remove(&old) {
                self.local_playlists.retired_covers.extend(image);
            }
        }
        if let Some((_, old)) = self
            .local_playlists
            .covers
            .insert(id.clone(), (url, image.clone()))
            && let Some(old) = old
            && image.as_ref().is_none_or(|new| !Arc::ptr_eq(new, &old))
        {
            self.local_playlists.retired_covers.push(old);
        }
        self.local_playlists.cover_order.retain(|old| old != &id);
        self.local_playlists.cover_order.push_back(id);
    }

    fn visible_playlist_cover(
        &mut self,
        id: String,
        url: String,
        cx: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        self.local_playlists.visible_cover_ids.insert(id.clone());
        if let Some((cached, image)) = self.local_playlists.covers.get(&id)
            && cached == &url
        {
            self.local_playlists.cover_order.retain(|old| old != &id);
            self.local_playlists.cover_order.push_back(id);
            return image.clone();
        }
        if self.local_playlists.cover_busy {
            return None;
        }
        self.local_playlists.cover_busy = true;
        let revision = self.local_playlists.list_revision;
        let renderer = cx.svg_renderer();
        let task = cx.background_executor().spawn(async move {
            let image = decode_url(&url, &renderer);
            (id, url, image)
        });
        cx.spawn(async move |this, cx| {
            let (id, url, image) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.local_playlists.cover_busy = false;
                if this.local_playlists.list_revision == revision {
                    this.cache_playlist_cover(id, url, image);
                }
                cx.notify();
            });
        })
        .detach();
        None
    }

    pub(super) fn choose_playlist_cover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.navigation.current() != Page::Playlist || self.favorites.busy {
            return;
        }
        let Some(playlist) = self.local_playlists.selected.clone() else {
            return;
        };
        let Some(path) = self.store.as_ref().map(Storage::library_path) else {
            return;
        };
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.t("music.list.editCover").into()),
        });
        let renderer = cx.svg_renderer();
        self.favorites.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = match selection.await {
                Ok(Ok(Some(files))) => match files.into_iter().next() {
                    Some(file) => Some(
                        cx.background_executor()
                            .spawn(async move {
                                let (url, image) = read_cover(&file, &renderer)?;
                                let saved = library::save_playlist_cover(&path, &playlist.id, &url)
                                    .map_err(|_| CoverError::Save)?;
                                Ok((saved, image))
                            })
                            .await,
                    ),
                    None => None,
                },
                Ok(Ok(None)) => None,
                _ => Some(Err(CoverError::Process)),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.favorites.busy = false;
                match result {
                    Some(Ok((saved, image))) => {
                        this.cache_playlist_cover(
                            saved.id.clone(),
                            saved.cover_img_url.clone(),
                            Some(image.clone()),
                        );
                        if this.navigation.current() == Page::Playlist
                            && this
                                .local_playlists
                                .selected
                                .as_ref()
                                .is_some_and(|p| p.id == saved.id)
                        {
                            this.local_playlists
                                .retired_covers
                                .extend(this.local_playlists.selected_cover.replace(image.clone()));
                            this.local_playlists.selected = Some(saved.clone());
                            // Invalidate an earlier detail read so it cannot restore the old cover.
                            this.load_local_playlist(cx);
                        }
                        this.load_playlists(cx);
                        window.push_notification(
                            Notification::success(this.t("music.list.coverUpdated")),
                            cx,
                        );
                    }
                    Some(Err(error)) => window.push_notification(
                        Notification::error(this.t(match error {
                            CoverError::Type => "music.list.selectImage",
                            CoverError::Size => "music.list.imageTooLarge",
                            CoverError::Process => "music.list.imageProcessFailed",
                            CoverError::Save => "music.list.coverUpdateFailed",
                        })),
                        cx,
                    ),
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }
}
