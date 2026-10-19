//! TTML lyrics from the `amll-ttml-db` library, for QQ Music and NetEase songs.
//!
//! The original asks the library first (`fetchTtmlLyrics`) and falls back to the source's own
//! lyric API. Parsing follows the AMLL TTML format as `TTMLParser` and `toAmllLyrics` read it:
//! `<p itunes:key>` lines of timed `<span>` words, `ttm:role="x-bg"` background vocals, inline
//! or `iTunesMetadata` translations and transliterations, and `ttm:agent` voices for duets.
use super::Lyric;
use crate::MusicItem;
use roxmltree::{Document, Node};
use std::collections::HashMap;

const NS_TTM: &str = "http://www.w3.org/ns/ttml#metadata";
const NS_ITUNES: &str = "http://music.apple.com/lyric-ttml-internal";
const NS_TTS: &str = "http://www.w3.org/ns/ttml#styling";
/// Romanization is matched to words by start time within this many milliseconds.
const ROMAN_TOLERANCE_MS: i64 = 30;

/// One timed word; `text` keeps the space that followed it in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedWord {
    pub start: u64,
    pub end: u64,
    pub text: String,
    /// Phonetic text matched to this word by the TTML timing information.
    pub romanization: String,
}

/// One lyric line of a TTML file (`LyricLine` of AMLL). Background vocals are lines of their own
/// that follow the main line they belong to.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedLine {
    pub start: u64,
    pub end: u64,
    pub words: Vec<TimedWord>,
    pub translation: String,
    pub romanization: String,
    pub background: bool,
    pub duet: bool,
}

/// What the lyric lookup found.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    /// A TTML file from the library.
    Ttml(Vec<TimedLine>),
    /// The source's own answer.
    Source(Lyric),
}

// ─── parsed model ──────────────────────────────────────────

#[derive(Clone, Default)]
struct Word {
    text: String,
    start: u64,
    end: u64,
    ends_with_space: bool,
}

/// A translation or romanization of a line or of its background vocal.
#[derive(Clone, Default)]
struct Sub {
    text: String,
    words: Vec<Word>,
}

#[derive(Clone, Default)]
struct Content {
    text: String,
    start: u64,
    end: u64,
    words: Vec<Word>,
    translations: Vec<Sub>,
    romanizations: Vec<Sub>,
    background: Option<Box<Content>>,
}

#[derive(Default)]
struct Sidecar {
    translations: Vec<Sub>,
    romanizations: Vec<Sub>,
    bg_translations: Vec<Sub>,
    bg_romanizations: Vec<Sub>,
}

#[derive(Default)]
struct State {
    full_text: String,
    words: Vec<Word>,
    translations: Vec<Sub>,
    romanizations: Vec<Sub>,
    bg_translations: Vec<Sub>,
    bg_romanizations: Vec<Sub>,
    background: Option<Content>,
}

// ─── text and time helpers ─────────────────────────────────

fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// `normalizeText`: runs of whitespace become one space, optionally trimmed.
fn normalize(text: &str, trim: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending = false;
    for ch in text.chars() {
        if is_space(ch) {
            pending = true;
        } else {
            if pending {
                out.push(' ');
                pending = false;
            }
            out.push(ch);
        }
    }
    if pending {
        out.push(' ');
    }
    if trim {
        out.trim_matches(is_space).to_owned()
    } else {
        out
    }
}

/// `parseTime`: `12.5s`, `12.520`, `1:02.5` and `1:02:03.5`; anything else is 0.
fn parse_time(text: &str) -> u64 {
    let text = text.trim();
    if text.is_empty() {
        return 0;
    }
    if let Some(seconds) = text.strip_suffix('s') {
        return seconds
            .parse::<f64>()
            .ok()
            .filter(|s| s.is_finite() && *s >= 0.)
            .map_or(0, |s| (s * 1000.).round() as u64);
    }
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return 0;
    }
    let seconds = parts[parts.len() - 1];
    if !seconds.bytes().all(|b| b.is_ascii_digit() || b == b'.') || seconds.matches('.').count() > 1
    {
        return 0;
    }
    let Ok(seconds) = seconds.parse::<f64>() else {
        return 0;
    };
    let mut whole = 0u64;
    for part in &parts[..parts.len() - 1] {
        let Ok(value) = part.parse::<u64>() else {
            return 0;
        };
        whole = whole * 60 + value;
    }
    ((whole as f64 * 60. + seconds) * 1000.).round() as u64
}

