// Small native installer. Downloads the original driver from its publisher only
// when missing; does not redistribute third-party binaries or bypass authorization.
import AppKit
import CryptoKit

let packageURL = "https://existential.audio/downloads/BlackHole2ch-0.7.1.pkg"
let packageSHA = "57b540f27a3e29c37e310e01bee0fdfab76733087e47f997ef9dccf851400dcf"
func quote(_ text: String) -> String { "'" + text.replacingOccurrences(of: "'", with: "'\\''") + "'" }
func run(_ executable: String, _ arguments: [String]) throws -> String {
    let task = Process(); task.executableURL = URL(fileURLWithPath: executable); task.arguments = arguments
    let pipe = Pipe(); task.standardOutput = pipe; task.standardError = pipe
    try task.run(); let data = pipe.fileHandleForReading.readDataToEndOfFile(); task.waitUntilExit()
    let text = String(decoding: data, as: UTF8.self)
    guard task.terminationStatus == 0 else { throw NSError(domain: "Installer", code: Int(task.terminationStatus), userInfo: [NSLocalizedDescriptionKey: text]) }
    return text
}
func verifyPackage(_ url: URL) throws {
    let handle = try FileHandle(forReadingFrom: url)
    defer { try? handle.close() }
    let data = try handle.read(upToCount: 1024 * 1024 + 1) ?? Data()
    guard data.count < 1024 * 1024,
          SHA256.hash(data: data).map({ String(format: "%02x", $0) }).joined() == packageSHA else {
        throw NSError(domain: "Installer", code: 1, userInfo: [NSLocalizedDescriptionKey: "BlackHole 安装包校验失败，请重新运行安装器。"])
    }
    let signature = try run("/usr/sbin/pkgutil", ["--check-signature", url.path])
    guard signature.contains("Developer ID Installer: Existential Audio Inc. (Q5C99V536K)") else {
        throw NSError(domain: "Installer", code: 2, userInfo: [NSLocalizedDescriptionKey: "BlackHole 安装包签名与官方发布者不一致。"])
    }
    _ = try run("/usr/sbin/spctl", ["--assess", "--type", "install", url.path])
}

