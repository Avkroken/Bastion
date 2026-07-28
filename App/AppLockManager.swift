#if canImport(SwiftUI)
import SwiftUI
import LocalAuthentication

enum AppLockKeys {
    static let enabled = "appLockEnabled"
}

/// Låser hela appen bakom Face ID/Touch ID/enhetslösenkod när den kommer
/// tillbaka från bakgrunden. Frivilligt (av som standard) — Keychain skyddar
/// redan hemligheterna på disk i sig, det här är ett extra UI-lager mot att
/// någon som plockar upp den olåsta telefonen rakt av ser host-listan.
@MainActor
final class AppLockManager: ObservableObject {
    // Startvärdet läses ur det sparade läget — inte bara `true` — annars
    // renderas host-listan olåst under ett ögonblick vid varje KALLSTART
    // (force-quit + omstart), eftersom lock() bara körs vid .background och
    // aldrig hinner köras innan första body-renderingen (CodeRabbit-fynd,
    // säkerhetskritiskt).
    @Published var isUnlocked: Bool
    /// Sant så fort scenen blir `.inactive` — tidigare och säkrare tidpunkt
    /// att dölja innehåll på än `.background`, eftersom iOS tar App Switcher-
    /// ögonblicksbilden strax efter att scenen blir bakgrundad men startar
    /// övergången redan vid `.inactive` (CodeRabbit-fynd). Kräver inte en ny
    /// autentisering i sig — bara ett visuellt lock tills appen är aktiv igen.
    @Published var isObscured = false

    /// Sant om enheten inte har någon autentiseringsmetod konfigurerad
    /// (varken biometri eller lösenkod) — AppLockView visar då en förklaring
    /// + en EXPLICIT "stäng av"-knapp istället för att tyst släppa igenom
    /// (fail-closed, CodeRabbit-fynd: den gamla vägen låste ute produktions-
    /// användare aldrig på riktigt, det räckte att sakna lösenkod).
    @Published var noAuthMethodAvailable = false

    var isEnabled: Bool {
        UserDefaults.standard.bool(forKey: AppLockKeys.enabled)
    }

    init() {
        isUnlocked = !UserDefaults.standard.bool(forKey: AppLockKeys.enabled)
    }

    /// Kort nådperiod för spökövergångar till `.background` — att stänga en
    /// nästlad `fullScreenCover` (t.ex. terminalsessionen i `SessionView`,
    /// särskilt med SwiftTerms egna tangentbords-accessory som first
    /// responder) kan på vissa iOS-versioner utlösa en BLIXTSNABB, äkta
    /// `.background`-övergång utan att appen faktiskt lämnat förgrunden
    /// (TestFlight-feedback 2026-07-28: "man hamnar direkt på låsskärmen"
    /// direkt efter att en session stängts — ingen riktig bakgrundning
    /// hann ske). Bara en `.background` som varar LÄNGRE än detta räknas
    /// som en riktig bakgrundning värd att kräva ny autentisering för.
    private static let backgroundGraceInterval: Duration = .seconds(2)

    /// Satt vid `.background` — tidsstämpeln avgör vid nästa `.active` om
    /// det var en riktig bakgrundning eller en spökövergång (se ovan).
    /// `ContinuousClock`, INTE `Date` (CodeRabbit-fynd) — `Date`/väggklockan
    /// kan hoppa (NTP-synk, manuell tidsändring, tidszonbyte) och skulle
    /// kunna förkorta/förlänga den uppmätta bakgrundstiden felaktigt;
    /// `ContinuousClock` fortsätter bara oavbrutet så länge enheten är på.
    private var backgroundedAt: ContinuousClock.Instant?

    /// Ökas varje `lock()`-anrop — låter en redan pågående `authenticate()`
    /// upptäcka att en NY bakgrundning inträffat sedan den startade och
    /// avstå från att committa ett förlegat resultat (CodeRabbit-fynd:
    /// utan detta kunde ett sent lyckat Face ID-svar från FÖRE en
    /// bakgrundning felaktigt låsa upp appen EFTER den).
    private var lockGeneration = 0

    /// Anropas vid `.inactive` — döljer innehållet direkt (se `isObscured`).
    func obscure() {
        guard isEnabled else { return }
        isObscured = true
    }

