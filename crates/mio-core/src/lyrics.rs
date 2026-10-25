//! Lyrics for the full player: line-level text from Subsonic or an online source's LRC, and the
//! word-timed form (`crlyric`) of the online sources that have one.
use mio_music_sdk::Lyric;
use regex::Regex;
use std::sync::LazyLock;

/// Same bounds the Subsonic client enforces on a server's answer.
const MAX_LINES: usize = 4096;
const MAX_LINE_BYTES: usize = 4096;
const MAX_START_MS: u64 = 604_800_000;

/// One timed word of a line, in milliseconds from the start of the song.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub start: u64,
    pub end: u64,
    pub text: String,
    pub romanization: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub start: Option<u64>,
    /// End of a word-timed line (`LyricLine.endTime`); `None` for line-level lyrics.
    pub end: Option<u64>,
    pub text: String,
    pub translation: Option<String>,
    pub romanization: Option<String>,
    /// Empty for line-level lyrics. `text` is always the words joined.
    pub words: Vec<Word>,
    /// A background vocal line (`isBG`): shown smaller, under the main line.
    pub background: bool,
    /// A duet line (`isDuet`): aligned to the other side.
    pub duet: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lyrics {
    pub synced: bool,
    /// LRC `[offset:]` semantics: positive values show lines earlier.
    pub offset: i64,
    pub lines: Vec<Line>,
    /// At least one line carries word timings.
    pub word_timed: bool,
}

/// Owned request data so hosts can load lyrics without holding application state locks.
pub enum Source {
    Online(mio_music_sdk::MusicItem),
    Local(std::path::PathBuf),
}

impl Source {
    pub fn load(self) -> Result<Option<Lyrics>, String> {
        match self {
            Self::Online(item) => fetch_online(&item),
            Self::Local(path) => crate::local_music::lyrics(&path),
        }
    }
}

pub fn fetch_online(item: &mio_music_sdk::MusicItem) -> Result<Option<Lyrics>, String> {
    match mio_music_sdk::find_lyric(item)? {
        mio_music_sdk::FoundLyric::Ttml(lines) => match from_ttml(lines) {
            Some(lyrics) => Ok(Some(lyrics)),
            None => mio_music_sdk::get_lyric(item).map(|lyric| from_online(&lyric)),
        },
        mio_music_sdk::FoundLyric::Source(lyric) => Ok(from_online(&lyric)),
    }
}

impl Lyrics {
    /// Background rows share the preceding foreground scroll anchor.
    pub fn active_line(&self, position: f64) -> Option<usize> {
        let effective = position * 1000. + self.offset as f64;
        if !self.synced || !effective.is_finite() || effective < 0. {
            return None;
        }
        let now = effective as u64;
        self.lines
            .iter()
            .enumerate()
            .rfind(|(i, line)| (!line.background || *i == 0) && line.start.unwrap_or(0) <= now)
            .map(|(i, _)| i)
    }
    pub fn line_position(&self, index: usize) -> Option<f64> {
        let start = self.lines.get(index)?.start?;
        Some(((start as f64 - self.offset as f64) / 1000.).max(0.))
    }
}

impl From<crate::song::Lyrics> for Lyrics {
    fn from(lyrics: crate::song::Lyrics) -> Self {
        Self {
            synced: lyrics.synced,
            offset: lyrics.offset,
            lines: lyrics
                .line
                .into_iter()
                .map(|line| Line {
                    start: line.start,
                    text: line.value,
                    ..Line::default()
                })
                .collect(),
            word_timed: false,
        }
    }
}

static TIME_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]").unwrap());
static META_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[([A-Za-z]+):([^\]]*)\]").unwrap());

struct Parsed {
    /// `(start in ms, text)` sorted by start; empty when the text carries no time tags.
    timed: Vec<(u64, String)>,
    /// Text lines without any time tag, in order.
    plain: Vec<String>,
    offset: i64,
}

fn parse(text: &str) -> Parsed {
    let mut timed = Vec::new();
    let mut plain = Vec::new();
    let mut offset = 0i64;
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut starts = Vec::new();
        while let Some(caps) = TIME_TAG.captures(rest) {
            let minutes: u64 = caps[1].parse().unwrap_or(0);
            let seconds: u64 = caps[2].parse().unwrap_or(0);
            let fraction = caps.get(3).map_or(0, |f| {
                let digits = f.as_str();
                let value: u64 = digits.parse().unwrap_or(0);
                match digits.len() {
                    1 => value * 100,
                    2 => value * 10,
                    _ => value,
                }
            });
            starts.push(minutes * 60_000 + seconds * 1000 + fraction);
            rest = rest[caps[0].len()..].trim_start();
        }
        if starts.is_empty() {
            if let Some(caps) = META_TAG.captures(rest) {
                if caps[1].eq_ignore_ascii_case("offset") {
                    offset = caps[2].trim().trim_start_matches('+').parse().unwrap_or(0);
                }
                continue;
            }
            let body = rest.trim();
            if !body.is_empty() {
                plain.push(body.to_owned());
            }
            continue;
        }
        let body = rest.trim();
        if body.is_empty() {
            continue;
        }
        for start in starts {
            timed.push((start, body.to_owned()));
        }
    }
    timed.sort_by_key(|(start, _)| *start);
    Parsed {
        timed,
        plain,
        offset,
    }
}

