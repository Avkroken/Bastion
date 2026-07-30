import Foundation
import SSHCore
import SwiftCrossUI

/// Samma polling-strategi som `App/DashboardView.swift`s `DashboardModel`:
/// hämtar direkt, sedan var 15:e sekund, och låter övergående fel under en
/// periodisk uppdatering behålla redan visad data i stället för att ersätta
/// den med en felskärm.
@MainActor
class DashboardModel: ObservableObject {
    enum LoadState {
        case loading
        case loaded(SystemSnapshot)
        case failed(String)
    }
    @Published var state: LoadState = .loading
    @Published var isRefreshing = false
    @Published var lastUpdated: Date?
    private let host: Host
    private let password: String?
    /// För att slå upp en ev. jump-host, se `resolveConnectionPlan`. `nil`
    /// på anropsplatser utan delad store — bara en host UTAN jump-host
    /// ansluter då direkt.
    private let store: HostStore?
    private static let pollInterval: Duration = .seconds(15)

    init(host: Host, password: String?, store: HostStore? = nil) {
        self.host = host
        self.password = password
        self.store = store
    }

    func load(isPoll: Bool = false) async {
        if isPoll, case .loaded = state {
            isRefreshing = true
        } else {
            state = .loading
        }
        guard let plan = resolveConnectionPlan(for: host, password: password, store: store) else {
            state = .failed("Kan inte autentisera värden (eller dess jump-host, om en är vald).")
            isRefreshing = false
            return
        }
        do {
            let chain = try await SSHConnectionChain.connect(target: host.target, targetAuth: plan.auth, jump: plan.jump)
            let snapshot: SystemSnapshot
            do {
                snapshot = try await SystemProbe.snapshot(over: chain.target)
            } catch {
                await chain.close()
                throw error
            }
            await chain.close()
            state = .loaded(snapshot)
            lastUpdated = Date()
        } catch {
            if !isPoll { state = .failed("\(error)") }
        }
        isRefreshing = false
    }

    func startPolling() async {
        await load()
        while !Task.isCancelled {
            try? await Task.sleep(for: Self.pollInterval)
            if Task.isCancelled { break }
            await load(isPoll: true)
        }
    }
}

struct DashboardView: View {
    @State private var model: DashboardModel

    init(host: Host, password: String?, store: HostStore? = nil) {
        self._model = State(wrappedValue: DashboardModel(host: host, password: password, store: store))
    }

    var body: some View {
        Group {
            switch model.state {
            case .loading:
                ProgressView("Hämtar status…")
            case .failed(let message):
                VStack(spacing: 8) {
                    Text("Kunde inte hämta status").font(.headline)
                    Text(message).foregroundColor(.gray)
                }
                .padding()
            case .loaded(let snapshot):
                ScrollView {
                    metrics(snapshot).padding()
                }
            }
        }
        .task { await model.startPolling() }
    }

    @ViewBuilder
    private var statusRow: some View {
        if let updated = model.lastUpdated {
            HStack {
                if model.isRefreshing { ProgressView().resizable().frame(width: 12, height: 12) }
                Text("Uppdaterad \(Self.timeFormatter.string(from: updated))")
                    .foregroundColor(.gray)
                Spacer()
            }
        }
    }

    @ViewBuilder
    private func metrics(_ s: SystemSnapshot) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            statusRow
            systemCard(s)
            if let load = s.load { loadCard(load, cpu: s.cpuCount) }
            if let mem = s.memory { meterCard("Minne", used: mem.usedBytes, total: mem.totalBytes) }
            ForEach(s.disks.filter { $0.mount == "/" || !$0.filesystem.hasPrefix("tmpfs") }, id: \.mount) { d in
                meterCard("Disk \(d.mount)", used: d.usedBytes, total: d.sizeBytes)
            }
            if !s.containers.isEmpty { dockerCard(s.containers) }
        }
    }

    private func systemCard(_ s: SystemSnapshot) -> some View {
        card("System") {
            VStack(alignment: .leading, spacing: 4) {
                infoRow("Värd", s.hostname)
                infoRow("OS", s.os)
                infoRow("Kärna", s.kernel)
                infoRow("CPU-kärnor", s.cpuCount.map(String.init))
                infoRow("Drifttid", s.uptimeSeconds.map(Self.formatUptime))
            }
        }
    }

    private func loadCard(_ load: LoadAverage, cpu: Int?) -> some View {
        card("Last (1 / 5 / 15 min)") {
            HStack {
                Text(String(format: "%.2f  %.2f  %.2f", load.one, load.five, load.fifteen))
                Spacer()
                if let cpu { Text("\(cpu) kärnor").foregroundColor(.gray) }
            }
        }
    }

    private func meterCard(_ title: String, used: Int64, total: Int64) -> some View {
        let fraction = total > 0 ? Double(used) / Double(total) : 0
        return card(title) {
            VStack(alignment: .leading, spacing: 4) {
                ProgressView(value: fraction)
                Text("\(Self.bytes(used)) av \(Self.bytes(total)) (\(Int(fraction * 100)) %)")
                    .foregroundColor(.gray)
            }
        }
    }

    private func dockerCard(_ containers: [DockerContainer]) -> some View {
        card("Docker (\(containers.count))") {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(containers, id: \.id) { c in
                    HStack {
                        Circle()
                            .fill(c.status.hasPrefix("Up") ? Color.green : Color.gray)
                            .frame(width: 8, height: 8)
                        VStack(alignment: .leading) {
                            Text(c.name)
                            Text(c.image).foregroundColor(.gray)
                        }
                        Spacer()
                        Text(c.status).foregroundColor(.gray)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func infoRow(_ label: String, _ value: String?) -> some View {
        if let value {
            HStack {
                Text(label).foregroundColor(.gray)
                Spacer()
                Text(value)
            }
        }
    }

    private func card<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).emphasized()
            content()
        }
        .padding()
        .background(RoundedRectangle(cornerRadius: 10).fill(Color.gray.opacity(0.12)))
    }

    // MARK: - Formatering

    private static let timeFormatter: DateFormatter = {
        let f = DateFormatter()
        f.timeStyle = .medium
        f.dateStyle = .none
        return f
    }()

    static func bytes(_ n: Int64) -> String {
        ByteCountFormatter.string(fromByteCount: n, countStyle: .binary)
    }

    static func formatUptime(_ seconds: Double) -> String {
        let total = Int(seconds)
        let d = total / 86400, h = (total % 86400) / 3600, m = (total % 3600) / 60
        if d > 0 { return "\(d) d \(h) h" }
        if h > 0 { return "\(h) h \(m) min" }
        return "\(m) min"
    }
}
