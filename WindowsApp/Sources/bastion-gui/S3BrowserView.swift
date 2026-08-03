import Foundation
import SSHCore
import SwiftCrossUI

@MainActor
final class S3ConnectionListModel: ObservableObject {
    private let store = S3ConnectionStore()
    @Published var connections: [S3Connection] = []

    init() { reload() }
    func reload() { connections = store.all() }
    func save(_ connection: S3Connection) { store.upsert(connection); reload() }
    func delete(_ connection: S3Connection) { store.delete(connection.id); reload() }
}

/// Innehållet skickas som text (klistra in/redigera) — samma pragmatiska
/// v1-avgränsning som WireGuard-profilernas råtextredigering och SFTP-
/// filhanterarens textredigerare: ingen native filväljare i SwiftCrossUI,
/// och binärt innehåll är inte målet för v1 (konfigfiler, loggar, README:er
/// osv. är de rimliga första användningsfallen för en objektlagringsvy).
@MainActor
final class S3BrowserModel: ObservableObject {
    enum Level {
        case buckets
        case objects(bucket: String)
    }
    @Published var level: Level = .buckets
    @Published var buckets: [S3Bucket] = []
    @Published var objects: [S3Object] = []
    @Published var errorMessage: String?
    @Published var isLoading = false

    private let client: S3Client

    init(connection: S3Connection) {
        client = S3Client(
            endpoint: connection.endpointURL ?? URL(string: "https://localhost")!,
            region: connection.region, credentials: connection.credentials)
    }

    func loadBuckets() async {
        isLoading = true
        errorMessage = nil
        do { buckets = try await client.listBuckets() } catch { errorMessage = "\(error)" }
        isLoading = false
    }

    func createBucket(_ name: String) async {
        do {
            try await client.createBucket(name)
            await loadBuckets()
        } catch { errorMessage = "\(error)" }
    }

    func deleteBucket(_ name: String) async {
        do {
            try await client.deleteBucket(name)
            await loadBuckets()
        } catch { errorMessage = "\(error)" }
    }

    func openBucket(_ name: String) async {
        level = .objects(bucket: name)
        await loadObjects(bucket: name)
    }

    func loadObjects(bucket: String) async {
        isLoading = true
        errorMessage = nil
        do { objects = try await client.listObjects(bucket: bucket) } catch { errorMessage = "\(error)" }
        isLoading = false
    }

    func upload(bucket: String, key: String, content: String) async {
        do {
            try await client.putObject(bucket: bucket, key: key, data: Data(content.utf8), contentType: "text/plain")
            await loadObjects(bucket: bucket)
        } catch { errorMessage = "\(error)" }
    }

    func download(bucket: String, key: String) async -> String? {
        do {
            let data = try await client.getObject(bucket: bucket, key: key)
            return String(data: data, encoding: .utf8) ?? "(binärt innehåll, \(data.count) bytes — kan inte visas som text)"
        } catch {
            errorMessage = "\(error)"
            return nil
        }
    }

    func deleteObject(bucket: String, key: String) async {
        do {
            try await client.deleteObject(bucket: bucket, key: key)
            await loadObjects(bucket: bucket)
        } catch { errorMessage = "\(error)" }
    }
}

/// Bläddra S3-kompatibel objektlagring: buckets -> objekt -> ladda upp/ner
/// (som text) / ta bort. v1-avgränsning: text-innehåll, inte godtyckliga
/// binärfiler — se modellens doc-kommentar.
struct S3BrowserView: View {
    let connection: S3Connection
    let onBack: () -> Void

    @State private var model: S3BrowserModel
    @State private var selectedBucket: String?
    @State private var selectedObjectKey: String?
    @State private var showNewBucket = false
    @State private var newBucketName = ""
    @State private var showUpload = false
    @State private var uploadKey = ""
    @State private var uploadContent = ""
    @State private var showViewer = false
    @State private var viewerContent = ""

