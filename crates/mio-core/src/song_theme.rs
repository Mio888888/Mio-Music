//! Dynamic song theme tokens shared by desktop and mobile.
use crate::cover_palette::Rgba;

/// Original useDynamicSongTheme.ts, with integer RGB mixing before alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SongColors {
    pub base: [u8; 3],
    pub use_black_text: bool,
}
impl SongColors {
    fn mix(color: [u8; 3], target: [u8; 3], amount: f64) -> [u8; 3] {
        std::array::from_fn(|i| {
            (color[i] as f64 + (target[i] as f64 - color[i] as f64) * amount)
                .round()
                .clamp(0., 255.) as u8
        })
    }
    fn rgba(color: [u8; 3], alpha: f32) -> Rgba {
        Rgba {
            r: color[0] as f32 / 255.,
            g: color[1] as f32 / 255.,
            b: color[2] as f32 / 255.,
            a: alpha,
        }
    }
    /// `top` at `alpha` over the opaque `bottom`.
    fn over(top: [u8; 3], alpha: f64, bottom: [u8; 3]) -> [u8; 3] {
        Self::mix(bottom, top, alpha)
    }
    /// WCAG relative luminance.
    fn luminance(color: [u8; 3]) -> f64 {
        let channel = |value: u8| {
            let c = value as f64 / 255.;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color[0]) + 0.7152 * channel(color[1]) + 0.0722 * channel(color[2])
    }
    /// WCAG contrast ratio.
    fn ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
        let (x, y) = (Self::luminance(a), Self::luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }
    /// The cover-derived surfaces brand-colored text and icons are drawn over, as opaque colors.
    fn surfaces(self) -> [[u8; 3]; 4] {
        let light = self.use_black_text;
        let (white, black) = ([255; 3], [0; 3]);
        let page = Self::mix(
            self.base,
            if light { white } else { black },
            if light { 0.9 } else { 0.76 },
        );
        let container = Self::over(
            Self::mix(
                self.base,
                if light { white } else { black },
                if light { 0.78 } else { 0.58 },
            ),
            if light { 0.98 } else { 0.94 },
            page,
        );
        let secondary = Self::over(
            Self::mix(
                self.base,
                if light { white } else { black },
                if light { 0.84 } else { 0.66 },
            ),
            if light { 0.96 } else { 0.92 },
            page,
        );
        let selected = Self::over(
            Self::mix(self.raw_brand(), white, 0.72),
            if light { 0.68 } else { 0.22 },
            container,
        );
        [page, container, secondary, selected]
    }
    /// The cover color, slightly toward the text color: the accent before readability.
    fn raw_brand(self) -> [u8; 3] {
        let (text, amount) = if self.use_black_text {
            ([0; 3], 0.12)
        } else {
            ([255; 3], 0.08)
        };
        Self::mix(self.base, text, amount)
    }
    /// A cover-derived accent can land close to the cover-derived backgrounds (red on dark
    /// red). Nudge it toward black on light pages, white on dark ones, until it reads at
    /// 4.5:1 on every surface it is drawn over; accents that already read are unchanged.
    fn readable(self, color: [u8; 3]) -> [u8; 3] {
        let surfaces = self.surfaces();
        let target = if Self::luminance(surfaces[0]) > 0.5 {
            [0; 3]
        } else {
            [255; 3]
        };
        let mut result = color;
        for _ in 0..24 {
            if surfaces
                .iter()
                .all(|surface| Self::ratio(result, *surface) >= 4.5)
            {
                break;
            }
            result = Self::mix(result, target, 0.05);
        }
        result
    }
    /// Text and icons on a brand fill: whichever of white and near-black reads better.
    fn on_brand(self) -> [u8; 3] {
        let fill = self.readable(self.raw_brand());
        let (light, dark) = ([255; 3], [26; 3]);
        if Self::ratio(light, fill) >= Self::ratio(dark, fill) {
            light
        } else {
            dark
        }
    }
    pub fn color(self, key: &str) -> Option<Rgba> {
        let field = match key {
            "--td-brand-color-1" => "brand1",
            "--td-brand-color-2" | "--td-brand-color-focus" | "--player-btn-bg-hover-idle" => {
                "brand2"
            }
            "--td-brand-color-3"
            | "--td-brand-color-disabled"
            | "--settings-source-card-hover-border" => "brand3",
            "--td-brand-color-4" | "--td-brand-color-hover" => "brand4",
            "--td-brand-color-5"
            | "--td-brand-color"
            | "--td-text-color-brand"
            | "--td-text-color-link"
            | "--settings-source-card-active-border"
            | "--plugins-card-selected-border" => "brand5",
            "--td-brand-color-6" | "--td-brand-color-active" => "brand6",
            "--td-brand-color-7" => "brand7",
            "--td-brand-color-8" => "brand8",
            "--td-brand-color-9" => "brand9",
            "--td-brand-color-10" => "brand10",
            "--td-text-color-anti" => "onBrand",
            "--td-error-color" => "error",
            "--td-brand-color-light"
            | "--song-list-item-current"
            | "--song-list-item-playing"
            | "--song-list-btn-bg-hover"
            | "--settings-source-card-active-bg"
            | "--plugins-card-selected-bg"
            | "--player-btn-bg-idle" => "brandLight",
            "--td-text-color-primary"
            | "--theme-text-primary"
            | "--settings-text-primary"
            | "--player-text-hover-idle" => "textPrimary",
            "--td-text-color-secondary"
            | "--theme-text-secondary"
            | "--settings-text-secondary"
            | "--player-text-idle" => "textSecondary",
            "--td-text-color-placeholder"
            | "--theme-text-tertiary"
            | "--theme-text-muted"
            | "--song-list-header-text"
            | "--song-list-track-number" => "textMuted",
            "--td-bg-color-page"
            | "--theme-bg-tertiary"
            | "--settings-main-bg"
            | "--settings-content-bg" => "pageBg",
            "--td-bg-color-container"
            | "--theme-bg-primary"
            | "--theme-card-bg"
            | "--find-card-info-bg"
            | "--settings-header-bg"
            | "--settings-sidebar-bg"
            | "--settings-group-bg"
            | "--settings-source-card-bg" => "elevatedBg",
            "--td-bg-color-container-hover"
            | "--td-bg-color-secondarycontainer-hover"
            | "--td-bg-color-component-hover"
            | "--theme-hover-bg"
            | "--song-list-item-hover"
            | "--recent-song-item-hover" => "componentHoverBg",
            "--td-bg-color-container-active"
            | "--td-bg-color-secondarycontainer-active"
            | "--td-bg-color-component-active" => "componentActiveBg",
            "--td-bg-color-secondarycontainer"
            | "--theme-bg-secondary"
            | "--theme-header-bg"
            | "--song-list-header-bg"
            | "--settings-preview-bg"
            | "--settings-feature-bg"
            | "--settings-api-tips-bg"
            | "--settings-quality-container-bg"
            | "--settings-status-item-bg"
            | "--settings-tech-item-bg"
            | "--settings-developer-item-bg"
            | "--settings-tag-option-bg"
            | "--settings-tag-status-bg" => "surfaceBg",
            "--td-bg-color-component"
            | "--td-bg-color-specialcomponent"
            | "--theme-badge-bg"
            | "--find-song-count-bg"
            | "--settings-source-icon-bg" => "componentBg",
            "--td-component-stroke"
            | "--td-border-level-1-color"
            | "--theme-border-light"
            | "--song-list-header-border"
            | "--song-list-item-border"
            | "--recent-song-item-border" => "borderLight",
            "--td-component-border"
            | "--td-border-level-2-color"
            | "--theme-border-medium"
            | "--theme-border-strong"
            | "--theme-border"
            | "--settings-sidebar-border"
            | "--settings-group-border"
            | "--settings-plugin-prompt-border"
            | "--settings-preview-border"
            | "--settings-feature-border"
            | "--settings-api-tips-border"
            | "--settings-source-card-border"
            | "--settings-quality-container-border"
            | "--settings-status-item-border"
            | "--settings-tech-item-border"
            | "--settings-developer-item-border"
            | "--settings-tag-option-border"
            | "--settings-tag-status-border" => "borderMedium",
            "--settings-nav-hover-bg" => "settingsNavHoverBg",
            "--settings-nav-active-bg" => "settingsNavActiveBg",
            "--settings-nav-active-border" => "settingsNavActiveBorder",
            "--settings-group-shadow" => "settingsGroupShadow",
            "--settings-nav-icon-color" => "settingsNavIconColor",
            "--settings-nav-icon-active" => "settingsNavIconActive",
            "--settings-nav-label-color" => "settingsNavLabelColor",
            "--settings-nav-label-active" => "settingsNavLabelActive",
            "--settings-nav-desc-color" => "settingsNavDescColor",
            "--settings-eq-visualizer-bg" => "settingsEqVisualizerBg",
            "--settings-eq-visualizer-trail" => "settingsEqVisualizerTrail",
            "--settings-eq-visualizer-bar-start" => "settingsEqVisualizerBarStart",
            "--settings-eq-visualizer-bar-end" => "settingsEqVisualizerBarEnd",
            "--player-bg-default" => "playerBg",
            "--player-bg-idle" => "playerIdleBg",
            "--mobile-glass-bg" => "mobileGlassBg",
            "--mobile-glass-bg-strong" => "mobileGlassBgStrong",
            "--mobile-glass-border" => "mobileGlassBorder",
            _ => return None,
        };
        let light = self.use_black_text;
        let base = self.base;
        let white = [255; 3];
        let black = [0; 3];
        let text = if light { black } else { white };
        let brand = self.raw_brand();
        let mix = |target, amount| Self::mix(base, target, amount);
        let brand_mix = |target, amount| Self::mix(brand, target, amount);
        let rgba = Self::rgba;
        Some(match field {
            "brand1" => rgba(brand_mix(white, 0.86), if light { 1. } else { 0.2 }),
            "brand2" => rgba(brand_mix(white, 0.72), 1.),
            "brand3" => rgba(brand_mix(white, 0.5), 1.),
            // Accents drawn as text, icons and fills stay readable on the cover surfaces.
            "brand4" => rgba(self.readable(brand_mix(white, 0.28)), 1.),
            "brand5" | "settingsEqVisualizerBarStart" => rgba(self.readable(brand), 1.),
            "brand6" => rgba(self.readable(brand_mix(black, 0.12)), 1.),
            "brand7" => rgba(self.readable(brand_mix(black, 0.24)), 1.),
            "onBrand" => rgba(self.on_brand(), 1.),
            // Error text and outlines (TDesign red) on a red cover would fade; solid danger
            // fills use the fixed `--td-error-color-6` instead.
            "error" => rgba(self.readable([227, 77, 89]), 1.),
            "brand8" => rgba(brand_mix(black, 0.38), 1.),
            "brand9" => rgba(brand_mix(black, 0.52), 1.),
            "brand10" => rgba(brand_mix(black, 0.66), 1.),
            "brandLight" => rgba(brand_mix(white, 0.72), if light { 0.68 } else { 0.22 }),
            "pageBg" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.9 } else { 0.76 },
                ),
                1.,
            ),
            "surfaceBg" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.84 } else { 0.66 },
                ),
                if light { 0.96 } else { 0.92 },
            ),
            "elevatedBg" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.78 } else { 0.58 },
                ),
                if light { 0.98 } else { 0.94 },
            ),
            "componentBg" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.72 } else { 0.5 },
                ),
                if light { 0.78 } else { 0.64 },
            ),
            "componentHoverBg" => rgba(
                mix(white, if light { 0.64 } else { 0.02 }),
                if light { 0.9 } else { 0.72 },
            ),
            "componentActiveBg" => rgba(
                mix(white, if light { 0.56 } else { 0.12 }),
                if light { 0.96 } else { 0.8 },
            ),
            "textPrimary" => rgba(text, if light { 0.9 } else { 0.92 }),
            "textSecondary" => rgba(text, if light { 0.62 } else { 0.68 }),
            "textMuted" => rgba(text, if light { 0.46 } else { 0.5 }),
            "borderLight" => rgba(text, if light { 0.1 } else { 0.12 }),
            "borderMedium" => rgba(text, if light { 0.16 } else { 0.2 }),
            "playerBg" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.78 } else { 0.35 },
                ),
                0.88,
            ),
            "playerIdleBg" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.86 } else { 0.45 },
                ),
                0.92,
            ),
            "mobileGlassBg" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.78 } else { 0.36 },
                ),
                0.78,
            ),
            "mobileGlassBgStrong" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.88 } else { 0.46 },
                ),
                0.9,
            ),
            "mobileGlassBorder" => rgba(text, if light { 0.1 } else { 0.18 }),
            "settingsNavHoverBg" => rgba(
                mix(white, if light { 0.8 } else { 0.04 }),
                if light { 0.86 } else { 0.7 },
            ),
            "settingsNavActiveBg" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.7 } else { 0.34 },
                ),
                if light { 0.92 } else { 0.82 },
            ),
            "settingsNavActiveBorder" => rgba(
                brand_mix(text, if light { 0.08 } else { 0.16 }),
                if light { 0.72 } else { 0.9 },
            ),
            "settingsNavIconColor" => rgba(text, if light { 0.5 } else { 0.58 }),
            "settingsNavIconActive" => rgba(text, if light { 0.86 } else { 0.94 }),
            "settingsNavLabelColor" => rgba(text, if light { 0.72 } else { 0.82 }),
            "settingsNavLabelActive" => rgba(text, if light { 0.92 } else { 0.96 }),
            "settingsNavDescColor" => rgba(text, if light { 0.48 } else { 0.58 }),
            "settingsGroupShadow" => rgba(
                if light { [15, 23, 42] } else { black },
                if light { 0.08 } else { 0.28 },
            ),
            "settingsEqVisualizerBg" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.9 } else { 0.72 },
                ),
                1.,
            ),
            "settingsEqVisualizerTrail" => rgba(
                mix(
                    if light { white } else { black },
                    if light { 0.72 } else { 0.42 },
                ),
                0.28,
            ),
            "settingsEqVisualizerBarEnd" => {
                rgba(brand_mix(text, if light { 0.22 } else { 0.28 }), 1.)
            }
            _ => return None,
        })
    }
}

