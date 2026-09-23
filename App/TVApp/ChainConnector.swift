#if canImport(SwiftUI)
import SSHCore

/// Litet hjälpfel för `connect`-closures som vill avbryta med ett färdigt
/// användarmeddelande (t.ex. "kan inte autentisera") utan att det
/// formateras om av `SSHError`s enum-baserade standardbeskrivning när
/// `onFailure` gör `"\(error)"`.
struct PlainMessageError: Error, CustomStringConvertible {
    let message: String
    var description: String { message }
}

/// Delad livscykelhanterare för en lazy, cachad SSH-anslutning som flera
/// åtgärder på samma vy kan använda samtidigt (refresh/act/browse ...).
///
/// Bakgrund: `DockerModel`, `PortForwardModel` och `SFTPBrowserModel`
/// implementerade tidigare EXAKT samma mönster var för sig (cache pågående
/// anslutningsförsök, avbryt vid `disconnect()`, kolla identitet efter
/// `await` för att upptäcka en teardown som hann köra under tiden). Den
/// största delen av granskningsfynden på PR #172 var samma livscykelbugg
/// återupptäckt i tre separata kopior (t.ex. `DockerModel` saknade länge
/// samma omkontroll `PortForwardModel` redan hade). Den här typen är den
/// enda platsen mönstret nu behöver underhållas (cubic #30).
@MainActor
final class ChainConnector<Client: AnyObject> {
    private var chain: SSHConnectionChain?
    // Se kommentaren i `ensure(...)`: ger `disconnect()` något att stänga
    // under FÖNSTRET mellan att `connect()` lyckats och `open(_:)` (ett ev.
    // extra öppningssteg ovanpå kedjan, t.ex. `SFTPClient.open`) återvänder
    // — `Task.cancel()` avbryter inte en pågående, icke-avbrytbar NIO-future.
    private var connectingChain: SSHConnectionChain?
    private var client: Client?
    // `Result`, inte bara `Client?` — VARJE anropare som väntar in samma
    // task (både den som startade den och alla som kom in via den DELADE
    // vägen nedan) måste kunna se DET FAKTISKA felet och rapportera det
    // själv. En tidigare version använde en delad `raisedError`-flagga för
    // att avgöra om en generisk fallback skulle visas, men det gjorde att
    // BARA den ursprungliga anroparens `onFailure` kördes — en samtidig
    // anropare som kom in via den delade vägen såg `raisedError == true`
    // och hoppade tyst över sin egen felrapportering (tidigare telemetryfynd MEDIUM).
    private var connectingTask: Task<Result<Client, PlainMessageError>, Never>?
    private var isTornDown = false

    private let closeClient: (Client) async -> Void

    /// `closeClient`: hur en redan öppnad klient stängs INNAN kedjan
    /// stängs, om `Client` är något annat än själva sessionen (t.ex.
    /// `SFTPClient`, som håller ett eget subsystem ovanpå kedjans target).
    /// Lämna default (no-op) när `Client` bara ÄR sessionen — då räcker
    /// det att stänga kedjan.
    init(closeClient: @escaping (Client) async -> Void = { _ in }) {
        self.closeClient = closeClient
    }

    /// Målsessionen i den aktiva kedjan, oavsett `Client`-typ — för
    /// anropsplatser (som `SFTPBrowserModel`s arkivoperationer) som behöver
    /// en `SSHSession` vid sidan av den öppnade klienten.
    var target: SSHSession? { chain?.target }

