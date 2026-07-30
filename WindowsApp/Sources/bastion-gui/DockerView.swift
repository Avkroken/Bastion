import Foundation
import SSHCore
import SwiftCrossUI

enum DockerAction { case start, stop, restart }

/// Docker-hantering över SSH: lista/start/stopp/omstart/logg/shell.
/// Motsvarar `App/DockerView.swift`, men med inline-knappar per rad i stället
/// för en meny (SwiftCrossUI har ingen `.swipeActions`/kontextmeny-motsvarighet
/// för List-rader).
@MainActor
final class DockerModel: ObservableObject {
    @Published var containers: [DockerContainer] = []
    @Published var errorMessage: String?
    @Published var loading = false
    // Set, inte en enda String? — annars kan en åtgärd på container B som
    // avslutas rensa "upptagen"-indikatorn för container A medan A:s egen
    // åtgärd fortfarande pågår (om användaren hinner starta båda i tur och ordning).
    @Published var busyNames: Set<String> = []
    private let host: Host
    private let password: String?
    private let store: HostStore?
    private var chain: SSHConnectionChain?
    // Cachar det pågående anslutningsförsöket så samtidiga anrop (t.ex.
    // refresh() och act() strax efter varandra, innan connect() svarat) väntar
    // in samma försök i stället för att skapa varsin SSHSession var.
    private var connectingTask: Task<SSHSession?, Never>?

    init(host: Host, password: String?, store: HostStore? = nil) {
        self.host = host
        self.password = password
        self.store = store
    }

    private func ensureSession() async -> SSHSession? {
        if let chain { return chain.target }
        if let connectingTask { return await connectingTask.value }

        // Skapad inifrån en @MainActor-metod (inte .detached), så den ärver
        // MainActor-isoleringen — säkert att sätta errorMessage direkt här.
        let task = Task<SSHSession?, Never> { [weak self] in
            guard let self else { return nil }
            guard let plan = resolveConnectionPlan(for: self.host, password: self.password, store: self.store) else {
                self.errorMessage = "Kan inte autentisera värden (eller dess jump-host, om en är vald)."
                return nil
            }
            do {
                let chain = try await SSHConnectionChain.connect(target: self.host.target, targetAuth: plan.auth, jump: plan.jump)
                // disconnect() kan ha körts (vyn stängd) medan vi väntade på
                // connect() — utan den här kollen skulle vi återuppliva
                // self.chain EFTER att disconnect() redan städat, och den nya
                // anslutningen skulle aldrig stängas (samma mönster som
                // SFTPBrowserModel, CodeRabbit/cubic-fynd PR #179).
                guard !Task.isCancelled else {
                    await chain.close()
                    return nil
                }
                self.chain = chain
                return chain.target
            } catch {
                self.errorMessage = "\(error)"
                return nil
            }
        }
        connectingTask = task
        let result = await task.value
        connectingTask = nil
        return result
    }

    func refresh() async {
        loading = true
        defer { loading = false }
        guard let s = await ensureSession() else { return }
        do {
            containers = try await DockerService.list(over: s)
            errorMessage = nil
        } catch {
            errorMessage = "\(error)"
        }
    }

    func act(_ kind: DockerAction, on name: String) async {
        guard let s = await ensureSession() else { return }
        busyNames.insert(name)
        defer { busyNames.remove(name) }
        do {
            switch kind {
            case .start: try await DockerService.start(name, over: s)
            case .stop: try await DockerService.stop(name, over: s)
            case .restart: try await DockerService.restart(name, over: s)
            }
            await refresh()
        } catch {
            errorMessage = "\(error)"
        }
    }

    func fetchLogs(_ name: String) async -> String {
        guard let s = await ensureSession() else { return "(ingen anslutning)" }
        do { return try await DockerService.logs(name, tail: 200, over: s) }
        catch { return "Fel: \(error)" }
    }

    func disconnect() {
        // Avbryter en ev. pågående anslutning — annars kan den hinna klart
        // EFTER städningen nedan och skriva tillbaka en levande chain som
        // aldrig stängs.
        connectingTask?.cancel()
        let chain = self.chain
        self.chain = nil
        Task { await chain?.close() }
    }
}

struct DockerView: View {
    @State private var model: DockerModel
    @State private var logsContainer: String?
    @State private var logsText = ""
    @State private var shellCommand: String?
    private let host: Host
    private let password: String?
    private let store: HostStore?

    init(host: Host, password: String?, store: HostStore? = nil) {
        self.host = host
        self.password = password
        self.store = store
        self._model = State(wrappedValue: DockerModel(host: host, password: password, store: store))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Docker").font(.headline)
                Spacer()
                Button("Uppdatera") { Task { await model.refresh() } }
            }
            if let e = model.errorMessage {
                Text(e).foregroundColor(.red)
            }
            if model.loading && model.containers.isEmpty {
                ProgressView("Hämtar containrar…")
            } else if model.containers.isEmpty {
                Text("Inga containrar.").foregroundColor(.gray)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(model.containers, id: \.id) { c in
                            row(c)
                        }
                    }
                }
            }
        }
        .padding()
        .task { await model.refresh() }
        .onDisappear { model.disconnect() }
        .sheet(isPresented: Binding(get: { logsContainer != nil }, set: { if !$0 { logsContainer = nil } })) {
            logsSheet
        }
        .sheet(isPresented: Binding(get: { shellCommand != nil }, set: { if !$0 { shellCommand = nil } })) {
            if let shellCommand {
                TerminalSessionView(host: host, password: password, initialCommand: shellCommand, store: store)
                    .padding()
            }
        }
    }

    private func row(_ c: DockerContainer) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Circle().fill(c.isRunning ? Color.green : Color.gray).frame(width: 10, height: 10)
                Text(c.name).emphasized()
                Spacer()
                if model.busyNames.contains(c.name) { ProgressView() }
            }
            Text("\(c.image) — \(c.status)").foregroundColor(.gray)
            HStack(spacing: 6) {
                if c.isRunning {
                    Button("Stoppa") { act(.stop, c) }
                    Button("Starta om") { act(.restart, c) }
                } else {
                    Button("Starta") { act(.start, c) }
                }
                Button("Logg") {
                    logsContainer = c.name
                    logsText = "Hämtar…"
                    Task { logsText = await model.fetchLogs(c.name) }
                }
                if c.isRunning, let cmd = try? DockerService.execShellCommand(c.name) {
                    Button("Shell") { shellCommand = cmd }
                }
            }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color.gray.opacity(0.08)))
    }

    private func act(_ kind: DockerAction, _ c: DockerContainer) {
        Task { await model.act(kind, on: c.name) }
    }

    private var logsSheet: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(logsContainer ?? "Logg").font(.headline)
            ScrollView {
                Text(logsText)
                    .font(.system(size: 12, design: .monospaced))
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(minHeight: 300)
            Button("Stäng") { logsContainer = nil }
        }
        .padding()
    }
}
