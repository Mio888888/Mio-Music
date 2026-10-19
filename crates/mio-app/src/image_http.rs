use gpui_kit::http_client::{AsyncBody, HttpClient, RedirectPolicy, Request, Response, Url, http};
use std::{future::Future, io::Read, pin::Pin, sync::OnceLock, time::Duration};

/// One pooled client per redirect limit (0–5): redirects are a client setting, and a
/// client per image would repeat the TLS setup and spawn a runtime thread every time.
fn client(redirects: usize) -> Option<&'static reqwest::blocking::Client> {
    static CLIENTS: [OnceLock<Option<reqwest::blocking::Client>>; 6] =
        [const { OnceLock::new() }; 6];
    let redirects = redirects.min(5);
    CLIENTS[redirects]
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(12))
                .redirect(if redirects == 0 {
                    reqwest::redirect::Policy::none()
                } else {
                    reqwest::redirect::Policy::limited(redirects)
                })
                .user_agent("Mio Music GPUI")
                .build()
                .ok()
        })
        .as_ref()
}

/// GPUI fetches image assets on its background executor. Keep this transport
/// separate from authenticated Subsonic requests and bound the complete body.
pub struct ImageHttpClient;

impl HttpClient for ImageHttpClient {
    fn user_agent(&self) -> Option<&http::HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Response<AsyncBody>>> + Send>> {
        Box::pin(async move {
            anyhow::ensure!(
                request.method() == http::Method::GET,
                "Image transport only supports GET"
            );
            let cache_key = request.uri().to_string();
            if let Some(bytes) = crate::image_cache::read(&cache_key) {
                return Ok(Response::builder().status(200).body(bytes.into())?);
            }
            let redirects = match request.extensions().get::<RedirectPolicy>() {
                Some(RedirectPolicy::FollowAll) => 5,
                Some(RedirectPolicy::FollowLimit(limit)) => (*limit).min(5) as usize,
                _ => 0,
            };
            let client = client(redirects)
                .ok_or_else(|| anyhow::anyhow!("Unable to create image HTTP client"))?;
            let response = client
                .get(request.uri().to_string())
                .timeout(Duration::from_secs(12))
                .headers(request.headers().clone())
                .send()
                .map_err(|_| anyhow::anyhow!("Unable to fetch image resource"))?;
            let success = response.status().is_success();
            let mut output = Response::builder().status(response.status());
            *output.headers_mut().expect("valid response status") = response.headers().clone();
            let mut bytes = Vec::new();
            response
                .take(128 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| anyhow::anyhow!("Unable to read image resource"))?;
            if success {
                crate::image_cache::write(&cache_key, &bytes);
            }
            Ok(output.body(bytes.into())?)
        })
    }
}
