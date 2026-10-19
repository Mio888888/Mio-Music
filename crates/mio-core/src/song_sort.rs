use crate::song::Song;
use std::{cmp::Ordering, sync::OnceLock};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Title,
    Album,
    Duration,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Default,
    TitleAsc,
    TitleDesc,
    ArtistAsc,
    ArtistDesc,
    AlbumAsc,
    AlbumDesc,
    DurationAsc,
    DurationDesc,
}

impl Sort {
    pub fn cycle(self, column: Column) -> Self {
        match (column, self) {
            (Column::Title, Self::TitleAsc) => Self::TitleDesc,
            (Column::Title, Self::TitleDesc) => Self::ArtistAsc,
            (Column::Title, Self::ArtistAsc) => Self::ArtistDesc,
            (Column::Title, Self::ArtistDesc) => Self::Default,
            (Column::Title, _) => Self::TitleAsc,
            (Column::Album, Self::AlbumAsc) => Self::AlbumDesc,
            (Column::Album, Self::AlbumDesc) => Self::Default,
            (Column::Album, _) => Self::AlbumAsc,
            (Column::Duration, Self::DurationAsc) => Self::DurationDesc,
            (Column::Duration, Self::DurationDesc) => Self::Default,
            (Column::Duration, _) => Self::DurationAsc,
        }
    }

    pub fn column(self) -> Option<Column> {
        match self {
            Self::Default => None,
            Self::TitleAsc | Self::TitleDesc | Self::ArtistAsc | Self::ArtistDesc => {
                Some(Column::Title)
            }
            Self::AlbumAsc | Self::AlbumDesc => Some(Column::Album),
            Self::DurationAsc | Self::DurationDesc => Some(Column::Duration),
        }
    }

    pub fn descending(self) -> bool {
        matches!(
            self,
            Self::TitleDesc | Self::ArtistDesc | Self::AlbumDesc | Self::DurationDesc
        )
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::TitleAsc => "music.songList.titleAsc",
            Self::TitleDesc => "music.songList.titleDesc",
            Self::ArtistAsc => "music.songList.artistAsc",
            Self::ArtistDesc => "music.songList.artistDesc",
            _ => "music.songList.defaultSort",
        }
    }

    pub fn apply(self, songs: &mut [Song]) {
        if self == Self::Default {
            return;
        }
        // Stable ties retain props.songs order, including when descending.
        songs.sort_by(|a, b| {
            let order = match self {
                Self::TitleAsc | Self::TitleDesc => compare_text(&a.title, &b.title),
                Self::ArtistAsc | Self::ArtistDesc => compare_text(&a.artist, &b.artist),
                Self::AlbumAsc | Self::AlbumDesc => compare_text(&a.album, &b.album),
                Self::DurationAsc | Self::DurationDesc => a.duration.cmp(&b.duration),
                Self::Default => Ordering::Equal,
            };
            if self.descending() {
                order.reverse()
            } else {
                order
            }
        });
    }
}

fn js_whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | ' ' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn clean(s: &str) -> String {
    // Match /[^\w\s一-龥]/g and JS trim, not Unicode alphanumeric/whitespace.
    let value: String = s
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || *c == '_' || js_whitespace(*c) || ('一'..='龥').contains(c)
        })
        .collect();
    value.trim_matches(js_whitespace).to_owned()
}

fn priority(s: &str) -> u8 {
    match s.chars().next() {
        Some('0'..='9') => 0,
        Some('A'..='Z') => 1,
        Some('a'..='z') => 2,
        Some(_) => 3,
        None => 4,
    }
}

fn compare_text(a: &str, b: &str) -> Ordering {
    static COLLATOR: OnceLock<icu_collator::CollatorBorrowed<'static>> = OnceLock::new();
    let a = clean(a);
    let b = clean(b);
    priority(&a).cmp(&priority(&b)).then_with(|| {
        COLLATOR
            .get_or_init(|| {
                icu_collator::Collator::try_new(
                    icu_locale_core::locale!("zh-CN").into(),
                    Default::default(),
                )
                .expect("compiled Chinese collation data")
            })
            .compare(&a, &b)
    })
}
