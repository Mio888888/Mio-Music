//! Online URL resolution and progressive playback share the existing PCM/device pipeline.
use super::*;
use crate::online::Online;
use std::sync::atomic::AtomicBool;
use tokio::io::AsyncWriteExt;

pub(super) fn retryable(error: &str) -> bool {
    if let Some(status) = error
        .strip_prefix("在线音频返回 HTTP ")
        .and_then(|s| s.split_whitespace().next()?.parse::<u16>().ok())
    {
        return status == 408 || (500..600).contains(&status);
    }
    [
        "在线音频连接失败或超时",
        "在线播放超时",
        "在线音频断网重试已用尽",
        "插件网络请求失败或超时",
        "插件执行超时",
    ]
    .iter()
    .any(|message| error.contains(message))
}

fn http_error(status: u16) -> String {
    let mut message = format!("在线音频返回 HTTP {status}");
    if status == 403 {
        message.push_str("\n当前音质地址被音源拒绝，请切换音质或音源后重试");
    }
    message
}

#[derive(Clone)]
pub(super) struct OnlineLoad {
    pub generation: u64,
    pub source: Arc<Online>,
    pub id: String,
    pub duration: u64,
}
struct Cancel(Arc<AtomicBool>);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
/// One client per loader: its pool and TLS sessions belong to the loader's runtime and are
/// reused by every song that loader fetches.
pub(super) fn audio_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .ok()
}
pub(super) async fn transfer(
    load: OnlineLoad,
    tx: mpsc::Sender<Command>,
    slots: Arc<tokio::sync::Semaphore>,
    staged: bool,
    client: Option<&reqwest::Client>,
) -> Result<Arc<Track>, String> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel = Cancel(cancelled.clone());
    let source = load.source.clone();
    let id = load.id.clone();
    // A song played before is served from the cache: no plugin call and no download.
    let cache_key = source.cache_key(&id);
    if let Some(path) = cache_key.as_deref().and_then(crate::audio_cache::lookup) {
        if let Some(track) = serve_cached(&load, &tx, &path, staged).await {
            return Ok(track);
        }
    }
    let permit = slots.acquire_owned().await.map_err(|_| "插件解析不可用")?;
    if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
        eprintln!(
            "[playback] online_fetch generation={} source={} staged={staged}",
            load.generation,
            load.id.split(':').nth(1).unwrap_or("unknown")
        );
    }
    let url = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        source.resolve(&id, cancelled)
    })
    .await
    .map_err(|_| "插件解析任务失败")??;
    let url = mio_plugin::runtime_url(&url)?;
    if load.id.starts_with("mio-online:tx:")
        && std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some()
    {
        let vkey_bytes = url
            .query_pairs()
            .find(|(key, _)| key == "vkey")
            .map_or(0, |(_, value)| value.len());
        eprintln!(
            "[playback] qq_url_auth generation={} staged={staged} vkey_bytes={vkey_bytes}",
            load.generation
        );
    }
    // Match the original player: music CDNs require these audio request headers.
    // Derive only the host, never include a signed URL's query or credentials.
    let host = url.host_str().unwrap_or("localhost");
    let parts: Vec<_> = host.split('.').collect();
    let referer = if parts.len() >= 2 {
        format!(
            "http://{}.{}",
            parts[parts.len() - 2],
            parts[parts.len() - 1]
        )
    } else {
        format!("http://{host}/")
    };
    let client = client.ok_or("无法创建在线音频客户端")?;
    let mut checkpoint = cache_key
        .as_deref()
        .and_then(crate::audio_cache::checkpoint);
    let offset = checkpoint
        .as_ref()
        .and_then(|c| c.file().metadata().ok())
        .map_or(0, |m| m.len());
    let mut request = client
        .get(url.clone())
        .header("Accept-Encoding", "identity");
    if offset > 0 {
        request = request.header("Range", format!("bytes={offset}-")).header(
            "If-Range",
            checkpoint
                .as_ref()
                .and_then(|c| c.validator.clone())
                .unwrap(),
        );
    }
    let mut response = request
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        )
        .header("Referer", &referer)
        .send()
        .await
        .map_err(|_| "在线音频连接失败或超时")?;
    if !response.status().is_success() {
        let status = response.status().as_u16();
        if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
            eprintln!(
                "[playback] online_http_rejected generation={generation} staged={staged} status={status} scheme={} host={}",
                response.url().scheme(),
                response.url().host_str().unwrap_or("unknown"),
                generation = load.generation
            );
        }
        if status == 416 {
            if let Some(checkpoint) = &mut checkpoint {
                let _ = checkpoint.reset(None);
            }
        }
        return Err(http_error(status));
    }
    const LIMIT: u64 = 256 * 1024 * 1024;
    if response.content_length().is_some_and(|n| {
        n.saturating_add(
            if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                offset
            } else {
                0
            },
        ) > LIMIT
    }) {
        return Err("在线音频过大".into());
    }
    let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT && offset > 0;
    let validator = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|h| h.to_str().ok())
        .filter(|v| !v.starts_with("W/") && !v.is_empty())
        .or_else(|| {
            response
                .headers()
                .get(reqwest::header::LAST_MODIFIED)
                .and_then(|h| h.to_str().ok())
                .filter(|v| !v.is_empty())
        })
        .map(str::to_owned);
    if resumed && checkpoint.as_ref().and_then(|c| c.validator.as_ref()) != validator.as_ref() {
        if let Some(checkpoint) = &mut checkpoint {
            let _ = checkpoint.reset(None);
        }
        return Err("音频续传内容已变化，请重试播放".into());
    }
    let expected_total = if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
        let range = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|h| h.to_str().ok())
            .and_then(|value| value.strip_prefix("bytes "))
            .and_then(|value| value.split_once('/'))
            .and_then(|(range, total)| {
                let (start, end) = range.split_once('-')?;
                Some((
                    start.parse::<u64>().ok()?,
                    end.parse::<u64>().ok()?,
                    total.parse::<u64>().ok()?,
                ))
            });
        let valid = range.filter(|(start, end, total)| {
            *start == offset
                && *end >= *start
                && *end < *total
                && *total <= LIMIT
                && *end + 1 == *total
                && response
                    .content_length()
                    .is_none_or(|bytes| bytes == *total - *start)
        });
        let Some((_, _, total)) = valid else {
            if let Some(checkpoint) = &mut checkpoint {
                let _ = checkpoint.reset(None);
            }
            return Err("音频续传范围无效，请重试播放".into());
        };
        Some(total)
    } else {
        response.content_length()
    };
    if !resumed {
        if let Some(checkpoint) = &mut checkpoint {
            if checkpoint.reset(validator.clone()).is_err() {
                checkpoint.validator = None;
            }
        }
    }
    if offset > 0 && response.status() == reqwest::StatusCode::OK {
        // The server ignored Range or changed the representation; start a clean file.
        if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
            eprintln!(
                "[playback] resume_reset generation={} offset={offset}",
                load.generation
            );
        }
    }
    let track = Track::new().map_err(|_| "无法创建音频临时文件")?;
    track.set_duration_hint(load.duration);
    let guard = TransferGuard(track.clone());
    if resumed {
        tokio::fs::copy(&checkpoint.as_ref().unwrap().path, track.path())
            .await
            .map_err(|_| "无法读取音频断点缓存")?;
        track.progress(offset);
    }
    let mut writer = tokio::fs::OpenOptions::new()
        .append(true)
        .open(track.path())
        .await
        .map_err(|_| "无法打开音频临时文件")?;
    let mut checkpoint_writer = checkpoint
        .as_ref()
        .filter(|c| c.validator.is_some())
        .and_then(|c| c.file().try_clone().ok())
        .map(tokio::fs::File::from_std);
    if let Some(writer) = &mut checkpoint_writer {
        use tokio::io::AsyncSeekExt;
        writer
            .seek(std::io::SeekFrom::End(0))
            .await
            .map_err(|_| "无法定位音频断点缓存")?;
    }
    if resumed && std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
        eprintln!(
            "[playback] resume_hit generation={} bytes={offset}",
            load.generation
        );
    }
    if !staged {
        tx.send(Command::Prepared(load.generation, Ok(track.clone())))
            .map_err(|_| "播放已关闭")?;
    }
    let generation = load.generation;
    let mut size = if resumed { offset } else { 0 };
    let audio = async {
        let mut retries = 0;
        loop {
            let chunk = match response.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) if expected_total.is_none_or(|total| size == total) => break,
                Ok(None) | Err(_) => {
                    let (Some(validator), Some(total)) = (&validator, expected_total) else {
                        return Err("在线音频传输中断，服务器不支持安全续传，请重新播放".to_owned());
                    };
                    response = resume_transfer(
                        client,
                        &url,
                        &referer,
                        validator,
                        size,
                        total,
                        generation,
                        staged,
                        &mut retries,
                    )
                    .await?;
                    continue;
                }
            };
            if size == 0 && std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
                eprintln!("[playback] online_body_started generation={generation} staged={staged}");
            }
            size += chunk.len() as u64;
            if size > LIMIT {
                return Err("在线音频超过大小限制".to_owned());
            }
            if expected_total.is_some_and(|total| size > total) {
                return Err("在线音频超过声明的续传范围".to_owned());
            }
            writer
                .write_all(&chunk)
                .await
                .map_err(|_| "无法保存在线音频")?;
            writer.flush().await.map_err(|_| "无法刷新在线音频")?;
            track.progress(size);
            if let Some(writer) = &mut checkpoint_writer {
                if writer.write_all(&chunk).await.is_err() || writer.flush().await.is_err() {
                    checkpoint_writer = None;
                    if let Some(checkpoint) = &mut checkpoint {
                        let _ = checkpoint.reset(None);
                    }
                }
            }
        }
        if size == 0 {
            return Err("在线音频为空".into());
        }
        if expected_total.is_some_and(|total| size != total) {
            return Err("在线音频尚未下载完整，请重试续传".into());
        }
        Ok(())
    };
    let result = audio.await;
    if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
        eprintln!(
            "[playback] online_body_finished generation={generation} staged={staged} bytes={size} success={}",
            result.is_ok()
        );
    }
    track.finish(result.is_err());
    if result.is_ok() {
        if let Some(key) = cache_key {
            // Only a complete download is kept, copied while the temp file is still ours.
            let path = track.path().to_path_buf();
            let saved =
                tokio::task::spawn_blocking(move || crate::audio_cache::insert(&key, &path)).await;
            if matches!(saved, Ok(Ok(()))) {
                drop(checkpoint_writer);
                if let Some(checkpoint) = checkpoint.take() {
                    checkpoint.remove();
                }
            }
        }
    }
    drop(guard);
    result.map(|()| track)
}

