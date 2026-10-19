//! P0 native dependency qualification. Build this library for each mobile target
//! before extracting desktop modules. No UI, application state, or device
//! acceptance is implied by a successful build.
pub use cpal;
pub use image;
pub use lofty;
pub use mio_music_sdk;
pub use mio_plugin;
pub use reqwest;
pub use rodio;
pub use rusqlite;
pub use wasmi;
