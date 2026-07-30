import Foundation
import SSHCore
import SwiftCrossUI

/// Redigerar en `S3Connection` fält-för-fält (till skillnad från WireGuards
/// råtextredigering — en S3-anslutning har inte en naturlig "klistra in
/// hela filen"-motsvarighet, användaren har separata värden: endpoint,
/// region, åtkomstnyckel, hemlig nyckel).
struct S3ConnectionEditView: View {
    let connection: S3Connection
    let onSave: (S3Connection) -> Void
    let onCancel: () -> Void

    @State private var name: String
    @State private var endpoint: String
    @State private var region: String
    @State private var accessKeyID: String
    @State private var secretAccessKey: String

    init(connection: S3Connection, onSave: @escaping (S3Connection) -> Void, onCancel: @escaping () -> Void) {
        self.connection = connection
        self.onSave = onSave
        self.onCancel = onCancel
        self._name = State(wrappedValue: connection.name)
        self._endpoint = State(wrappedValue: connection.endpoint)
        self._region = State(wrappedValue: connection.region)
        self._accessKeyID = State(wrappedValue: connection.accessKeyID)
        self._secretAccessKey = State(wrappedValue: connection.secretAccessKey)
    }

    private var canSave: Bool {
        !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && URL(string: endpoint) != nil
            && !accessKeyID.isEmpty
            && !secretAccessKey.isEmpty
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("S3-anslutning").font(.headline)
            TextField("Namn", text: $name)
            TextField("Endpoint (t.ex. https://s3.hostup.se)", text: $endpoint)
            TextField("Region (t.ex. us-east-1)", text: $region)
            TextField("Åtkomstnyckel (Access Key ID)", text: $accessKeyID)
            SecureField("Hemlig nyckel (Secret Access Key)", text: $secretAccessKey)
            HStack {
                Button("Spara") {
                    var updated = connection
                    updated.name = name.trimmingCharacters(in: .whitespacesAndNewlines)
                    updated.endpoint = endpoint
                    updated.region = region.trimmingCharacters(in: .whitespacesAndNewlines)
                    updated.accessKeyID = accessKeyID
                    updated.secretAccessKey = secretAccessKey
                    onSave(updated)
                }
                .disabled(!canSave)
                Button("Avbryt") { onCancel() }
            }
        }
        .padding()
    }
}
