#if canImport(SwiftUI)
import SwiftUI

@main
struct BastionApp: App {
    @StateObject private var lock = AppLockManager()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            ZStack {
                HostListView()
                if lock.isEnabled && !lock.isUnlocked {
                    AppLockView(manager: lock)
                } else if lock.isEnabled && lock.isObscured {
                    PrivacyCoverView()
                }
            }
        }
        .onChange(of: scenePhase) { newPhase in
            switch newPhase {
            case .inactive: lock.obscure()
            case .background: lock.lock()
            case .active:
                // `resolveForeground()` avgör FÖRST om en föregående
                // `.background` (om någon) var äkta eller en spökövergång
                // (se AppLockManager) — annars skulle en snabb stängd
                // `fullScreenCover` (t.ex. avslutad terminalsession) kunna
                // trigga en oönskad omlåsning trots att appen aldrig
                // faktiskt lämnade förgrunden (TestFlight-feedback
                // 2026-07-28: "man hamnar direkt på låsskärmen").
                if lock.resolveForeground() {
                    // Mer pålitlig utlösningspunkt än AppLockViews egen
                    // `.task` (vy-appearing) — den kan racea mot systemets
                    // egen scen-övergång och tystnad utan att Face ID-
                    // dialogen någonsin faktiskt visas (TestFlight-feedback
                    // 2026-07-28). `authenticate()`s egen `isAuthenticating`-
                    // spärr gör att båda trigga-vägarna kan finnas kvar utan
                    // att dubbelanropa.
                    Task { await lock.authenticate() }
                } else if lock.isUnlocked {
                    lock.reveal()
                }
            @unknown default: break
            }
        }
    }
}
#endif
