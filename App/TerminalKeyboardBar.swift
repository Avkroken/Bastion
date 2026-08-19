#if os(iOS)
import UIKit
import SSHCore

/// Raden med terminaltangenter ovanför iOS mjukvarutangentbord.
///
/// VISION:s egen sektion: *"Här kan konkurrenter överträffas: Ctrl, Esc,
/// Tab, Pilar, Alt, F1–F12, snabbkommandon, programmerbara knappar."*
/// Ingen av dem finns på systemtangentbordet, och utan dem går det inte
/// att avbryta ett kommando, komplettera en sökväg eller ta sig ur `vi`.
///
/// # Vad som ligger här och vad som ligger i SSHCore
///
/// Den här filen är knappar och layout. Vilka BYTES en tangent skickar,
/// och hur klibbiga modifierare beter sig, ligger i
/// `SSHCore.TerminalKeys` — där går det att testa i CI. Att lägga
/// sekvenserna i en vy hade betytt att `ESC [ 1 8 ~` bara kunde
/// kontrolleras för hand på en enhet.
///
/// # Varför inte SwiftTerms egen `TerminalAccessory`
///
/// SwiftTerm installerar en egen rad som standard, och den täcker en del
/// av listan. Uppmätt i v1.19.0 (`iOSAccessoryView.swift`), inte antaget:
///
/// - **finns**: esc, ctrl, tab, pilar med autorepetition
/// - **saknas helt**: Alt/Meta — ingen träff i hela filen
/// - **saknas**: F11 och F12; bara F1–F10 är definierade
/// - **villkorat**: F-tangenterna läggs till via `addOptional`, som
///   hoppar över dem när den horisontella platsen tar slut. På en telefon
///   försvinner de tysta — precis den skärm de behövs mest på.
/// - **saknas**: programmerbara knappar
///
/// Den här raden ersätter alltså inte något som redan fungerar, utan
/// stänger de fyra luckorna. Ctrl finns i båda, men här som en klibbig
/// modifierare som också kan kombineras med Alt.
///
/// # Två rader, inte en lång
///
/// F1–F12 får en egen rad bakom en `Fn`-växel i stället för att ligga
/// efter pilarna i samma scrollvy. Det är samma problem `addOptional`
/// försöker lösa, fast utan att tappa tangenter: tolv extra knappar hade
/// tryckt ut Ctrl och Esc utanför skärmen, och de behövs oftast.
@MainActor
final class TerminalKeyboardBar: UIInputView {

    /// Anropas med de bytes som ska skickas till fjärrsidan.
    private let send: ([UInt8]) -> Void
    /// Anropas när användaren vill infoga ett sparat kommando.
    private let onSnippets: (() -> Void)?

    private var modifiers = TerminalKeys.Modifiers()
    private var showingFunctionKeys = false

    private let scroller = UIScrollView()
    private let stack = UIStackView()
    private var controlButton: UIButton?
    private var altButton: UIButton?

    init(send: @escaping ([UInt8]) -> Void, onSnippets: (() -> Void)? = nil) {
        self.send = send
        self.onSnippets = onSnippets
        super.init(
            frame: CGRect(x: 0, y: 0, width: 0, height: 48),
            inputViewStyle: .keyboard
        )
        allowsSelfSizing = true
        translatesAutoresizingMaskIntoConstraints = false

        scroller.showsHorizontalScrollIndicator = false
        scroller.translatesAutoresizingMaskIntoConstraints = false
        stack.axis = .horizontal
        stack.spacing = 6
        stack.alignment = .center
        stack.translatesAutoresizingMaskIntoConstraints = false

        addSubview(scroller)
        scroller.addSubview(stack)
        NSLayoutConstraint.activate([
            scroller.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            scroller.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            scroller.topAnchor.constraint(equalTo: topAnchor, constant: 6),
            scroller.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -6),
            stack.leadingAnchor.constraint(equalTo: scroller.contentLayoutGuide.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: scroller.contentLayoutGuide.trailingAnchor),
            stack.topAnchor.constraint(equalTo: scroller.contentLayoutGuide.topAnchor),
            stack.bottomAnchor.constraint(equalTo: scroller.contentLayoutGuide.bottomAnchor),
            stack.heightAnchor.constraint(equalTo: scroller.frameLayoutGuide.heightAnchor),
        ])

