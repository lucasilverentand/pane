import Foundation

@Observable
final class WorkspaceModel: Identifiable {
    let id = UUID()
    var name: String
    var roots: [URL]

    init(name: String, roots: [URL]) {
        self.name = name
        self.roots = roots
    }
}