    /// Returnerar den cachade klienten om en finns, väntar in ett redan
    /// pågående försök om ett sådant är i gång, eller startar ett nytt.
    ///
    /// `connect`/`open` delar upp anslutningen i två steg av samma skäl som
    /// `SFTPBrowserModel` ursprungligen gjorde det: `open` kan vara en
    /// trivial identitet (`{ $0.target }` för Docker/PortForward) eller ett
    /// eget förhandlingssteg (`SFTPClient.open`) — bara genom att hålla
    /// isär dem kan `connectingChain` publiceras EFTER `connect()` men
    /// FÖRE `open(_:)`, vilket är precis det fönster `disconnect()` annars
    /// inte hade något att agera på under.
    ///
    /// `onFailure`: ett SPECIFIKT fel från just det här anslutningsförsöket
    /// (auth/anslutning/öppning) — visas alltid.
    /// `onInterrupted`: anropet avbröts av en teardown/racea mot en NYARE
    /// anslutning, ORSAKEN är okänd för den här anroparen. Standardbeteendet
    /// (no-op) matchar `DockerModel`/`SFTPBrowserModel`s ursprungliga tysta
    /// beteende i det här fallet; `PortForwardModel` skickar in en egen
    /// closure som bara sätter ett meddelande OM inget mer specifikt redan
    /// visas (`errorMessage == nil`) — annars kunde den här generiska
    /// fallbacken skriva över ett meddelande en NYARE, samtidig anslutning
    /// redan hunnit sätta (cubic P2 på PR #186).
    func ensure(
        connect: @escaping () async throws -> SSHConnectionChain,
        open: @escaping (SSHConnectionChain) async throws -> Client,
        onFailure: @escaping (String) -> Void,
        onInterrupted: () -> Void = {}
    ) async -> Client? {
        guard !isTornDown else { return nil }
        if let client { return client }

        let task: Task<Result<Client, PlainMessageError>, Never>
        // Bara ÄGAREN (den som faktiskt startade tasken) nollar
        // `connectingTask` efteråt — en delad awaiter som bara väntade in
        // NÅGON ANNANS task vet inte om en ÄNNU NYARE task redan hunnit
        // ersätta fältet under tiden, och skulle annars kunna nolla bort
        // den nya, fortfarande pågående tasken (`Task` är ett värde, inte
        // en klass, så det finns ingen identitetsjämförelse att skydda med
        // här — bara ägarskap).
        let isOwner: Bool
        if let existing = connectingTask {
            task = existing
            isOwner = false
        } else {
            isOwner = true
            task = Task<Result<Client, PlainMessageError>, Never> { [weak self] in
                guard let self else { return .failure(PlainMessageError(message: "")) }
                let c: SSHConnectionChain
                do {
                    c = try await connect()
                } catch {
                    return .failure(PlainMessageError(message: "\(error)"))
                }
                // `disconnect()` kan ha körts medan vi väntade på `connect()`
                // ovan — utan den här kollen skulle vi återuppliva
                // `self.chain` EFTER att `disconnect()` redan städat, och den
                // nya anslutningen skulle aldrig stängas.
                guard !Task.isCancelled else {
                    await c.close()
                    return .failure(PlainMessageError(message: "Anslutningen avbröts, försök igen."))
                }
                // Synlig för disconnect() REDAN nu, se kommentaren vid fältet.
                self.connectingChain = c
                do {
                    let opened = try await open(c)
                    // "Claimar" kedjan (kollar OCH nollar `connectingChain` i
                    // samma MainActor-tur, ingen `await` emellan) innan den
                    // ev. stängs nedan — om `disconnect()` redan hunnit nolla
                    // fältet och stänga `c` själv, ska VI inte stänga den igen
                    // (dubbelstängning av samma `SSHConnectionChain` är inte
                    // garanterat säkert).
                    let stillOwnedByUs = self.connectingChain != nil
                    self.connectingChain = nil
                    // `disconnect()` sätter `isTornDown` OCH anropar
                    // `connectingTask?.cancel()` i samma oavbrutna
                    // `@MainActor`-tur — givet aktörsisoleringen kan den här
                    // kollen aldrig se `isTornDown` utan att `Task.isCancelled`
                    // redan är sant, så `Task.isCancelled` ensamt räcker
                    // (cubic P3 på PR #186: en tidigare version kollade även
                    // `!self.isTornDown` här, ett skydd mot en race som
                    // aktörsisoleringen redan utesluter).
                    guard !Task.isCancelled else {
                        await self.closeClient(opened)
                        if stillOwnedByUs { await c.close() }
                        return .failure(PlainMessageError(message: "Anslutningen avbröts, försök igen."))
                    }
                    self.chain = c
                    self.client = opened
                    return .success(opened)
                } catch {
                    let stillOwnedByUs = self.connectingChain != nil
                    self.connectingChain = nil
                    if stillOwnedByUs { await c.close() }
                    return .failure(PlainMessageError(message: "\(error)"))
                }
            }
            connectingTask = task
        }
        let result = await task.value
        if isOwner { connectingTask = nil }

        switch result {
        case .success(let opened):
            // Samma race som beskrivs i tasken ovan (mellan `Task.isCancelled`
            // och att `await task.value` returnerar HÄR): `disconnect()` kan
            // ha hunnit köra FÄRDIGT i det fönstret, ELLER en NYARE anslutning
            // kan redan ha ersatt `self.client` — kollas per ANROPARE (inte
            // via en delad flagga), så varje väntande part avgör själv.
            guard !isTornDown, client === opened else {
                onInterrupted()
                return nil
            }
            return opened
        case .failure(let error):
            if !isTornDown { onFailure(error.message) }
            return nil
        }
    }

