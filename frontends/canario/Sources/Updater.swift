import AppKit
import SwiftUI

// Auto-updater, GitHub Releases as the feed. Sparkle is the long-term
// answer (what Ghostty and iTerm2 use), but it wants an Xcode project and
// an appcast pipeline; this covers the same user story with what the
// release process already publishes: a notarized Canario.dmg with a stable
// asset name per tag. Integrity rests on HTTPS + Apple's codesign/notarize
// checks of the downloaded bundle.
//
// Flow: check (launch + daily + menu item) -> a quiet pill next to the
// sidebar controls with a dropdown (install / skip) -> download -> mount ->
// replace the app bundle in place -> relaunch.

@Observable
final class Updater {
    enum State: Equatable {
        case idle
        case checking
        case upToDate
        case available(version: String, url: URL)
        case downloading(progress: Double)
        case installing
        case failed(String)
    }

    var state: State = .idle

    @ObservationIgnored private var checkedThisLaunch = false

    private static let feed = URL(
        string: "https://api.github.com/repos/raphamorim/rio/releases/latest")!

    /// Dev builds carry goreleaser's unsubstituted placeholder; they can't
    /// meaningfully compare versions, so the updater stays quiet.
    private var currentVersion: String? {
        guard
            let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"]
                as? String,
            !version.isEmpty, !version.hasPrefix("{{")
        else { return nil }
        return version
    }

    // MARK: Checking

    /// Launch-time check: at most once per day, silent unless an update
    /// exists.
    func checkAutomatically() {
        guard !checkedThisLaunch, currentVersion != nil else { return }
        checkedThisLaunch = true
        let last = UserDefaults.standard.double(forKey: "lastUpdateCheck")
        guard Date().timeIntervalSince1970 - last > 86_400 else { return }
        check(userInitiated: false)
    }

    /// Menu-driven check: always reports, including "you're up to date".
    func checkNow() {
        check(userInitiated: true)
    }

    private func check(userInitiated: Bool) {
        guard let current = currentVersion else {
            if userInitiated {
                state = .failed("Development builds don't self-update.")
            }
            return
        }
        state = .checking
        UserDefaults.standard.set(
            Date().timeIntervalSince1970, forKey: "lastUpdateCheck")

        var request = URLRequest(url: Self.feed)
        request.setValue(
            "application/vnd.github+json", forHTTPHeaderField: "Accept")
        URLSession.shared.dataTask(with: request) { [weak self] data, _, error in
            DispatchQueue.main.async {
                self?.handleFeed(
                    data: data, error: error,
                    current: current, userInitiated: userInitiated)
            }
        }.resume()
    }

    private func handleFeed(
        data: Data?, error: Error?, current: String, userInitiated: Bool
    ) {
        guard
            let data,
            let release = try? JSONSerialization.jsonObject(with: data)
                as? [String: Any],
            let tag = release["tag_name"] as? String,
            let assets = release["assets"] as? [[String: Any]]
        else {
            if userInitiated {
                state = .failed(
                    error?.localizedDescription ?? "Couldn't reach GitHub.")
            } else {
                state = .idle
            }
            return
        }

        let latest = tag.hasPrefix("v") ? String(tag.dropFirst()) : tag
        let dmg = assets.first { ($0["name"] as? String) == "Canario.dmg" }
        guard
            let urlString = dmg?["browser_download_url"] as? String,
            let url = URL(string: urlString),
            Self.isNewer(latest, than: current)
        else {
            // "Up to date" is only worth a moment of pill; it fades out.
            state = .upToDate
            if userInitiated { autoHide(after: 4) }
            else { state = .idle }
            return
        }

        // A version the user chose to skip only surfaces via explicit check.
        if !userInitiated,
            UserDefaults.standard.string(forKey: "skippedVersion") == latest
        {
            state = .idle
            return
        }

        state = .available(version: latest, url: url)
    }

