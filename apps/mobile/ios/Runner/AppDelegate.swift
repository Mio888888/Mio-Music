import Flutter
import UIKit
import ImageIO
import AVFAudio
import MediaPlayer
import UniformTypeIdentifiers

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  private var hostChannel: FlutterMethodChannel?
  private var deviceChannel: FlutterMethodChannel?
  private var deviceRegistrar: FlutterPluginRegistrar?
  private var mediaHost: MioMediaHost?
  private var microphonePlugin: MioMicrophonePlugin?
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func orientationDidChange() {
    deviceChannel?.invokeMethod("orientationChanged", arguments: nil)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    if let device = engineBridge.pluginRegistry.registrar(forPlugin: "MioDevice") {
      deviceRegistrar = device
      let channel = FlutterMethodChannel(name: "mio/device", binaryMessenger: device.messenger())
      deviceChannel = channel
      channel.setMethodCallHandler { [weak self] call, result in
        guard call.method == "portraitOnly" else { result(FlutterMethodNotImplemented); return }
        guard let scene = self?.deviceRegistrar?.viewController?.viewIfLoaded?.window?.windowScene else {
          result(nil); return
        }
        result(SceneDelegate.portraitOnly(in: scene))
      }
    }
    if let files = engineBridge.pluginRegistry.registrar(forPlugin: "MioFilePicker") {
      MioFilePicker.register(with: files)
    }
    if let downloads = engineBridge.pluginRegistry.registrar(forPlugin: "MioDownloadFiles") {
      MioDownloadFiles.register(with: downloads)
    }
    guard let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "MioHost") else { return }
    let channel = FlutterMethodChannel(name: "mio/host", binaryMessenger: registrar.messenger())
    hostChannel = channel
    let microphone = FlutterMethodChannel(name: "mio/microphone", binaryMessenger: registrar.messenger())
    let microphonePlugin = MioMicrophonePlugin()
    self.microphonePlugin = microphonePlugin
    registrar.addMethodCallDelegate(microphonePlugin, channel: microphone)
    registrar.publish(microphonePlugin)
    if let host = mediaHost { microphonePlugin.attach(host) }
    channel.setMethodCallHandler { [weak self] call, result in
      if call.method == "lyricOverlayState" {
        if #available(iOS 16.2, *) { result(MioLyricsActivityHost.shared.state()) }
        else { result(["supported": false, "permission": false, "enabled": false, "locked": true, "scale": 1.0]) }
        return
      }
      if call.method == "configureLyricOverlay" {
        guard let args = call.arguments as? [String: Any], let enabled = args["enabled"] as? Bool,
          UIApplication.shared.applicationState == .active else {
          result(FlutterError(code: "activity_changed", message: "Open the app to change Live Activities", details: nil)); return
        }
        if #available(iOS 16.2, *) {
          do { try MioLyricsActivityHost.shared.configure(enabled); result(MioLyricsActivityHost.shared.state()) }
          catch { result(FlutterError(code: "activity_unavailable", message: error.localizedDescription, details: nil)) }
        } else { result(FlutterError(code: "activity_unavailable", message: "Requires iOS 16.2 or iPadOS 17", details: nil)) }
        return
      }
      if call.method == "requestLyricOverlayPermission" {
        if let url = URL(string: UIApplication.openSettingsURLString) {
          UIApplication.shared.open(url, options: [:]) { opened in result(opened) }
        } else { result(false) }
        return
      }
      if call.method == "audioOutputInfo" {
        result(AVAudioSession.sharedInstance().currentRoute.outputs.map { $0.portName }.joined(separator: " · ")); return
      }
      if call.method == "appVersion" {
        guard let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
          !version.isEmpty else {
          result(FlutterError(code: "invalid_version", message: "Package version unavailable", details: nil))
          return
        }
        result(version)
        return
      }
      if call.method == "openAboutLink" {
        let links = ["original": "https://github.com/timeshiftsauce/CeruMusic", "docs": "https://ceru.docs.shiqianjiang.cn/", "eqVault": "https://mio888888.github.io/EQVault/"]
        guard let key = call.arguments as? String, let address = links[key], let url = URL(string: address),
          UIApplication.shared.applicationState == .active else {
          result(FlutterError(code: "link_invalid", message: "About link unavailable", details: nil)); return
        }
        UIApplication.shared.open(url, options: [:]) { opened in
          if opened { result(nil) } else { result(FlutterError(code: "open_failed", message: "No browser could open the link", details: nil)) }
        }
        return
      }
      if call.method == "openBackupProvider" {
        let providers = ["hi168": "https://www.hi168.com", "cstcloud": "https://data.cstcloud.cn/"]
        guard let provider = call.arguments as? String,
          let address = providers[provider], let url = URL(string: address),
          self != nil, UIApplication.shared.applicationState == .active else {
          result(FlutterError(code: "provider_link_invalid", message: "Provider page unavailable", details: nil))
          return
        }
        UIApplication.shared.open(url, options: [:]) { opened in
          if opened { result(nil) }
          else { result(FlutterError(code: "provider_open_failed", message: "Provider page unavailable", details: nil)) }
        }
        return
      }
      if call.method == "openRelease" {
        guard let tag = call.arguments as? String,
          !tag.isEmpty, tag.utf8.count <= 128,
          tag.rangeOfCharacter(from: CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789.+-").inverted) == nil,
          UIApplication.shared.applicationState == .active,
          let base = URL(string: "https://github.com/Mio888888/Mio-Music/releases/tag/") else {
          result(FlutterError(code: "release_link_invalid", message: "Release page unavailable", details: nil))
          return
        }
        UIApplication.shared.open(base.appendingPathComponent(tag), options: [:]) { opened in
          if opened { result(nil) }
          else { result(FlutterError(code: "release_open_failed", message: "No browser could open the release page", details: nil)) }
        }
        return
      }
      guard call.method == "initializeAudioContext" else {
        result(FlutterMethodNotImplemented)
        return
      }
      do {
        guard let self = self, let pointers = call.arguments as? [String: NSNumber] else {
          throw MioMediaHost.HostError.invalidBinding
        }
        if self.mediaHost == nil { self.mediaHost = try MioMediaHost(pointers: pointers) }
        if let host = self.mediaHost { self.microphonePlugin?.attach(host) }
        result(nil)
      } catch {
        result(FlutterError(code: "audio_session_failed", message: "Audio session unavailable", details: nil))
      }
    }
  }
}

