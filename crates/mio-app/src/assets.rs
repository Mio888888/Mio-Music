use gpui_kit::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Files;
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(file) = Files::get(path) {
            return Ok(Some(file.data));
        }
        gpui_kit::assets::Assets.load(path)
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(
            Files::iter()
                .filter(|name| name.starts_with(path))
                .map(Into::into),
        );
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
pub fn fonts() -> Vec<Cow<'static, [u8]>> {
    ["fonts/iconfont.ttf", "fonts/lyricfont.ttf"]
        .into_iter()
        .map(|name| Files::get(name).expect("bundled font").data)
        .collect()
}
/// A bundled file's bytes, for code that decodes images itself (the tray icon).
pub fn bytes(path: &str) -> Option<Cow<'static, [u8]>> {
    Files::get(path).map(|file| file.data)
}
