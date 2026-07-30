import SSHCore
import SwiftCrossUI

/// Ansluter till en Telnet-värd — inget lösenord/nyckelval, till skillnad
/// från SSH: Telnet-autentisering (om servern ens kräver någon) sker inuti
/// själva terminalsessionen (login-prompt), inte som ett separat handskaknings-
/// steg. Ingen sparning i värdlistan. Speglar App/TelnetConnectView.swift.
struct TelnetConnectView: View {
    @State private var hostName = ""
    @State private var portText = "23"
    let onConnect: (TelnetTarget) -> Void
    let onCancel: () -> Void

    private var isValid: Bool {
        !hostName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && Int(portText.trimmingCharacters(in: .whitespacesAndNewlines)).map { (1...65_535).contains($0) } == true
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Telnet").font(.title2)
            TextField("Värd (t.ex. 10.0.0.5)", text: $hostName)
            TextField("Port", text: $portText)
            Text("Telnet är okrypterat — använd bara på ett nätverk du litar på "
                 + "(t.ex. mot nätverksutrustning som saknar SSH-stöd).")
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
        let cleanHost = hostName.trimmingCharacters(in: .whitespacesAndNewlines)
        onConnect(TelnetTarget(host: cleanHost, port: port))
    }
}
