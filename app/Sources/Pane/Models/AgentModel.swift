import Foundation

@Observable
final class AgentModel: Identifiable {
    let id = UUID()
    var name: String
    var provider: String
    var model: String
    var status: AgentStatus = .idle
    var messages: [AgentMessage] = []
    var projectRoot: URL?

    init(name: String, provider: String = "anthropic", model: String = "claude-sonnet-4-20250514") {
        self.name = name
        self.provider = provider
        self.model = model
    }
}

enum AgentStatus: String {
    case idle
    case running
    case waitingForTool
    case error
}

struct AgentMessage: Identifiable {
    let id = UUID()
    let role: AgentRole
    let content: String
    let timestamp: Date = .now
}

enum AgentRole {
    case user
    case assistant
    case tool
}
