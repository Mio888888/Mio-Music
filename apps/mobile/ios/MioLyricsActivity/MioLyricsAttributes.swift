import ActivityKit

@available(iOS 16.2, *)
struct MioLyricsAttributes: ActivityAttributes {
  struct ContentState: Codable, Hashable {
    var title: String
    var artist: String
    var lyric: String
    var playing: Bool
  }
  var runtime: String
}