// Engine detach revokes only this engine's authorization. Rust still owns input teardown.
private final class MioMicrophonePlugin: NSObject, FlutterPlugin {
  private let owner = UUID()
  private weak var host: MioMediaHost?
  private var detached = false
  static func register(with registrar: FlutterPluginRegistrar) {
    let instance = MioMicrophonePlugin()
    let channel = FlutterMethodChannel(name: "mio/microphone", binaryMessenger: registrar.messenger())
    registrar.addMethodCallDelegate(instance, channel: channel); registrar.publish(instance)
  }
  func attach(_ host: MioMediaHost) {
    guard !detached else { return }
    self.host = host; host.attachMicrophone(owner)
  }
  func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    guard !detached, let host = host else {
      result(FlutterError(code: "microphone_unavailable", message: "Audio host unavailable", details: nil)); return
    }
    host.microphoneMethod(owner, call, result)
  }
  func detachFromEngine(for registrar: FlutterPluginRegistrar) {
    detached = true; host?.detachMicrophone(owner); host = nil
  }
}

// The function pointers refer to the exact Rust image already loaded by FRB.
// No Flutter channel, second player or Dart timer handles background controls.
private final class MioMediaHost {
  enum HostError: Error { case invalidBinding }
  typealias Snapshot = @convention(c) (UInt64, UInt8) -> UnsafeMutablePointer<CChar>?
  typealias Free = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void
  typealias Command = @convention(c) (UInt64, UInt32, Double) -> Int32
  typealias Prepare = @convention(c) (UInt64) -> Int32
  typealias Register = @convention(c) (Prepare?) -> Int32
  typealias Microphone = @convention(c) (UInt64, UInt64, UInt32) -> Int32
  typealias RegisterMicrophone = @convention(c) (Microphone?) -> Int32
  typealias CancelMicrophone = @convention(c) (UInt64) -> Void
  private static let bindingLock = NSLock()
  private static weak var current: MioMediaHost?
  private static let prepareCallback: Prepare = { id in MioMediaHost.prepareFromRust(id) }
  private static let microphoneCallback: Microphone = { id, token, operation in
    MioMediaHost.microphoneFromRust(id, token, operation)
  }
  private let cancelMicrophone: CancelMicrophone
  private let snapshot: Snapshot
  private let free: Free
  private let command: Command
  private let commands = DispatchQueue(label: "mio.media.commands")
  private let stateLock = NSLock()
  private var pendingCommands = 0
  private var runtimeId: UInt64 = 0
  private var observationPending = false // stateLock; bounds background decode + main apply
  private var artworkRevision: UInt64 = 0 // protected by stateLock; acknowledged after main-thread apply
  private var artwork: MPMediaItemArtwork? // main-thread only
  private var newestRuntimeId: UInt64 = 0 // main-thread only; rejects late snapshots after clearing
  private var timer: DispatchSourceTimer?
  private var observers: [NSObjectProtocol] = []
  private var targets: [(MPRemoteCommand, Any)] = []
  private var preparingUntil = Date.distantPast
  private var interrupted = false
  private var sessionActive = false
  // Main-thread only. The Rust stream owns the active reservation until finish.
  private var microphoneOwner: UUID?
  private var microphoneIssued: UInt64 = 0
  private var microphoneAuthorized = false
  private var microphoneActive: UInt64 = 0
  private var microphoneForeground = UIApplication.shared.applicationState == .active
  private struct AudioState {
    let category: AVAudioSession.Category
    let mode: AVAudioSession.Mode
    let options: AVAudioSession.CategoryOptions
    let policy: AVAudioSession.RouteSharingPolicy
    let active: Bool
  }
  private var microphoneSaved: AudioState?
  private final class PermissionRequest {
    let token: UInt64
    let result: FlutterResult
    var granted = false
    var timeout: DispatchWorkItem?
    init(_ token: UInt64, _ result: @escaping FlutterResult) { self.token = token; self.result = result }
  }
  private var permission: PermissionRequest?
  init(pointers: [String: NSNumber]) throws {
    func pointer(_ key: String) throws -> UnsafeRawPointer {
      guard let address = pointers[key]?.uint64Value, address != 0,
        let pointer = UnsafeRawPointer(bitPattern: UInt(address)) else { throw HostError.invalidBinding }
      return pointer
    }
    snapshot = unsafeBitCast(try pointer("snapshot"), to: Snapshot.self)
    free = unsafeBitCast(try pointer("free"), to: Free.self)
    command = unsafeBitCast(try pointer("command"), to: Command.self)
    let register = unsafeBitCast(try pointer("register_prepare"), to: Register.self)
    let registerMicrophone = unsafeBitCast(try pointer("register_microphone"), to: RegisterMicrophone.self)
    cancelMicrophone = unsafeBitCast(try pointer("cancel_microphone"), to: CancelMicrophone.self)
    try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
    Self.bindingLock.lock(); Self.current = self; Self.bindingLock.unlock()
    guard register(Self.prepareCallback) != 0, registerMicrophone(Self.microphoneCallback) != 0 else { throw HostError.invalidBinding }
    configureCommands()
    let center = NotificationCenter.default
    observers.append(center.addObserver(forName: UIApplication.willResignActiveNotification, object: nil, queue: .main) { [weak self] _ in
      self?.revokeMicrophone(leavingForeground: true)
    })
    observers.append(center.addObserver(forName: UIApplication.didEnterBackgroundNotification, object: nil, queue: .main) { [weak self] _ in
      self?.failPermission("cancelled")
    })
    observers.append(center.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
      self?.microphoneForeground = true; self?.deliverPermission()
    })
    observers.append(center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] note in
      guard let self = self, let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
        let type = AVAudioSession.InterruptionType(rawValue: raw) else { return }
      if type == .began { self.revokeMicrophone(); self.interrupted = true; self.sessionActive = false; self.event(8) }
      else {
        self.interrupted = false
        let options = AVAudioSession.InterruptionOptions(rawValue: note.userInfo?[AVAudioSessionInterruptionOptionKey] as? UInt ?? 0)
        if options.contains(.shouldResume) { self.preparingUntil = Date().addingTimeInterval(3.5); self.event(10) }
        else { self.event(9) }
      }
    })
    observers.append(center.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { [weak self] note in
      let reason = AVAudioSession.RouteChangeReason(rawValue: note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt ?? 0)
      if reason == .oldDeviceUnavailable { self?.revokeMicrophone(); self?.event(11) }
    })
    for name in [AVAudioSession.mediaServicesWereLostNotification, AVAudioSession.mediaServicesWereResetNotification] {
      observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
        self?.revokeMicrophone(); self?.sessionActive = false; self?.event(9)
      })
    }
    let timer = DispatchSource.makeTimerSource(queue: DispatchQueue(label: "mio.media.observer"))
    timer.schedule(deadline: .now() + 0.25, repeating: 0.5)
    timer.setEventHandler { [weak self] in
      guard let self = self else { return }
      self.stateLock.lock()
      if self.observationPending { self.stateLock.unlock(); return }
      self.observationPending = true
      let observedId = self.runtimeId
      let seenArtwork = self.artworkRevision
      self.stateLock.unlock()
      guard let raw = self.snapshot(seenArtwork, UserDefaults.standard.bool(forKey: "mio.lyricsActivity") ? 1 : 0) else {
        DispatchQueue.main.async { [weak self] in
          guard let self = self else { return }
          defer { self.finishObservation() }
          guard self.currentId() == observedId else { return }
          self.clearPlayback()
        }
        return
      }
      let data = Data(String(cString: raw).utf8)
      self.free(raw)
      guard let value = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { self.finishObservation(); return }
      let revision = (value["artwork_revision"] as? NSNumber)?.uint64Value ?? 0
      let image = revision != seenArtwork ? Self.decodeArtwork(value["artwork_png"] as? String) : nil
      DispatchQueue.main.async { [weak self] in
        guard let self = self else { return }
        defer { self.finishObservation() }
        self.update(value, image)
      }
    }
    self.timer = timer; timer.resume()
  }
  private final class Preparation {
    let lock = NSLock()
    let ready = DispatchSemaphore(value: 0)
    var expired = false
    var granted = false
    var completed = false
  }
  private static func prepareFromRust(_ id: UInt64) -> Int32 {
    guard !Thread.isMainThread else { return 0 }
    bindingLock.lock(); let host = current; bindingLock.unlock()
    guard let host = host else { return 0 }
    let pending = Preparation()
    DispatchQueue.main.async {
      pending.lock.lock(); defer { pending.lock.unlock(); pending.ready.signal() }
      if !pending.expired { pending.granted = host.activate(id) }
    }
    if pending.ready.wait(timeout: .now() + 3) == .timedOut {
      pending.lock.lock(); pending.expired = true; pending.lock.unlock(); return 0
    }
    pending.lock.lock(); defer { pending.lock.unlock() }
    return pending.granted ? 1 : 0
  }
  private func activate(_ id: UInt64, preserveConfiguration: Bool = false) -> Bool {
    do {
      if interrupted || microphoneActive != 0 { return false }
      if microphoneSaved != nil && !restoreMicrophone(reactivate: false) { return false }
      let audio = AVAudioSession.sharedInstance()
      if !preserveConfiguration { try audio.setCategory(.playback, mode: .default) }
      if !sessionActive { try audio.setActive(true); sessionActive = true }
      stateLock.lock(); runtimeId = id; stateLock.unlock()
      newestRuntimeId = max(newestRuntimeId, id)
      preparingUntil = Date().addingTimeInterval(3.5)
      return true
    } catch { return false }
  }
  func attachMicrophone(_ owner: UUID) {
    if microphoneOwner == owner { return }
    revokeMicrophone(leavingForeground: true); failPermission("cancelled")
    microphoneOwner = owner
    microphoneForeground = UIApplication.shared.applicationState == .active
  }
  func detachMicrophone(_ owner: UUID) {
    guard microphoneOwner == owner else { return }
    revokeMicrophone(leavingForeground: true); failPermission("cancelled")
    microphoneOwner = nil
  }
  func microphoneMethod(_ owner: UUID, _ call: FlutterMethodCall, _ result: @escaping FlutterResult) {
    guard microphoneOwner == owner else {
      result(FlutterError(code: "microphone_unavailable", message: "Audio host detached", details: nil)); return
    }
    switch call.method {
    case "available": result(AVAudioSession.sharedInstance().isInputAvailable)
    case "request":
      guard let number = call.arguments as? NSNumber, number.int64Value > 0,
        microphoneForeground, UIApplication.shared.applicationState == .active,
        !interrupted, microphoneActive == 0, microphoneIssued == 0, permission == nil else {
        result(FlutterError(code: "microphone_busy", message: "Microphone unavailable", details: nil)); return
      }
      let request = PermissionRequest(number.uint64Value, result)
      permission = request
      let timeout = DispatchWorkItem { [weak self, weak request] in
        guard let self = self, let request = request, self.permission === request else { return }
        self.failPermission("microphone_timeout")
      }
      request.timeout = timeout
      DispatchQueue.main.asyncAfter(deadline: .now() + 60, execute: timeout)
      AVAudioSession.sharedInstance().requestRecordPermission { [weak self, weak request] granted in
        DispatchQueue.main.async {
          guard let self = self, let request = request, self.permission === request else { return }
          if !granted { self.failPermission("microphone_denied"); return }
          request.granted = true; self.deliverPermission()
        }
      }
    case "release":
      if let token = (call.arguments as? NSNumber)?.uint64Value, token > 0 {
        if permission?.token == token { failPermission("cancelled") }
        if microphoneIssued == token {
          cancelMicrophone(token); microphoneIssued = 0; microphoneAuthorized = false
        }
      }
      result(nil)
    default: result(FlutterMethodNotImplemented)
    }
  }
  private func failPermission(_ code: String) {
    guard let pending = permission else { return }
    permission = nil; pending.timeout?.cancel()
    pending.result(FlutterError(code: code, message: "Microphone permission unavailable", details: nil))
  }
  private func deliverPermission() {
    guard let pending = permission, pending.granted, microphoneForeground,
      UIApplication.shared.applicationState == .active else { return }
    guard microphoneActive == 0, microphoneIssued == 0 else { failPermission("microphone_busy"); return }
    permission = nil; pending.timeout?.cancel()
    microphoneIssued = pending.token; microphoneAuthorized = true
    pending.result(nil)
  }
  private func revokeMicrophone(leavingForeground: Bool = false) {
    // Serialize revocation with Rust's short final resume gate before marking background.
    let token = microphoneIssued != 0 ? microphoneIssued : microphoneActive
    if token != 0 { cancelMicrophone(token) }
    if leavingForeground { microphoneForeground = false }
    microphoneIssued = 0; microphoneAuthorized = false
  }
  private static func microphoneFromRust(_ id: UInt64, _ token: UInt64, _ operation: UInt32) -> Int32 {
    guard !Thread.isMainThread else { return 0 }
    bindingLock.lock(); let host = current; bindingLock.unlock()
    guard let host = host else { return 0 }
    let pending = Preparation()
    DispatchQueue.main.async {
      pending.lock.lock(); defer { pending.lock.unlock(); pending.ready.signal() }
      // Finish must still release the reservation if the main queue was delayed.
      if !pending.expired || operation == 3 {
        pending.granted = host.microphoneOperation(id, token, operation, !pending.expired)
        pending.completed = true
      }
    }
    if pending.ready.wait(timeout: .now() + 3) == .timedOut {
      pending.lock.lock(); defer { pending.lock.unlock() }
      if pending.completed { return pending.granted ? 1 : 0 }
      pending.expired = true; return 0
    }
    pending.lock.lock(); defer { pending.lock.unlock() }
    return pending.granted ? 1 : 0
  }
  private func microphoneOperation(_ id: UInt64, _ token: UInt64, _ operation: UInt32, _ timely: Bool) -> Bool {
    let audio = AVAudioSession.sharedInstance()
    switch operation {
    case 1:
      guard token != 0, token == microphoneIssued, microphoneAuthorized,
        microphoneForeground, UIApplication.shared.applicationState == .active,
        !interrupted, microphoneActive == 0, microphoneSaved == nil,
        audio.recordPermission == .granted else { return false }
      microphoneSaved = AudioState(category: audio.category, mode: audio.mode, options: audio.categoryOptions,
        policy: audio.routeSharingPolicy, active: sessionActive)
      microphoneAuthorized = false; microphoneActive = token
      return true
    case 2:
      guard token == microphoneActive, token == microphoneIssued, microphoneForeground,
        UIApplication.shared.applicationState == .active, !interrupted else { return false }
      do {
        try audio.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker])
        try audio.setActive(true); sessionActive = true
        return true
      } catch { return false }
    case 3:
      guard token == microphoneActive else { return microphoneActive == 0 && microphoneSaved == nil }
      let restored = restoreMicrophone(reactivate: timely && token == microphoneIssued && microphoneForeground && !interrupted)
      microphoneActive = 0
      return restored
    case 4:
      guard token != 0, token == microphoneIssued, !microphoneAuthorized,
        microphoneActive == 0, microphoneSaved == nil, microphoneForeground,
        UIApplication.shared.applicationState == .active else { return false }
      return activate(id, preserveConfiguration: true)
    default: return false
    }
  }
  private func restoreMicrophone(reactivate: Bool) -> Bool {
    guard let saved = microphoneSaved else { return true }
    let audio = AVAudioSession.sharedInstance()
    do {
      if sessionActive { try audio.setActive(false, options: .notifyOthersOnDeactivation); sessionActive = false }
      try audio.setCategory(saved.category, mode: saved.mode, policy: saved.policy, options: saved.options)
      if reactivate && saved.active { try audio.setActive(true); sessionActive = true }
      microphoneSaved = nil
      return true
    } catch { return false }
  }
  private func currentId() -> UInt64 { stateLock.lock(); defer { stateLock.unlock() }; return runtimeId }
  private func event(_ event: UInt32) { let id = currentId(); if id != 0 { _ = command(id, event, 0) } }
  private func submit(_ action: UInt32, _ value: Double = 0) -> MPRemoteCommandHandlerStatus {
    stateLock.lock()
    let id = runtimeId
    if id == 0 || pendingCommands >= 8 { stateLock.unlock(); return .commandFailed }
    pendingCommands += 1; stateLock.unlock()
    commands.async { [weak self] in
      guard let self = self else { return }
      _ = self.command(id, action, value)
      self.stateLock.lock(); self.pendingCommands -= 1; self.stateLock.unlock()
    }
    return .success
  }
  private func configureCommands() {
    let center = MPRemoteCommandCenter.shared()
    func add(_ command: MPRemoteCommand, _ action: UInt32) {
      command.isEnabled = true
      let target = command.addTarget { [weak self] _ in self?.submit(action) ?? .commandFailed }
      targets.append((command, target))
    }
    add(center.playCommand, 1); add(center.pauseCommand, 2); add(center.togglePlayPauseCommand, 7)
    add(center.nextTrackCommand, 3); add(center.previousTrackCommand, 4); add(center.stopCommand, 5)
    center.changePlaybackPositionCommand.isEnabled = true
    targets.append((center.changePlaybackPositionCommand, center.changePlaybackPositionCommand.addTarget { [weak self] event in
      guard let event = event as? MPChangePlaybackPositionCommandEvent else { return .commandFailed }
      return self?.submit(6, event.positionTime) ?? .commandFailed
    }))
  }
  private func finishObservation() {
    stateLock.lock(); observationPending = false; stateLock.unlock()
  }
  private static func decodeArtwork(_ encoded: String?) -> UIImage? {
    guard let encoded = encoded, encoded.utf8.count <= 700_000,
      let data = Data(base64Encoded: encoded), data.count <= 512 * 1024,
      let source = CGImageSourceCreateWithData(data as CFData, nil),
      let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
      let width = properties[kCGImagePropertyPixelWidth] as? Int,
      let height = properties[kCGImagePropertyPixelHeight] as? Int,
      (1...256).contains(width), (1...256).contains(height),
      CGImageSourceGetType(source) as String? == UTType.png.identifier,
      let image = CGImageSourceCreateImageAtIndex(source, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary)
      else { return nil }
    return UIImage(cgImage: image)
  }
  private func update(_ value: [String: Any], _ image: UIImage?) {
    guard let id = (value["runtime_id"] as? NSNumber)?.uint64Value else { return }
    stateLock.lock(); let activeId = runtimeId; stateLock.unlock()
    if id < newestRuntimeId { return }
    newestRuntimeId = id
    if activeId != 0 && activeId != id { clearPlayback() }
    let incomingArtwork = (value["artwork_revision"] as? NSNumber)?.uint64Value ?? 0
    stateLock.lock()
    if incomingArtwork < artworkRevision { stateLock.unlock(); return }
    let changedArtwork = incomingArtwork != artworkRevision
    runtimeId = id
    artworkRevision = incomingArtwork
    stateLock.unlock()
    if changedArtwork {
      artwork = image.map { image in MPMediaItemArtwork(boundsSize: image.size) { _ in image } }
    }
    if #available(iOS 16.2, *) { MioLyricsActivityHost.shared.update(value) }
    let status = value["status"] as? String ?? "idle"
    let playing = status == "playing"
    if let track = value["current"] as? [String: Any] {
      var info: [String: Any] = [
        MPMediaItemPropertyTitle: track["title"] as? String ?? "Mio Music",
        MPMediaItemPropertyArtist: track["artist"] as? String ?? "",
        MPMediaItemPropertyAlbumTitle: track["album"] as? String ?? "",
        MPMediaItemPropertyPlaybackDuration: value["duration"] as? Double ?? 0,
        MPNowPlayingInfoPropertyElapsedPlaybackTime: value["position"] as? Double ?? 0,
        MPNowPlayingInfoPropertyPlaybackRate: playing ? 1.0 : 0.0,
      ]
      if let artwork = artwork { info[MPMediaItemPropertyArtwork] = artwork }
      MPNowPlayingInfoCenter.default().nowPlayingInfo = info
    } else { artwork = nil; MPNowPlayingInfoCenter.default().nowPlayingInfo = nil }
    if microphoneActive == 0 && microphoneSaved == nil && !playing && status != "loading" && !interrupted && Date() >= preparingUntil && sessionActive {
      do { try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation); sessionActive = false }
      catch { /* Keep activation state for a later retry. */ }
    }
  }
  private func clearPlayback() {
    if #available(iOS 16.2, *) { MioLyricsActivityHost.shared.end() }
    MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
    artwork = nil
    stateLock.lock(); runtimeId = 0; artworkRevision = 0; stateLock.unlock()
    preparingUntil = .distantPast
    if microphoneActive == 0 && microphoneSaved == nil && sessionActive {
      do { try AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation); sessionActive = false }
      catch { /* Retry on the next empty snapshot. */ }
    }
  }
  deinit {
    revokeMicrophone(leavingForeground: true); failPermission("cancelled")
    timer?.cancel()
    observers.forEach(NotificationCenter.default.removeObserver)
    targets.forEach { $0.0.removeTarget($0.1) }
  }
}

