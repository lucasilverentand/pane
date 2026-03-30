import SwiftUI
import PaneKit

/// Bottom status bar showing system stats and connection info, floating over content.
struct StatusBarView: View {
    @Environment(PaneClient.self) private var client

    var body: some View {
        HStack(spacing: 12) {
            // Connection indicator
            connectionIndicator

            Text("pane")
                .font(.caption)
                .fontWeight(.medium)

            if let stats = client.systemStats {
                Spacer()

                HStack(spacing: 10) {
                    statLabel("CPU", value: String(format: "%.0f%%", stats.cpuPercent))
                    statLabel("MEM", value: String(format: "%.0f%%", stats.memoryPercent))
                    statLabel("LOAD", value: String(format: "%.2f", stats.loadAvg1))
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .glassEffect(.regular, in: .capsule)
        .padding(.horizontal, 12)
        .padding(.bottom, 6)
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