final class InstallerDelegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var label: NSTextField!
    var progress: NSProgressIndicator!
    func applicationDidFinishLaunching(_ notification: Notification) {
        let alert = NSAlert(); alert.messageText = "安装语音遥控"
        alert.informativeText = "将安装到“应用程序”，并在首次启动时启用登录自启动。\n\n缺少虚拟麦克风时，会从 BlackHole 官网下载并校验原版安装包（约 101KB），随本次安装完成配置。系统会要求管理员授权；辅助功能权限仍需你在系统设置中允许。"
        alert.addButton(withTitle: "安装"); alert.addButton(withTitle: "取消")
        guard alert.runModal() == .alertFirstButtonReturn else { NSApp.terminate(nil); return }
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 440, height: 160), styleMask: [.titled], backing: .buffered, defer: false)
        window.title = "安装语音遥控"; window.center()
        label = NSTextField(labelWithString: "正在准备安装…"); label.frame = NSRect(x: 24, y: 82, width: 392, height: 40)
        label.lineBreakMode = .byWordWrapping
        progress = NSProgressIndicator(frame: NSRect(x: 24, y: 44, width: 392, height: 18))
        progress.style = .bar; progress.isIndeterminate = true; progress.startAnimation(nil)
        window.contentView?.addSubview(label); window.contentView?.addSubview(progress); window.makeKeyAndOrderFront(nil)
        DispatchQueue.global(qos: .userInitiated).async { self.prepare() }
    }
    func prepare() {
        do {
            guard let source = Bundle.main.resourceURL?.appendingPathComponent("DoubaoPTT.app"), FileManager.default.fileExists(atPath: source.path) else {
                throw NSError(domain: "Installer", code: 3, userInfo: [NSLocalizedDescriptionKey: "安装器缺少主应用，请重新获取完整安装包。"])
            }
            _ = try run("/usr/bin/codesign", ["--verify", "--deep", "--strict", source.path])
            var package: URL?
            if !FileManager.default.fileExists(atPath: "/Library/Audio/Plug-Ins/HAL/BlackHole2ch.driver") {
                DispatchQueue.main.async { self.label.stringValue = "正在准备官方 BlackHole 虚拟麦克风…" }
                let cache = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("DoubaoPTT")
                try FileManager.default.createDirectory(at: cache, withIntermediateDirectories: true)
                let cached = cache.appendingPathComponent("BlackHole2ch-0.7.1.pkg")
                if (try? verifyPackage(cached)) == nil {
                    let temp = cache.appendingPathComponent(UUID().uuidString + ".pkg")
                    defer { try? FileManager.default.removeItem(at: temp) }
                    _ = try run("/usr/bin/curl", ["--fail", "--location", "--proto", "=https", "--proto-redir", "=https", "--max-time", "45", "--max-filesize", "1048576", "--user-agent", "Homebrew", "--output", temp.path, packageURL])
                    try verifyPackage(temp)
                    if FileManager.default.fileExists(atPath: cached.path) { try FileManager.default.removeItem(at: cached) }
                    try FileManager.default.moveItem(at: temp, to: cached)
                }
                package = cached
            }
            let staged = "/Applications/.DoubaoPTT.install.\(UUID().uuidString).app"
            let backup = "/Applications/.DoubaoPTT.backup.\(UUID().uuidString).app"
            var shell = "set -eu\n"
            if let package {
                shell += "test \"$(/usr/bin/shasum -a 256 \(quote(package.path)) | /usr/bin/awk '{print $1}')\" = \(quote(packageSHA))\n"
                shell += "/usr/sbin/pkgutil --check-signature \(quote(package.path)) | /usr/bin/grep -F 'Developer ID Installer: Existential Audio Inc. (Q5C99V536K)' >/dev/null\n"
                shell += "/usr/sbin/installer -pkg \(quote(package.path)) -target /\n"
            }
            shell += "trap 'if [ -d \(quote(backup)) ] && [ ! -e /Applications/DoubaoPTT.app ]; then /bin/mv \(quote(backup)) /Applications/DoubaoPTT.app; fi; /bin/rm -rf \(quote(staged))' EXIT\n"
            shell += "/usr/bin/ditto \(quote(source.path)) \(quote(staged))\n/usr/bin/codesign --verify --deep --strict \(quote(staged))\n"
            shell += "if [ -e /Applications/DoubaoPTT.app ]; then /bin/mv /Applications/DoubaoPTT.app \(quote(backup)); fi\n/bin/mv \(quote(staged)) /Applications/DoubaoPTT.app\n/bin/rm -rf \(quote(backup))\n"
            DispatchQueue.main.async {
                self.label.stringValue = "请在系统弹窗中完成管理员授权…"
                let running = NSRunningApplication.runningApplications(withBundleIdentifier: "local.doubao-ptt.app")
                if !running.isEmpty {
                    self.finish(error: NSError(domain: "Installer", code: 4, userInfo: [NSLocalizedDescriptionKey: "请先从菜单栏退出正在运行的语音遥控，再重新运行安装器。"])); return
                }
                var error: NSDictionary?
                let script = "do shell script \"" + shell.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"").replacingOccurrences(of: "\n", with: "\\n") + "\" with administrator privileges"
                guard let appleScript = NSAppleScript(source: script), appleScript.compileAndReturnError(&error) else {
                    self.finish(error: NSError(domain: "Installer", code: 6, userInfo: [NSLocalizedDescriptionKey: error?.description ?? "无法准备系统安装请求。"])); return
                }
                appleScript.executeAndReturnError(&error)
                if let error { self.finish(error: NSError(domain: "Installer", code: 5, userInfo: [NSLocalizedDescriptionKey: error.description])); return }
                let desktop = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Desktop/语音遥控.app")
                if !FileManager.default.fileExists(atPath: desktop.path) { try? FileManager.default.createSymbolicLink(atPath: desktop.path, withDestinationPath: "/Applications/DoubaoPTT.app") }
                let configuration = NSWorkspace.OpenConfiguration(); configuration.arguments = ["serve", "--open"]
                NSWorkspace.shared.openApplication(at: URL(fileURLWithPath: "/Applications/DoubaoPTT.app"), configuration: configuration)
                self.finish(error: nil)
            }
        } catch { DispatchQueue.main.async { self.finish(error: error) } }
    }
    func finish(error: Error?) {
        progress.stopAnimation(nil); window.orderOut(nil)
        let alert = NSAlert(); alert.messageText = error == nil ? "安装完成" : "安装未完成"
        alert.informativeText = error?.localizedDescription ?? "语音遥控已启动。请在系统设置中允许主应用及输入代理的辅助功能权限，并在豆包中选择 BlackHole 2ch。"
        alert.addButton(withTitle: "好"); alert.runModal(); NSApp.terminate(nil)
    }
}
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = InstallerDelegate(); app.delegate = delegate
app.activate(ignoringOtherApps: true); app.run()