/// `getAttr`: the attribute in the given namespace, else any attribute with that local name.
fn attr<'a>(el: Node<'a, '_>, ns: Option<&str>, local: &str) -> Option<&'a str> {
    if let Some(ns) = ns
        && let Some(value) = el.attribute((ns, local)).filter(|v| !v.is_empty())
    {
        return Some(value);
    }
    el.attributes()
        .find(|a| a.name() == local && !a.value().is_empty())
        .map(|a| a.value())
}

fn text_content(el: Node) -> String {
    el.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect()
}

fn strip_open(text: &str) -> &str {
    text.trim_start_matches(['(', '（'])
}

fn strip_close(text: &str) -> &str {
    text.trim_end_matches([')', '）'])
}

// ─── element parsing (TTMLParser) ──────────────────────────

fn parse_content(el: Node) -> Content {
    let begin = attr(el, None, "begin");
    let end = attr(el, None, "end");
    let original_start = begin.map_or(0, parse_time);
    let original_end = end.map_or(0, parse_time);
    let mut state = State::default();
    for node in el.children() {
        if node.is_text() {
            process_text(&mut state, node.text().unwrap_or(""));
        } else if node.is_element() {
            process_element(&mut state, node);
        }
    }
    let mut background = state.background.take();
    if let Some(bg) = &mut background {
        bg.translations.append(&mut state.bg_translations);
        bg.romanizations.append(&mut state.bg_romanizations);
    }
    finalize_words(&mut state.words);
    // calculateTimeRange: children widen the range the attributes give.
    let (mut start, mut end_time) = (original_start, original_end);
    let mut min_child = u64::MAX;
    let mut max_child = 0u64;
    let mut any = false;
    for (s, e) in state
        .words
        .iter()
        .map(|w| (w.start, w.end))
        .chain(background.iter().map(|b| (b.start, b.end)))
    {
        any = true;
        min_child = min_child.min(s);
        max_child = max_child.max(e);
    }
    if any {
        if start == 0 || (min_child > 0 && min_child < start) {
            start = if min_child == u64::MAX { 0 } else { min_child };
        }
        if end_time == 0 || max_child > end_time {
            end_time = max_child;
        }
    }
    let clean = normalize(&state.full_text, true);
    let has_time_attrs = begin.is_some() || end.is_some();
    if state.words.is_empty() && !clean.is_empty() && has_time_attrs {
        state.words.push(Word {
            text: clean.clone(),
            start: if original_start > 0 {
                original_start
            } else {
                start
            },
            end: if original_end > 0 {
                original_end
            } else {
                end_time
            },
            ends_with_space: false,
        });
    }
    Content {
        text: clean,
        start,
        end: end_time,
        words: state.words,
        translations: state.translations,
        romanizations: state.romanizations,
        background: background.map(Box::new),
    }
}

fn process_text(state: &mut State, raw: &str) {
    let formatting = raw.contains('\n');
    if formatting && raw.trim_matches(is_space).is_empty() {
        return;
    }
    let normalized = normalize(raw, false);
    state.full_text.push_str(&normalized);
    if !formatting
        && !normalized.is_empty()
        && normalized.trim_matches(is_space).is_empty()
        && let Some(last) = state.words.last_mut()
    {
        last.ends_with_space = true;
    }
}