    /// En användning av den cachade sessionen misslyckades (t.ex. ett
    /// kommando kastade ett handskaknings-/kanalfel, inte bara en icke-noll
    /// exitkod) — stänger och nollar den cachade klienten/kedjan så nästa
    /// `ensure(...)` återansluter, men (till skillnad från `disconnect()`)
    /// sätter INTE `isTornDown`: anropsplatsen ska kunna återansluta.
    ///
    /// `session` måste vara ANROPETS session, inte bara "den nuvarande" —
    /// om ett sent fel kommer från en GAMMAL session efter att en ny redan
    /// hunnit koppla upp (t.ex. ett överlappande `act()`/`refresh()`) ska
    /// den nya, redan uppkopplade kedjan inte stängas.
    func invalidateIfCurrent(_ session: SSHSession) {
        guard chain?.target === session else { return }
        let c = chain
        let cl = client
        chain = nil
        client = nil
        // `disconnect()` kör alltid `closeClient` på en satt klient innan
        // kedjan stängs — den här metoden hoppade tidigare över det steget
        // och lämnade klientens underliggande subsystem utan ordnad
        // nedstängning (cubic P3). Samma ordning här.
        Task { [closeClient] in
            if let cl { await closeClient(cl) }
            await c?.close()
        }
    }

    /// Stänger klienten (om en egen stängning behövs), kedjan, och en ev.
    /// kedja som fortfarande höll på att öppnas — avbryter också ett
    /// pågående anslutningsförsök så det inte kan återuppliva en ny
    /// anslutning efter att den här metoden returnerat.
    ///
    /// `isTornDown` sätts och referenserna nollas SYNKRONT innan `before`
    /// körs — annars kunde ett samtidigt `ensure(...)`-anrop (t.ex. från en
    /// åtgärd som redan hunnit förbi den cachade klienten) hinna återansluta
    /// under fönstret medan `before` fortfarande väntar.
    ///
    /// `before`: extra städning (t.ex. att stänga beroende resurser som
    /// portvidarebefordringar) som ska köras och bli KLAR innan kedjan
    /// själv stängs — samma ordning `PortForwardModel` redan förlitade sig
    /// på (tunnlarna dirigerar trafik genom sessionen).
    func disconnect(before: @escaping () async -> Void = {}) {
        isTornDown = true
        connectingTask?.cancel()
        let c = chain
        let cc = connectingChain
        let cl = client
        chain = nil
        connectingChain = nil
        client = nil
        Task { [closeClient] in
            await before()
            if let cl { await closeClient(cl) }
            await c?.close()
            await cc?.close()
        }
    }
}
#endif