// Keep the same growing file and PCM lane during a temporary disconnect. Never append a
// changed representation or a response that ignored Range. The loader's overall timeout and
// watch cancellation also cover every backoff and request here.
async fn resume_transfer(
    client: &reqwest::Client,
    url: &reqwest::Url,
    referer: &str,
    validator: &str,
    offset: u64,
    total: u64,
    generation: u64,
    staged: bool,
    retries: &mut u32,
) -> Result<reqwest::Response, String> {
    while *retries < 6 {
        let delay = (2_u64 << *retries).min(30);
        *retries += 1;
        if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
            eprintln!(
                "[playback] transfer_retry generation={generation} staged={staged} attempt={retries} bytes={offset} delay={delay}"
            );
        }
        tokio::time::sleep(Duration::from_secs(delay)).await;
        let response = client
            .get(url.clone())
            .header("Accept-Encoding", "identity")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
            )
            .header("Referer", referer)
            .header("Range", format!("bytes={offset}-"))
            .header("If-Range", validator)
            .send()
            .await;
        let Ok(response) = response else {
            continue;
        };
        if response.status().is_server_error()
            || response.status() == reqwest::StatusCode::REQUEST_TIMEOUT
        {
            continue;
        }
        if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            if !response.status().is_success() {
                return Err(http_error(response.status().as_u16()));
            }
            return Err("在线音频续传被拒绝或内容已变化，请重新播放".to_owned());
        }
        if ![reqwest::header::ETAG, reqwest::header::LAST_MODIFIED]
            .iter()
            .any(|header| {
                response.headers().get(header).and_then(|v| v.to_str().ok()) == Some(validator)
            })
        {
            return Err("在线音频续传内容已变化，请重新播放".to_owned());
        }
        let range = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("bytes "))
            .and_then(|v| v.split_once('/'))
            .and_then(|(range, total)| {
                let (start, end) = range.split_once('-')?;
                Some((
                    start.parse::<u64>().ok()?,
                    end.parse::<u64>().ok()?,
                    total.parse::<u64>().ok()?,
                ))
            });
        if !range.is_some_and(|(start, end, length)| {
            start == offset
                && end >= start
                && end < length
                && length == total
                && end + 1 == total
                && response
                    .content_length()
                    .is_none_or(|bytes| bytes == end - start + 1)
        }) {
            return Err("在线音频续传范围无效，请重新播放".to_owned());
        }
        if std::env::var_os("MIO_GPUI_PLAYBACK_TRACE").is_some() {
            eprintln!(
                "[playback] transfer_resumed generation={generation} staged={staged} bytes={offset}"
            );
        }
        return Ok(response);
    }
    Err("在线音频断网重试已用尽，请重新播放".to_owned())
}