fn process_element(state: &mut State, el: Node) {
    if attr(el, Some(NS_TTS), "ruby") == Some("container") {
        process_ruby(state, el);
        return;
    }
    match attr(el, Some(NS_TTM), "role") {
        Some("x-bg") => state.background = Some(parse_background(el)),
        Some("x-translation") => {
            let (main, bg) = parse_inline_sub(el);
            state.translations.extend(main);
            state.bg_translations.extend(bg);
        }
        Some("x-roman") => {
            let (main, bg) = parse_inline_sub(el);
            state.romanizations.extend(main);
            state.bg_romanizations.extend(bg);
        }
        _ => process_word(state, el),
    }
}

/// A plain `<span begin end>`: a timed word. Spans without both times only add to the text.
fn process_word(state: &mut State, el: Node) {
    let begin = attr(el, None, "begin");
    let end = attr(el, None, "end");
    let raw = text_content(el);
    let normalized = normalize(&raw, false);
    state.full_text.push_str(&normalized);
    let (Some(begin), Some(end)) = (begin, end) else {
        return;
    };
    let formatting = raw.contains('\n');
    let (mut starts, mut ends) = (false, false);
    if !formatting {
        starts = normalized.starts_with(is_space);
        ends = normalized.ends_with(is_space);
    }
    let clean = normalized.trim_matches(is_space).to_owned();
    if starts && let Some(last) = state.words.last_mut() {
        last.ends_with_space = true;
    }
    if !clean.is_empty() {
        state.words.push(Word {
            text: clean,
            start: parse_time(begin),
            end: parse_time(end),
            ends_with_space: ends,
        });
    }
}

/// `tts:ruby="container"`: a base text with furigana-style timed annotations. The base text is
/// the word; its time span is that of the annotations.
fn process_ruby(state: &mut State, container: Node) {
    let mut base = String::new();
    let mut tags: Vec<(u64, u64)> = Vec::new();
    for child in container.children().filter(|n| n.is_element()) {
        match attr(child, Some(NS_TTS), "ruby") {
            Some("base") => base = normalize(&text_content(child), false),
            Some("textContainer") => {
                for text in child.children().filter(|n| n.is_element()) {
                    if attr(text, Some(NS_TTS), "ruby") != Some("text") {
                        continue;
                    }
                    let (Some(begin), Some(end)) =
                        (attr(text, None, "begin"), attr(text, None, "end"))
                    else {
                        continue;
                    };
                    if !normalize(&text_content(text), false).trim().is_empty() {
                        tags.push((parse_time(begin), parse_time(end)));
                    }
                }
            }
            _ => {}
        }
    }
    if base.is_empty() {
        return;
    }
    state.full_text.push_str(&base);
    let (start, end) = if tags.is_empty() {
        (0, 0)
    } else {
        (
            tags.iter().map(|t| t.0).min().unwrap_or(0),
            tags.iter().map(|t| t.1).max().unwrap_or(0),
        )
    };
    let clean = base.trim_matches(is_space);
    if !clean.is_empty() {
        if base.starts_with(is_space)
            && let Some(last) = state.words.last_mut()
        {
            last.ends_with_space = true;
        }
        state.words.push(Word {
            text: clean.to_owned(),
            start,
            end,
            ends_with_space: base.ends_with(is_space),
        });
    }
}

/// `parseBackgroundVocal`: the content of a `x-bg` span without its enclosing parentheses.
fn parse_background(el: Node) -> Content {
    let mut content = parse_content(el);
    content.background = None;
    content.text = strip_close(strip_open(&content.text)).to_owned();
    if let Some(first) = content.words.first_mut() {
        first.text = strip_open(&first.text)
            .trim_start_matches(is_space)
            .to_owned();
    }
    if let Some(last) = content.words.last_mut() {
        last.text = strip_close(&last.text)
            .trim_end_matches(is_space)
            .to_owned();
    }
    content
}

