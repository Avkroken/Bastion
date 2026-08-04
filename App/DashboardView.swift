#if canImport(SwiftUI)
import SwiftUI
import SSHCore

@MainActor
final class DashboardModel: ObservableObject {
    enum LoadState {
        case loading
        case loaded(SystemSnapshot)
        case failed(String)
    }
    @Published var state: LoadState = .loading
    @Published var isRefreshing = false
    @Published var lastUpdated: Date?
    private let request: ConnectRequest
    /// För att slå upp en ev. jump-host, se `resolveConnectionPlan`. `nil`
    /// på anropsplatser utan delad store — bara en host UTAN jump-host
    /// ansluter då direkt; en host MED jumpHostID nekas anslutning
    /// (jump-hosten går inte att lösa upp utan store), se `resolveConnectionPlan`.
    private let store: HostStore?
    private static let pollInterval: Duration = .seconds(15)

    init(request: ConnectRequest, store: HostStore? = nil) {
        self.request = request
        self.store = store
    }

    func load(isPoll: Bool = false) async {
        if isPoll, case .loaded = state {
            isRefreshing = true
        } else {
            state = .loading
        }
        guard let plan = resolveConnectionPlan(for: request.host, password: request.password, store: store) else {
            state = .failed("Kan inte autentisera värden (eller dess jump-host, om en är vald).")
            isRefreshing = false
            return
        }
        do {
            let chain = try await SSHConnectionChain.connect(
                target: request.host.target, targetAuth: plan.auth, jump: plan.jump)
            // Kedjan måste stängas här om snapshot() kastar också — annars
            // läcker anslutningen tyst vid varje misslyckad hämtning
            // (CodeRabbit-fynd, PR #172).
            let snapshotResult: Result<SystemSnapshot, Error>
            do {
                snapshotResult = .success(try await SystemProbe.snapshot(over: chain.target))
            } catch {
                snapshotResult = .failure(error)
            }
            await chain.close()
            let snapshot = try snapshotResult.get()
            state = .loaded(snapshot)
            lastUpdated = Date()
        } catch {
            // Ett övergående fel under periodisk uppdatering ska inte ersätta
            // redan visad data med en felskärm — bara den allra första hämtningen gör det.
            if !isPoll { state = .failed("\(error)") }
        }
        isRefreshing = false
    }

    /// Hämtar direkt, sedan om och om igen med intervall tills vyn försvinner
    /// (SwiftUI avbryter `.task`-anropet automatiskt vid disappear).
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
    @StateObject private var model: DashboardModel
    // Samma fix som HostDetailView: `@AppStorage`, inte en direkt
    // `UserDefaults.standard`-läsning — annars observerar SwiftUI inte
    // ändringen och Docker-kortet kan bli kvar synligt/dolt efter att
    // `FeatureSettingsView` togglat samma nyckel på en annan flik.
    @AppStorage(FeatureToggleKeys.showDocker) private var showDockerCard = true

    init(request: ConnectRequest, store: HostStore? = nil) {
        _model = StateObject(wrappedValue: DashboardModel(request: request, store: store))
    }

    var body: some View {
        Group {
            switch model.state {
            case .loading:
                ProgressView("Hämtar status…").frame(maxWidth: .infinity, maxHeight: .infinity)
            case .failed(let message):
                ContentUnavailableView("Kunde inte hämta status", systemImage: "exclamationmark.triangle",
                                       description: Text(message))
            case .loaded(let snapshot):
                ScrollView { metrics(snapshot).padding() }
            }
        }
        .task { await model.startPolling() }
    }

    @ViewBuilder
    private var statusRow: some View {
        if let updated = model.lastUpdated {
            HStack(spacing: 6) {
                if model.isRefreshing {
                    ProgressView().controlSize(.small)
                }
                Text("Uppdaterad \(updated.formatted(date: .omitted, time: .standard))")
                Spacer()
            }
            .font(.caption2).foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func metrics(_ s: SystemSnapshot) -> some View {
        VStack(spacing: 16) {
            statusRow
            systemCard(s)
            if let load = s.load { loadCard(load, cpu: s.cpuCount) }
            if let mem = s.memory { meterCard("Minne", used: mem.usedBytes, total: mem.totalBytes) }
            ForEach(s.disks.filter { $0.mount == "/" || !$0.filesystem.hasPrefix("tmpfs") }, id: \.mount) { d in
                meterCard("Disk \(d.mount)", used: d.usedBytes, total: d.sizeBytes)
            }
            if !s.containers.isEmpty && showDockerCard { dockerCard(s.containers) }
        }
    }

    private func systemCard(_ s: SystemSnapshot) -> some View {
        Card("System") {
            infoRow("Värd", s.hostname)
            infoRow("OS", s.os)
            infoRow("Kärna", s.kernel)
            infoRow("CPU-kärnor", s.cpuCount.map(String.init))
            infoRow("Drifttid", s.uptimeSeconds.map(Self.formatUptime))
        }
    }

    private func loadCard(_ load: LoadAverage, cpu: Int?) -> some View {
        Card("Last (1 / 5 / 15 min)") {
            HStack {
                Text(String(format: "%.2f  %.2f  %.2f", load.one, load.five, load.fifteen))
                    .font(.title3.monospacedDigit())
                Spacer()
                if let cpu { Text("\(cpu) kärnor").font(.caption).foregroundStyle(.secondary) }
            }
        }
    }

    private func meterCard(_ title: String, used: Int64, total: Int64) -> some View {
        let fraction = total > 0 ? Double(used) / Double(total) : 0
        return Card(title) {
            VStack(alignment: .leading, spacing: 6) {
                ProgressView(value: fraction)
                    .tint(fraction > 0.9 ? .red : (fraction > 0.75 ? .orange : .accentColor))
                Text("\(Self.bytes(used)) av \(Self.bytes(total)) (\(Int(fraction * 100)) %)")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    private func dockerCard(_ containers: [DockerContainer]) -> some View {
        Card("Docker (\(containers.count))") {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(containers, id: \.id) { c in
                    HStack {
                        Circle().fill(c.status.hasPrefix("Up") ? .green : .gray).frame(width: 8, height: 8)
                        VStack(alignment: .leading) {
                            Text(c.name).font(.subheadline.weight(.medium))
                            Text(c.image).font(.caption2).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Text(c.status).font(.caption2).foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func infoRow(_ label: String, _ value: String?) -> some View {
        if let value {
            HStack {
                Text(label).foregroundStyle(.secondary)
                Spacer()
                Text(value).multilineTextAlignment(.trailing)
            }.font(.subheadline)
        }
    }

    // MARK: - Formatering

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

/// Enkel kort-container.
struct Card<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content
    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.headline)
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(Color.cardFill, in: RoundedRectangle(cornerRadius: 12))
    }
}
#endif