    private func autoHide(after seconds: TimeInterval) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in
            guard let self, self.state == .upToDate else { return }
            self.state = .idle
        }
    }

    /// Numeric dotted-version compare; unequal lengths pad with zeros.
    static func isNewer(_ candidate: String, than current: String) -> Bool {
        let a = candidate.split(separator: ".").map { Int($0) ?? 0 }
        let b = current.split(separator: ".").map { Int($0) ?? 0 }
        for i in 0..<max(a.count, b.count) {
            let x = i < a.count ? a[i] : 0
            let y = i < b.count ? b[i] : 0
            if x != y { return x > y }
        }
        return false
    }

    // MARK: Actions

    func skip() {
        if case .available(let version, _) = state {
            UserDefaults.standard.set(version, forKey: "skippedVersion")
        }
        dismiss()
    }

    func dismiss() {
        state = .idle
    }

    func install() {
        guard case .available(_, let url) = state else { return }
        state = .downloading(progress: 0)

        let task = URLSession.shared.downloadTask(with: url) {
            [weak self] location, _, error in
            guard let self else { return }
            guard let location, error == nil else {
                DispatchQueue.main.async {
                    self.state = .failed(
                        error?.localizedDescription ?? "Download failed.")
                }
                return
            }
            // Move out of the ephemeral location before the handler returns.
            let dmg = FileManager.default.temporaryDirectory
                .appendingPathComponent("Canario-update.dmg")
            try? FileManager.default.removeItem(at: dmg)
            do {
                try FileManager.default.moveItem(at: location, to: dmg)
            } catch {
                DispatchQueue.main.async {
                    self.state = .failed(error.localizedDescription)
                }
                return
            }
            DispatchQueue.main.async { self.state = .installing }
            DispatchQueue.global(qos: .userInitiated).async {
                self.installDMG(at: dmg)
            }
        }
        observeProgress(of: task)
        task.resume()
    }

    private func observeProgress(of task: URLSessionDownloadTask) {
        // Poll rather than KVO-observe: simpler, and per-second is plenty.
        Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) {
            [weak self, weak task] timer in
            guard let self, let task else {
                timer.invalidate()
                return
            }
            switch task.state {
            case .running:
                if case .downloading = self.state {
                    self.state = .downloading(
                        progress: task.progress.fractionCompleted)
                }
            default:
                timer.invalidate()
            }
        }
    }

    /// Mount, copy the new bundle over this one, unmount, relaunch. Errors
    /// fall back to revealing the DMG so the user can finish by hand.
    private func installDMG(at dmg: URL) {
        func fail(_ message: String) {
            DispatchQueue.main.async {
                self.state = .failed(message)
                NSWorkspace.shared.activateFileViewerSelecting([dmg])
            }
        }

        guard
            let attach = run(
                "/usr/bin/hdiutil",
                ["attach", "-nobrowse", "-readonly", "-plist", dmg.path]),
            let mount = mountPoint(fromHdiutilPlist: attach)
        else {
            fail("Couldn't open the downloaded image.")
            return
        }
        defer { _ = run("/usr/bin/hdiutil", ["detach", mount, "-force"]) }

        let source = URL(fileURLWithPath: mount)
            .appendingPathComponent("Canario.app")
        guard FileManager.default.fileExists(atPath: source.path) else {
            fail("The image doesn't contain Canario.app.")
            return
        }

        let destination = Bundle.main.bundleURL
        // ditto preserves signatures and replaces in place; a running app's
        // bundle can be swapped on macOS as long as the location is writable.
        guard run("/usr/bin/ditto", [source.path, destination.path]) != nil
        else {
            fail("Couldn't replace \(destination.lastPathComponent). It may not be writable.")
            return
        }

        DispatchQueue.main.async {
            let relaunch = Process()
            relaunch.executableURL = URL(fileURLWithPath: "/usr/bin/open")
            relaunch.arguments = ["-n", destination.path]
            try? relaunch.run()
            // Skip the confirm-quit dialog: this is an explicit update.
            UserDefaults.standard.set(true, forKey: "updaterRelaunch")
            NSApp.terminate(nil)
        }
    }

    /// Run a tool to completion; nil on any failure or nonzero exit.
    private func run(_ path: String, _ args: [String]) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: path)
        process.arguments = args
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = Pipe()
        do {
            try process.run()
        } catch {
            return nil
        }
        let output = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { return nil }
        return String(data: output, encoding: .utf8)
    }

    private func mountPoint(fromHdiutilPlist output: String) -> String? {
        guard
            let data = output.data(using: .utf8),
            let plist = try? PropertyListSerialization.propertyList(
                from: data, format: nil) as? [String: Any],
            let entities = plist["system-entities"] as? [[String: Any]]
        else { return nil }
        return entities.compactMap { $0["mount-point"] as? String }.first
    }

}