        rebuild()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) används inte") }

    // MARK: - Uppbyggnad

    private func rebuild() {
        stack.arrangedSubviews.forEach {
            stack.removeArrangedSubview($0)
            $0.removeFromSuperview()
        }
        if showingFunctionKeys {
            stack.addArrangedSubview(makeButton("abc") { [weak self] in
                self?.showingFunctionKeys = false
                self?.rebuild()
            })
            for n in 1...12 {
                stack.addArrangedSubview(makeButton("F\(n)") { [weak self] in
                    guard let bytes = TerminalKeys.function(n) else { return }
                    self?.send(bytes)
                })
            }
            return
        }

        let esc = makeButton("esc") { [weak self] in self?.send(TerminalKeys.escape) }
        stack.addArrangedSubview(esc)

        let ctrl = makeButton("ctrl") { [weak self] in
            self?.modifiers.toggleControl()
            self?.refreshModifierAppearance()
        }
        controlButton = ctrl
        stack.addArrangedSubview(ctrl)

        let alt = makeButton("alt") { [weak self] in
            self?.modifiers.toggleAlt()
            self?.refreshModifierAppearance()
        }
        altButton = alt
        stack.addArrangedSubview(alt)

        stack.addArrangedSubview(makeButton("tab") { [weak self] in self?.send(TerminalKeys.tab) })

        for (title, arrow) in [
            ("←", TerminalKeys.Arrow.left),
            ("↓", .down),
            ("↑", .up),
            ("→", .right),
        ] {
            stack.addArrangedSubview(makeButton(title) { [weak self] in self?.send(arrow.bytes) })
        }

        for (title, key) in [
            ("home", TerminalKeys.Navigation.home),
            ("end", .end),
            ("pgup", .pageUp),
            ("pgdn", .pageDown),
        ] {
            stack.addArrangedSubview(makeButton(title) { [weak self] in self?.send(key.bytes) })
        }

        stack.addArrangedSubview(makeButton("Fn") { [weak self] in
            self?.showingFunctionKeys = true
            self?.rebuild()
        })

        // "Programmerbara knappar" ur VISION: kommandobiblioteket finns
        // redan, så knappen öppnar det i stället för att införa ett andra
        // ställe att spara kommandon på.
        if onSnippets != nil {
            stack.addArrangedSubview(makeButton("⌘") { [weak self] in self?.onSnippets?() })
        }

        refreshModifierAppearance()
    }

    private func makeButton(_ title: String, action: @escaping () -> Void) -> UIButton {
        var config = UIButton.Configuration.gray()
        config.title = title
        config.baseForegroundColor = .label
        config.cornerStyle = .medium
        config.contentInsets = NSDirectionalEdgeInsets(top: 6, leading: 10, bottom: 6, trailing: 10)

        let button = UIButton(configuration: config, primaryAction: UIAction { _ in action() })
        button.titleLabel?.adjustsFontForContentSizeCategory = true
        // Fysisk minsta träffyta. Under 44 pt blir raden oanvändbar med
        // tummen, vilket är hela poängen med att den finns.
        button.heightAnchor.constraint(greaterThanOrEqualToConstant: 36).isActive = true
        button.widthAnchor.constraint(greaterThanOrEqualToConstant: 44).isActive = true
        return button
    }

    /// En armerad modifierare måste SYNAS. Utan återkoppling vet man inte
    /// om nästa tryck blir `c` eller `Ctrl+C`, och det är skillnaden
    /// mellan att skriva ett tecken och att avbryta en körning.
    private func refreshModifierAppearance() {
        for (button, active) in [
            (controlButton, modifiers.control),
            (altButton, modifiers.alt),
        ] {
            guard let button else { continue }
            var config = button.configuration
            config?.baseBackgroundColor = active ? .tintColor : nil
            config?.baseForegroundColor = active ? .white : .label
            button.configuration = config
            button.accessibilityTraits = active ? [.button, .selected] : [.button]
        }
    }

    // MARK: - Tecken från systemtangentbordet

    /// Kopplar ett vanligt tecken genom de väntande modifierarna.
    ///
    /// Returnerar `true` när raden tog hand om tecknet — då ska
    /// terminalen INTE också skicka det själv, annars kommer det fram två
    /// gånger.
    func intercept(character: Character) -> Bool {
        guard !modifiers.isEmpty else { return false }
        var pending = modifiers
        let bytes = TerminalKeys.resolve(character: character, modifiers: &pending)
        modifiers = pending
        refreshModifierAppearance()
        if let bytes {
            send(bytes)
        }
        // Även en omöjlig kombination räknas som hanterad: användaren bad
        // om Ctrl+1, inte om en etta.
        return true
    }
}
#endif