// Selected plugin bytes only; security-scoped URLs never cross into Dart/Rust.
private final class MioFilePicker: NSObject, FlutterPlugin, UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
  private static let maxBytes = 1024 * 1024
  private static let processID = UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased()
  private static let extensions: Set<String> = ["mp3", "flac", "wav", "ogg", "m4a", "aac", "wma", "opus", "ape", "alac"]
  private static let owner = "mio-audio-staging-v1"
  private var handedStages: Set<URL> = []
  private let reader = DispatchQueue(label: "mio.files.reader")
  private var picker: UIDocumentPickerViewController?
  private var pending: Request?
  private var workerBusy = false
  private final class Request {
    let lock = NSLock()
    let coordinator = NSFileCoordinator()
    private var cancelled = false
    var result: FlutterResult?
    var timer: Timer?
    let token: Int64
    let audio: Bool
    var singleAudio = false
    var playlist = false
    var queue = false
    var cover = false
    var exportDirectory: URL?
    init(_ token: Int64, _ audio: Bool, _ result: @escaping FlutterResult) { self.token = token; self.audio = audio; self.result = result }
    func cancel() { lock.lock(); cancelled = true; lock.unlock(); coordinator.cancel() }
    func isCancelled() -> Bool { lock.lock(); defer { lock.unlock() }; return cancelled }
  }
  static func register(with registrar: FlutterPluginRegistrar) {
    let channel = FlutterMethodChannel(name: "mio/files", binaryMessenger: registrar.messenger())
    let instance = MioFilePicker()
    registrar.addMethodCallDelegate(instance, channel: channel)
    registrar.publish(instance)
  }
  func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "pickPlugin", "pickAudio", "pickRecognitionAudio", "pickEq", "pickPlaylist", "pickQueue", "pickCover":
      guard let token = call.arguments as? NSNumber, token.int64Value > 0 else {
        result(FlutterError(code: "invalid_input", message: "Invalid picker request", details: nil)); return
      }
      pick(token.int64Value, call.method == "pickAudio" || call.method == "pickRecognitionAudio", result, singleAudio: call.method == "pickRecognitionAudio", playlist: call.method == "pickPlaylist", queue: call.method == "pickQueue", cover: call.method == "pickCover")
    case "saveQueue":
      guard let args = call.arguments as? [String: Any], let token = args["token"] as? NSNumber,
        token.int64Value > 0, let bytes = args["bytes"] as? FlutterStandardTypedData,
        !bytes.data.isEmpty, bytes.data.count <= 64 * 1024 * 1024,
        let name = args["name"] as? String,
        name.range(of: "^cerumusic-playlist-[0-9]{4}-[0-9]{2}-[0-9]{2}\\.cmpl$", options: .regularExpression) != nil else {
        result(FlutterError(code: "invalid_input", message: "Invalid export", details: nil)); return
      }
      exportDocument(token.int64Value, bytes.data, result, name: name)
    case "saveEq":
      guard let args = call.arguments as? [String: Any], let token = args["token"] as? NSNumber,
            token.int64Value > 0, let bytes = args["bytes"] as? FlutterStandardTypedData,
            !bytes.data.isEmpty, bytes.data.count <= 512 * 1024 else {
        result(FlutterError(code: "invalid_input", message: "Invalid export", details: nil)); return
      }
      exportDocument(token.int64Value, bytes.data, result)
    case "discardAudio":
      if let ticket = call.arguments as? String, let stage = handedStages.first(where: { $0.lastPathComponent == ticket }) {
        handedStages.remove(stage); Self.removeStage(stage)
      }
      result(nil)
    case "cancel":
      if (call.arguments as? NSNumber)?.int64Value == pending?.token { cancelPending() }
      result(nil)
    default: result(FlutterMethodNotImplemented)
    }
  }
  private func pick(_ token: Int64, _ audio: Bool, _ result: @escaping FlutterResult, singleAudio: Bool = false, playlist: Bool = false, queue: Bool = false, cover: Bool = false) {
    guard pending == nil && !workerBusy else {
      result(FlutterError(code: "picker_busy", message: "File picker busy", details: nil)); return
    }
    let window = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
      .filter { $0.activationState == .foregroundActive }.flatMap { $0.windows }.first { $0.isKeyWindow }
    guard var top = window?.rootViewController else {
      result(FlutterError(code: "picker_unavailable", message: "File picker unavailable", details: nil)); return
    }
    while let presented = top.presentedViewController { top = presented }
    guard !top.isBeingDismissed && !(top is UIAlertController) else {
      result(FlutterError(code: "picker_unavailable", message: "File picker unavailable", details: nil)); return
    }
    let request = Request(token, audio, result)
    request.singleAudio = singleAudio
    request.playlist = playlist
    request.queue = queue
    request.cover = cover
    let types: [UTType] = cover ? ["png", "jpg", "gif", "webp"].compactMap { UTType(filenameExtension: $0) } : [.data]
    let controller = UIDocumentPickerViewController(forOpeningContentTypes: types, asCopy: false)
    controller.allowsMultipleSelection = audio && !singleAudio; controller.delegate = self
    picker = controller; pending = request
    top.present(controller, animated: true)
    controller.presentationController?.delegate = self
  }
  private func exportDocument(_ token: Int64, _ bytes: Data, _ result: @escaping FlutterResult, name: String = "mio-eq-config.json") {
    guard pending == nil && !workerBusy else {
      result(FlutterError(code: "picker_busy", message: "File picker busy", details: nil)); return
    }
    let request = Request(token, false, result)
    pending = request; workerBusy = true
    reader.async { [weak self] in
      let directory = FileManager.default.temporaryDirectory.appendingPathComponent("mio-eq-" + UUID().uuidString, isDirectory: true)
      let file = directory.appendingPathComponent(name)
      var failed = false
      do {
        guard !request.isCancelled() else { throw AudioFailure.unreadable }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        try bytes.write(to: file, options: .atomic)
      } catch { failed = true }
      DispatchQueue.main.async { [weak self] in
        guard let self = self else { try? FileManager.default.removeItem(at: directory); return }
        self.workerBusy = false
        guard self.pending === request && !request.isCancelled() else {
          try? FileManager.default.removeItem(at: directory); return
        }
        request.exportDirectory = directory
        if failed { self.finish(request, error: "file_unwritable"); return }
        let window = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
          .filter { $0.activationState == .foregroundActive }.flatMap { $0.windows }.first { $0.isKeyWindow }
        guard var top = window?.rootViewController else { self.finish(request, error: "picker_unavailable"); return }
        while let presented = top.presentedViewController { top = presented }
        guard !top.isBeingDismissed && !(top is UIAlertController) else { self.finish(request, error: "picker_unavailable"); return }
        let controller = UIDocumentPickerViewController(forExporting: [file], asCopy: true)
        controller.delegate = self; self.picker = controller
        top.present(controller, animated: true)
        controller.presentationController?.delegate = self
      }
    }
  }
  func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
    if picker === controller { cancelPending() }
  }
  func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
    if picker === presentationController.presentedViewController { cancelPending() }
  }
  func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
    guard picker === controller, let request = pending, !workerBusy else { return }
    if request.exportDirectory != nil {
      if urls.count == 1 { finish(request, saved: true) }
      else { finish(request, error: "file_unwritable") }
      return
    }
    if request.audio { readAudio(request, urls); return }
    guard urls.count == 1, let url = urls.first, url.isFileURL else {
      finish(request, error: "file_unreadable"); return
    }
    if request.playlist || request.queue || request.cover {
      let name = url.lastPathComponent
      guard !name.isEmpty, name.utf8.count <= 1024,
        !name.contains("/"), !name.contains("\\"),
        !name.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }),
        (request.cover ? ["png", "jpg", "jpeg", "gif", "webp"] : (request.queue ? ["cpl", "cmpl"] : ["json", "cpl", "cmpl"])).contains(url.pathExtension.lowercased()) else {
        finish(request, error: "file_unreadable"); return
      }
    }
    workerBusy = true
    request.timer = Timer.scheduledTimer(withTimeInterval: 30, repeats: false) { [weak self, weak request] _ in
      guard let request = request else { return }; self?.finish(request, error: "file_timeout")
    }
    reader.async { [weak self] in
      let scoped = url.startAccessingSecurityScopedResource()
      defer { if scoped { url.stopAccessingSecurityScopedResource() } }
      var bytes: Data?
      var failure: String?
      var coordinationError: NSError?
      request.coordinator.coordinate(readingItemAt: url, options: .withoutChanges, error: &coordinationError) { local in
        guard !request.isCancelled(), let input = InputStream(url: local) else { failure = "file_unreadable"; return }
        input.open(); defer { input.close() }
        var data = Data(); var buffer = [UInt8](repeating: 0, count: 8192)
        while !request.isCancelled() {
          let maxBytes = request.queue ? 64 * 1024 * 1024 : (request.playlist ? 32 * 1024 * 1024 : (request.cover ? 5 * 1024 * 1024 : Self.maxBytes))
          let limit = min(buffer.count, maxBytes + 1 - data.count)
          let count = input.read(&buffer, maxLength: limit)
          if count < 0 { failure = "file_unreadable"; break }
          if count == 0 { break }
          data.append(contentsOf: buffer.prefix(count))
          if data.count > maxBytes { failure = "file_too_large"; break }
        }
        if data.isEmpty || request.isCancelled() { failure = "file_unreadable" }
        if failure == nil { bytes = data }
      }
      if coordinationError != nil || bytes == nil { failure = failure ?? "file_unreadable" }
      DispatchQueue.main.async { [weak self] in
        guard let self = self else { return }; self.workerBusy = false
        self.finish(request, data: bytes, error: failure, fileName: (request.playlist || request.queue || request.cover) ? url.lastPathComponent : nil)
      }
    }
  }
  private enum AudioFailure: Error { case unreadable, tooLarge }
  private static func removeStage(_ stage: URL) {
    let fm = FileManager.default
    let marker = stage.appendingPathComponent(".owner")
    guard (try? stage.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == false,
      let markerInfo = try? marker.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]),
      markerInfo.isRegularFile == true, markerInfo.isSymbolicLink == false, (markerInfo.fileSize ?? 65) <= 64,
      (try? String(contentsOf: marker, encoding: .utf8)) == owner,
      let children = try? fm.contentsOfDirectory(at: stage, includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey]) else { return }
    for child in children {
      guard let values = try? child.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]),
        values.isRegularFile == true, values.isSymbolicLink == false,
        child.lastPathComponent == ".owner" || child.lastPathComponent == "manifest.json" ||
          child.lastPathComponent.range(of: #"^[0-9]{1,2}\.[a-z0-9]{2,5}$"#, options: .regularExpression) != nil else { return }
    }
    do {
      for child in children where child.lastPathComponent != ".owner" { try fm.removeItem(at: child) }
      try fm.removeItem(at: stage.appendingPathComponent(".owner")); try fm.removeItem(at: stage)
    } catch {}
  }
  private static func copyAudio(_ urls: [URL], _ request: Request) throws -> URL {
    let fileLimit = (request.singleAudio ? 128 : 256) * 1024 * 1024
    let fm = FileManager.default
    guard let cache = fm.urls(for: .cachesDirectory, in: .userDomainMask).first else { throw AudioFailure.unreadable }
    let root = cache.appendingPathComponent("mio-audio-staging", isDirectory: true)
    try fm.createDirectory(at: root, withIntermediateDirectories: true)
    guard (try root.resourceValues(forKeys: [.isSymbolicLinkKey])).isSymbolicLink != true else { throw AudioFailure.unreadable }
    for old in (try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)) ?? [] {
      if !old.lastPathComponent.hasPrefix(processID + "-") && old.lastPathComponent.range(of: #"^[0-9a-f]{32}-[0-9a-f]{32}$"#, options: .regularExpression) != nil { removeStage(old) }
    }
    let stage = root.appendingPathComponent(processID + "-" + UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased(), isDirectory: true)
    try fm.createDirectory(at: stage, withIntermediateDirectories: false)
    try owner.write(to: stage.appendingPathComponent(".owner"), atomically: true, encoding: .utf8)
    var success = false
    defer { if !success { removeStage(stage) } }
    var files: [[String: String]] = []
    var total = 0
    for (index, url) in urls.enumerated() {
      guard !request.isCancelled() else { throw AudioFailure.unreadable }
      let scoped = url.startAccessingSecurityScopedResource()
      defer { if scoped { url.stopAccessingSecurityScopedResource() } }
      let ext = url.pathExtension.lowercased()
      guard extensions.contains(ext) else { throw AudioFailure.unreadable }
      let stem = String(String.UnicodeScalarView(url.deletingPathExtension().lastPathComponent.unicodeScalars.prefix(50)))
        .map { character -> Character in character == "/" || character == "\\" || character.unicodeScalars.contains(where: { $0.value < 32 }) ? "_" : character }
      let name = (stem.isEmpty ? "audio" : String(stem)) + "." + ext
      let relative = "\(index).\(ext)"
      var coordinationError: NSError?
      var readError: Error?
      var copied = false
      request.coordinator.coordinate(readingItemAt: url, options: .withoutChanges, error: &coordinationError) { local in
        do {
          guard !request.isCancelled(), let input = InputStream(url: local) else { throw AudioFailure.unreadable }
          let target = stage.appendingPathComponent(relative)
          guard fm.createFile(atPath: target.path, contents: nil) else { throw AudioFailure.unreadable }
          let output = try FileHandle(forWritingTo: target)
          defer { try? output.close() }
          input.open(); defer { input.close() }
          var buffer = [UInt8](repeating: 0, count: 64 * 1024)
          var size = 0
          while true {
            guard !request.isCancelled() else { throw AudioFailure.unreadable }
            let count = input.read(&buffer, maxLength: min(buffer.count, fileLimit + 1 - size, 512 * 1024 * 1024 + 1 - total))
            if count < 0 { throw AudioFailure.unreadable }
            if count == 0 { break }
            size += count; total += count
            if size > fileLimit || total > 512 * 1024 * 1024 { throw AudioFailure.tooLarge }
            try output.write(contentsOf: Data(buffer.prefix(count)))
          }
          guard size > 0 else { throw AudioFailure.unreadable }
          try output.synchronize(); copied = true
        } catch { readError = error }
      }
      if let readError = readError { throw readError }
      guard coordinationError == nil && copied else { throw AudioFailure.unreadable }
      files.append(["path": relative, "name": name])
    }
    guard !request.isCancelled() else { throw AudioFailure.unreadable }
    try JSONSerialization.data(withJSONObject: ["version": 1, "files": files]).write(to: stage.appendingPathComponent("manifest.json"), options: .atomic)
    success = true
    return stage
  }
  private func readAudio(_ request: Request, _ urls: [URL]) {
    if request.singleAudio && urls.count != 1 { finish(request, error: "file_unreadable"); return }
    guard !urls.isEmpty && urls.count <= 100 && urls.allSatisfy({ $0.isFileURL }) else {
      finish(request, error: "file_too_large"); return
    }
    workerBusy = true
    request.timer = Timer.scheduledTimer(withTimeInterval: 600, repeats: false) { [weak self, weak request] _ in
      if let request = request { self?.finish(request, error: "file_timeout") }
    }
    reader.async { [weak self] in
      var stage: URL?
      var failure: String?
      do { stage = try Self.copyAudio(urls, request) }
      catch AudioFailure.tooLarge { failure = "file_too_large" }
      catch { failure = "file_unreadable" }
      DispatchQueue.main.async { [weak self] in
        guard let self = self else { if let stage = stage { Self.removeStage(stage) }; return }
        self.workerBusy = false
        self.finish(request, error: failure, stage: stage)
      }
    }
  }
  private func finish(_ request: Request, data: Data? = nil, error: String? = nil, stage: URL? = nil, saved: Bool? = nil, fileName: String? = nil) {
    guard pending === request else { if let stage = stage { Self.removeStage(stage) }; return }
    if error == nil, let stage = stage { handedStages.insert(stage) }
    pending = nil; picker = nil
    request.timer?.invalidate(); request.timer = nil; request.cancel()
    if let directory = request.exportDirectory {
      request.exportDirectory = nil; try? FileManager.default.removeItem(at: directory)
    }
    let result = request.result; request.result = nil
    if let error = error { result?(FlutterError(code: error, message: "Selected files unavailable or exceed import limits", details: nil)) }
    else if let saved = saved { result?(saved) }
    else if let stage = stage { result?(stage.lastPathComponent) }
    else if let fileName = fileName, let data = data { result?(["name": fileName, "bytes": FlutterStandardTypedData(bytes: data)]) }
    else { result?(data.map { FlutterStandardTypedData(bytes: $0) }) }
  }
  private func cancelPending() {
    guard let request = pending else { return }
    picker?.dismiss(animated: true); finish(request)
  }
  func detachFromEngine(for registrar: FlutterPluginRegistrar) {
    cancelPending()
    // Delivered stages can outlive Flutter in a Rust worker. Rust or the next
    // process's owned-stage cleanup removes them after their reader is gone.
    handedStages.removeAll()
  }
}


