use gpui_kit::*;
use mio_tokens::Mode;

/// Per-view colors: CSS overrides belong to this music window, not process globals.
#[derive(Clone, Copy)]
pub struct Colors {
    pub mode: Mode,
    pub song: Option<SongColors>,
}
impl From<Mode> for Colors {
    fn from(mode: Mode) -> Self {
        Self { mode, song: None }
    }
}

/// Original useDynamicSongTheme.ts, with integer RGB mixing before alpha.
#[derive(Clone, Copy)]
pub struct SongColors {
    pub base: [u8; 3],
    pub use_black_text: bool,
}
impl SongColors {
    pub fn color(self, key: &str) -> Option<Hsla> {
        mio_core::song_theme::SongColors {
            base: self.base,
            use_black_text: self.use_black_text,
        }
        .color(key)
        .map(|c| {
            Rgba {
                r: c.r,
                g: c.g,
                b: c.b,
                a: c.a,
            }
            .into()
        })
    }
    pub fn mobile_background(self) -> Background {
        linear_gradient(
            180.,
            linear_color_stop(self.color("--td-bg-color-secondarycontainer").unwrap(), 0.),
            linear_color_stop(self.color("--td-bg-color-page").unwrap(), 1.),
        )
        .color_space(ColorSpace::Srgb)
    }
}