/// The text of the entry of `sub` closest in time to `start`, within `tolerance_ms`.
fn nearest(sub: &[(u64, String)], start: u64, tolerance_ms: u64) -> Option<String> {
    sub.iter()
        .filter(|(at, _)| at.abs_diff(start) <= tolerance_ms)
        .min_by_key(|(at, _)| at.abs_diff(start))
        .map(|(_, text)| text.clone())
        .filter(|text| text.trim() != "//")
}

fn clip(text: String) -> String {
    if text.len() <= MAX_LINE_BYTES {
        return text;
    }
    let mut end = MAX_LINE_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Builds the player's lyrics from an online source's answer. `None` when there is no text.
/// Word timings win when the source has them and they parse, as in the original's
/// `fetchSdkLyrics`; otherwise the line-level text is used.
pub fn from_online(lyric: &Lyric) -> Option<Lyrics> {
    word_timed(lyric).or_else(|| line_level(lyric))
}

fn line_level(lyric: &Lyric) -> Option<Lyrics> {
    let main = parse(&lyric.lyric);
    if main.timed.is_empty() {
        // No time tags: show the text as it is, unsynchronised.
        if main.plain.is_empty() {
            return None;
        }
        return Some(Lyrics {
            synced: false,
            offset: 0,
            lines: main
                .plain
                .into_iter()
                .take(MAX_LINES)
                .map(|text| Line {
                    text: clip(text),
                    ..Line::default()
                })
                .collect(),
            word_timed: false,
        });
    }
    let translation = parse(&lyric.tlyric).timed;
    let romanization = parse(&lyric.rlyric).timed;
    let mut lines: Vec<Line> = Vec::new();
    // Some lyric texts interleave the translation as a second line with the same
    // time. When that is how most lines look and there is no separate translation, fold it in.
    let repeats = main
        .timed
        .windows(2)
        .filter(|pair| pair[0].0 == pair[1].0)
        .count();
    let bilingual =
        translation.is_empty() && main.timed.len() >= 4 && repeats * 10 >= main.timed.len() * 4;
    for (start, text) in main.timed.into_iter().take(MAX_LINES * 2) {
        if bilingual {
            if let Some(last) = lines.last_mut() {
                if last.start == Some(start) && last.translation.is_none() {
                    last.translation = Some(clip(text));
                    continue;
                }
            }
        }
        if start > MAX_START_MS {
            continue;
        }
        lines.push(Line {
            start: Some(start),
            translation: nearest(&translation, start, 600).map(clip),
            romanization: nearest(&romanization, start, 600).map(clip),
            text: clip(text),
            ..Line::default()
        });
    }
    lines.truncate(MAX_LINES);
    Some(Lyrics {
        synced: true,
        offset: main
            .offset
            .clamp(-(MAX_START_MS as i64), MAX_START_MS as i64),
        lines,
        word_timed: false,
    })
}

// ─── word-timed lyrics ─────────────────────────────────────

/// A parsed word-timed line before it becomes a player line (`LyricLine` of the original).
struct RawLine {
    start: u64,
    end: u64,
    words: Vec<Word>,
    background: bool,
    duet: bool,
    /// Translation and romanization that came with the line (TTML); `None` when the source
    /// delivers them as separate LRC texts matched by time.
    translation: Option<String>,
    romanization: Option<String>,
}

/// Words per line and bytes per word the player accepts, like the line limits above.
const MAX_WORDS: usize = 256;
/// `defaultLineDuration` of `sanitizeLyricLines`.
const DEFAULT_LINE_MS: u64 = 3000;

static LINE_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[(\d+),(\d+)\]").unwrap());
static YRC_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.*?)\((\d+),(\d+),0\)").unwrap());
static TEXT_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(.*?)\((\d+),(\d+)\)").unwrap());
static LYS_PROP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[(\d+)\]").unwrap());

fn number(text: &str) -> u64 {
    text.parse().unwrap_or(0)
}