    /// Anropas vid `.background` — noterar BARA tidpunkten. Om appen är
    /// tillbaka i förgrunden inom `backgroundGraceInterval` betraktas det
    /// som en spökövergång (se ovan) och `isUnlocked` rörs aldrig — annars
    /// låser `resolveForeground()` appen retroaktivt vid `.active`.
    func lock() {
        guard isEnabled else { return }
        backgroundedAt = ContinuousClock.now
        lockGeneration += 1
    }

    /// Anropas vid `.active`. Avgör om den senaste `.background`-övergången
    /// (om någon) var äkta eller en spökövergång, och låser appen retroaktivt
    /// bara i det äkta fallet. Returnerar `true` om appen förblir/blir låst
    /// (anroparen ska då trigga `authenticate()`), annars `false`.
    @discardableResult
    func resolveForeground() -> Bool {
        defer { backgroundedAt = nil }
        guard isEnabled else { return false }
        if let backgroundedAt, ContinuousClock.now - backgroundedAt < Self.backgroundGraceInterval {
            debugLog("applock", "spökövergång till bakgrunden ignorerad (< \(Self.backgroundGraceInterval))")
            isObscured = false
            // Redan olåst förblir olåst (ren spökövergång) — men var appen
            // redan LÅST innan denna korta blink ska den förbli låst, inte
            // tyst rapporteras som upplåst (CodeRabbit-fynd: den gamla
            // koden retunerade ovillkorligt `false` här).
            return !isUnlocked
        }
        if backgroundedAt != nil { isUnlocked = false }
        return !isUnlocked
    }

    /// Anropas vid `.active` om appen ALDRIG hann låsas (t.ex. en kort
    /// `.inactive`-blink från Kontrollcenter/ett systemlarm) — då krävs ingen
    /// ny autentisering, bara att ta bort skyddet igen.
    func reveal() {
        isObscured = false
    }

    /// Skyddar mot att `.task` (vy-appearing) OCH den explicita
    /// `scenePhase == .active`-triggern (se `BastionApp.swift`) båda kallar
    /// `authenticate()` för samma upplåsningsförsök — Face ID-dialogen kan
    /// annars öppnas två gånger i rad (TestFlight-feedback 2026-07-28: Face
    /// ID kändes igen men appen krävde ändå ett manuellt tryck — troligen
    /// för att det FÖRSTA anropet racade mot systemets egen scen-övergång
    /// och tystnade, utan att något andra försök någonsin gjordes).
    private var isAuthenticating = false

    @discardableResult
    func authenticate() async -> Bool {
        guard isEnabled else { isUnlocked = true; isObscured = false; return true }
        guard !isAuthenticating else {
            debugLog("applock", "authenticate() anropad medan ett försök redan pågår — ignorerar")
            return false
        }
        isAuthenticating = true
        defer { isAuthenticating = false }
        // Fångar generationen VID START — om lock() kör medan det här
        // försöket väntar på LocalAuthentication (ny bakgrundning under en
        // pågående Face ID-dialog) ska ett sent lyckat svar INTE committa
        // isUnlocked=true för en app som redan blivit låst under tiden
        // (CodeRabbit-fynd).
        let generationAtStart = lockGeneration
        func commit(_ unlocked: Bool) -> Bool {
            guard generationAtStart == lockGeneration else {
                debugLog("applock", "authenticate()-resultat förkastat — appen bakgrundades igen under väntan")
                return false
            }
            isUnlocked = unlocked
            if unlocked { isObscured = false }
            return unlocked
        }
        debugLog("applock", "authenticate() startar")
        let context = LAContext()
        var error: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &error) else {
            // Fail-closed: förbli låst. noAuthMethodAvailable låter AppLockView
            // visa en förklaring + en EXPLICIT "stäng av App-lås"-knapp, i
            // stället för att tyst släppa igenom (vilket gjorde funktionen
            // verkningslös för alla utan lösenkod konfigurerat).
            noAuthMethodAvailable = true
            isUnlocked = false
            return false
        }
        noAuthMethodAvailable = false

