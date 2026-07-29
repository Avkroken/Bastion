import SwiftCrossUI

/// Motsvarar `App/HostColor.swift` — samma namnpalett, samexisterar via det
/// gemensamma `Host.colorTag: String?` i SSHCore.
enum HostColorPalette {
    static let names: [String] = ["red", "orange", "yellow", "green", "blue", "purple", "gray"]

    static func color(for name: String?) -> Color? {
        switch name {
        case "red": return .red
        case "orange": return .orange
        case "yellow": return .yellow
        case "green": return .green
        case "blue": return .blue
        case "purple": return .purple
        case "gray": return .gray
        default: return nil
        }
    }
}

/// Rad av färgcirklar — tryck för att välja, tryck igen på vald för att ta bort.
/// SwiftCrossUI saknar `.onTapGesture`/ram-highlight på formvyer, så varje
/// cirkel är en `Button` istället — vald färg markeras med en tjockare kant.
struct HostColorPicker: View {
    @Binding var selection: String?

    var body: some View {
        HStack(spacing: 8) {
            ForEach(HostColorPalette.names, id: \.self) { name in
                Button(selection == name ? "●" : "○") {
                    selection = (selection == name) ? nil : name
                }
                .foregroundColor(HostColorPalette.color(for: name) ?? .gray)
            }
        }
    }
}