/// `checkIsBG` and its parenthesis trimming: a line wrapped in one pair of (full-width)
/// parentheses is a background vocal and loses them.
fn mark_background(words: &mut [Word]) -> bool {
    let open = |c: char| c == '(' || c == '（';
    let close = |c: char| c == ')' || c == '）';
    let is_bg = match (words.first(), words.last()) {
        (Some(first), Some(last)) => {
            first.text.chars().next().is_some_and(open)
                && last.text.chars().next_back().is_some_and(close)
        }
        _ => false,
    };
    if is_bg {
        if let Some(first) = words.first_mut() {
            first.text = first.text.chars().skip(1).collect();
        }
        if let Some(last) = words.last_mut() {
            let mut chars = last.text.chars();
            chars.next_back();
            last.text = chars.as_str().to_owned();
        }
    }
    is_bg
}

/// `wordTagBefore`: `[start,duration]` then `(start,duration,0)word` pairs.
fn parse_yrc(text: &str) -> Vec<RawLine> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let Some(prefix) = LINE_PREFIX.captures(line) else {
            continue;
        };
        let start = number(&prefix[1]);
        let duration = number(&prefix[2]);
        let mut content = line[prefix[0].len()..].trim();
        if content.is_empty() {
            continue;
        }
        let mut words = Vec::new();
        // The text before a marker belongs to the previous marker: `(s,d,0)word(s,d,0)word`.
        let mut last: Option<(u64, u64)> = None;
        while let Some(found) = YRC_WORD.captures(content) {
            let before = &found[1];
            if let (false, Some((s, e))) = (before.is_empty(), last) {
                words.push(Word {
                    start: s,
                    end: e,
                    text: before.to_owned(),
                    romanization: String::new(),
                });
            }
            let word_start = number(&found[2]);
            last = Some((word_start, word_start + number(&found[3])));
            content = &content[found[0].len()..];
        }
        if let (Some((s, e)), false) = (last, content.is_empty()) {
            words.push(Word {
                start: s,
                end: e,
                text: content.to_owned(),
                romanization: String::new(),
            });
        }
        let background = mark_background(&mut words);
        out.push(RawLine {
            start,
            end: start + duration,
            words,
            background,
            duet: false,
            translation: None,
            romanization: None,
        });
    }
    out
}

fn text_words(content: &str) -> Vec<Word> {
    TEXT_WORD
        .captures_iter(content)
        .map(|found| {
            let start = number(&found[2]);
            Word {
                start,
                end: start + number(&found[3]),
                text: found[1].to_owned(),
                romanization: String::new(),
            }
        })
        .collect()
}

/// `wordTagAfter`: `[start,duration]` then `word(start,duration)` pairs. Other lines of the text
/// (an XML wrapper, say) simply do not start with a time prefix.
fn parse_qrc(text: &str) -> Vec<RawLine> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let Some(prefix) = LINE_PREFIX.captures(line) else {
            continue;
        };
        let start = number(&prefix[1]);
        let duration = number(&prefix[2]);
        let content = line[prefix[0].len()..].trim();
        if content.is_empty() {
            continue;
        }
        let mut words = text_words(content);
        let background = mark_background(&mut words);
        out.push(RawLine {
            start,
            end: start + duration,
            words,
            background,
            duet: false,
            translation: None,
            romanization: None,
        });
    }
    out
}

/// `wordTagAfterWithProperty` (Lyricify Syllable): `[property]` then `word(start,duration)`
/// pairs. The property encodes duet and background.
fn parse_lys(text: &str) -> Vec<RawLine> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let Some(prop) = LYS_PROP.captures(line) else {
            continue;
        };
        let prop = number(&prop[1]);
        let prop = if prop > 8 { 0 } else { prop };
        let duet = prop % 3 == 2;
        let content = &line[LYS_PROP.find(line).map_or(0, |m| m.end())..];
        let mut words = text_words(content);
        let (Some(first), Some(last)) = (words.first(), words.last()) else {
            continue;
        };
        let (start, end) = (first.start, last.end);
        // Properties 0..=2 leave the background flag to the parentheses test.
        let background = if prop <= 2 {
            mark_background(&mut words)
        } else {
            let bg = prop >= 6;
            if bg {
                let open = |c: char| c == '(' || c == '（';
                let close = |c: char| c == ')' || c == '）';
                if let Some(first) = words.first_mut() {
                    if first.text.chars().next().is_some_and(open) {
                        first.text = first.text.chars().skip(1).collect();
                    }
                }
                if let Some(last) = words.last_mut() {
                    if last.text.chars().next_back().is_some_and(close) {
                        let mut chars = last.text.chars();
                        chars.next_back();
                        last.text = chars.as_str().to_owned();
                    }
                }
            }
            bg
        };
        out.push(RawLine {
            start,
            end,
            words,
            background,
            duet,
            translation: None,
            romanization: None,
        });
    }
    out
}

