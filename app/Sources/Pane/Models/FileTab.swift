import Foundation

struct FileTab: Identifiable {
    let id = UUID()
    let url: URL
    var content: String
    var isDirty: Bool = false

    var filename: String {
        url.lastPathComponent
    }

    var language: String {
        switch url.pathExtension.lowercased() {
        case "swift": "swift"
        case "rs": "rust"
        case "ts", "tsx": "typescript"
        case "js", "jsx": "javascript"
        case "py": "python"
        case "go": "go"
        case "json": "json"
        case "toml": "toml"
        case "yaml", "yml": "yaml"
        case "md": "markdown"
        case "html": "html"
        case "css": "css"
        default: "plaintext"
        }
    }
}
