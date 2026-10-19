import ActivityKit
import UIKit

/// All entry points are called on main by the host channel/media observer.
/// One coalescing task prevents delayed lyric updates from reviving an ended activity.
@available(iOS 16.2, *)
final class MioLyricsActivityHost {
  static let shared = MioLyricsActivityHost()
  private var activity: Activity<MioLyricsAttributes>?
  private var pending: [String: Any]?
  private var worker: Task<Void, Never>?
  private var generation = 0
  private var endingIDs = Set<String>()
  private var suppressed = false
  private var lastState: MioLyricsAttributes.ContentState?
  private(set) var error: String?
  var enabled: Bool { UserDefaults.standard.bool(forKey: "mio.lyricsActivity") }
  var supported: Bool {
    if UIDevice.current.userInterfaceIdiom == .pad {
      if #available(iOS 17.0, *) { return true }
      return false
    }
    return true
  }
  func state() -> [String: Any] {
    ["supported": supported, "permission": ActivityAuthorizationInfo().areActivitiesEnabled,
     "enabled": enabled, "locked": true, "scale": 1.0,
     "active": activity?.activityState == .active, "error": error ?? ""]
  }
  func configure(_ enabled: Bool) throws {
    if enabled && (!supported || !ActivityAuthorizationInfo().areActivitiesEnabled) {
      throw NSError(domain: "LiveActivities", code: 1, userInfo: [NSLocalizedDescriptionKey: "Live Activities are unavailable or disabled in system settings"])
    }
    UserDefaults.standard.set(enabled, forKey: "mio.lyricsActivity")
    error = nil; suppressed = false
    if !enabled { end() }
  }
  func end() {
    generation += 1; pending = nil; lastState = nil
    var victims = Activity<MioLyricsAttributes>.activities
    if let active = activity, !victims.contains(where: { $0.id == active.id }) { victims.append(active) }
    activity = nil
    victims = victims.filter { !endingIDs.contains($0.id) }
    endingIDs.formUnion(victims.map { $0.id })
    Task { @MainActor in
      for item in victims {
        await item.end(nil, dismissalPolicy: .immediate)
        self.endingIDs.remove(item.id)
      }
    }
  }

  func update(_ snapshot: [String: Any]) {
    guard enabled && supported else { return }
    guard snapshot["current"] is [String: Any],
      !["idle", "failed", "ended"].contains(snapshot["status"] as? String ?? "idle") else {
      if activity != nil { end() }
      return
    }
    pending = snapshot
    guard worker == nil else { return }
    worker = Task { @MainActor [weak self] in
      guard let self = self else { return }
      defer { self.worker = nil }
      while let snapshot = self.pending {
        self.pending = nil
        guard self.enabled && !self.suppressed, let track = snapshot["current"] as? [String: Any] else { continue }
        let runtime = String(describing: snapshot["runtime_id"] ?? "")
        let state = MioLyricsAttributes.ContentState(
          title: String((track["title"] as? String ?? "Mio Music").prefix(128)),
          artist: String((track["artist"] as? String ?? "").prefix(128)),
          lyric: String((snapshot["lyric"] as? String ?? "").prefix(256)),
          playing: snapshot["status"] as? String == "playing")
        let content = ActivityContent(state: state, staleDate: Date().addingTimeInterval(30))
        if let active = self.activity, active.activityState == .dismissed || active.activityState == .ended {
          self.activity = nil; self.suppressed = true; continue
        }
        if let active = self.activity, active.attributes.runtime != runtime {
          self.activity = nil; self.lastState = nil
          let epoch = self.generation
          self.endingIDs.insert(active.id)
          await active.end(nil, dismissalPolicy: .immediate)
          self.endingIDs.remove(active.id)
          guard self.enabled && epoch == self.generation else { continue }
        }
        if self.activity == nil {
          guard UIApplication.shared.applicationState == .active else { continue }
          do {
            // Adopt this runtime's activity after a Flutter reconnect; no duplicates.
            if let existing = Activity<MioLyricsAttributes>.activities.first(where: { $0.attributes.runtime == runtime && $0.activityState == .active && !self.endingIDs.contains($0.id) }) {
              self.activity = existing
            } else {
              self.activity = try Activity.request(attributes: MioLyricsAttributes(runtime: runtime), content: content, pushType: nil)
            }
            self.lastState = nil; self.error = nil
          } catch { self.error = error.localizedDescription; self.suppressed = true; continue }
        }
        if state != self.lastState, let active = self.activity {
          self.lastState = state
          await active.update(content)
        }
      }
    }
  }
}
