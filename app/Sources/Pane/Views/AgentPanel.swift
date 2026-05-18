import SwiftUI

struct AgentPanel: View {
    @Environment(AppModel.self) private var appModel
    @State private var inputText = ""

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Label("Agents", systemImage: "sparkles")
                    .font(.headline)
                Spacer()
                Button {
                    let agent = AgentModel(name: "Agent \(appModel.agents.count + 1)")
                    agent.projectRoot = appModel.workspace?.roots.first
                    appModel.agents.append(agent)
                } label: {
                    Image(systemName: "plus")
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.bar)

            Divider()

            if appModel.agents.isEmpty {
                emptyState
            } else {
                agentList
            }
        }
        .background(.background)
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Image(systemName: "sparkles")
                .font(.system(size: 36))
                .foregroundStyle(.tertiary)
            Text("No agents running")
                .foregroundStyle(.secondary)
            Text("Create an agent to start working\nacross your projects with AI")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .multilineTextAlignment(.center)
            Button("New Agent") {
                let agent = AgentModel(name: "Agent 1")
                agent.projectRoot = appModel.workspace?.roots.first
                appModel.agents.append(agent)
            }
            .buttonStyle(.borderedProminent)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var agentList: some View {
        VStack(spacing: 0) {
            // Agent tabs
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 4) {
                    ForEach(appModel.agents) { agent in
                        AgentTabButton(agent: agent)
                    }
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
            }
            .background(.bar)

            Divider()

            // Active agent conversation
            if let agent = appModel.agents.first {
                AgentConversationView(agent: agent)
            }
        }
    }
}

struct AgentTabButton: View {
    let agent: AgentModel

    var body: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(statusColor)
                .frame(width: 6, height: 6)
            Text(agent.name)
                .font(.system(size: 11))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
        .background(.selection.opacity(0.5), in: RoundedRectangle(cornerRadius: 6))
    }

    private var statusColor: Color {
        switch agent.status {
        case .idle: .secondary
        case .running: .green
        case .waitingForTool: .orange
        case .error: .red
        }
    }
}

struct AgentConversationView: View {
    @Bindable var agent: AgentModel
    @State private var inputText = ""

    var body: some View {
        VStack(spacing: 0) {
            // Messages
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    ForEach(agent.messages) { message in
                        MessageBubble(message: message)
                    }
                }
                .padding(12)
            }

            Divider()

            // Input
            HStack(spacing: 8) {
                TextField("Ask the agent...", text: $inputText, axis: .vertical)
                    .textFieldStyle(.plain)
                    .lineLimit(1...5)
                    .onSubmit {
                        sendMessage()
                    }
                Button {
                    sendMessage()
                } label: {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.title3)
                }
                .buttonStyle(.plain)
                .disabled(inputText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .padding(12)
        }
    }

    private func sendMessage() {
        let text = inputText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        agent.messages.append(AgentMessage(role: .user, content: text))
        inputText = ""
        // TODO: send to Rust engine → agent provider
    }
}

struct MessageBubble: View {
    let message: AgentMessage

    var body: some View {
        HStack {
            if message.role == .user { Spacer(minLength: 40) }

            Text(message.content)
                .font(.system(size: 13))
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
                .background(backgroundColor, in: RoundedRectangle(cornerRadius: 10))
                .foregroundStyle(message.role == .user ? .white : .primary)

            if message.role != .user { Spacer(minLength: 40) }
        }
    }

    private var backgroundColor: Color {
        switch message.role {
        case .user: .accentColor
        case .assistant: Color(.controlBackgroundColor)
        case .tool: Color(.controlBackgroundColor).opacity(0.5)
        }
    }
}
