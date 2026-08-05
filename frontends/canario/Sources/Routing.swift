import Foundation

/// Air-traffic-control-style routing: a new root terminal whose cwd matches
/// `prefix` is filed into the root folder named `folder` (created if needed).
/// Rules live in ~/Library/Application Support/canario/routing.json:
///
///     [{ "prefix": "~/Documents/a/rio-2", "folder": "Rio" }]
struct RoutingRule: Codable {
    var prefix: String
    var folder: String

    func matches(_ cwd: String) -> Bool {
        let expanded = NSString(string: prefix).expandingTildeInPath
        guard !expanded.isEmpty else { return false }
        return cwd == expanded || cwd.hasPrefix(expanded + "/")
    }
}

enum RoutingRules {
    private static var file: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first!
            .appendingPathComponent("canario", isDirectory: true)
            .appendingPathComponent("routing.json")
    }

    static func load() -> [RoutingRule] {
        guard let data = try? Data(contentsOf: file) else {
            seedEmptyFile()
            return []
        }
        return (try? JSONDecoder().decode([RoutingRule].self, from: data)) ?? []
    }

    /// Leave an empty rules file behind so the format is discoverable.
    private static func seedEmptyFile() {
        try? FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? Data("[]".utf8).write(to: file)
    }
}
