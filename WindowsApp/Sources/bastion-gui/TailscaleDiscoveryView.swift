import Foundation
import SSHCore
import SwiftCrossUI

/// Föreslå SSH-värdar ur ett tailnet — två källor att välja mellan
/// (användaren avgör vad som är bekvämast, inte appen): den här maskinen
/// (`fetchLocal`, som ssh-config-import läser en lokal resurs) eller en
/// redan sparad fjärrvärd (`fetch(over:)`, som Dashboard-datan).
@MainActor
final class TailscaleDiscoveryModel: ObservableObject {
    enum Source { case local, remote(Host) }
    enum LoadState {
        case idle
        case loading
        case loaded([(hostName: String, address: String)])
        case failed(String)
    }
    @Published var state: LoadState = .idle

    func fetch(source: Source, password: String?, store: HostStore? = nil) async {
        state = .loading
        do {
            let status: TailscaleStatus
            switch source {
            case .local:
                status = try TailscaleStatus.fetchLocal()
            case .remote(let host):
                guard let plan = resolveConnectionPlan(for: host, password: password, store: store) else {
                    state = .failed("Kan inte autentisera värden (eller dess jump-host, om en är vald).")
                    return
                }
                let chain = try await SSHConnectionChain.connect(target: host.target, targetAuth: plan.auth, jump: plan.jump)
                // Strukturerad do-catch (inte `defer { Task {...} }`) — en
                // fristående Task frikopplar städningen från det pågående
                // async-flödet och respekterar inte förälder-tasken cancellation
                // (CodeRabbit/cubic-fynd PR #179), samma mönster som
                // DashboardView/KeyDeployView redan använder.
                do {
                    status = try await TailscaleStatus.fetch(over: chain.target)
                    await chain.close()
                } catch {
                    await chain.close()
                    throw error
                }
            }
            state = .loaded(status.suggestedHosts)
        } catch {
            state = .failed("\(error)")
        }
    }
}

/// Motsvarar `ImportConfigView`/`WireGuardProfileListView` som toppnivåsheet.
/// `onAddHost` öppnar samma "Ny värd"-flöde som sidopanelens knapp, bara
/// förifyllt med tailnet-adressen — Tailscale känner inte till SSH-
/// användarnamnet, så det sista steget är alltid det vanliga redigeringsläget.
struct TailscaleDiscoveryView: View {
    let hosts: [Host]
    let onAddHost: (_ alias: String, _ hostName: String) -> Void
    let onCancel: () -> Void
    var store: HostStore? = nil

    @State private var model = TailscaleDiscoveryModel()
    @State private var useLocal = true
    @State private var selectedHostLabel: String?
    @State private var selectedSuggestionKey: String?
    @State private var password = ""

    private func label(for host: Host) -> String {
        host.alias.isEmpty ? host.hostName : host.alias
    }

    private var selectedHost: Host? {
        hosts.first { label(for: $0) == selectedHostLabel }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Tailscale-värdar").font(.headline)
            Text("Föreslå SSH-värdar ur ett tailnet — antingen den här maskinens, eller en redan sparad fjärrvärds.")
                .foregroundColor(.gray)

            HStack {
                Button(useLocal ? "● Denna maskin" : "○ Denna maskin") { useLocal = true }
                Button(useLocal ? "○ Fjärrvärd" : "● Fjärrvärd") { useLocal = false }
            }

            if !useLocal {
                Picker(of: hosts.map(label(for:)), selection: $selectedHostLabel)
                SecureField("Lösenord (om värden kräver det)", text: $password)
            }

            Button("Hämta") {
                Task {
                    if useLocal {
                        await model.fetch(source: .local, password: nil, store: store)
                    } else if let selectedHost {
                        await model.fetch(source: .remote(selectedHost), password: password.isEmpty ? nil : password, store: store)
                    }
                }
            }
            .disabled(!useLocal && selectedHost == nil)

            resultsView

            HStack {
                Spacer()
                Button("Stäng") { onCancel() }
            }
        }
        .padding()
    }

    @ViewBuilder
    private var resultsView: some View {
        switch model.state {
        case .idle:
            EmptyView()
        case .loading:
            ProgressView("Hämtar…")
        case .failed(let message):
            Text(message).foregroundColor(.gray)
        case .loaded(let suggestions):
            if suggestions.isEmpty {
                Text("Inga online-peers med IP hittades i det tailnetet.")
                    .foregroundColor(.gray)
            } else {
                List(suggestions, id: \.hostName, selection: $selectedSuggestionKey) { suggestion in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(suggestion.hostName)
                            Text(suggestion.address).foregroundColor(.gray)
                        }
                        Spacer()
                        Button("Lägg till") { onAddHost(suggestion.hostName, suggestion.address) }
                    }
                }
            }
        }
    }
}
