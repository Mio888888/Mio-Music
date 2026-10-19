# Mio extensions to flutter_lyric 4.1.0

Vendored lib and package metadata from pub.dev 4.1.0; original MIT LICENSE retained. UPSTREAM.json records the unmodified source hashes. No upstream test files are copied or run.

- Optional LyricLine.textAlign overrides paragraph and content alignment per line. Null preserves upstream behavior.
- LyricLayout shares that geometry between main/translation painting, word layouts, animated pivots and transformed click regions. Mio maps the shared core duet flag to absolute right alignment.
- Upstream timing, parsing, caches and progress ownership remain in place. Exact desktop motion remains pending.

- Optional LyricDocument.useLineIntervals enables independent explicit [start,end) activity in fixed scrollable views. A single focus still drives scrolling; visible rows retarget at their own boundaries, retain the live clock while singing and freeze their last word position when exiting. Other layouts and default documents retain upstream semantics.

- LyricWord.romanization carries real per-word annotation text. LyricRubyPainter reserves half-font bands beneath shaped rows; text, masks, boxes and metrics share row shifts without changing horizontal advances. Normal annotations fit their words and follow real timing; oversized split-token annotations use the desktop static line fallback. The Mio romanization switch supplies/removes annotations and suppresses duplicate line romanization when word data exists. This extension is verified only with the app Flutter SDK, not legacy Flutter versions.

- Optional LyricLine.background groups backing vocals with the preceding lead for scroll focus, motion delay and distance blur; each keeps its own timing, alignment and seek target. Main/auxiliary type is 70% size; main/ruby opacity is multiplied by 0.4. Fixed interval documents hide inactive backing rows while playing, removing height/spacing/hit/selection; paused/unsynced documents show all rows. Visibility prefixes rebuild only at transport/interval boundaries. Optional LyricPlaybackStatusSource distinguishes transport intent from bounded clock sampling. Existing progress sources and background=false documents retain default behavior. An initial background row may own focus; missing end uses focus fallback. Background spans extend occupied time but never become countdown targets.

- Backing-vocal height changes rebase the viewport and an in-flight scroll to keep the reference lead stable. Row transitions compensate actual native scroll correction plus any delayed/clamped travel displacement change. Pure visibility changes no longer start another follow animation; actual lead/countdown changes retain their normal follow path.
