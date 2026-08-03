import SSHCore
import SwiftCrossUI

/// Anslut till en ad-hoc värd UTAN att spara den i host-listan — Termius
/// kallar detta "Quick Connect". Bygger en `Host` i minnet (aldrig skickad
/// till `HostStore`). Speglar App/QuickConnectView.swift.
struct QuickConnectView: View {
    @State private var hostName = ""
    @State private var user = ""
    @State private var portText = "22"
    @State private var password = ""
    let onConnect: (Host, String?) -> Void
    let onCancel: () -> Void

    private var isValid: Bool {
        !hostName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !user.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && Int(portText.trimmingCharacters(in: .whitespacesAndNewlines)).map { (1...65_535).contains($0) } == true
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Snabbanslutning").font(.title2)
            TextField("Värd (t.ex. 10.0.0.5)", text: $hostName)
            TextField("Användare", text: $user)
            TextField("Port", text: $portText)
            SecureField("Lösenord (tomt = standardnyckeln ~/.ssh/id_ed25519)", text: $password)
            Text("Den här värden sparas INTE i din värdlista — perfekt för en "
                 + "engångsanslutning. Lägg till den vanligt om du vill återansluta senare.")
                .foregroundColor(.gray)
            HStack {
                Button("Avbryt") { onCancel() }
                Button("Anslut") { connect() }.disabled(!isValid)
            }
        }
        .padding()
        .frame(minWidth: 320)
    }

    private func connect() {
        guard let port = Int(portText.trimmingCharacters(in: .whitespacesAndNewlines)) else { return }
        // Lösenordet skickas OBESKURET — trimning hade tyst korrumperat ett
        // giltigt lösenord med inlednings-/avslutande blanktecken (samma
        // fynd som App/QuickConnectView.swift, cubic PR #173). Värd/
        // användare TRIMMAS dock (till skillnad från lösenordet) — annars
        // godkänner isValid ett fält med bara omgivande blanktecken, men
        // anslutningen skickas obeskuren och misslyckas (cubic-fynd, denna PR).
        let host = Host(
            alias: "",
            hostName: hostName.trimmingCharacters(in: .whitespacesAndNewlines),
            user: user.trimmingCharacters(in: .whitespacesAndNewlines),
            port: port,
            auth: password.isEmpty ? .agentDefault : .askPassword)
        onConnect(host, password.isEmpty ? nil : password)
    }
}
