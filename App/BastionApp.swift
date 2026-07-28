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
                if lock.isUnlocked {
                    lock.reveal()
                } else if lock.isEnabled {
                    // Mer pålitlig utlösningspunkt än AppLockViews egen
                    // `.task` (vy-appearing) — den kan racea mot systemets
                    // egen scen-övergång och tystnad utan att Face ID-
                    // dialogen någonsin faktiskt visas (TestFlight-feedback
                    // 2026-07-28). `authenticate()`s egen `isAuthenticating`-
                    // spärr gör att båda trigga-vägarna kan finnas kvar utan
                    // att dubbelanropa.
                    Task { await lock.authenticate() }
                }
            @unknown default: break
            }
        }
    }
}
#endif
