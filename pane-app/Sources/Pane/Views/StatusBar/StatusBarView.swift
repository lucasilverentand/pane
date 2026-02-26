import SwiftUI
import PaneKit

/// Bottom status bar showing system stats and connection info.
struct StatusBarView: View {
    @Environment(PaneClient.self) private var client

    var body: some View {
        HStack {
            // Left: app name
            Text("pane")
                .font(.caption)
                .fontWeight(.medium)

            Spacer()

            // Right: system stats
            if let stats = client.systemStats {
                HStack(spacing: 12) {
                    statLabel("CPU", value: String(format: "%.0f%%", stats.cpuPercent))
                    statLabel("MEM", value: String(format: "%.0f%%", stats.memoryPercent))
                    statLabel("LOAD", value: String(format: "%.2f", stats.loadAvg1))
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }

            // Connection indicator
            connectionIndicator
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .background(.bar)
    }

    @ViewBuilder
    private func statLabel(_ label: String, value: String) -> some View {
        HStack(spacing: 4) {
            Text(label)
                .foregroundStyle(.tertiary)
            Text(value)
        }
    }

    @ViewBuilder
    private var connectionIndicator: some View {
        switch client.connectionState {
        case .connected:
            Circle()
                .fill(.green)
                .frame(width: 6, height: 6)
        case .connecting:
            ProgressView()
                .controlSize(.mini)
        case .disconnected, .error:
            Circle()
                .fill(.red)
                .frame(width: 6, height: 6)
        }
    }
}