// Only a basename is accepted. The original Downloads root is chosen by the host,
// never by a Dart path; the document UI owns any external handoff after admission.
private final class MioDownloadFiles: NSObject, FlutterPlugin, UIDocumentInteractionControllerDelegate {
  private var document: UIDocumentInteractionController?
  private var presenter: UIViewController?
  static func register(with registrar: FlutterPluginRegistrar) {
    let channel = FlutterMethodChannel(name: "mio/downloads", binaryMessenger: registrar.messenger())
    let instance = MioDownloadFiles()
    registrar.addMethodCallDelegate(instance, channel: channel)
    registrar.publish(instance)
  }
  func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    guard call.method == "openCompleted" else { result(FlutterMethodNotImplemented); return }
    guard let name = call.arguments as? String, !name.isEmpty, name.utf8.count <= 1024,
      name != ".", name != "..", !name.contains("/"), !name.contains("\\"), !name.contains(":"),
      !name.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
      result(FlutterError(code: "invalid_input", message: "Invalid download file", details: nil)); return
    }
    guard document == nil, UIApplication.shared.applicationState == .active else {
      result(FlutterError(code: "open_unavailable", message: "File opening unavailable", details: nil)); return
    }
    do {
      guard let base = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first else {
        throw CocoaError(.fileReadNoSuchFile)
      }
      let root = base.appendingPathComponent("Downloads", isDirectory: true)
      let file = root.appendingPathComponent(name, isDirectory: false)
      let rootInfo = try root.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
      let info = try file.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
      guard rootInfo.isDirectory == true, rootInfo.isSymbolicLink != true,
        info.isRegularFile == true, info.isSymbolicLink != true, (info.fileSize ?? 0) > 0,
        file.resolvingSymlinksInPath().deletingLastPathComponent().path == root.resolvingSymlinksInPath().path else {
        throw CocoaError(.fileReadNoPermission)
      }
      let extensions: Set<String> = ["mp3", "flac", "wav", "ogg", "m4a", "aac", "wma", "opus", "ape", "alac", "aiff", "aif"]
      guard extensions.contains(file.pathExtension.lowercased()) else {
        result(FlutterError(code: "unsupported_file", message: "Audio file type unsupported", details: nil)); return
      }
      let window = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        .filter { $0.activationState == .foregroundActive }.flatMap { $0.windows }.first { $0.isKeyWindow }
      guard var top = window?.rootViewController else {
        result(FlutterError(code: "open_unavailable", message: "File opening unavailable", details: nil)); return
      }
      while let presented = top.presentedViewController { top = presented }
      guard !top.isBeingDismissed && !(top is UIAlertController) else {
        result(FlutterError(code: "open_unavailable", message: "File opening unavailable", details: nil)); return
      }
      let controller = UIDocumentInteractionController(url: file)
      controller.delegate = self
      presenter = top; document = controller
      let bounds = top.view.bounds
      let anchor = CGRect(x: bounds.midX, y: bounds.midY, width: 1, height: 1)
      if controller.presentOpenInMenu(from: anchor, in: top.view, animated: true) || controller.presentPreview(animated: true) {
        result(nil) // Presentation/handoff only; not proof of external playback.
      } else {
        clear(controller)
        result(FlutterError(code: "no_handler", message: "No application can open this audio file", details: nil))
      }
    } catch {
      result(FlutterError(code: "file_unavailable", message: "Download file unavailable", details: nil))
    }
  }
  private func clear(_ controller: UIDocumentInteractionController) {
    if document === controller { document = nil; presenter = nil }
  }
  func documentInteractionControllerViewControllerForPreview(_ controller: UIDocumentInteractionController) -> UIViewController {
    return presenter ?? UIViewController()
  }
  func documentInteractionControllerDidEndPreview(_ controller: UIDocumentInteractionController) { clear(controller) }
  func documentInteractionControllerDidDismissOpenInMenu(_ controller: UIDocumentInteractionController) { clear(controller) }
  func documentInteractionController(_ controller: UIDocumentInteractionController, didEndSendingToApplication application: String?) { clear(controller) }
}
