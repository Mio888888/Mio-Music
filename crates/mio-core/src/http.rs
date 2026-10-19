//! Process-wide blocking HTTP client. Building a client per request costs a TLS
//! configuration, a runtime thread and every pooled connection; callers set their
//! own timeout on the request instead.
use std::{sync::OnceLock, time::Duration};

pub fn blocking() -> Option<&'static reqwest::blocking::Client> {
    static CLIENT: OnceLock<Option<reqwest::blocking::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .ok()
        })
        .as_ref()
}