/// `finalizeWords`: the first word loses leading space, the last trailing space.
fn finalize_words(words: &mut [Word]) {
    if let Some(first) = words.first_mut() {
        first.text = first.text.trim_start_matches(is_space).to_owned();
    }
    if let Some(last) = words.last_mut() {
        last.text = last.text.trim_end_matches(is_space).to_owned();
        last.ends_with_space = false;
    }
}

/// `extractSubContent`: the text (and, unless ignored, word timings) of a parsed element as a
/// translation, and of its background vocal. A lone `0`..`0` word is a placeholder, not timing.
fn extract_sub(base: &Content, ignore_words: bool) -> (Option<Sub>, Option<Sub>) {
    let sub = |content: &Content| {
        let text = normalize(&content.text, true);
        let words = if ignore_words || content.words.is_empty() {
            Vec::new()
        } else if content.words.len() == 1
            && content.words[0].start == 0
            && content.words[0].end == 0
        {
            Vec::new()
        } else {
            content.words.clone()
        };
        let has_words = !ignore_words && !content.words.is_empty();
        (!text.is_empty() || has_words).then_some(Sub { text, words })
    };
    (sub(base), base.background.as_deref().and_then(sub))
}

fn parse_inline_sub(el: Node) -> (Option<Sub>, Option<Sub>) {
    extract_sub(&parse_content(el), true)
}

fn child_elements<'a, 'input>(
    el: Node<'a, 'input>,
    local: &'static str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    el.descendants()
        .filter(move |n| n.is_element() && n.tag_name().name() == local)
}

/// `parseiTunesExtensions`: translations and transliterations kept apart from the lines, keyed
/// by the line's `itunes:key`.
fn parse_sidecar(doc: &Document) -> HashMap<String, Sidecar> {
    let mut sidecar: HashMap<String, Sidecar> = HashMap::new();
    let Some(head) = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "head")
    else {
        return sidecar;
    };
    for meta in child_elements(head, "iTunesMetadata") {
        for (container, item, romanization) in [
            ("translations", "translation", false),
            ("transliterations", "transliteration", true),
        ] {
            let Some(container) = child_elements(meta, container).next() else {
                continue;
            };
            for entry in child_elements(container, item) {
                for text in child_elements(entry, "text") {
                    let Some(target) = text.attribute("for") else {
                        continue;
                    };
                    let parsed = parse_content(text);
                    let (main, bg) = extract_sub(&parsed, false);
                    let slot = sidecar.entry(target.to_owned()).or_default();
                    if romanization {
                        slot.romanizations.extend(main);
                        slot.bg_romanizations.extend(bg);
                    } else {
                        slot.translations.extend(main);
                        slot.bg_translations.extend(bg);
                    }
                }
            }
        }
    }
    sidecar
}

// ─── conversion (toAmllLyrics) ─────────────────────────────

struct AmllWord {
    start: u64,
    end: u64,
    text: String,
    roman: String,
}

/// `alignRomanization`: romanized words belong to the main words that start at the same time.
fn align_romanization(words: &mut [AmllWord], roman: &[Word]) {
    let (mut i, mut j) = (0, 0);
    while i < words.len() && j < roman.len() {
        let delta = words[i].start as i64 - roman[j].start as i64;
        if delta.abs() < ROMAN_TOLERANCE_MS {
            words[i].roman = roman[j].text.clone();
            i += 1;
            j += 1;
        } else if roman[j].start < words[i].start {
            j += 1;
        } else {
            i += 1;
        }
    }
}

