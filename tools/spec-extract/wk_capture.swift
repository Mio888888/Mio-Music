// Captures a page of the original frontend with WKWebView — the engine the original's Tauri
// window uses on macOS — and writes a screenshot plus the bounds of the elements asked for.
//
//   swiftc -O tools/spec-extract/wk_capture.swift -o /tmp/wk_capture
//   /tmp/wk_capture <url> <width> <height> <out-prefix> <theme: light|dark> <mock.js> <selectors.json> [wait-seconds]
//
// The Tauri bridge (`window.__TAURI_INTERNALS__`) is replaced by the script in <mock.js>, which
// answers every command with an empty value, so only screens that need no backend data are
// meaningful. Nothing is written but the two output files.
import AppKit
import WebKit

let args = CommandLine.arguments
guard args.count >= 8 else {
    FileHandle.standardError.write("usage: wk_capture url width height out-prefix theme mock.js selectors.json [wait]\n".data(using: .utf8)!)
    exit(2)
}
let url = URL(string: args[1])!
let width = CGFloat(Double(args[2])!)
let height = CGFloat(Double(args[3])!)
let prefix = args[4]
let theme = args[5]
let mock = try! String(contentsOfFile: args[6])
let selectors = try! String(contentsOfFile: args[7])
let wait = args.count > 8 ? Double(args[8])! : 4.0

final class Delegate: NSObject, WKNavigationDelegate {
    var finished: (() -> Void)?
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) { finished?() }
    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        FileHandle.standardError.write("navigation failed: \(error)\n".data(using: .utf8)!)
        exit(1)
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)

let config = WKWebViewConfiguration()
let controller = WKUserContentController()
controller.addUserScript(WKUserScript(source: mock, injectionTime: .atDocumentStart, forMainFrameOnly: true))
config.userContentController = controller

let frame = NSRect(x: 0, y: 0, width: width, height: height)
let window = NSWindow(contentRect: frame, styleMask: [.borderless], backing: .buffered, defer: false)
window.isOpaque = true
window.backgroundColor = .white
let webView = WKWebView(frame: frame, configuration: config)
window.contentView = webView
window.setFrameOrigin(NSPoint(x: 40, y: 40))
window.orderFrontRegardless()
let delegate = Delegate()
webView.navigationDelegate = delegate

let extract = """
(function(){
  const selectors = \(selectors);
  const props = ['color','backgroundColor','backgroundImage','fontFamily','fontSize','fontWeight','lineHeight','borderRadius','borderColor','boxShadow'];
  const elements = {};
  for (const [id, selector] of Object.entries(selectors)) {
    const node = document.querySelector(selector);
    if (!node) { elements[id] = null; continue; }
    const r = node.getBoundingClientRect(), s = getComputedStyle(node);
    const style = {}; for (const p of props) style[p] = s[p];
    elements[id] = {selector, bounds: {x: r.x, y: r.y, width: r.width, height: r.height}, style};
  }
  return JSON.stringify({route: location.hash, theme: document.documentElement.dataset.theme || 'light',
    variant: 'wkwebview', viewport: {width: innerWidth, height: innerHeight, dpr: devicePixelRatio}, elements,
    diagnostics: {errors: (window.__errs || []).slice(0, 8), text: document.body ? document.body.innerText.slice(0, 200) : null}});
})()
"""

delegate.finished = {
    DispatchQueue.main.asyncAfter(deadline: .now() + wait) {
        let setTheme = "document.documentElement.dataset.theme = '\(theme)'; 0"
        webView.evaluateJavaScript(setTheme) { _, _ in
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
                webView.evaluateJavaScript(extract) { value, error in
                    if let text = value as? String {
                        try? text.write(toFile: prefix + ".json", atomically: true, encoding: .utf8)
                    } else {
                        FileHandle.standardError.write("extract failed: \(String(describing: error))\n".data(using: .utf8)!)
                    }
                    let snapshot = WKSnapshotConfiguration()
                    snapshot.rect = CGRect(x: 0, y: 0, width: width, height: height)
                    snapshot.snapshotWidth = NSNumber(value: Double(width))
                    webView.takeSnapshot(with: snapshot) { image, error in
                        guard let image = image,
                              let tiff = image.tiffRepresentation,
                              let rep = NSBitmapImageRep(data: tiff),
                              let png = rep.representation(using: .png, properties: [:]) else {
                            FileHandle.standardError.write("snapshot failed: \(String(describing: error))\n".data(using: .utf8)!)
                            exit(1)
                        }
                        try? png.write(to: URL(fileURLWithPath: prefix + ".png"))
                        exit(0)
                    }
                }
            }
        }
    }
}
webView.load(URLRequest(url: url))
app.run()
