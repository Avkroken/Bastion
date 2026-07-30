import Foundation
import SSHCore
import SwiftCrossUI

/// Vilken typ av portvidarebefordran som skapas — matchar `ssh -L`/`-R`/`-D`.
enum PortForwardKind: Equatable, CustomStringConvertible {
    case local, remote, dynamic

    var description: String {
        switch self {
        case .local: return "Lokal (-L)"
        case .remote: return "Fjärr (-R)"
        case .dynamic: return "Dynamisk/SOCKS5 (-D)"
        }
    }
}

/// Ett aktivt portvidarebefordran, oavsett typ — `LocalPortForward`/
/// `RemotePortForward`/`DynamicPortForward` är tre skilda typer i SSHCore
/// (delar ingen gemensam protokoll), så den här slår ihop dem till en
/// enhetlig form UI:t kan lista och stänga utan att bry sig om vilken det är.
struct ActivePortForward: Identifiable {
    let id = UUID()
    let kind: PortForwardKind
    let bindHost: String
    let actualBindPort: Int
    /// Tomt för `.dynamic` (målet väljs av SOCKS-klienten per anslutning,
    /// finns inget fast mål att visa).
    let targetDescription: String
    let close: () async -> Void
}

@MainActor
final class PortForwardModel: ObservableObject {
    @Published var active: [ActivePortForward] = []
    @Published var errorMessage: String?
    @Published var starting = false
    private let host: Host
    private let password: String?
    private let store: HostStore?
    private var chain: SSHConnectionChain?
    /// Sätts av `disconnect()` — kollas EFTER `SSHConnectionChain.connect()`
    /// eftersom den kan hinna klart efter att vyn redan stängts/disconnect()
    /// redan körts; utan kollen skulle en sen anslutning återuppliva
    /// `self.chain` och aldrig stängas (CodeRabbit/cubic-fynd PR #179).
    private var disconnected = false

    init(host: Host, password: String?, store: HostStore? = nil) {
        self.host = host
        self.password = password
        self.store = store
    }

    private func ensureSession() async -> SSHSession? {
        if let chain { return chain.target }
        guard let plan = resolveConnectionPlan(for: host, password: password, store: store) else {
            errorMessage = "Kan inte autentisera värden (eller dess jump-host, om en är vald)."
            return nil
        }
        do {
            let chain = try await SSHConnectionChain.connect(target: host.target, targetAuth: plan.auth, jump: plan.jump)
            guard !disconnected else {
                await chain.close()
                return nil
            }
            self.chain = chain
            return chain.target
        } catch {
            errorMessage = "\(error)"
            return nil
        }
    }

    /// `targetHost`/`targetPort` ignoreras för `.dynamic` (SOCKS-klienten
    /// väljer målet per anslutning — det är hela poängen med "dynamisk").
    func start(kind: PortForwardKind, bindPort: Int, targetHost: String, targetPort: Int) async {
        starting = true
        defer { starting = false }
        guard let s = await ensureSession() else { return }
        do {
            switch kind {
            case .local:
                let f = try await s.openLocalPortForward(bindPort: bindPort, targetHost: targetHost, targetPort: targetPort)
                active.append(ActivePortForward(
                    kind: .local, bindHost: f.bindHost, actualBindPort: f.actualBindPort,
                    targetDescription: "\(targetHost):\(targetPort)", close: { await f.close() }))
            case .remote:
                let f = try await s.openRemotePortForward(bindPort: bindPort, targetHost: targetHost, targetPort: targetPort)
                active.append(ActivePortForward(
                    kind: .remote, bindHost: host.hostName, actualBindPort: f.actualBindPort,
                    targetDescription: "\(targetHost):\(targetPort) (lokalt)", close: { await f.close() }))
            case .dynamic:
                let f = try await s.openDynamicPortForward(bindPort: bindPort)
                active.append(ActivePortForward(
                    kind: .dynamic, bindHost: f.bindHost, actualBindPort: f.actualBindPort,
                    targetDescription: "", close: { await f.close() }))
            }
            errorMessage = nil
        } catch {
            errorMessage = "\(error)"
        }
    }

    func stop(_ forward: ActivePortForward) async {
        await forward.close()
        active.removeAll { $0.id == forward.id }
    }

    func disconnect() {
        disconnected = true
        let chain = self.chain
        self.chain = nil
        let forwards = active
        active = []
        Task {
            for f in forwards { await f.close() }
            await chain?.close()
        }
    }
}

struct PortForwardView: View {
    @State private var model: PortForwardModel
    @State private var kind: PortForwardKind = .local
    @State private var bindPortText = "0"
    @State private var targetHostText = ""
    @State private var targetPortText = ""

    init(host: Host, password: String?, store: HostStore? = nil) {
        self._model = State(wrappedValue: PortForwardModel(host: host, password: password, store: store))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Portvidarebefordran").font(.headline)
            if let e = model.errorMessage {
                Text(e).foregroundColor(.red)
            }

            Picker(of: [PortForwardKind.local, .remote, .dynamic], selection: kindBinding)

            TextField("Lokal bindport (0 = valfri ledig)", text: $bindPortText)
            if kind != .dynamic {
                TextField("Målvärd (t.ex. 10.0.0.5)", text: $targetHostText)
                TextField("Målport", text: $targetPortText)
            }

            Button(startButtonTitle) { Task { await start() } }
                .disabled(model.starting || !isValid)

            Divider()

            if model.active.isEmpty {
                Text("Inga aktiva tunnlar.").foregroundColor(.gray)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(model.active, id: \.id) { f in row(f) }
                    }
                }
            }
        }
        .padding()
        .onDisappear { model.disconnect() }
    }

    private var kindBinding: Binding<PortForwardKind?> {
        Binding(get: { kind }, set: { if let v = $0 { kind = v } })
    }

    private var startButtonTitle: String {
        model.starting ? "Startar…" : "Starta"
    }

    private var isValid: Bool {
        guard Int(bindPortText) != nil else { return false }
        if kind == .dynamic { return true }
        guard let p = Int(targetPortText), p > 0 else { return false }
        return !targetHostText.trimmingCharacters(in: .whitespaces).isEmpty && p <= 65_535
    }

    private func start() async {
        guard let bindPort = Int(bindPortText) else { return }
        let targetPort = Int(targetPortText) ?? 0
        await model.start(kind: kind, bindPort: bindPort, targetHost: targetHostText, targetPort: targetPort)
    }

    private func row(_ f: ActivePortForward) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(f.kind.description).emphasized()
                if f.targetDescription.isEmpty {
                    Text("\(f.bindHost):\(f.actualBindPort)").foregroundColor(.gray)
                } else {
                    Text("\(f.bindHost):\(f.actualBindPort) → \(f.targetDescription)").foregroundColor(.gray)
                }
            }
            Spacer()
            Button("Stoppa") { Task { await model.stop(f) } }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color.gray.opacity(0.08)))
    }
}