fn to_line(source: &Content, background: bool, duet: bool) -> TimedLine {
    let mut words: Vec<AmllWord> = if source.words.is_empty() {
        vec![AmllWord {
            start: source.start,
            end: source.end,
            text: source.text.clone(),
            roman: String::new(),
        }]
    } else {
        source
            .words
            .iter()
            .map(|w| AmllWord {
                start: w.start,
                end: w.end,
                text: format!("{}{}", w.text, if w.ends_with_space { " " } else { "" }),
                roman: String::new(),
            })
            .collect()
    };
    let translation = source
        .translations
        .first()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    let mut romanization = String::new();
    if let Some(target) = source.romanizations.first() {
        if target.words.is_empty() {
            romanization = target.text.clone();
        } else {
            align_romanization(&mut words, &target.words);
            // Keep the flattened fallback as well as the exact per-word alignment.
            romanization = words
                .iter()
                .map(|w| w.roman.trim())
                .filter(|r| !r.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
        }
    }
    TimedLine {
        start: source.start,
        end: source.end,
        words: words
            .into_iter()
            .map(|w| TimedWord {
                start: w.start,
                end: w.end,
                text: w.text,
                romanization: w.roman,
            })
            .collect(),
        translation,
        romanization,
        background,
        duet,
    }
}

/// Parses a TTML document into player lines: main lines, each followed by its background vocal.
pub fn parse(text: &str) -> Result<Vec<TimedLine>, String> {
    let doc = Document::parse(text).map_err(|_| "TTML 歌词格式无效".to_string())?;
    let mut agents: HashMap<String, String> = HashMap::new();
    for agent in doc.descendants().filter(|n| {
        n.is_element() && n.tag_name().name() == "agent" && n.tag_name().namespace() == Some(NS_TTM)
    }) {
        if let (Some(id), Some(kind)) = (
            agent.attribute(("http://www.w3.org/XML/1998/namespace", "id")),
            attr(agent, Some(NS_TTM), "type"),
        ) {
            agents.insert(id.to_owned(), kind.to_owned());
        }
    }
    let sidecar = parse_sidecar(&doc);
    let Some(body) = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "body")
    else {
        return Err("TTML 歌词没有正文".into());
    };
    let mut paragraphs: Vec<Node> = Vec::new();
    for child in body.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "div" => paragraphs.extend(child_elements(child, "p")),
            "p" => paragraphs.push(child),
            _ => {}
        }
    }
    let mut lines = Vec::new();
    let mut last_agent: Option<String> = None;
    let mut last_duet = false;
    for p in paragraphs {
        let Some(key) = attr(p, Some(NS_ITUNES), "key") else {
            continue;
        };
        let mut content = parse_content(p);
        if let Some(extra) = sidecar.get(key) {
            content.translations.extend(extra.translations.clone());
            content.romanizations.extend(extra.romanizations.clone());
            if let Some(bg) = content.background.as_deref_mut() {
                bg.translations.extend(extra.bg_translations.clone());
                bg.romanizations.extend(extra.bg_romanizations.clone());
            }
        }
        // Which voice sings: the first person is the main side, the next different one the
        // other side, and a group never sits on the other side.
        let agent_id = attr(p, Some(NS_TTM), "agent").unwrap_or("v1").to_owned();
        let kind = agents.get(&agent_id).map(String::as_str);
        let duet = if kind == Some("group") {
            false
        } else {
            match &last_agent {
                None => {
                    last_agent = Some(agent_id.clone());
                    last_duet = kind == Some("other");
                    last_duet
                }
                Some(previous) if *previous == agent_id => last_duet,
                Some(_) => {
                    last_agent = Some(agent_id.clone());
                    last_duet = !last_duet;
                    last_duet
                }
            }
        };
        lines.push(to_line(&content, false, duet));
        if let Some(bg) = content.background.as_deref() {
            lines.push(to_line(bg, true, duet));
        }
    }
    if lines.is_empty() {
        return Err("TTML 歌词没有歌词行".into());
    }
    Ok(lines)
}

/// Decode the enabled plugin's TTML, or its own source-lyric payload from the same request.
pub fn find(item: &MusicItem) -> Result<Found, String> {
    let payload = super::request(item)?;
    if let Some(ttml) = payload["ttml"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        && let Ok(lines) = parse(ttml)
        && !lines.is_empty()
    {
        return Ok(Found::Ttml(lines));
    }
    super::decode_payload(&payload, &item.source).map(Found::Source)
}
