import ActivityKit
import SwiftUI
import WidgetKit

@main
struct MioLyricsWidget: Widget {
  var body: some WidgetConfiguration {
    ActivityConfiguration(for: MioLyricsAttributes.self) { context in
      VStack(alignment: .leading, spacing: 8) {
        HStack {
          Image(systemName: context.state.playing ? "music.note" : "pause.fill")
          Text(context.state.title).font(.headline).lineLimit(1)
          Spacer()
          Text("Mio Music").font(.caption).foregroundStyle(.secondary)
        }
        Text(context.state.lyric.isEmpty ? context.state.artist : context.state.lyric)
          .font(.title3).lineLimit(3)
      }
      .padding(16)
      .activityBackgroundTint(Color.black.opacity(0.85))
      .activitySystemActionForegroundColor(.white)
      .foregroundStyle(.white)
    } dynamicIsland: { context in
      DynamicIsland {
        DynamicIslandExpandedRegion(.leading) {
          Image(systemName: context.state.playing ? "music.note" : "pause.fill")
        }
        DynamicIslandExpandedRegion(.trailing) { Text("Mio Music").font(.caption) }
        DynamicIslandExpandedRegion(.bottom) {
          VStack(alignment: .leading, spacing: 6) {
            Text(context.state.title).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            Text(context.state.lyric.isEmpty ? context.state.artist : context.state.lyric).lineLimit(3)
          }.frame(maxWidth: .infinity, alignment: .leading)
        }
      } compactLeading: {
        Image(systemName: context.state.playing ? "music.note" : "pause.fill")
      } compactTrailing: {
        Text(context.state.lyric.isEmpty ? context.state.title : context.state.lyric).lineLimit(1).frame(maxWidth: 90)
      } minimal: {
        Image(systemName: context.state.playing ? "music.note" : "pause.fill")
      }
    }
  }
}
