#if canImport(SwiftUI)
import SwiftUI
import os

/// Enkel, synlig debug-logg — TestFlight-testare kan inte skicka
/// Console.app-loggar utan en Mac uppkopplad, så det här ger samma data
/// direkt i appen (och speglas ändå till os.Logger/Console för den som HAR
/// en Mac). Ingen fjärrrapportering (Sentry togs bort 2026-07-27) — bara
/// synligt lokalt, delbart som text via standard-dela-arket.
///
/// Ringbuffer i minnet (senaste 500 raderna) — medvetet INTE
/// diskpersisterad, för att undvika att känslig terminal-/host-data
/// (kommandon, IP-adresser, användarnamn) ligger kvar på disk efter att
/// appen stängts. Räcker för att fånga en enda testsession.
@MainActor
final class DebugLog: ObservableObject {
    static let shared = DebugLog()

    struct Entry: Identifiable {
        let id = UUID()
        let timestamp: Date
        let category: String
        let message: String
    }

    @Published private(set) var entries: [Entry] = []
    private let maxEntries = 500
    private let osLog = Logger(subsystem: "se.denied.bastion", category: "debug")

    private init() {}

    func log(_ category: String, _ message: String) {
        let entry = Entry(timestamp: Date(), category: category, message: message)
        entries.append(entry)
        if entries.count > maxEntries {
            entries.removeFirst(entries.count - maxEntries)
        }
        osLog.debug("[\(category, privacy: .public)] \(message, privacy: .public)")
    }

    func clear() {
        entries.removeAll()
    }

    var exportText: String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss.SSS"
        return entries.map { "\(formatter.string(from: $0.timestamp)) [\($0.category)] \($0.message)" }
            .joined(separator: "\n")
    }
}

/// Genväg så anropsplatser inte behöver `DebugLog.shared.log(...)` överallt.
@MainActor func debugLog(_ category: String, _ message: String) {
    DebugLog.shared.log(category, message)
}

struct DebugLogView: View {
    @ObservedObject private var log = DebugLog.shared
    @Environment(\.dismiss) private var dismiss
    @State private var showShareSheet = false

    var body: some View {
        NavigationStack {
            List(log.entries.reversed()) { entry in
                VStack(alignment: .leading, spacing: 2) {
                    Text(entry.category)
                        .font(.caption2.bold())
                        .foregroundStyle(.secondary)
                    Text(entry.message)
                        .font(.system(.caption, design: .monospaced))
                }
            }
            .navigationTitle("Debug-logg")
            .navInlineTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Klar") { dismiss() }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button("Rensa") { log.clear() }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button { showShareSheet = true } label: {
                        Image(systemName: "square.and.arrow.up")
                    }
                }
            }
            .overlay {
                if log.entries.isEmpty {
                    ContentUnavailableView("Inga loggrader än", systemImage: "doc.text.magnifyingglass")
                }
            }
            #if os(iOS)
            .sheet(isPresented: $showShareSheet) {
                ShareSheet(items: [log.exportText])
            }
            #endif
        }
    }
}

#if os(iOS)
import UIKit
struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]
    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }
    func updateUIViewController(_ uiViewController: UIActivityViewController, context: Context) {}
}
#endif
#endif