/// Supported dynamic overrides; other tokens keep the selected base theme.
pub const KEYS: &[&str] = &[
    "--td-brand-color-1",
    "--td-brand-color-2",
    "--td-brand-color-focus",
    "--player-btn-bg-hover-idle",
    "--td-brand-color-3",
    "--td-brand-color-disabled",
    "--settings-source-card-hover-border",
    "--td-brand-color-4",
    "--td-brand-color-hover",
    "--td-brand-color-5",
    "--td-brand-color",
    "--td-text-color-brand",
    "--td-text-color-link",
    "--settings-source-card-active-border",
    "--plugins-card-selected-border",
    "--td-brand-color-6",
    "--td-brand-color-active",
    "--td-brand-color-7",
    "--td-brand-color-8",
    "--td-brand-color-9",
    "--td-brand-color-10",
    "--td-text-color-anti",
    "--td-error-color",
    "--td-brand-color-light",
    "--song-list-item-current",
    "--song-list-item-playing",
    "--song-list-btn-bg-hover",
    "--settings-source-card-active-bg",
    "--plugins-card-selected-bg",
    "--player-btn-bg-idle",
    "--td-text-color-primary",
    "--theme-text-primary",
    "--settings-text-primary",
    "--player-text-hover-idle",
    "--td-text-color-secondary",
    "--theme-text-secondary",
    "--settings-text-secondary",
    "--player-text-idle",
    "--td-text-color-placeholder",
    "--theme-text-tertiary",
    "--theme-text-muted",
    "--song-list-header-text",
    "--song-list-track-number",
    "--td-bg-color-page",
    "--theme-bg-tertiary",
    "--settings-main-bg",
    "--settings-content-bg",
    "--td-bg-color-container",
    "--theme-bg-primary",
    "--theme-card-bg",
    "--find-card-info-bg",
    "--settings-header-bg",
    "--settings-sidebar-bg",
    "--settings-group-bg",
    "--settings-source-card-bg",
    "--td-bg-color-container-hover",
    "--td-bg-color-secondarycontainer-hover",
    "--td-bg-color-component-hover",
    "--theme-hover-bg",
    "--song-list-item-hover",
    "--recent-song-item-hover",
    "--td-bg-color-container-active",
    "--td-bg-color-secondarycontainer-active",
    "--td-bg-color-component-active",
    "--td-bg-color-secondarycontainer",
    "--theme-bg-secondary",
    "--theme-header-bg",
    "--song-list-header-bg",
    "--settings-preview-bg",
    "--settings-feature-bg",
    "--settings-api-tips-bg",
    "--settings-quality-container-bg",
    "--settings-status-item-bg",
    "--settings-tech-item-bg",
    "--settings-developer-item-bg",
    "--settings-tag-option-bg",
    "--settings-tag-status-bg",
    "--td-bg-color-component",
    "--td-bg-color-specialcomponent",
    "--theme-badge-bg",
    "--find-song-count-bg",
    "--settings-source-icon-bg",
    "--td-component-stroke",
    "--td-border-level-1-color",
    "--theme-border-light",
    "--song-list-header-border",
    "--song-list-item-border",
    "--recent-song-item-border",
    "--td-component-border",
    "--td-border-level-2-color",
    "--theme-border-medium",
    "--theme-border-strong",
    "--theme-border",
    "--settings-sidebar-border",
    "--settings-group-border",
    "--settings-plugin-prompt-border",
    "--settings-preview-border",
    "--settings-feature-border",
    "--settings-api-tips-border",
    "--settings-source-card-border",
    "--settings-quality-container-border",
    "--settings-status-item-border",
    "--settings-tech-item-border",
    "--settings-developer-item-border",
    "--settings-tag-option-border",
    "--settings-tag-status-border",
    "--settings-nav-hover-bg",
    "--settings-nav-active-bg",
    "--settings-nav-active-border",
    "--settings-group-shadow",
    "--settings-nav-icon-color",
    "--settings-nav-icon-active",
    "--settings-nav-label-color",
    "--settings-nav-label-active",
    "--settings-nav-desc-color",
    "--settings-eq-visualizer-bg",
    "--settings-eq-visualizer-trail",
    "--settings-eq-visualizer-bar-start",
    "--settings-eq-visualizer-bar-end",
    "--player-bg-default",
    "--player-bg-idle",
    "--mobile-glass-bg",
    "--mobile-glass-bg-strong",
    "--mobile-glass-border",
];
