import Foundation
import SSHCore
import SwiftCrossUI

/// SFTP-filhanterare — motsvarar `App/SFTPBrowserView.swift` +
/// `App/SFTPBrowserModel.swift`. Samma lazy-connect-mönster som
/// `DockerModel`/`SnippetListModel` här. Ingen swipe-actions eller
/// tappbar rad i SwiftCrossUI, så åtgärder körs via knappar under
/// listan istället, styrda av `List`s obligatoriska selection-binding.
@MainActor
final class SFTPBrowserModel: ObservableObject {
    @Published var currentPath = "."
    @Published var entries: [SFTPNameEntry] = []
    @Published var errorMessage: String?

    private let host: Host
    private let password: String?
    private let store: HostStore?
    private var chain: SSHConnectionChain?
    private var sftp: SFTPClient?
    private var connectingTask: Task<SFTPClient?, Never>?

    init(host: Host, password: String?, store: HostStore? = nil) {
        self.host = host
        self.password = password
        self.store = store
    }

    var sortedEntries: [SFTPNameEntry] {
        entries.sorted { a, b in
            if a.attributes.isDirectory != b.attributes.isDirectory { return a.attributes.isDirectory }
            return a.filename.lowercased() < b.filename.lowercased()
        }
    }

    var canNavigateUp: Bool { currentPath != "." }

    private func joined(_ name: String) -> String {
        currentPath == "." ? name : currentPath + "/" + name
    }