        // Begär biometri EXPLICIT först. Med enbart den kombinerade
        // .deviceOwnerAuthentication-policyn hoppar iOS ofta direkt till
        // lösenkoden i stället för att ens visa Face ID/Touch ID — användaren
        // fick skriva sin PIN varje gång (TestFlight-feedback 2026-07-10).
        // .deviceOwnerAuthenticationWithBiometrics tvingar fram biometri när
        // enheten har det inrullat; misslyckas/avbryts det faller vi vidare
        // till lösenkoden nedan.
        if context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil) {
            do {
                let ok = try await context.evaluatePolicy(
                    .deviceOwnerAuthenticationWithBiometrics, localizedReason: "Lås upp Bastion")
                if ok {
                    debugLog("applock", "biometri lyckades")
                    return commit(true)
                }
            } catch {
                // do/catch, INTE try? (CodeRabbit-fynd) — loggar det FAKTISKA
                // felet från evaluatePolicy istället för canEvaluatePolicys
                // bioError, som bara beskriver FÖRUTSÄTTNINGARNA (är
                // biometri konfigurerat?) inte varför SJÄLVA försöket
                // misslyckades (avbrutet, för många fel, låst ut, etc).
                debugLog("applock", "biometri-försöket kastade fel: \(error.localizedDescription), faller tillbaka på lösenkod")
            }
        } else {
            debugLog("applock", "biometri otillgänglig, faller tillbaka på lösenkod")
        }

        // Biometri saknas/nekades/misslyckades → lösenkod. En förbrukad
        // LAContext återanvänds inte, så skapa en färsk.
        let fallback = LAContext()
        do {
            let success = try await fallback.evaluatePolicy(
                .deviceOwnerAuthentication, localizedReason: "Lås upp Bastion")
            return commit(success)
        } catch {
            debugLog("applock", "lösenkodsförsöket kastade fel: \(error.localizedDescription)")
            return commit(false)
        }
    }

    /// Explicit utväg när enheten saknar autentiseringsmetod — kräver ett
    /// aktivt tryck från användaren (skiljer sig från en tyst bypass).
    func disableLockDueToMissingAuthMethod() {
        UserDefaults.standard.set(false, forKey: AppLockKeys.enabled)
        noAuthMethodAvailable = false
        isUnlocked = true
        isObscured = false
    }
}

/// Enkelt, icke-interaktivt skydd mot att känsligt innehåll syns i App
/// Switcher-ögonblicksbilden — visas direkt vid `.inactive`, ingen
/// autentisering krävs för att den ska försvinna igen (se `reveal()`).
struct PrivacyCoverView: View {
    var body: some View {
        Image(systemName: "lock.shield")
            .font(.system(size: 48))
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            // Solid, INTE .regularMaterial — en genomskinlig bakgrund kan
            // fortfarande läcka host-listan i App Switcher-ögonblicksbilden,
            // som är hela poängen med den här vyn (CodeRabbit-fynd).
            .background(Color(white: 0.05))
            // Tvingar mörkt färgschema så .secondary/ikonen faktiskt syns mot
            // den fasta mörka bakgrunden — annars följer de systemets läge
            // och blir oläsliga i ljust läge (CodeRabbit-fynd).
            .environment(\.colorScheme, .dark)
    }
}

/// Visas ovanpå allt annat när appen är låst.
struct AppLockView: View {
    @ObservedObject var manager: AppLockManager

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "lock.shield")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
            Text("Bastion är låst").font(.headline)
            if manager.noAuthMethodAvailable {
                Text("Ingen Face ID/Touch ID/lösenkod konfigurerad på enheten.")
                    .font(.footnote).foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal)
                Button("Stäng av App-lås") { manager.disableLockDueToMissingAuthMethod() }
            } else {
                Button("Lås upp") { Task { await manager.authenticate() } }
                    .buttonStyle(.borderedProminent)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(white: 0.05))
        .environment(\.colorScheme, .dark)
        .task { await manager.authenticate() }
    }
}

/// Minimal, egen inställningsyta (inte del av Sync-vyn — orelaterat syfte).
struct AppLockSettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @AppStorage(AppLockKeys.enabled) private var enabled = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Toggle("Kräv Face ID/Touch ID", isOn: $enabled)
                } footer: {
                    Text("Låser appen när den kommer tillbaka från bakgrunden. Faller tillbaka på enhetens lösenkod om biometri inte är tillgänglig.")
                }
            }
            .navigationTitle("App-lås")
            .navInlineTitle()
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Klar") { dismiss() }
                }
            }
        }
    }
}
#endif