/// `sanitizeLyricLines`: words never start before the previous one ended and last at least a
/// millisecond; a line ends no earlier than its last word (and gets 3 s when it has no end);
/// lines without words go; the rest are ordered by start.
fn sanitize(lines: Vec<RawLine>) -> Vec<RawLine> {
    let mut cleaned: Vec<RawLine> = lines
        .into_iter()
        .filter_map(|mut line| {
            let mut previous_end = 0u64;
            for word in &mut line.words {
                let mut start = word.start.max(previous_end);
                let mut end = word.end;
                if end <= start {
                    end = start + 1;
                }
                if start < previous_end {
                    start = previous_end;
                }
                previous_end = end;
                word.start = start;
                word.end = end;
            }
            let last_end = line.words.last()?.end;
            if line.end <= line.start {
                line.end = line.start + DEFAULT_LINE_MS;
            }
            line.end = line.end.max(last_end);
            Some(line)
        })
        .collect();
    cleaned.sort_by_key(|line| line.start);
    cleaned
}

/// The word-timed lyrics of an online answer, or `None` when the plugin gave none or its form
/// does not parse (the caller then shows the line-level text). The plugin names the layout of
/// `crlyric` in `crlyricFormat`: `wordTagBefore` (the default: `[start,duration]` then
/// `(start,duration,0)word` pairs), `wordTagAfter` (`[start,duration]` then `word(start,duration)`
/// pairs) or `wordTagAfterWithProperty` (`[property]` then `word(start,duration)` pairs, the
/// property encoding duet and background). Any other text matches none of them and stays
/// line-level.
fn word_timed(lyric: &Lyric) -> Option<Lyrics> {
    let text = lyric.crlyric.trim();
    if text.is_empty() {
        return None;
    }
    let raw = match lyric.crlyric_format.as_str() {
        "wordTagAfterWithProperty" => parse_lys(text),
        "wordTagAfter" => parse_qrc(text),
        _ => parse_yrc(text),
    };
    build_word_lines(raw, &lyric.tlyric, &lyric.rlyric)
}

/// The lines of a TTML file from the lyric library, as the player's word-timed lyrics. `None`
/// when no line has a word.
pub fn from_ttml(lines: Vec<mio_music_sdk::TimedLine>) -> Option<Lyrics> {
    let raw = lines
        .into_iter()
        .map(|line| RawLine {
            start: line.start,
            end: line.end,
            words: line
                .words
                .into_iter()
                .map(|w| Word {
                    start: w.start,
                    end: w.end,
                    text: w.text,
                    romanization: w.romanization,
                })
                .collect(),
            background: line.background,
            duet: line.duet,
            translation: Some(line.translation).filter(|t| !t.trim().is_empty()),
            romanization: Some(line.romanization).filter(|t| !t.trim().is_empty()),
        })
        .collect();
    build_word_lines(raw, "", "")
}

fn build_word_lines(raw: Vec<RawLine>, tlyric: &str, rlyric: &str) -> Option<Lyrics> {
    let translation = parse(tlyric).timed;
    let romanization = parse(rlyric).timed;
    let mut lines: Vec<Line> = Vec::new();
    for line in sanitize(raw) {
        if line.start > MAX_START_MS {
            continue;
        }
        let mut words = line.words;
        words.truncate(MAX_WORDS);
        let text: String = words.iter().map(|w| w.text.as_str()).collect();
        // A line of nothing but spaces has nothing to show.
        if text.trim().is_empty() {
            continue;
        }
        if text.len() > MAX_LINE_BYTES {
            // Drop whole words past the limit so `text` stays the words joined.
            let mut bytes = 0;
            words.retain(|word| {
                bytes += word.text.len();
                bytes <= MAX_LINE_BYTES
            });
        }
        let text: String = words.iter().map(|w| w.text.as_str()).collect();
        lines.push(Line {
            start: Some(line.start),
            end: Some(line.end),
            translation: line
                .translation
                .or_else(|| nearest(&translation, line.start, 600).map(clip)),
            romanization: line
                .romanization
                .or_else(|| nearest(&romanization, line.start, 600).map(clip)),
            text,
            words,
            background: line.background,
            duet: line.duet,
        });
        if lines.len() >= MAX_LINES {
            break;
        }
    }
    if lines.is_empty() {
        return None;
    }
    Some(Lyrics {
        synced: true,
        offset: 0,
        lines,
        word_timed: true,
    })
}
