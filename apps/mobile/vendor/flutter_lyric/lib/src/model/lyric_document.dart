import 'dart:ui' show TextAlign;

import '../parser/lyric_parser.dart';

/// Immutable, time-ordered lyrics. Times use the lyric timeline; [offset] is
/// added to the player's position before looking up the active line.
class LyricDocument {
  final Map<String, String> tags;
  final List<LyricLine> lines;
  final Duration offset;

  /// Foreground rows own scroll focus; background vocals attach to the prior
  /// foreground row in stable time order. An initial background owns itself.
  late final List<int> focusIndices = List.unmodifiable([
    for (var i = 0; i < lines.length; i++)
      if (i == 0 || !lines[i].background) i,
  ]);
  late final List<int> groupIndices = _groups();
  List<int> _groups() {
    var group = -1;
    return List.unmodifiable([
      for (var i = 0; i < lines.length; i++)
        (i == 0 || !lines[i].background) ? ++group : group,
    ]);
  }

  /// In fixed, scrollable views, explicit [LyricLine.start]/end intervals
  /// determine singing activity independently of the single scrolling focus.
  /// Untimed/open-ended entries retain the ordinary focus-based behavior.
  final bool useLineIntervals;

  /// Start of the first sung lyric on the lyric timeline, when supplied by the
  /// provider (for example QQ Music's `startTs`, in milliseconds).
  ///
  /// Countdown detection treats earlier entries as intro credits, regardless
  /// of their durations. The intro spans zero to the first sung line's start
  /// so credit time can supply the full countdown duration. Intros shorter
  /// than that duration remain hidden. Entries and word timing are unchanged.
  /// Null uses the normal gap detection from the first non-empty entry.
  final Duration? vocalStart;

  LyricDocument({
    required List<LyricLine> lines,
    Map<String, String> tags = const {},
    Duration? offset,
    this.vocalStart,
    this.useLineIntervals = false,
  })  : tags = Map.unmodifiable(tags),
        lines = _ordered(lines),
        offset = offset ??
            Duration(milliseconds: int.tryParse(tags['offset'] ?? '') ?? 0) {
    if (vocalStart?.isNegative ?? false) {
      throw ArgumentError.value(
          vocalStart, 'vocalStart', 'Must be non-negative.');
    }
  }

  factory LyricDocument.parse(String text,
      {String? translation, LyricParser? parser, Duration? vocalStart}) {
    final document = parser == null
        ? LyricParser.autoDetect(text, translationLyric: translation)
        : parser.parse(text, translationLyric: translation);
    return vocalStart == null
        ? document
        : document.copyWith(vocalStart: vocalStart);
  }

  static List<LyricLine> _ordered(List<LyricLine> lines) {
    final indexed = lines.asMap().entries.toList()
      ..sort((a, b) {
        final order = a.value.start.compareTo(b.value.start);
        return order == 0 ? a.key.compareTo(b.key) : order;
      });
    return List.unmodifiable(indexed.map((entry) => entry.value));
  }

  LyricDocument copyWith({
    List<LyricLine>? lines,
    Map<String, String>? tags,
    Duration? offset,
    Duration? vocalStart,
    bool clearVocalStart = false,
    bool? useLineIntervals,
  }) {
    if (clearVocalStart && vocalStart != null) {
      throw ArgumentError('Cannot set and clear vocalStart together.');
    }
    return LyricDocument(
      lines: lines ?? this.lines,
      tags: tags ?? this.tags,
      offset: offset ?? this.offset,
      vocalStart: clearVocalStart ? null : vocalStart ?? this.vocalStart,
      useLineIntervals: useLineIntervals ?? this.useLineIntervals,
    );
  }

  String get title => tags['ti'] ?? '';
  String get artist => tags['ar'] ?? '';
  String get album => tags['al'] ?? '';
  String get by => tags['by'] ?? '';

  @override
  String toString() => 'LyricDocument(${lines.length} lines, offset: $offset)';
}

/// One lyric entry; it may wrap visually and may contain a translation.
class LyricLine {
  final Duration start;
  final Duration? end;
  final String text;
  final String? translation;

  /// Separate auxiliary paragraphs, each retaining its own internal newlines.
  /// [translation] joins these for semantics and consumers needing plain text.
  final List<String>? translationBlocks;
  final List<LyricWord>? words;

  /// Optional per-line paragraph and block alignment. Null inherits the preset.
  /// Absolute left/right remain absolute in RTL views as well (duet singers).
  final TextAlign? textAlign;

  /// Mio backing vocal: smaller, dimmer, and grouped with preceding lead.
  final bool background;

  LyricLine({
    required this.start,
    required this.text,
    this.end,
    String? translation,
    List<String>? translationBlocks,
    this.textAlign,
    this.background = false,
    List<LyricWord>? words,
  })  : assert(translation == null || translationBlocks == null),
        translationBlocks = translationBlocks == null
            ? null
            : List.unmodifiable(translationBlocks),
        translation = translationBlocks == null
            ? translation
            : translationBlocks.join('\n'),
        words = words == null ? null : List.unmodifiable(words);

  @override
  String toString() => 'LyricLine(start: $start, text: $text)';
}

/// A timed word or syllable. Both times are relative to the complete document.
class LyricWord {
  final String text;
  final Duration start;
  final Duration? end;

  final String romanization;

  const LyricWord(
      {required this.text,
      required this.start,
      this.end,
      this.romanization = ''});

  @override
  String toString() => 'LyricWord($text, $start–$end)';
}

/// Internal parser result for an LRC metadata tag.
class LyricTag {
  final String tag;
  final String value;
  const LyricTag({required this.tag, required this.value});
}