/// The cover of a song, at most 5 MiB, within 10 seconds.
/// Plays a cached file through the same temp-file path as a download, finished at once.
async fn serve_cached(
    load: &OnlineLoad,
    tx: &mpsc::Sender<Command>,
    cached: &std::path::Path,
    staged: bool,
) -> Option<Arc<Track>> {
    let Ok(track) = Track::new() else {
        return None;
    };
    track.set_duration_hint(load.duration);
    let guard = TransferGuard(track.clone());
    let Ok(size) = tokio::fs::copy(cached, track.path()).await else {
        return None;
    };
    if size == 0 {
        return None;
    }
    track.progress(size);
    track.finish(false);
    if !staged
        && tx
            .send(Command::Prepared(load.generation, Ok(track.clone())))
            .is_err()
    {
        return None;
    }
    drop(guard);
    Some(track)
}
pub(super) async fn loader(
    mut requests: watch::Receiver<Option<OnlineLoad>>,
    tx: mpsc::Sender<Command>,
) {
    let slots = Arc::new(tokio::sync::Semaphore::new(1));
    let client = audio_client();
    loop {
        let load = requests.borrow_and_update().clone();
        if let Some(load) = load {
            let generation = load.generation;
            tokio::select! {biased;
                changed=requests.changed()=>{if changed.is_err(){break;}}
                result=tokio::time::timeout(Duration::from_secs(150),transfer(load,tx.clone(),slots.clone(),false,client.as_ref()))=>{
                    if let Err(error)=result.unwrap_or_else(|_|Err("在线播放超时".into())){let _=tx.send(Command::TransferFailed(generation,error));}
                    if requests.changed().await.is_err(){break;}
                }
            }
        } else if requests.changed().await.is_err() {
            break;
        }
    }
}