    init(connection: S3Connection, onBack: @escaping () -> Void) {
        self.connection = connection
        self.onBack = onBack
        self._model = State(wrappedValue: S3BrowserModel(connection: connection))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            header
            if let error = model.errorMessage {
                Text(error).foregroundColor(.red)
            }
            if model.isLoading {
                ProgressView("Laddar…")
            } else {
                content
            }
        }
        .padding()
        .task { await model.loadBuckets() }
        .sheet(isPresented: $showNewBucket) { newBucketSheet }
        .sheet(isPresented: $showUpload) { uploadSheet }
        .sheet(isPresented: $showViewer) { viewerSheet }
    }

    @ViewBuilder
    private var header: some View {
        switch model.level {
        case .buckets:
            HStack {
                Button("← Anslutningar") { onBack() }
                Text(connection.name).font(.title2)
                Spacer()
                Button("Ny bucket") { newBucketName = ""; showNewBucket = true }
            }
        case .objects(let bucket):
            HStack {
                Button("← Buckets") {
                    model.level = .buckets
                    selectedObjectKey = nil
                    Task { await model.loadBuckets() }
                }
                Text(bucket).font(.title2)
                Spacer()
                Button("Ladda upp") { uploadKey = ""; uploadContent = ""; showUpload = true }
            }
        }
    }

    @ViewBuilder
    private var content: some View {
        switch model.level {
        case .buckets:
            if model.buckets.isEmpty {
                Text("Inga buckets än.").foregroundColor(.gray)
            } else {
                List(model.buckets, id: \.name, selection: $selectedBucket) { bucket in
                    Text(bucket.name)
                }
                if let selected = selectedBucket {
                    HStack {
                        Button("Öppna") { Task { await model.openBucket(selected) } }
                        Button("Ta bort") { Task { await model.deleteBucket(selected) }; selectedBucket = nil }
                    }
                }
            }
        case .objects(let bucket):
            if model.objects.isEmpty {
                Text("Inga objekt än.").foregroundColor(.gray)
            } else {
                List(model.objects, id: \.key, selection: $selectedObjectKey) { object in
                    HStack {
                        Text(object.key)
                        Spacer()
                        Text(Self.formatBytes(object.size)).foregroundColor(.gray)
                    }
                }
                if let selected = selectedObjectKey {
                    HStack {
                        Button("Visa") {
                            Task {
                                if let text = await model.download(bucket: bucket, key: selected) {
                                    viewerContent = text
                                    showViewer = true
                                }
                            }
                        }
                        Button("Ta bort") {
                            Task { await model.deleteObject(bucket: bucket, key: selected) }
                            selectedObjectKey = nil
                        }
                    }
                }
            }
        }
    }

    private var newBucketSheet: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Ny bucket").font(.headline)
            TextField("Namn", text: $newBucketName)
            HStack {
                Button("Skapa") {
                    let name = newBucketName.trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !name.isEmpty else { return }
                    Task { await model.createBucket(name) }
                    showNewBucket = false
                }
                .disabled(newBucketName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("Avbryt") { showNewBucket = false }
            }
        }
        .padding()
    }

    private var uploadSheet: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Ladda upp objekt").font(.headline)
            TextField("Nyckel (t.ex. mapp/fil.txt)", text: $uploadKey)
            TextEditor(text: $uploadContent)
            HStack {
                Button("Ladda upp") {
                    guard case .objects(let bucket) = model.level else { return }
                    let key = uploadKey.trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !key.isEmpty else { return }
                    Task { await model.upload(bucket: bucket, key: key, content: uploadContent) }
                    showUpload = false
                }
                .disabled(uploadKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("Avbryt") { showUpload = false }
            }
        }
        .padding()
    }

    private var viewerSheet: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(selectedObjectKey ?? "").font(.headline)
            TextEditor(text: $viewerContent)
            HStack {
                Button("Spara ändringar") {
                    guard case .objects(let bucket) = model.level, let key = selectedObjectKey else { return }
                    Task { await model.upload(bucket: bucket, key: key, content: viewerContent) }
                    showViewer = false
                }
                Button("Stäng") { showViewer = false }
            }
        }
        .padding()
    }

    static func formatBytes(_ n: Int64) -> String {
        ByteCountFormatter.string(fromByteCount: n, countStyle: .binary)
    }
}

/// Toppnivåsheet: lista sparade S3-anslutningar, lägg till/ändra/ta bort,
/// öppna en för att bläddra. Motsvarar `WireGuardProfileListView` för
/// S3-kompatibel objektlagring.
struct S3ConnectionListView: View {
    @State private var model = S3ConnectionListModel()
    @State private var editingConnection: S3Connection?
    @State private var showEditor = false
    @State private var selectedConnectionID: UUID?
    @State private var browsingConnection: S3Connection?

    private var selectedConnection: S3Connection? {
        model.connections.first { $0.id == selectedConnectionID }
    }

    var body: some View {
        Group {
            if let browsingConnection {
                S3BrowserView(connection: browsingConnection, onBack: { self.browsingConnection = nil })
            } else {
                listView
            }
        }
    }

    private var listView: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("S3-lagring").font(.title2)
                Spacer()
                Button("Ny anslutning") {
                    editingConnection = S3Connection(
                        name: "", endpoint: "", region: "us-east-1", accessKeyID: "", secretAccessKey: "")
                    showEditor = true
                }
            }
            Text("S3-kompatibel objektlagring (AWS S3, Hostup, MinIO, Ceph RGW m.fl.) — egna nycklar, inget OAuth.")
                .foregroundColor(.gray)

            if model.connections.isEmpty {
                Text("Inga sparade anslutningar än.").foregroundColor(.gray)
            } else {
                List(model.connections, id: \.id, selection: $selectedConnectionID) { connection in
                    VStack(alignment: .leading) {
                        Text(connection.name)
                        Text("\(connection.endpoint) · \(connection.region)").foregroundColor(.gray)
                    }
                }
                if let selected = selectedConnection {
                    HStack {
                        Button("Bläddra") { browsingConnection = selected }
                        Button("Ändra") { editingConnection = selected; showEditor = true }
                        Button("Ta bort") { model.delete(selected); selectedConnectionID = nil }
                    }
                }
            }
        }
        .padding()
        .sheet(isPresented: $showEditor) {
            if let editingConnection {
                S3ConnectionEditView(
                    connection: editingConnection,
                    onSave: { model.save($0); showEditor = false },
                    onCancel: { showEditor = false }
                )
            }
        }
    }
}
