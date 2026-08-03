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
            case .active: if lock.isUnlocked { lock.reveal() }
            @unknown default: break
            }
        }
    }
}
#endif