    private func ensureClient() async -> SFTPClient? {
        if let sftp { return sftp }
        if let connectingTask { return await connectingTask.value }

        let task = Task<SFTPClient?, Never> { [weak self] in
            guard let self else { return nil }
            guard let plan = resolveConnectionPlan(for: self.host, password: self.password, store: self.store) else {
                self.errorMessage = "Kan inte autentisera värden (eller dess jump-host, om en är vald)."
                return nil
            }
            let chain: SSHConnectionChain
            do {
                chain = try await SSHConnectionChain.connect(target: self.host.target, targetAuth: plan.auth, jump: plan.jump)
            } catch {
                // SSHConnectionChain.connect() städar redan sina EGNA fel
                // internt (se dess doc-kommentar) — inget att stänga här.
                self.errorMessage = "\(error)"
                return nil
            }
            do {
                let client = try await SFTPClient.open(on: chain.target)
                // disconnect() kan ha körts (vyn stängd) medan vi väntade på
                // connect()/open — utan den här kollen skulle vi återuppliva
                // self.chain/self.sftp EFTER att disconnect() redan städat,
                // och den nya anslutningen skulle aldrig stängas (CodeRabbit-
                // fynd, PR #48).
                guard !Task.isCancelled else {
                    await client.close()
                    await chain.close()
                    return nil
                }
                // self.sftp sätts HÄR, tillsammans med self.chain, inte av
                // anroparen efter att task.value returnerat — annars finns
                // ett fönster där disconnect() hinner köra (chain redan
                // satt av oss, men sftp fortfarande nil ur anroparens
                // perspektiv) mellan att tasken returnerar och anroparen
                // skriver tillbaka sftp, vilket skulle skriva över
                // disconnect()s städning med en redan stängd klient
                // (CodeRabbit-fynd, PR #50).
                self.chain = chain
                self.sftp = client
                return client
            } catch {
                // Samma läcka som i App/SFTPBrowserModel.swift: om connect()
                // lyckades men SFTPClient.open(on:) kastade sattes
                // self.chain aldrig — stäng den öppna anslutningen explicit
                // (CodeRabbit-fynd, PR #47).
                await chain.close()
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
        guard let client = await ensureClient() else { return }
        do {
            entries = try await client.listDirectory(currentPath)
                .filter { $0.filename != "." && $0.filename != ".." }
            errorMessage = nil
        } catch {
            errorMessage = "\(error)"
        }
    }

    func open(_ entry: SFTPNameEntry) async {
        guard entry.attributes.isDirectory else { return }
        currentPath = joined(entry.filename)
        await refresh()
    }

    func navigateUp() async {
        guard canNavigateUp else { return }
        if let slash = currentPath.lastIndex(of: "/") {
            currentPath = String(currentPath[currentPath.startIndex..<slash])
        } else {
            currentPath = "."
        }
        await refresh()
    }

    func delete(_ entry: SFTPNameEntry) async {
        guard let client = await ensureClient() else { return }
        let path = joined(entry.filename)
        do {
            if entry.attributes.isDirectory {
                try await client.rmdir(path)
            } else {
                try await client.remove(path)
            }
            await refresh()
        } catch {
            errorMessage = "\(error)"
        }
    }

    func createDirectory(named name: String) async {
        guard let client = await ensureClient() else { return }
        do {
            try await client.mkdir(joined(name))
            await refresh()
        } catch {
            errorMessage = "\(error)"
        }
    }

    func rename(_ entry: SFTPNameEntry, to newName: String) async {
        guard let client = await ensureClient() else { return }
        do {
            try await client.rename(joined(entry.filename), to: joined(newName))
            await refresh()
        } catch {
            errorMessage = "\(error)"
        }
    }

    /// `mode`: oktal sträng utan `0o`-prefix, t.ex. "644"/"755" — samma
    /// notation som `chmod` på kommandoraden, vilket är vad användaren
    /// redan känner till.
    func chmod(_ entry: SFTPNameEntry, mode: String) async {
        guard let value = UInt32(mode, radix: 8) else {
            errorMessage = "Ogiltig behörighet — ange tre oktala siffror, t.ex. 644."
            return
        }
        guard let client = await ensureClient() else { return }
        do {
            try await client.setPermissions(joined(entry.filename), mode: value)
            await refresh()
        } catch {
            errorMessage = "\(error)"
        }
    }

    /// Komprimerar EN post (fil eller mapp) till ett nytt arkiv i samma
    /// katalog. `archiveName` bör sluta på `.tar.gz`/`.zip` (styr inte
    /// formatet — det gör `useZip` — men läses av `extract` nedan för att
    /// välja rätt uppackningskommando).
    func compress(_ entry: SFTPNameEntry, archiveName: String, useZip: Bool) async {
        guard let client = await ensureClient(), let chain else { return }
        do {
            // SFTP:s `currentPath` och exec-kanalens arbetskatalog delar
            // typiskt startkatalog (användarens hem) men är INTE garanterat
            // samma sak — `realpath` slår upp den FAKTISKA absoluta
            // sökvägen via SFTP-protokollet, så `cd` i arkivkommandot
            // landar exakt där filhanteraren visar, oavsett.
            let absoluteDir = try await client.realpath(currentPath)
            if useZip {
                try await ArchiveOperations.createZip(
                    paths: [entry.filename], archiveName: archiveName, in: absoluteDir, over: chain.target)
            } else {
                try await ArchiveOperations.createTarGz(
                    paths: [entry.filename], archiveName: archiveName, in: absoluteDir, over: chain.target)
            }
            await refresh()
        } catch {
            errorMessage = "\(error)"
        }
    }

    /// Packar upp ett arkiv i SAMMA katalog den ligger i. Formatet avgörs
    /// av filändelsen — `.tar.gz`/`.tgz` eller `.zip`, annat avvisas tydligt
    /// istället för att gissa fel kommando.
    func extract(_ entry: SFTPNameEntry) async {
        guard let client = await ensureClient(), let chain else { return }
        do {
            let absoluteDir = try await client.realpath(currentPath)
            if entry.filename.hasSuffix(".tar.gz") || entry.filename.hasSuffix(".tgz") {
                try await ArchiveOperations.extractTarGz(archiveName: entry.filename, in: absoluteDir, over: chain.target)
            } else if entry.filename.hasSuffix(".zip") {
                try await ArchiveOperations.extractZip(archiveName: entry.filename, in: absoluteDir, over: chain.target)
            } else {
                errorMessage = "Okänt arkivformat — stödjer .tar.gz/.tgz/.zip."
                return
            }
            await refresh()
        } catch {
            errorMessage = "\(error)"
        }
    }

    /// `uidText`/`gidText`: NUMERISKA ID:n, inte användarnamn — SFTP version 3
    /// känner bara till UID/GID, aldrig namn (se `SFTPClient.chown`s
    /// doc-kommentar för varför).
    func chown(_ entry: SFTPNameEntry, uidText: String, gidText: String) async {
        guard let uid = UInt32(uidText), let gid = UInt32(gidText) else {
            errorMessage = "Ogiltigt UID/GID — ange numeriska ID:n, t.ex. 1000."
            return
        }
        guard let client = await ensureClient() else { return }
        do {
            try await client.chown(joined(entry.filename), uid: uid, gid: gid)
            await refresh()
        } catch {
            errorMessage = "\(error)"
        }
    }

    /// `nil` betyder antingen att läsningen misslyckades (se `errorMessage`)
    /// ELLER att innehållet inte är giltig UTF-8 (binärfil). `String(bytes:
    /// encoding:)` (till skillnad från `String(decoding:as:)`, som ALLTID
    /// "lyckas" genom att ersätta ogiltiga sekvenser med U+FFFD) ger `nil`
    /// vid minsta ogiltiga byte — den korrekta kollen. En tidigare version
    /// försökte detta med en round-trip-längdjämförelse istället, men det
    /// missar det ovanliga fallet där en ogiltig flerbytesekvens råkar
    /// avkodas+återkodas till exakt lika många byte som originalet.
    func loadFileContent(_ entry: SFTPNameEntry) async -> String? {
        guard let client = await ensureClient() else { return nil }
        do {
            let bytes = try await client.readFile(joined(entry.filename))
            guard let text = String(bytes: bytes, encoding: .utf8) else {
                errorMessage = "Filen verkar inte vara text (ogiltig UTF-8) — öppnas inte som redigerbar text."
                return nil
            }
            return text
        } catch {
            errorMessage = "\(error)"
            return nil
        }
    }

    func saveFileContent(_ entry: SFTPNameEntry, content: String) async {
        guard let client = await ensureClient() else { return }
        do {
            try await client.writeFile(joined(entry.filename), data: Array(content.utf8))
            errorMessage = nil
        } catch {
            errorMessage = "\(error)"
        }
    }

    func disconnect() {
        // Avbryter en ev. pågående anslutning — annars kan den hinna klart
        // EFTER städningen nedan och skriva tillbaka ett levande session/
        // sftp som aldrig stängs (CodeRabbit-fynd, PR #48).
        connectingTask?.cancel()
        let chain = self.chain
        let c = sftp
        self.chain = nil
        sftp = nil
        Task {
            await c?.close()
            await chain?.close()
        }
    }
}

struct SFTPBrowserView: View {
    @State private var model: SFTPBrowserModel
    @State private var selectedFilename: String?
    @State private var showNewFolder = false
    @State private var newFolderName = ""
    @State private var showRename = false
    @State private var renameText = ""
    @State private var showChmod = false
    @State private var chmodText = ""
    @State private var showChown = false
    @State private var chownUIDText = ""
    @State private var chownGIDText = ""
    @State private var showCompress = false
    @State private var compressNameText = ""
    @State private var compressUseZip = false
    @State private var showEditor = false
    @State private var editorText = ""
    @State private var editorFilename: String?

    init(host: Host, password: String?, store: HostStore? = nil) {
        self._model = State(wrappedValue: SFTPBrowserModel(host: host, password: password, store: store))
    }

    private var selectedEntry: SFTPNameEntry? {
        model.entries.first { $0.filename == selectedFilename }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Filer: \(model.currentPath == "." ? "/" : model.currentPath)").font(.title2)
                Spacer()
                Button("Upp") { Task { await model.navigateUp() } }
                Button("Ny mapp") { newFolderName = ""; showNewFolder = true }
                Button("Uppdatera") { Task { await model.refresh() } }
            }

            if let errorMessage = model.errorMessage {
                Text(errorMessage).foregroundColor(.red)
            }

            if showNewFolder {
                HStack {
                    TextField("Mappnamn", text: $newFolderName)
                    Button("Skapa") {
                        let name = newFolderName.trimmingCharacters(in: .whitespacesAndNewlines)
                        showNewFolder = false
                        guard !name.isEmpty else { return }
                        Task { await model.createDirectory(named: name) }
                    }
                    Button("Avbryt") { showNewFolder = false }
                }
            }

            if model.entries.isEmpty {
                Text("Tom mapp.").foregroundColor(.gray)
            } else {
                List(model.sortedEntries, id: \.filename, selection: $selectedFilename) { entry in
                    HStack {
                        Text(entry.attributes.isDirectory ? "📁 \(entry.filename)" : "📄 \(entry.filename)")
                        Spacer()
                        if !entry.attributes.isDirectory, let size = entry.attributes.size {
                            Text("\(size) B").foregroundColor(.gray)
                        }
                    }
                }
            }

            if let selected = selectedEntry {
                if showRename {
                    HStack {
                        TextField("Nytt namn", text: $renameText)
                        Button("Spara") {
                            let name = renameText.trimmingCharacters(in: .whitespacesAndNewlines)
                            showRename = false
                            guard !name.isEmpty else { return }
                            Task { await model.rename(selected, to: name) }
                        }
                        Button("Avbryt") { showRename = false }
                    }
                } else if showChmod {
                    HStack {
                        TextField("Behörighet (t.ex. 644)", text: $chmodText)
                        Button("Spara") {
                            let mode = chmodText.trimmingCharacters(in: .whitespacesAndNewlines)
                            showChmod = false
                            guard !mode.isEmpty else { return }
                            Task { await model.chmod(selected, mode: mode) }
                        }
                        Button("Avbryt") { showChmod = false }
                    }
                } else if showChown {
                    HStack {
                        TextField("UID (t.ex. 1000)", text: $chownUIDText)
                        TextField("GID (t.ex. 1000)", text: $chownGIDText)
                        Button("Spara") {
                            let uidText = chownUIDText.trimmingCharacters(in: .whitespacesAndNewlines)
                            let gidText = chownGIDText.trimmingCharacters(in: .whitespacesAndNewlines)
                            showChown = false
                            guard !uidText.isEmpty, !gidText.isEmpty else { return }
                            Task { await model.chown(selected, uidText: uidText, gidText: gidText) }
                        }
                        Button("Avbryt") { showChown = false }
                    }
                } else if showCompress {
                    HStack {
                        TextField("Arkivnamn (t.ex. \(selected.filename).tar.gz)", text: $compressNameText)
                        Toggle("zip", isOn: $compressUseZip)
                        Button("Skapa") {
                            let name = compressNameText.trimmingCharacters(in: .whitespacesAndNewlines)
                            let useZip = compressUseZip
                            showCompress = false
                            guard !name.isEmpty else { return }
                            Task { await model.compress(selected, archiveName: name, useZip: useZip) }
                        }
                        Button("Avbryt") { showCompress = false }
                    }
                } else {
                    HStack {
                        if selected.attributes.isDirectory {
                            Button("Öppna") { Task { await model.open(selected) } }
                        } else {
                            Button("Redigera") {
                                Task {
                                    guard let content = await model.loadFileContent(selected) else { return }
                                    editorFilename = selected.filename
                                    editorText = content
                                    showEditor = true
                                }
                            }
                        }
                        Button("Döp om") { renameText = selected.filename; showRename = true }
                        Button("chmod") { chmodText = ""; showChmod = true }
                        Button("chown") { chownUIDText = ""; chownGIDText = ""; showChown = true }
                        Button("Komprimera") {
                            compressNameText = selected.filename + ".tar.gz"
                            compressUseZip = false
                            showCompress = true
                        }
                        if selected.filename.hasSuffix(".tar.gz") || selected.filename.hasSuffix(".tgz")
                            || selected.filename.hasSuffix(".zip")
                        {
                            Button("Packa upp") { Task { await model.extract(selected) } }
                        }
                        Button("Ta bort") { Task { await model.delete(selected) } }
                    }
                }
            }
        }
        .padding()
        .sheet(isPresented: $showEditor) {
            VStack(alignment: .leading, spacing: 8) {
                Text(editorFilename ?? "").font(.headline)
                TextEditor(text: $editorText)
                HStack {
                    Button("Spara") {
                        guard let filename = editorFilename,
                              let entry = model.entries.first(where: { $0.filename == filename })
                        else { showEditor = false; return }
                        let content = editorText
                        showEditor = false
                        Task { await model.saveFileContent(entry, content: content) }
                    }
                    Button("Avbryt") { showEditor = false }
                }
            }
            .padding()
        }
        .task { await model.refresh() }
        // Utan detta kan ett filnamn från FÖREGÅENDE mapp råka matcha ett
        // likadant namn i den nya mappen efter navigering och visa fel
        // "vald post" — samma buggklass som CodeRabbit fann i Snippets/
        // Kommandobiblioteket (PR #34), fixad proaktivt här direkt.
        .onChange(of: model.currentPath) { selectedFilename = nil }
        // Samma mönster som DockerView.swift — utan detta stängs aldrig den
        // underliggande SSH/SFTP-anslutningen när arket stängs (CodeRabbit-
        // fynd, PR #47).
        .onDisappear { model.disconnect() }
    }
}
