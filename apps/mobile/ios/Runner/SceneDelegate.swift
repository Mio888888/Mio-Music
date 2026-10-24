import Flutter
import UIKit

class SceneDelegate: FlutterSceneDelegate {
  static func portraitOnly(in scene: UIWindowScene) -> Bool {
    let size = scene.screen.fixedCoordinateSpace.bounds.size
    return UIDevice.current.userInterfaceIdiom == .phone && min(size.width, size.height) < 600
  }

  @available(iOS 26.0, *)
  @objc(windowScene:didUpdateEffectiveGeometry:)
  func mioWindowScene(_ windowScene: UIWindowScene, didUpdateEffectiveGeometry previousEffectiveGeometry: UIWindowScene.Geometry) {
    (UIApplication.shared.delegate as? AppDelegate)?.orientationDidChange()
  }

  @available(iOS 27.0, *)
  @objc(supportedInterfaceOrientationsForWindowScene:)
  func mioSupportedInterfaceOrientations(for windowScene: UIWindowScene) -> UIInterfaceOrientationMask {
    Self.portraitOnly(in: windowScene) ? .portrait : .all
  }
}
