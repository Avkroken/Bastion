# Roadmap

Status mot [VISION.md](VISION.md). Se [README.md](README.md) för hur man
bygger/kör. Uppdateras löpande i samma PR som ändrar funktionaliteten.

## Tekniska avsteg från visionen

VISION.md är bevarad orört som historisk referens — de faktiska valen blev
delvis andra, av konkreta skäl:

| Vision | Faktiskt val | Varför |
|---|---|---|
| SSH: "OpenSSH eller ett välunderhållet bibliotek" | SwiftNIO SSH | Ren Swift, samma kärna på Linux och Apple utan att brygga mot C-OpenSSH |
| Databas: SQLite | JSON (`~/.bastion/hosts.json`) | Host-databasen är liten (taggar + metadata, inga hemligheter) — SQLite vore över­dimensionerat just nu. Kan bytas senare utan att påverka API:t |
| Synk: "iCloud och Git som första alternativ" | Mapp-baserad synk (funkar med iCloud/Dropbox/Syncthing/Git) + OAuth2/PKCE-kontointegration (Dropbox/Google Drive/OneDrive) | Mappmetoden funkar med vilken synktjänst som helst utan extra kod; kontointegration byggd för Dropbox/Google/OneDrive specifikt eftersom de har öppna REST-API:er — iCloud saknar en jämförbar tredjepartsvänlig API utan CloudKit/native-integration (se "Ännu inte påbörjat") |
| Terminalemulering: "en etablerad VT100/xterm-kompatibel motor" | SwiftTerm (Apple), egenskriven VT100/ANSI-tolk (Linux) | SwiftTerm är den etablerade motorn på Apple-sidan. Linux-GUI:t (SwiftCrossUI) har ingen bindning till någon befintlig terminalmotor, så en egen (minimal, testad) tolk skrevs istället |
| — (inget motsvarande i visionen) | Linux-GUI (SwiftCrossUI/GTK4) | Fas 3 (Linux) i visionen nämner ingen specifik teknik — SwiftCrossUI valdes eftersom det är den enda mogna cross-platform Swift-UI-lösningen för Linux |

## Status

| Del | Läge |
|-----|------|
| SSH-transport + handshake | ✅ (NIOSSH) |
| Lösenordsauth | ✅ testad end-to-end |
| Ed25519-auth (rått frö + OpenSSH-nyckelfil) | ✅ testad end-to-end |
| OpenSSH-nyckelfilsparser (`~/.ssh/id_ed25519`, okrypterad) | ✅ testad, autoupptäcks av CLI |
| ECDSA-nyckelauth (P256/P384/P521, okrypterad) | ✅ testad end-to-end (3 kurvor, riktiga `ssh-keygen`-nycklar) |
| Krypterad nyckel (lösenfras) + RSA | ⬜ se "Uppskjutet med avsikt" (kastar tydligt fel nu) |
| Exec + strömmad stdout/stderr | ✅ testad |
| Exitkod-hantering | ✅ |
| Misslyckad auth utan att hänga | ✅ testad |
| Interaktiv shell + PTY (stdin/stdout, resize) | ✅ testad end-to-end |
| known_hosts / TOFU (SHA256-fingeravtryck, MITM-skydd) | ✅ testad, `~/.bastion/known_hosts` |
| ssh-config-parsing (`Host`-alias, jokertecken, `IdentityFile`) | ✅ testad, CLI slår upp alias |
| Host-databas (JSON, taggar, CRUD) | ✅ testad, `~/.bastion/hosts.json` |
| Dashboard-data (last/minne/disk/uptime/OS/Docker via SSH) | ✅ parser testad, ett kommando — App/-UI (Xcode-only) — ✅ (2026-08-05) LinuxApp Rust (`dashboard.rs`), egen flik, ingen auto-poll än (se "Klart") |
| Docker-åtgärder (lista/start/stopp/omstart/logg) | ✅ testad, injektionssäker referens |
| Sync mellan enheter (LWW-merge + gravstenar, mapp-transport) | ✅ testad, konvergens bevisad |
| E2E-krypterad sync (AES-256-GCM + PBKDF2, testvektorer) | ✅ testad, chiffertext läcker inget |
| Importera `~/.ssh/config` → host-DB | ✅ testad (parser + dedup) |
| Docker-shell-kommando (`docker exec -it`, injektionssäkert) | ✅ testad |
| Kontoinloggning (OAuth2 + PKCE, Dropbox/Google Drive/OneDrive) | ✅ PKCE-kärna testad mot RFC 7636; alla tre `SyncProvider`-implementationer klara, kräver eget klient-ID (se README "Konton") |
| iOS-app (host-lista, dashboard, Docker+shell, sync, import) | 🧩 `App/`, byggs i Xcode via XcodeGen |
| SwiftTerm-terminalvy | 🧩 `App/TerminalView.swift`, byggs i Xcode |
| macOS-target | ✅ `Bastion-macOS` i `project.yml`, `Platform.swift` bär plattformsskillnaderna, `TerminalView` villkorad på `UIViewRepresentable`/`NSViewRepresentable` |
| Nyckelimport i appen (Keychain) | 🧩 `HostEditView` klistra-in + validering, `HostAuth.keychainKey`, städas vid borttagning |
| Auto-poll av dashboard | 🧩 `DashboardModel.startPolling()`, 15 s intervall, behåller data vid övergående fel |
| App-ikon + launch screen | ✅ `App/Assets.xcassets` |
| Linux-GUI (`bastion-gui`, SwiftCrossUI/GTK4) | 🗑️ RIVEN (2026-08-03) — ersatt av native Rust/GTK4-`LinuxApp`, se "Arkitekturbeslut" nedan. Raden bevaras som historik, inte aktuell status. |
| Linux-terminal (VT100/ANSI-tolk, bestående PTY-shell) | ✅ 42 fristående parser-tester gröna (`LinuxApp/Tests/`), körd (Xvfb) — riktig tangentbordsinmatning (2026-08-02, `KeyEventBridge.swift`, se "Klart"), ingen interaktiv GUI-verifiering än; inget musstöd (ingen rå gest-position-API i SwiftCrossUI) |
| Linux-Docker-hantering (`DockerView`) | ✅ lista/start/stopp/omstart/logg/shell — motsvarar `App/DockerView.swift` |
| Portvidarebefordran (`PortForwardView`) | ✅ lokal/fjärr/dynamisk — LinuxApp (byggd+körd, Xvfb; interaktiv klick-genom-menyn ej gjord) OCH App/ (2026-07-08, Xcode-only) |
| ProxyJump (`ssh -J`) | ✅ `SSHSession.connect(via:)`, `bastion-cli` läser `ProxyJump` ur ssh-config automatiskt — ✅ (2026-08-04) LinuxApp Rust (`ssh::connect_via_jump`), testat mot två oberoende riktiga `sshd`-instanser, ingen egen UI-väljare i LinuxApps host-dialog än (se "Klart") |
| WireGuard-profiler | ✅ parsning/serialisering + lagring — LinuxApp OCH App/-UI (2026-07-08, Xcode-only) |
| OpenSSH-certifikat | ✅ parsning + CA-signaturverifiering + `SSHUserAuth`/`HostAuth`-wiring (`.certificateFile`) — testad mot RIKTIGA `ssh-keygen -s`-certifikat, App/-UI (Xcode-only) — ✅ (2026-08-05) LinuxApp Rust (`ssh::authenticate`, `russh::keys::load_openssh_certificate`), permanent CI-test mot en RIKTIG `sshd` med `TrustedUserCAKeys` (går längre än Swift-sidans engångsverifiering, se "Klart") |
| ssh-agent-protokollklient | ✅ `SSHAgentClient.swift`, testad mot en RIKTIG `ssh-agent` — 🚫 kanal-forwarding till fjärrserver BLOCKERAD (se ROADMAP) |
| Tailscale-värdförslag | ✅ `TailscaleStatus.swift` (fetch/fetchLocal) — LinuxApp OCH App/-UI (2026-07-08, Xcode-only; `fetchLocal` villkorsstyrd bort på iOS, `Foundation.Process` saknas där) |
| S3-kompatibel objektlagring | ✅ `S3Client.swift` + `S3ConnectionStore` — LinuxApp OCH App/-UI (2026-07-08, Xcode-only) |

## Riktmärke: Termius (2026-08-05)

Ägarbeslut: Termius är måttstocken för layout, funktioner och var saker
hör hemma — bastion ska vara bättre i alla avseenden och helt gratis.
Anledningen till att projektet finns är prislappen: Termius tar betalt per
månad för en SSH-klient, och inte småpengar.

**Vad Termius tar betalt för** (deras egen prissida, 2026-08-05):

| Nivå | Pris | Vad som ingår utöver nivån under |
|---|---|---|
| Starter | Gratis | Lokalt valv, SSH och SFTP, AI-autokomplettering, portvidarebefordran |
| Pro | $10/mån (årsvis) | Personligt molnvalv, **synk mellan enheter**, **snippets-automation**, log-bookmarks |
| Team | $20/användare/mån | Delat teamvalv, realtidssamarbete, samlad fakturering |
| Business | $30/användare/mån | Flera teamvalv, behörighetsstyrning per valv, SAML SSO (tillägg) |

**Tre av fyra Pro-funktioner är redan gratis i bastion**: synk mellan
enheter (mappbaserad transport + AES-256-GCM-kryptering + OAuth-konton),
snippets/kommandobibliotek, och portvidarebefordran (som Termius ändå har
i sin gratisnivå). Det som återstår på deras Pro-lista är log-bookmarks.
Det är alltså inte funktionslistan som är den stora luckan — det är
formen.

**Termius desktoplayout att lära av** (deras redesign, "Termius X", och
Workspaces-lanseringen):

- **Horisontella flikar på översta nivån**, webbläsarlikt. Sidopanelen
  tonades medvetet ned. bastion har redan `AdwTabView`-flikar.
- **Ett samlat "Vaults"** — all sparad data på ETT ställe, skilt från de
  aktiva sessionerna. Deras uttalade skäl: "each new element increased
  navigation complexity". Det är precis bastions nuvarande svaga punkt:
  värdar bor i sidopanelen medan WireGuard-profiler, S3-anslutningar,
  kommandobibliotek och known_hosts ligger utspridda i var sin dialog
  bakom primärmenyn. Samma innehåll, ingen gemensam plats.
- **Kommandopalett** (`Ctrl+J` byt session, `Ctrl+T` ny anslutning) med
  luddig sökning på värdnamn. bastion har **inga tangentbordsgenvägar
  alls** — noll `set_accels_for_action`, ingen `ShortcutController`
  (kontrollerat 2026-08-05). För en klient riktad mot vana användare är
  det en verklig lucka, och den är dessutom grunden en palett byggs på.
- **Workspaces med Focus Mode och Split View** — flikar grupperas genom
  att dras på varandra, och en grupp visas antingen som en enda terminal
  eller som upp till 16 sida vid sida. bastion har flikar men ingen
  delad vy.

**Prioritering som följer av det här** (inte huggen i sten, men i den
ordning nyttan per arbetsinsats är störst):

1. **Tangentbordsgenvägar + kommandopalett.** Störst daglig nytta, helt
   fristående, inget protokollarbete. Genvägarna behövs ändå.
2. **Delad vy (split).** `gtk::Paned` runt den befintliga terminalvyn;
   flikarna finns redan.
3. **En samlad plats för sparad data.** Rör mest UI-struktur och bör
   göras efter att genvägarna finns, så navigeringen kan byggas åt båda
   hållen samtidigt.
4. **Log-bookmarks** — det enda som återstår av Termius Pro-lista.

## Nästa steg (i ordning)

1. **Verifiera kontointegrationen i Xcode** — `OAuthAccountManager` och alla tre
   `SyncProvider`-implementationerna (Dropbox/Google Drive/OneDrive) är skrivna
   men aldrig byggda (Xcode-only, kan inte kompileras på Linux). Kräver ett
   registrerat klient-ID per leverantör (se README "Konton") för att testas på riktigt.
2. **Få appen på en riktig iPhone (TestFlight)** — ✅ KLART, 2026-07-08.
   Fjortonde riktiga `testflight.yml`-körningen lyckades helt: byggd,
   signerad och uppladdad — "Successfully uploaded the new binary to App
   Store Connect", verifierat äkta genom att leta upp den exakta raden i
   CI-loggen (inte bara "jobbet blev grönt"). Historik nedan.

   Apple Developer
   Program-kontot aktivt. Alla fyra TestFlight-secrets satta (`APP_STORE_
   CONNECT_TEAM_ID`/`KEY_ID`/`ISSUER_ID`/`KEY_CONTENT`, App Store Connect
   API-nyckel med rollen App Manager, "Bastion CI"). App-ID (`se.denied.
   bastion`) registrerat i Certificates, Identifiers & Profiles.
   **App Store-listningsnamn ≠ projektnamn**: "Bastion" som exakt App
   Store-namn var upptaget av en annan app (ingen hittad mjukvaru-
   varumärkeskonflikt vid kontroll — bara en namnkrock på plattformen).
   Löst genom `WABL SSH` som ett första FORMELLT App Store-listningsnamn,
   sedan bytt igen 2026-07-08 till **`Pantalongen`** (ägarbeslut) — satt
   direkt via App Store Connect API:t (`Spaceship::ConnectAPI::
   AppInfoLocalization#update`, samma autentisering som testflight.yml
   redan använder, `en-GB`-lokaliseringen — appen registrerades med den
   som primärt språk, inte `en-US`). Rent tekniskt, påverkar INGET annat:
   repo, kodbas, bundle-ID (`se.denied.bastion`), README/VISION.md-
   varumärket och hur alla faktiskt pratar om projektet förblir
   **"Bastion"** oförändrat. Skulle någon framtida ändring beröra
   `CFBundleDisplayName` eller annan App Store-metadata, kom ihåg den
   här distinktionen.

   **App/-koden bevisad bygga på riktig Xcode** (2026-07-08, en hyrd
   MacInCloud-session): `xcodegen generate` + `xcodebuild -destination
   'generic/platform=iOS Simulator' build` → **BUILD SUCCEEDED**, första
   gången någonsin. (Krävde en separat nedladdning av Metal Toolchain,
   `xcodebuild -downloadComponent MetalToolchain` — Xcode 26.5 bundlar
   den inte längre, SwiftTerms Metal-shader kompilerar annars inte.)

   **TestFlight-signeringssagan, löst (2026-07-08)**: åtta riktiga
   `testflight.yml`-körningar (2026-07-07/08) misslyckades alla med
   varianter av "No profile ... found"/"No signing certificate ... found".
   Rotorsak, till slut bekräftad via App Store Connect API:t direkt:
   `-allowProvisioningUpdates` skapade ALDRIG ett distributionscertifikat
   på den engångskörda CI-runnern — bara ett utvecklingscertifikat, om
   ens det. Löst genom att helt byta signeringsmodell till `fastlane
   match` (readonly i CI): certifikat+profil skapas EN gång, sparas
   krypterat i det nya privata repot `blixten85/bastion-certificates`
   (`MATCH_PASSWORD`-skyddat, `MATCH_DEPLOY_KEY` skrivbehörig deploy key
   avgränsad till bara det repot). `App/project.yml`s Release-
   `PROVISIONING_PROFILE_SPECIFIER` bytt till `match AppStore
   se.denied.bastion` (matchs egen namngivningskonvention) från det
   påhittade "Bastion App Store" som `-allowProvisioningUpdates` aldrig
   lyckades skapa. (De faktiska namnen från de två bootstrap-körningarna
   hade tidsstämpel-suffix — se nedan för 2026-07-22-omgenereringen,
   som bytte suffixet från `1783541247` till
   `1784708595` efter att certifikaten raderades manuellt i Apple
   Developer-portalen.)

   Bootstrap-steget (`fastlane match ... readonly: false`, den ENDA
   gången ett nytt certifikat/profil behöver skapas) visade sig kräva en
   RIKTIG Mac med en INLOGGAD GUI-session — `security`/Keychain-
   operationer (`SecKeychainItemImport`) vägrar interagera över ren SSH
   utan fönsterserver, oavsett rätt lösenord. Löstes med: (1) en egen,
   fristående Keychain (`security create-keychain`, inte inloggnings-
   Keychain, som verkar ha en extra begränsning specifik för det här
   hyrda kontot) + (2) `git config --global user.email/user.name` (saknades
   helt på den hyrda maskinen, blockerade den sista `git push` in i
   certifikatförvaret). Träffade Apples gräns för antal distributions-
   certifikat en gång under felsökningen (många omförsök skapade fem
   föräldralösa certifikat) — städat, ett rent bootstrap-försök lyckades
   sedan hela vägen: **`fastlane.tools finished successfully`**,
   verifierat att `certs/`/`profiles/` faktiskt commitades till
   `bastion-certificates`.

   `.github/workflows/testflight.yml` uppdaterad att sätta upp
   `MATCH_DEPLOY_KEY`/`MATCH_PASSWORD` innan `fastlane beta` körs.

   **Tre sista, distinkta fel innan grönt** (alla hittade genom att
   faktiskt inspektera CI-loggar/artefakter, inte gissade i förväg —
   ett tillfälligt `actions/upload-artifact`-diagnostiksteg i
   `testflight.yml`, borttaget igen efter att sista felet löstes):
   1. **`Info-macOS.plist` läckte in i iOS-appen.** `Bastion`
      (iOS)-targetets `sources.excludes`-lista i `App/project.yml`
      exkluderade aldrig macOS-motsvarighetens Info.plist (macOS-
      targetet exkluderar omvänt redan iOS "Info.plist") — den bäddades
      in RÅ (ingen `$(VARIABEL)`-substitution) som en extra resursfil.
      Xcodes arkivvalidering skannar tydligen alla bäddade plist-
      liknande filer, inte bara appens egen — den trasiga kopian gav
      "Couldn't find platform family for Bastion.app", trots att
      appens EGEN Info.plist redan hade en korrekt
      `CFBundleSupportedPlatforms`.
   2. **`UISupportedInterfaceOrientations`(`~ipad`) tystades ut under
      arkiveringen.** Fanns i den råa `Info.plist`-filen men syntes
      aldrig i den faktiskt byggda appen (verifierat genom att
      inspektera den byggda plisten direkt) — Apple avvisade
      uppladdningen: "Invalid bundle. No orientations were specified".
      Fix: flyttade nycklarna till XcodeGens egna `info.properties`-
      block i `project.yml` (samma mönster som redan fungerade
      pålitligt för `CFBundleDisplayName`/`UILaunchScreen`/
      `UIApplicationSceneManifest`) — bara nycklar satta DÄR verkar
      garanterat överleva byggprocessen när `GENERATE_INFOPLIST_FILE`
      är `NO`.
   3. Efter dessa två: **fjortonde riktiga körningen, helt grön** —
      "Successfully uploaded the new binary to App Store Connect."

   Fjorton riktiga körningar totalt (2026-07-07/08) för att komma hela
   vägen: fem olika, äkta rotorsaker hittade och fixade i tur och
   ordning (saknat certifikat, saknad profil, fel profilnamn,
   Keychain-interaktion/git-identitet vid bootstrap, två läckande/
   tystade Info.plist-nycklar) — ingen av dem en gissning som råkade
   fungera, alla verifierade direkt mot antingen App Store Connect
   API:t eller det faktiska byggda innehållet.
3. **Windows-GUI via `WinUIBackend`** — ✅ **`windowsapp-build` grönt för
   första gången någonsin (2026-07-10)**, efter 74 raka misslyckade
   körningar sedan CI:t skapades (2026-07-04). Rotorsaken var två
   bekräftade uppströmsbuggar i swift-nio, inte något i Bastions egen kod
   (historik nedan) — men FIXEN gick att göra i Bastion självt, utan att
   vänta på eller forka uppströms:

   **Fixen**: `Package.swift` pinnar nu `swift-nio` till exakt `2.86.2`
   istället för `from: "2.101.2"`. Insikten: buggarna triggas bara för att
   swift-nios EGNA källor kompileras under Swift 6-strict-concurrency-läge
   — vilket styrs av PAKETETS EGEN deklarerade `swift-tools-version`, inte
   konsumentens. `2.86.2` är den sista swift-nio-releasen med
   `swift-tools-version:5.10` (`2.87.0`+ gick till `6.0`/`6.1`) —
   `swift-nio-ssh` 0.14.0 kräver bara `from: "2.81.0"`, så `2.86.2`
   satisfierar beroendet utan konflikt. Verifierat: `swift build`/
   `swift test` (230 tester) grönt på Linux med pinningen INNAN push
   (ingen regression), och `windows-gui.yml` gick grönt hela vägen —
   inklusive själva kompileringssteget som alltid kraschat förut — kört
   manuellt via `workflow_dispatch` mot en egen testbranch innan en riktig
   PR öppnades, för att inte slösa en PR-öppning på en ren gissning.

   **Historik — buggarna som blockerade innan fixen** (kvar dokumenterat
   för framtida referens, t.ex. om en framtida beroendeuppdatering av
   swift-nio råkar dra in `2.87.0`+ igen och samma fel återkommer):
   1. `NIOThread.handle: NIOLockedValueBox<ThreadOpsSystem.ThreadHandle?>`
      (`ThreadWindows.swift:22`) kan inte konformera till `Sendable` under
      Swifts strikta concurrency, eftersom `UnsafeMutableRawPointer` har
      `Sendable`-konformansen explicit omarkerad `unavailable` i
      standardbiblioteket.
   2. `System.swift:572` — `static let SOL_UDP: CInt = CInt(IPPROTO_UDP)`
      — `IPPROTO` konformerar inte till `BinaryFloatingPoint`, en
      typmismatch i swift-nios egen Windows-portering av
      POSIX-konstanterna.
   **Rapporterat uppströms, 2026-07-08: `apple/swift-nio#3647`.** Fanns
   redan rapporterat en gång (#3460), men den stängdes felaktigt som
   duplicate av `#2065` — vid granskning visade sig #2065 handla om ett
   HELT annat, orelaterat `log2`-importfel (Swift 5.5, 2022), inte
   Sendable/IPPROTO alls. En tyst tumme-upp på en felaktigt stängd
   issue hade aldrig synts, så en ny, fullständigt dokumenterad rapport
   skickades istället — med alla tre bekräftade toolchain-körningarna
   (6.1/6.2/6.3.3) som bevis. Rapporten kvar öppen uppströms (nyttig för
   andra swift-nio-på-Windows-projekt), men blockerar inte längre Bastion
   tack vare `2.86.2`-pinningen ovan.
   **Sidospår som löstes under samma utredning** (dokumenterat separat så
   det inte återupptäcks i onödan): ett tredje, TILLFÄLLIGT fel dök upp
   vid 2026-07-07-omtestet — `STL1000: Unexpected compiler version,
   expected Clang 20 or newer` — orsakat av att VPS:ens Visual Studio
   Build Tools hade auto-uppdaterats till en version som kräver nyare
   Clang än Swift 6.1 (Clang 19.1.4) bundlar. Löst genom att uppgradera
   till Swift 6.3.3-RELEASE (bundlar Clang 21.1.6) — inte en riktig
   Bastion-relaterad bugg, bara toolchain-miljödrift på testmaskinen.
   **2026-07-28: verifierat på RIKTIG Windows-hårdvara för första gången**
   (lokal Windows Server 2025-VM på mp100, inte bara CI:s cross-compile).
   `bastion-gui.exe` kraschade omedelbart ("This application requires the
   Windows App Runtime Version 1.5" / `WinUI/SwiftApplication.swift:64:
   Fatal error: fatal` i swift-winuis `SwiftApplication.main()`) tills
   EXAKT rätt version av Windows App Runtime installerades —
   `1.5-preview1` (`1.5.240205001-preview1`, dokumenterad i swift-winuis
   README), INTE en senare 1.5.x-patch (2.3.1 eller 1.5.250108004 gav
   samma krasch). Efter det: ett riktigt WinUI3-fönster renderade
   ("Bastion för Windows", platshållar-UI). Ny `WindowsApp/Install-
   Bastion.ps1` + `WindowsApp/README.md` paketerar detta beroende
   (kontrollerar/installerar automatiskt) — se commit `addc43e`.
   MSIX-paketregistrering nekar åtkomst (`0x80070005`) över en icke-
   interaktiv fjärrshell (WinRM) — måste köras i en riktig inloggad
   session, samma begränsningsklass som Microsoft Store-installationer.
   **Touch-svep tillagt samma dag** (användarbegäran: "Kan du få touchen
   att funka på Windows och Linux"): ny `WinUISwipeGestureBridge.swift` —
   till skillnad från GTK-sidan (som behövde en rå ABI-koppling) exponerar
   swift-winui redan `UIElement.manipulationMode`/`.manipulationCompleted`
   direkt via sin genererade WinRT-projektion, ingen egen C-bindning
   behövdes. Minimal tvåsidig platshållar-demo i `ContentView` (svep
   vänster/höger växlar sida). Ett riktigt kompileringsfel hittades och
   fixades på riktig hårdvara (`.inspect` efter `.padding()` matchar
   `FrameworkElement`-overloaden, inte `VStack`s egen `Canvas`-variant) —
   commit `087b0e7`. **EJ verifierat på riktig touchhårdvara** — ingen
   tillgänglig i den här miljön (bara en iPhone 16 Pro och en Samsung
   Q80D, ingen Windows-touchskärm), bekräftat av användaren själv
   (2026-07-28). Bara verifierat: kompilerar/länkar/startar utan krasch.
   **Riktig installer tillagd och VERIFIERAD på riktig hårdvara 2026-07-29**
   (`WindowsApp/Installer.iss` + `Build-Installer.ps1`, Inno Setup 6):
   bygger `Output\BastionSetup.exe` som buntar `bastion-gui.exe` + Windows
   App Runtime-installeraren i EN fil — runtimen körs tyst som ett
   `[Run]`-steg innan appen startas, ingen PowerShell/manuell paketjakt
   synlig för slutanvändaren. Direkt svar på packningsklagomålet ("allt
   som bastion behöver måste följas med"). Två riktiga buggar hittades
   och fixades under verifieringen (inte antaganden — hittade via faktisk
   `iscc`-körning på `bastion-winserver`-VM:en):
   - Icke-ASCII-tecken (å/ä/ö/em-dash) i `Build-Installer.ps1`/
     `Installer.iss`/`Install-Bastion.ps1` korrumperades vid git-checkout
     på Windows och gav "missing string terminator" i PowerShell-parsern
     — fixat genom att skriva om alla tre filer i ren ASCII (commit
     `18b3b5e`).
   - `Installer.iss` saknade obligatoriskt `AppVersion`-direktiv — ISCC
     vägrade kompilera ("[Setup] section must include an AppVersion or
     AppVerName directive") — fixat (commit `76911ad`).
   Efter fixarna: `iscc Installer.iss` gav "Successful compile (63.281
   sec)", `BastionSetup.exe` (89MB) kördes interaktivt via RDP, hela
   guiden (Select Destination → Ready to Install → Installing → Finish)
   fungerade felfritt, skrivbordsikon skapades korrekt.

   **Krasch hittad OCH LÖST samma session (2026-07-29)**: `bastion-gui.exe`
   kraschade omedelbart efter installation. Windows händelselogg visade
   `Exception code: 0xc000001d` (STATUS_ILLEGAL_INSTRUCTION) i
   `swiftCore.dll` — en första hypotes (AVX-512-instruktion som VM:ens
   AMD Ryzen 7430U saknar) verifierades vara FEL genom disassemblering
   (`objdump`): instruktionen på kraschadressen var en avsiktlig `ud2`-
   trap direkt efter ett `call`, inte en olaglig CPU-instruktion — det
   vanliga kompilatormönstret efter anrop till en `noreturn`-funktion.
   Den riktiga orsaken hittades genom att köra appen i en riktig konsol
   (`cmd.exe`, inte en bakgrundslogg — WinRM/Start-Process-omdirigerad
   stdout buffrades bort och visade aldrig felet): swift-winuis egen
   `catch`-block skrev ut det verkliga felet innan `fatalError("fatal")`:

       Failed to initialize WindowsAppRuntimeInitializer:
       missingBootstrapper(["swift-winui_CWinAppSDK.resources\
       Microsoft.WindowsAppRuntime.Bootstrap.dll", ...])

   SwiftPM skapar en resursmapp `swift-winui_CWinAppSDK.resources`
   (innehåller `Microsoft.WindowsAppRuntime.Bootstrap.dll`) bredvid
   `.exe`-filen vid `swift build` — men `Installer.iss` kopierade bara
   själva `.exe`-filen, aldrig denna mapp. Fixat genom att lägga till en
   `[Files]`-post för mappen (commit `0e9952f`). **Verifierat på riktig
   hårdvara**: ombyggd installer + ren installation + körning från
   skrivbordsikon i en interaktiv session gav ett fullt renderat WinUI-
   fönster ("Bastion för Windows", "0 sparade värdar", läser `HostStore`
   korrekt) — ingen krasch. Svep-gesten testades INTE denna gång (kräver
   äkta touch-input, en syntetisk musdrag via xdotool triggar inte
   `ManipulationCompleted`) — kvarstår overifierad på riktig
   touchhårdvara, som redan dokumenterat.

   **Swift-runtime-DLL:erna buntade och verifierade OCKSÅ 2026-07-29**
   (`Build-Installer.ps1` hittar automatiskt Swift-runtimens bin-katalog
   och kopierar `swiftCore.dll` m.fl. till `Redist/SwiftRuntime/` innan
   Inno Setup-kompilering, commit `addc8f0`+`0d6bb34`). **Hårt verifierat
   på riktig hårdvara**: bytte tillfälligt namn på hela
   `...\Programs\Swift`-katalogen (så toolchainen INTE kunde hittas av
   något fallback), installerade om `BastionSetup.exe`, körde
   `bastion-gui.exe` från den nya installationen — fullt renderat
   fönster, ingen "DLL saknas"-krasch. `BastionSetup.exe` är nu en
   genuint fristående installer: kräver INGEN separat Swift- eller
   Windows App Runtime-installation på målmaskinen.

   **Nästa steg**: porta de riktiga vyerna från `LinuxApp/Sources/bastion-
   gui/` hit (inte påbörjat, svep-bryggan väntar på riktiga flikar att
   växla mellan) — enda återstående punkten för denna sektion.
4. **`bastion-cli` som headless/skriptbar fallback** (användarförslag
   2026-07-28): "Den borde kunna integreras med Linux och Windows Shell
   också, bash osv, cmd, PowerShell, så att det går att köra den remote
   utan GUI." `bastion-cli` (rot-paketet, `Sources/bastion-cli/`) gör
   redan detta — exec, `-L`/`-R`/-D`-portvidarebefordran, ProxyJump, ren
   `SSHCore` utan GUI-beroende — men var ENDAST testad på macOS/Linux
   (`xcode.yml`/CodeQL). Verifierat 2026-07-28: bygger och länkar rent
   på Windows med samma Swift 6.1/MSVC-toolchain som `bastion-gui`, INGET
   Windows App Runtime-beroende (ren SwiftNIO, ingen WinUI/MSIX
   inblandad) — bekräftar att detta redan fungerar som den GUI-fria
   reservlösningen användaren efterfrågade.

   **BUGG hittad vid samma verifiering, PLATTFORMSSPECIFIK**: en
   misslyckad autentisering (fel lösenord) mot en riktig SSH-server
   hänger OÄNDLIGT på Windows (`bastion-cli.exe ... ` blockerar för
   alltid, 0% CPU — inte en TCP/nätverksfråga, `Test-NetConnection`
   bekräftar porten når fram). Samma exakta anrop (fel lösenord) på
   Linux/macOS misslyckas KORREKT och snabbt: `Fel:
   channelFailed("End of file")`, exit-kod 2. Roten är sannolikt i hur
   swift-nio-ssh/swift-nio hanterar kanalstängning eller async-avbrott
   annorlunda i sin Windows-portering (jämför den redan kända,
   uppströms-rapporterade `apple/swift-nio#3647` ovan för samma
   allmänna riskbild: swift-nios Windows-portering skiljer sig
   strukturellt från POSIX-sidan). EJ rotorsaksbestämt än — kräver
   vidare felsökning i `SSHSession`/`NIOSSHHandler`-felvägen specifikt
   under Windows. Blockerar `bastion-cli` som en pålitlig Windows-
   fallback tills löst (en trasig autentisering ska INTE kunna hänga
   processen för alltid).

   Kvar utöver detta: ingen dedikerad Windows-CI-check för `bastion-cli`
   än (bara macOS/CodeQL bygger rot-paketet idag). `-J`-kommandoradsflagg
   ✅ klart, se "Klart" nedan.
5. ~~Riktig rå tangentbordsinmatning i Linux-terminalen~~ — ✅ klart
   (2026-08-02), se "Klart" → "Linux-terminal". (Avsåg ursprungligen den
   SwiftCrossUI-baserade `LinuxApp/`, sedan riven till förmån för Rust/GTK4
   — se "Uppskjutet med avsikt"-beslutet nedan.)

## Klart

- **Åtgärder i kommandopaletten** (2026-08-06, `LinuxApp/src/main.rs` +
  nya `LinuxApp/src/palette_actions.rs`) — det "Kvar" kommandopaletten
  lämnade efter sig:
  - **Paletten nådde bara värdar och öppna sessioner.** Allt annat appen
    gör — importera en ssh-config, öppna S3-anslutningarna, ändra
    inställningar — låg bakom primärmenyn och krävde musen. Det är
    precis den halvan en palett finns till för. Tio åtgärder finns nu i
    listan, sist efter värdarna: en maskin ska aldrig hamna under en
    åtgärd, och eftersom `fuzzy::rank` sorterar stabilt räcker det att
    lägga in dem sist.
  - **Sökord, inte bara etiketter.** Menyposten heter "Funktioner", men
    den som letar efter den skriver "inställningar" — eller "settings".
    Varje åtgärd har därför en egen uppsättning synonymer att söka i
    utöver sitt menynamn. Etiketten är fortfarande exakt menyns, två
    namn på samma sak är värre än ett klumpigt namn.
  - **Paletten lär också ut kortkommandona.** Undertiteln visar
    åtgärdens tangentbordsgenväg när den har en, hämtad ur appen via
    `accels_for_action` i stället för inskriven för hand — annars kan
    paletten lära ut ett kortkommando som sedan ändrats.
  - **`app.palette` är med avsikt INTE med** (paletten öppnar inte sig
    själv), inte heller `app.focus-search` (att söka sig fram till en
    annan sökruta är en omväg). "Stäng fliken" göms när ingen flik är
    öppen — en rad som garanterat inte gör något är bara brus.
  - **`palette_actions.rs` är GTK-fri och har nio tester.** Det viktiga
    av dem läser `main.rs` och kräver att varje åtgärdsnamn faktiskt är
    registrerat där: GTK ritar en post med felstavat åtgärdsnamn utan
    att klaga, den gör bara ingenting när man klickar. Verifierat att
    testet fångar felet genom att felstava ett namn med flit. 221 → 230
    tester.
  - **Verifierat i den körande appen**, inte bara byggt: `inställ` +
    Enter öppnade Inställningar, och `stäng` utan öppen flik gav ingen
    "Stäng fliken"-rad. Inga CRITICAL i loggen.

- **Två flikbuggar som bara syntes när appen kördes** (2026-08-05,
  `LinuxApp/src/main.rs` + nya `LinuxApp/src/tab_title.rs`):
  - **`page_position` som existensfråga loggade en CRITICAL.** Fem
    ställen frågade `area.tab_view.page_position(&page) >= 0` för att få
    veta om fliken fanns kvar — men `adw_tab_view_get_page_position`
    SVARAR inte för en sida som inte hör till vyn, den loggar
    `assertion 'page_belongs_to_this_view (self, page)' failed`. Varje
    flik som stängdes innan dess anslutning hann ge upp skrev alltså ut
    en CRITICAL. Reproducerat med enbart mus (öppna en session mot en
    oanträffbar adress, stäng fliken, vänta ut försöket), så det var
    inte något kortkommandona införde. Ersatt av `tab_view_contains`,
    som går igenom `pages()` och faktiskt svarar.
  - **Snabbanslutningens flikar saknade namn.** De sparas aldrig i
    värdlistan och får därför tomt alias, vilket gick rakt in i
    fliknamnet — fliken hette bokstavligen ingenting, eller " (2)" när
    det fanns fler än en. Faller nu tillbaka på `användare@värd`, samma
    form som `ssh` självt. Verifierat på skärm: två anslutningar mot
    samma värd ger `provare@10.255.255.1` och
    `provare@10.255.255.1 (2)`.
  - **Titellogiken lyftes ur `main.rs` till `tab_title.rs`** just för
    att den skulle kunna testas — `main.rs` har ingen
    `#[cfg(test)]`-täckning av hävd, så allt testvärt behöver bo
    utanför. Sju tester, varav ett för vartdera felet ovan (214 → 221).

- **Kommandopalett (`Ctrl+Shift+P`)** (2026-08-05, `LinuxApp/src/main.rs`
  + nya `LinuxApp/src/fuzzy.rs`) — andra halvan av steg 1 i
  Termius-prioriteringen, kortkommandona var första:
  - **En sökruta över allt man rimligen vill nå snabbt**: de öppna
    sessionerna och de sparade värdarna i samma lista. Öppna sessioner
    läggs in först, och eftersom rankningen är stabil vinner "byt till
    fliken du redan har" över "öppna en till mot samma värd" när poängen
    är lika. Ett test vaktar just den ordningen — den är lätt att råka
    förstöra vid en framtida sorteringsändring.
  - **Sökningen är luddig men förutsägbar.** `pw1` hittar `prod-web-1`.
    Tre regler, inte fler: tecknen måste finnas i ordning, träffar som
    sitter ihop väger tyngre än utspridda, och träffar i början av ett
    ord väger tyngre än mitt inne i ett. En palett som rankar
    oförklarligt är värre än ingen palett alls. Det söks i alias,
    `användare@värd` OCH taggar — man ska kunna leta upp en maskin på
    sin IP eller sin tagg, inte bara på namnet man gav den.
  - **`fuzzy.rs` är GTK-fri och har elva tester**, inklusive de
    jämförelser som ordningen faktiskt bygger på (sammanhängande slår
    utspridd, ordbörjan slår mitt i ordet, kort exakt namn slår långt).
    Samma skäl som för `tab_title.rs`: `main.rs` har ingen
    `#[cfg(test)]`-täckning av hävd. 203 → 214 tester.
  - **Tangentbordet räcker hela vägen**: `Ctrl+Shift+P` öppnar, man
    skriver, upp/ner flyttar markeringen UTAN att fokus lämnar sökrutan
    (annars går det inte att fortsätta skriva efter att ha pilat), Enter
    aktiverar och Escape stänger. Verifierat i den körande appen —
    `dbb` + Enter anslöt till `db-backup`, och när paletten öppnades
    igen låg den sessionen överst som "Öppen session · flik 1".
  - **Ingen `adw::HeaderBar`**, till skillnad från appens övriga
    dialoger: en palett ska vara sökrutan, inget annat.
  - **Kvar**: åtgärder (inte bara värdar och sessioner) i paletten —
    "öppna inställningar", "importera ssh-config" och liknande borde
    också gå att nå därifrån.

- **Kortkommandon — appen hade inga alls** (2026-08-05,
  `LinuxApp/src/main.rs`), första steget i Termius-prioriteringen ovan:
  - **`Ctrl+Shift`, inte `Ctrl`.** Appen ÄR en terminal: `Ctrl+T`,
    `Ctrl+W` och `Ctrl+K` tillhör readline på andra sidan (transponera
    tecken, radera ord bakåt, klipp rad). Tar appen dem försvinner de ur
    skalet. Därför `Ctrl+Shift`+bokstav och `Alt`+siffra, samma val som
    GNOME Terminal och Ptyxis. Termius använder `Ctrl+T`/`Ctrl+J` — här
    är det avsiktligt INTE härmat. Att vara bättre än riktmärket betyder
    ibland att inte kopiera det.
  - `Ctrl+Shift+N` ny värd · `Ctrl+Shift+T` snabbanslutning ·
    `Ctrl+Shift+W` stäng flik · `Ctrl+Shift+F` sök värdar ·
    `Ctrl+,` inställningar · `Alt+1`–`Alt+9` växla flik.
  - **Åtgärderna flyttades till APPEN** (`app.new-host` osv.) i stället
    för gruppen som lades på sidopanelens behållare i föregående PR.
    Skälet är tvingande: `set_accels_for_action` når bara `app.`- och
    `win.`-prefix, inte en egen grupp på en widget. Vinsten är att meny,
    knapp och tangentbord nu delar EN definition — knapparna fick
    `set_action_name` i stället för egna stängningar, och GTK ritar ut
    kortkommandot bredvid menyposten av sig självt (verifierat på skärm:
    "Ctrl+," står bredvid Funktioner utan att texten skrivits någonstans).
  - **Escape stänger dialoger.** GTK4 gör inte det av sig självt —
    `GtkDialog` hade beteendet och är avvecklad. Ingen av appens tjugo
    dialoger gick alltså att avbryta från tangentbordet; man var tvungen
    att sikta på "Avbryt" med musen. Fixat på ETT ställe tack vare att
    `dialog_window` blev enda vägen in i föregående PR, via en
    `ShortcutController` mot GTK:s inbyggda `window.close` — knappen och
    tangenten gör bokstavligen samma sak.
  - **Två defekter i den nya koden hittades genom att köra den**, inte
    genom att bygga den: `Alt+2` med bara en flik öppen gav en
    Adwaita-CRITICAL i loggen (`ListModel::item` svarar normalt `None`
    utanför intervallet, men den här modellen går via
    `adw_tab_view_get_nth_page` som i stället loggar) och snabbt
    upprepat `Ctrl+Shift+W` kunde röra en flik på väg ut. Båda kollar
    nu antalet först. Ingenting av det syntes i UI:t — bara i loggen.
  - **Kvar**: kommandopaletten (steg 1 i Termius-listan är därmed halv),
    och ett fönster som listar kortkommandona.

- **Dialogfönstren byggs genom en gemensam hjälpare i stället för
  tjugo handplockade pixelpar** (2026-08-05, `LinuxApp/src/main.rs`):
  - **Problemet var inte ett fel utan en form.** Var och en av appens
    tjugo dialoger byggde sitt eget `adw::Window` med ett eget
    `default_width`/`default_height`-par. Varje justering av hur
    dialoger ser ut var alltså tjugo ändringar — och när något får
    kosta tjugo ändringar blir det inte gjort. Sex dialoger hade hunnit
    glida ifrån och saknade titel helt, däribland lösenordsprompten,
    "Lägg till värd" och SFTP-filredigeraren: de skyltade
    "bastion-linuxapp" i sin header, alltså appens namn där en rubrik
    skulle stått.
  - **`dialog_window(förälder, titel, storlek, innehåll)` är nu enda
    vägen in.** Titeln är ett obligatoriskt ARGUMENT, inte en valfri
    byggarmetod — det är det som gör "dialog utan titel" omöjligt att
    råka ut för igen, i stället för att bara rätta de sex som råkade
    sakna den. `DialogSize` uttrycker en avsikt (`Compact`/`Form`/
    `List`/`Viewer`); pixlarna står på ett ställe.
  - **Formulärhöjden mäts på innehållet i stället för att skrivas som en
    siffra.** `content_height` frågar widgeten om dess naturliga höjd, så
    ett formulär som får en rad till växer av sig självt. Det var inte
    hypotetiskt: "Lägg till värd" hade en gång fem rader och ett
    `default_height(360)`, men har nu tretton — fem av dem låg utanför
    fönstret och nåddes bara genom att skrolla i en dialog som inte såg
    skrollbar ut. Färgväljarens rad var dessutom så trång att etiketten
    bröts mitt i ordet ("Färgmärk-ning").
  - **Höjden har ett tak, och taket är skärmen.** Naturlig höjd är annars
    obegränsad — `Inställningar` blev längre än skärmen så fort mätningen
    infördes. Taket räknas ur bildskärmens faktiska arbetsyta (4/5 av
    den) i stället för att vara ett fast tal, så samma kod ger en rimlig
    dialog både på en liten laptop och på en stor skärm. Listor och
    innehållsvyer (loggar, filredigerare) behåller en uttalad starthöjd:
    en `gtk::ScrolledWindow` har ingen egen naturlig höjd att växa efter.
  - **De två sätten att sätta titel blev ett.** WireGuard- och
    S3-listorna satte en `adw::WindowTitle`-widget i headern med tom
    undertitel — exakt vad fönstertiteln redan ritar. Widgetarna är
    borttagna. Importdialogen behåller sin, eftersom den bär en verklig
    undertitel ("Klistra in innehållet från din ~/.ssh/config").
  - **Verifierat genom att köra appen**, dialog för dialog under Xvfb:
    titlarna stämmer, formulären får plats, `Inställningar` ryms på
    skärmen med resten skrollbart. Nettoresultat −210/+128 rader.

- **Sidopanelens header: nio ikonknappar → två knappar + primärmeny**
  (2026-08-05, `LinuxApp/src/main.rs`) — den "Kvar" som lämnades i
  posten om ikonbuggen nedan:
  - **Problemet var synligt, inte logiskt.** Nio `pack_end`-knappar
    trängde ut rubriken så att `adw::NavigationPage`-titeln "Värdar"
    klipptes till "Vär…". Ingen kompilator, inget test och ingen
    kodgranskning ser det — det upptäcktes, precis som ikonbuggen, genom
    att köra appen under Xvfb och titta på resultatet.
  - **Kvar som egna knappar**: lägg till värd (`+`) och snabbanslutning.
    De två används oftast och tål att ta plats. **Flyttade till en
    primärmeny** (`open-menu-symbolic`, GNOME:s eget mönster så fort en
    header behöver mer än ett par åtgärder): Telnet, Seriell/USB,
    Tailscale, WireGuard-profiler, S3-anslutningar, Importera ssh-config
    och Funktioner. Menyn är sektionsindelad efter vad posterna gör
    (andra anslutningstyper / nätverk+lagring / appens egna
    inställningar) i stället för en odelad lista på sju rader.
  - **Knapparnas `connect_clicked` blev `SimpleAction`-poster** i
    gruppen `sidebar`, inlagd på sidopanelens behållare — GTK slår upp
    `sidebar.*` uppåt i widgetträdet från posten som aktiveras, så
    popovern hittar gruppen därifrån. Samma mönster som värdradernas
    `host`-grupp. Stängningarna är oförändrade i sak; bara
    signaturen (`|_|` → `|_, _|`).
  - **Verifierat visuellt och funktionellt**, inte bara byggt: rubriken
    "Värdar" står nu oklippt, menyn öppnas med rätt sektioner, och
    Telnet- respektive Funktioner-posten öppnar sina dialoger på riktigt.
    Att samtliga sju poster ritas aktiva (inte gråade) är i sig beviset
    för att varje `sidebar.*`-namn faktiskt hittas — GTK gråar ut poster
    vars action saknas, vilket gör ett felstavat actionnamn synligt utan
    att man behöver klicka på varje rad.

- **CI-kontroll av MSRV + BUGGFIX: Seriell/USB-knappen renderade tom**
  (2026-08-05, `linuxapp-build.yml` + `main.rs`):
  - **Nytt `linuxapp-msrv`-jobb** bygger med EXAKT den toolchain
    `rust-version` utlovar. Siffran sätts inte av vår egen kod (edition
    2024 kräver bara 1.85) utan av beroendena — gtk4-rs höjde sitt krav
    till 1.92 utan att vi märkte det. Utan kontrollen upptäcks en
    föråldrad MSRV först när någon med äldre toolchain försöker bygga,
    vilket är precis vad fältet finns för att undvika. Versionen LÄSES ur
    `Cargo.toml` i stället för att dupliceras i workflowen, annars hade
    de två kunnat glida isär. Jobbet står medvetet utanför
    `required_status_checks` — det ska informera, inte blockera.
  - **Ikonbuggen hittades genom att FAKTISKT KÖRA appen** under Xvfb och
    ta en skärmdump — första gången den möjligheten användes i det här
    arbetet. `cable-modem-symbolic` finns inte i ikontemat, så
    Seriell/USB-knappen ritades som en tom yta. Ingen kompilator, inget
    test och ingen kodgranskning kunde ha sett det; GTK loggar inte ens
    en varning för en saknad ikon. Bytt till `modem-symbolic` (finns, och
    ligger närmast ursprungsavsikten) och visuellt verifierad före/efter.
    Samtliga 23 ikonnamn i `main.rs` kontrollerades mot temat — bara den
    här saknades.
  - **Kvar**: sidopanelens rubrik klipps till "Vär…" eftersom nio
    ikonknappar trängs i headern. Designfråga snarare än bugg — de mindre
    använda hör troligen hemma i en meny.

- **`rust-version = "1.92"` (MSRV) deklarerad i `LinuxApp/Cargo.toml`**
  (2026-08-05): fältet saknades helt, så den som hade en för gammal
  toolchain fick ett kryptiskt syntaxfel någonstans i ett beroende i
  stället för ett tydligt besked. Nu står `bastion-linuxapp@0.1.0
  requires rustc 1.92` överst i felutskriften.
  - **Siffran är VERIFIERAD, inte antagen.** Första gissningen var 1.88
    (let-chains, som är det nyaste vår EGEN kod använder) — den var FEL.
    Ett riktigt `cargo +1.88 build` visade att golvet i stället sätts av
    GTK-stacken: `gdk4`/`cairo-rs`/`gio` kräver 1.92. Sedan kördes
    `cargo +1.92 build` OCH hela testsviten (203/203) på just den
    toolchainen för att bekräfta att den faktiskt duger.
  - `edition = "2024"` kräver i sig bara 1.85 — MSRV:n kommer alltså helt
    från beroendena, vilket är precis varför den behöver mätas om vid
    varje större beroendeuppgradering.
  - CI kör `dtolnay/rust-toolchain@stable`, alltså alltid senaste stabila.
    MSRV:n är golvet, inte vad vi bygger med till vardags.

- **SÄKERHET: russh 0.45 → 0.62.5, stänger 13 dependabot-varningar**
  (2026-08-05, `Cargo.toml` + `ssh.rs`/`key_deploy.rs`/`port_forward.rs`):
  samtliga öppna sårbarhetsvarningar i repot satt i `russh` — SSH-
  biblioteket hela `LinuxApp` vilar på. Varningarna hade stått i varje
  push-utskrift ("GitHub found 13 vulnerabilities") utan att åtgärdas.
  - Flera är CLIENT-side, alltså nåbara från en illasinnad eller kapad
    server vi ansluter till: panik vid X25519-nyckel med fel längd
    (pre-auth DoS), panik vid all-zero Curve25519-värde, obegränsad
    allokering efter dekomprimering, okontrollerat antal
    keyboard-interactive-prompter, samt icke-kanoniska SSH-bannrar utan
    längdgräns. Resten är server-side och berör inte oss (Bastion är
    alltid klient — samma avgränsning som vid cert-auth).
  - `russh-sftp` följde med 2.3.0 → 2.4.0.
  - **API-migrering över 17 minorversioner**, de fem substantiella:
    1. `Handler` använder nu native `async fn in trait` — vårt
       `#[async_trait]` måste BORT, annars livstidskrock.
    2. Nycklar bygger på `ssh_key`-typerna: `KeyPair` → `PrivateKey`,
       `KeyPair::generate_ed25519()` → `PrivateKey::random(rng, alg)`,
       `clone_public_key()` → `public_key()`, `key.name()` →
       `algorithm().as_str()`.
    3. Alla `authenticate_*` returnerar `AuthResult` i stället för
       `bool` (`.success()`).
    4. `authenticate_publickey` tar `PrivateKeyWithHashAlg` — RSA kräver
       ett uttryckligt hash-val, hämtas via `best_supported_rsa_hash()`.
       `authenticate_future` ersatt av `authenticate_publickey_with`,
       som LÅNAR agenten i stället för att flytta den fram och tillbaka.
       Agentidentiteter är nu `AgentIdentity` (nyckel + kommentar, eller
       certifikat), inte rena `PublicKey`.
    5. `server_channel_open_forwarded_tcpip` har fått ett
       `ChannelOpenHandleInner`-argument som MÅSTE `accept()`:as eller
       `reject()`:as — att bara låta det droppas skickar automatiskt
       `AdministrativelyProhibited`.
  - **Punkt 5 var en riktig regression som testsviten fångade**: första
    migreringen lämnade handtaget oanvänt, vilket tyst stängde varje
    vidarebefordrad anslutning ("Connection reset by peer"). Upptäckt av
    `remote_forward_reaches_a_real_separate_echo_server_through_real_sshd`
    — ett av de tester som kör mot en RIKTIG `sshd`, inte en mock. Utan
    den sortens test hade `-R` gått sönder tyst.
  - 201/201 `cargo test` gröna (stabilt över 4 körningar), `clippy`
    oförändrat på baseline.

- **BUGGFIX: S3-nedladdning läste hela objektet i minnet** (2026-08-05,
  `s3.rs` + `main.rs`): `get_object` hämtade hela kroppen via
  `Response::bytes()` innan något skrevs till disk. För en S3-bucket —
  där flergigabyte-objekt (backuper, diskavbildningar, video) är helt
  vardagliga — hade en nedladdning därmed krävt lika mycket RAM som
  filen är stor och i praktiken dödat appen. Inkonsekvent med kodbasens
  egen princip: `ssh.rs` har sedan tidigare ett 4 MiB-tak
  (`MAX_COMMAND_OUTPUT_BYTES`) just för att inte svälla minnet.
  - Ny `get_object_to_file` strömmar bit för bit direkt till disk, utan
    att någon gång hålla hela filen i minnet. Skriver till en temporär
    `.part`-fil i SAMMA katalog och byter namn först när hela
    hämtningen lyckats — en avbruten nedladdning lämnar aldrig en halv
    fil på målsökvägen (samma resonemang som
    `external_binary_fetcher::fetch`). Övriga anrop
    (`list_buckets`/`list_objects`/felsvar) är små XML-dokument och
    läses fortfarande i ett svep.
  - **Ingen ny beroendeyta**: `reqwest::Response::chunk()` fungerar utan
    `stream`-featuren. Enda tillägget är `fs` på tokio — en extra
    feature-flagga på ett redan befintligt beroende, inget nytt krat.
  - Två nya tester: en ~3 MiB-kropp med ett mönstrat innehåll strömmas
    ner och jämförs byte för byte (bevisar att bitarna sätts ihop rätt,
    inte bara att den första stämmer) och att ingen `.part`-fil lämnas
    kvar; samt att ett 404-svar ger ett tydligt fel UTAN att skapa någon
    fil alls på målsökvägen.

- **BUGGFIX: misslyckad nyckeldistribution rapporterades som ett
  verifieringsfel** (2026-08-05, `key_deploy.rs`): `deploy_and_verify`
  ignorerade distributionskommandots exitkod helt. Misslyckades det
  (skrivskyddad `~/.ssh`, full disk, saknad rättighet) upptäcktes det
  visserligen ändå — verifieringsanslutningen föll — men felet blev
  missvisande: *"nyckeln deployades men verifieringen misslyckades"*,
  när sanningen var att den aldrig deployades. Den säkerhetskritiska
  egenskapen höll hela tiden (lösenordet tas aldrig bort utan bevisat
  fungerande nyckel), men felsökningen blev onödigt svår.
  - Nu läses exitkoden, och serverns stderr tas med i meddelandet:
    "distributionskommandot misslyckades (exitkod N): … — nyckeln
    deployades INTE".
  - **Intressant fynd om meddelandeordningen** (empiriskt verifierat mot
    en riktig `sshd`, inte antaget): här anländer `Eof`/`Close` FÖRE
    `exit-status`. Ett första försök som bröt loopen på `Eof`/`Close`
    tappade därför exitkoden helt och tolkade allt som lyckat. Det är
    spegelbilden av #269:s buggmönster, och lärdomen är att de två
    slutvillkoren INTE är utbytbara: utdata kan per definition inte komma
    efter EOF, men `exit-status` är en kanalFÖRFRÅGAN och får det. Här
    läses därför tills kanalen är HELT stängd (`wait()` → `None`), med
    `COMMAND_TIMEOUT` som skydd mot en server som aldrig stänger.
  - Nytt test som framkallar ett genuint misslyckat distributions-
    kommando mot en riktig `TestSshd` och kräver att felet pekar ut
    distributionen, inte verifieringen.

- **BUGGFIX: kommandoutdata kunde tappas HELT (tom systemöversikt/
  Docker-lista)** (2026-08-05, `ssh.rs`): såg först ut som ett flakigt
  test, visade sig vara en riktig produktionsbugg.
  - `run_command_on_session` gjorde `ChannelMsg::ExitStatus => break`.
    Men SSH garanterar INTE att all `Data` hunnit levereras innan
    `exit-status` — servern skickar typiskt `exit-status` så fort
    processen dör, medan utdatan fortfarande kan ligga kvar i kanalens
    kö. Kom meddelandena i den ordningen returnerade kommandot **tom
    sträng trots att det lyckats**.
  - Drabbar allt som läser kommandoutdata: systemöversikten
    (`dashboard.rs`), Docker-listan/-loggarna, Tailscale-hämtning över
    SSH, nyckeldistributionens verifiering. Symptom: en tom vy, utan
    felmeddelande. Lastberoende — därför sporadiskt.
  - Upptäckt genom att `connect_via_jump_reaches_the_real_separate_
    target_sshd` föll i CI TVÅ gånger (PR #263 och #268) med
    `left: ""` medan den passerade lokalt. Första gången avfärdad som
    CI-resurskonkurrens; andra gången granskades loopen ordentligt och
    orsaken hittades. **Lärdom: ett "flakigt" test som alltid failar på
    SAMMA sätt är en bugghypotes, inte brus.**
  - Fix: `Eof`/`Close` (eller att `wait()` ger `None`) är de enda
    korrekta slutvillkoren — efter `Eof` kommer per definition ingen mer
    data. `COMMAND_TIMEOUT` skyddar redan mot en server som aldrig
    stänger kanalen.

- **SÄKERHETSFIX: en oläsbar `known_hosts` tappade tyst hela
  MITM-skyddet** (2026-08-05, `known_hosts.rs` + `ssh.rs`): hittad vid
  egen genomläsning av de säkerhetskänsligaste modulerna (CodeRabbit
  borttaget ur repot).
  - `KnownHosts::load` returnerade en TOM karta vid VARJE läsfel, inte
    bara när filen saknades. Blev `~/.bastion/known_hosts` oläsbar
    (rättighetsfel, I/O-fel, sökvägen pekar fel) betydde det att varje
    värd blev `Learned` — inlärd på nytt — i stället för kontrollerad
    mot sin lagrade nyckel. Alltså: **TOFU-skyddet mot MITM försvann
    tyst, utan minsta indikation**, precis det enda filen finns för.
  - Fix: `load` skiljer nu `NotFound` (första körningen → tom lagring,
    korrekt) från alla andra fel (→ `Err`). `KnownHosts::open` är
    fallibel och `ssh.rs` **faller stängt**: anslutningen avbryts med
    "vägrar ansluta utan värdnyckelskontroll" i stället för att fortsätta
    oskyddat. Gäller både target- och jump-hosten.
  - **Medveten AVVIKELSE från Swift-referensen**, som använder `try?`
    och sväljer alla fel på samma ställe (`KnownHosts.loadEntries`) —
    men i linje med kodbasens egen princip på andra håll
    (`HostStore::load`/`AppSettingsStore::load` skiljer redan "finns
    inte" från "går inte att tolka"). Swift-sidan har samma svaghet och
    bör åtgärdas där också.
  - Två nya tester: saknad fil är INTE ett fel (första körningen ska
    fungera), och ett läsfel som inte är `NotFound` ger `Err` i stället
    för tom lagring. Det senare är **verifierat att faktiskt fånga
    buggen** — med fixen bortplockad misslyckas det. Felet framkallas
    via en katalog i stället för rättigheter, så det är deterministiskt
    även om testet skulle köras som root.

- **BUGGFIX: seriell läs-tråd + filbeskrivare läckte vid varje stängd
  flik** (2026-08-05, `serial.rs`): hittad vid egen genomläsning av
  sessionens egna moduler (CodeRabbit borttaget ur repot).
  - Rotorsak: `cfmakeraw` sätter `VMIN=1/VTIME=0` — "blockera i `read()`
    tills minst en byte kommer, hur länge som helst" — och
    `open_and_configure` rensar medvetet bort `O_NONBLOCK`. `stop`-
    flaggan (som skriv-tråden sätter när fliken stängs) kontrolleras
    bara i läsloopens TOPP. På en TYST port — en helt vanlig konsolport
    som inte skriver något av sig själv — vaknade läs-tråden därför
    aldrig: tråden OCH den öppna filbeskrivaren läckte permanent, en
    gång per stängd seriell flik. Kodens egen kommentar kallade den
    "pollningsloopen", men ingenting pollade.
  - Fix: `VMIN=0/VTIME=1` (0,1 s) efter `cfmakeraw`, så `read()`
    returnerar regelbundet även utan data och loopen hinner se flaggan.
    Med `VMIN=0` betyder `read() == 0` "timeout, ingen data" — INTE EOF
    — så läsloopen `continue`:ar i stället för att `break`:a på 0;
    frånkoppling syns i stället som ett fel (EIO) i `Err`-grenen.
  - Regressionstest mot en riktig PTY som ALDRIG skickar något, med
    master medvetet öppen (så det bevisas att det är `stop`-flaggan som
    fungerar, inte att porten råkade dö). **Testet är verifierat att
    faktiskt fånga buggen**: med fixen bortplockad misslyckas det på
    10 s. Det väntar via en `mpsc::recv_timeout`-brygga i stället för
    `recv_blocking` just för att FAILA i stället för att HÄNGA (en
    hängande CI-körning hade ätit hela jobbets tidsbudget) — även det
    verifierat empiriskt: den direkta varianten hängde >180 s.
  - Även fixat i samma svep: `libc::dup` kontrolleras nu mot -1 (t.ex.
    EMFILE). Utan kontrollen gav `File::from_raw_fd(-1)` en session som
    SÅG ansluten ut och kunde läsa, men där varje skrivning tyst
    misslyckades.

- **ROBUSTHETSFIX: OAuth-loopbacken band sig till FÖRSTA anslutningen**
  (2026-08-05, `oauth.rs`): hittad vid egen genomläsning av den nyss
  skeppade inloggningen (CodeRabbit borttaget ur repot — självgranskning
  + CI är det som finns kvar).
  - `await_redirect` gjorde ETT `accept()` och behandlade den
    anslutningen som redirecten. Men webbläsare öppnar rutinmässigt
    anslutningar som INTE är redirecten: spekulativ TCP-"preconnect"
    (öppnas, skickar aldrig något) och `/favicon.ico` efter att
    svarssidan renderats. En preconnect hade slukat platsen och lämnat
    den riktiga redirecten obesvarad tills 5-minuterstimeouten löpte ut
    — inloggningen "hänger" utan förklaring. En tidig favicon-begäran
    hade i stället gett ett förvirrande "leverantören returnerade ett
    fel: okänt fel".
  - Fix: loopa över anslutningar tills en visar sig vara den riktiga
    (har `code` eller `error`). Övriga får ett artigt 404 och loopen
    väntar vidare. Per anslutning finns dessutom en 5 s lästimeout så
    en tyst preconnect inte stoppar upp kön.
  - Skärpte samtidigt state-hanteringen: en callback med `code` men
    HELT UTAN `state` rapporterades tidigare som "okänt fel" (föll
    igenom till leverantörsfel-grenen) i stället för det korrekta
    state-felet — CSRF-skyddet kan inte verifieras utan `state`, så det
    ÄR ett state-fel.
  - Nytt test: en strö-begäran utan `code`/`state` besvaras med 404 och
    den efterföljande riktiga redirecten accepteras ändå.

- **BUGGFIX: klick i värdlistan öppnade session mot FEL VÄRD** (2026-08-05,
  `main.rs` + regressionstest i `host_grouping.rs`): en riktig bugg som
  infördes av den sektionerade listvyn tidigare samma dag och hittades
  vid egen genomläsning av koden (CodeRabbit togs bort ur repot under
  sessionen — självgranskning + CI/dependabot är det som finns kvar).
  - Rotorsak: raderna kopplades via en `ListBox::connect_row_activated`
    som slog upp värden på radens INDEX i `HostStore::all()`. Det var
    korrekt så länge listan var platt och i exakt samma ordning som
    `all()` — men den sektionerade vyn lägger till rubrikrader, sorterar
    på alias inom varje sektion, plockar ut favoriter ur sina
    taggsektioner och låter en multi-taggad värd förekomma i FLERA
    sektioner. Dessutom filtrerar sökfältet bort rader. Index pekade
    därför på fel post — ett klick anslöt till en annan värd än den man
    klickade på.
  - Fix: `adw::ActionRow::connect_activated` PER RAD, med värdens `id`
    fångat direkt i closuren (samma mönster som radens övriga
    host-åtgärder redan använde). Robust även mot att listan hinner
    ändras mellan att raden byggs och att den klickas.
  - Regressionstest (`flattened_group_order_does_not_match_raw_store_order`)
    som visar exakt varför index-uppslag är ogiltigt: den utplattade
    sektionsordningen har både annan ordning OCH fler element än
    rålistan. Skyddar mot en framtida "förenkling" tillbaka till index.

- **BUGGFIX: favoritstjärnan uppdaterade inte listans gruppering**
  (2026-08-05, `main.rs`): att stjärnmärka en värd sparade till disk men
  byggde inte om listan, så värden låg kvar i sin gamla sektion tills
  något annat råkade trigga en uppdatering. Swift-sidans `toggleFavorite`
  → `save` → `reload()` gör det direkt. Fixat, med två detaljer värda
  att notera:
  - Ombyggnaden är UPPSKJUTEN till nästa huvudloopsvarv
    (`glib::idle_add_local_once`) i stället för att köras rakt av —
    `refresh_list` river ut alla rader, inklusive den rad vars knapp just
    står mitt i sin egen signalhantering. GTK håller en referens under
    signalemissionen (ingen use-after-free), men att låta hanteraren
    returnera först undviker hela frågan.
  - Vakt mot att en toggling som bara speglar ett redan sparat värde
    triggar en onödig ombyggnad (och därmed mot att ombyggnaden i
    förlängningen skulle kunna trigga sig själv).

- **Auto-poll av systemöversikten (dashboard) i den NYA Rust/GTK4-
  `LinuxApp`** (2026-08-05, `main.rs`): "Kvar" från dashboard-PR:en
  (#254) — motsvarar Swift-sidans `DashboardModel.startPolling()`,
  15 s intervall, hämtar direkt sedan om och om igen tills fliken
  stängs.
  - **Genuin race hittad och fixad under självgranskningen** (inte
    CodeRabbit den här gången — se nedan): `ssh::COMMAND_TIMEOUT` är
    30 s, LÄNGRE än det tänkta 15 s-pollintervallet. Den ursprungliga
    implementationen hade eldat av en NY fire-and-forget-uppdatering var
    15:e sekund oavsett om föregående ännu väntade på svar — en ovanligt
    långsam anslutning hade kunnat ge två samtidiga rensa/fylla-
    körningar mot samma `ListBox` (flimmer eller en halvfärdig lista).
    Löst genom att bryta ut kärnan (`refresh_dashboard_once`) och låta
    pollningsloopen AWAITA den direkt i stället för att elda och glömma
    — värsta fall blir hämtningstid+15 s mellan uppdateringar, aldrig
    en överlappning.
  - Stoppvillkor: `AdwTabView::page_position(&page) < 0` (fliken inte
    längre i trädet), kollat både innan och efter varje uppdatering —
    INTE en `clone!`-`#[weak]`-uppgradering (den sker bara en gång, vid
    loopens start, inte per varv — en git-historik-anteckning värd att
    komma ihåg nästa gång ett liknande mönster används).
  - Ren GTK-limkod, ingen ny modul/inga nya tester. 195/195 `cargo test`
    fortsatt gröna, `clippy` tyst.
  - **CodeRabbit borttaget från repot under den här sessionen** — från
    och med nu självgranskas varje PR mer noggrant innan den skickas,
    med CI/dependabot som enda kvarvarande automatiska hjälp.

- **Interaktiv OAuth2/PKCE-inloggning i den NYA Rust/GTK4-`LinuxApp`**
  (2026-08-05, `LinuxApp/src/oauth.rs` + `main.rs`): den del av
  OAuth-portningen (#256) som medvetet sköts upp — öppna en
  webbläsare, fånga koden som skickas tillbaka. Löst med en lokal
  loopback-HTTP-lyssnare enligt RFC 8252 ("OAuth 2.0 for Native Apps"),
  det designval som lämnades öppet i #256 (kontra ett registrerat
  anpassat URI-schema via `xdg-mime`/en `.desktop`-fil) — fungerar
  oavsett skrivbordsmiljö/paketformat (deb/rpm/flatpak), ingen extra
  registrering krävs.
  - `oauth::start_login`/`finish_login`: binder en `TcpListener` på en
    SLUMPAD ledig port (`127.0.0.1:0`), bygger auktoriseringsURL:en med
    den dynamiska `redirect_uri`:n, väntar in EN inkommande begäran
    (5 minuters timeout — en användare som stänger fliken ska inte
    hänga appen för evigt), tolkar `code`/`state` ur query-strängen,
    svarar webbläsaren med en enkel bekräftelsesida, och byter koden mot
    ett riktigt token. Uppdelad från själva webbläsaröppningen
    (`gtk::UriLauncher`, portal-baserad — fungerar även sandboxat, till
    skillnad från att skala ut till `xdg-open`) så resten av flödet
    förblir testbart utan GTK.
  - **3 nya GENUINT end-to-end-testade fall**, inte mockade: en riktig
    `TcpListener` på en riktig slumpad port, en riktig HTTP-klient som
    spelar "webbläsaren" (gör exakt den GET-begäran webbläsaren skulle
    gjort), och en riktig lokal token-server för själva bytet — lyckat
    fall, fel `state` (CSRF-skydd, avvisas tydligt men webbläsaren får
    ändå ett svar, inte en trasig anslutning), och ett leverantörsfel
    (`?error=access_denied`).
  - Ny "Kontosynk"-sektion i inställningsdialogen: en rad per
    leverantör (Dropbox/Google Drive/OneDrive) med Logga in/ut-knapp,
    motsvarar `App/SyncSettingsView.swift`s `accountRow`. **Bara
    inloggningen** — att faktiskt ladda upp/ner den krypterade synkfilen
    via kontot är ett eget, större steg, se "Kvar".
  - **VIKTIGT för Dropbox specifikt**: det registrerade `client_id`:t
    (`ira5qtb04w4qikk`) är bara godkänt för
    `se.denied.bastion://oauth/dropbox` (Apple-sidans schema) i Dropboxs
    app-konsol — en loopback-URL måste läggas till som ytterligare en
    tillåten redirect-URI där innan inloggning mot Dropbox fungerar i
    produktion. Ett kontoägar-beslut, görs inte av koden.
  - **Kvar**: den faktiska kontosynk-transporten (ladda upp/ner
    `bastion-sync.enc` via Dropbox-/Google Drive-/OneDrive-API:et,
    motsvarande `DropboxSyncProvider.swift` m.fl.) är INTE byggd —
    `OAuthTokenStore::load`/`valid_access_token`/`refresh_access_token`
    väntar redan färdiga på den (bara testade, ej ansluten till
    `main.rs` än, `#[allow(dead_code)]`-markerade med motivering).
  - 195/195 `cargo test` gröna totalt, `cargo build`/`clippy` helt
    tysta.

- **`ExternalBinaryFetcher` — generisk hämta+verifiera+cacha-hjälpare för
  externa binärer, i den NYA Rust/GTK4-`LinuxApp`** (2026-08-05,
  `LinuxApp/src/external_binary_fetcher.rs`): port av
  `Sources/SSHCore/ExternalBinaryFetcher.swift` — **medvetet
  framåtblickande infrastruktur, inte en stängd lucka**: varken denna
  Rust-modul eller Swift-referensen har någon anropare än. Byggstenen
  för VISION.md "Native WireGuard/Tailscale — inget externt beroende"
  (se Swift-sidans "första byggstenen"-notering, 2026-08-03).
  - Genererar+jämför SHA256 av de NEDLADDADE bytesen INNAN något skrivs
    till disk — en manipulerad/fel binär hamnar aldrig i cachen ens
    tillfälligt. En redan cachad fil med fel checksumma (korrupt eller
    ett gammalt felaktigt försök) städas bort och laddas ner på nytt.
    Skriver till en temporär fil i SAMMA katalog, byter sedan namn — en
    process som läser destinationen mitt under en nedladdning kan
    aldrig se en halvskriven fil. `chmod 755` på Unix.
  - 4 tester porterade rakt av från `Tests/SSHCoreTests/
    ExternalBinaryFetcherTests.swift` — RIKTIGA nätverksanrop mot en
    pinnad, oföränderlig GitHub-tagg (`raw.githubusercontent.com/
    torvalds/linux/v6.6/COPYING`), samma URL/checksumma som Swift-
    sidans tester, inte mockat. Verifierar att en cache-träff genuint
    undviker nätverket helt (andra anropet pekas mot en overksam URL —
    om cachningen INTE fungerade hade anropet failat, inte tyst
    returnerat rätt svar av misstag).
  - 192/192 `cargo test` gröna totalt (alla 4 nya gick igenom mot
    riktigt nätverk, ingen skippning), `cargo build`/`clippy` helt
    tysta (modulen är `#![allow(dead_code)]` tills en faktisk
    WireGuard-/Tailscale-nedladdningsfunktion kopplar in den).

- **Reglage för Portvidarebefordran/SSH-nyckel-knapparna i
  Funktioner-inställningarna i den NYA Rust/GTK4-`LinuxApp`**
  (2026-08-05, `main.rs`): `settings::FeatureToggles::show_port_forward`/
  `show_key_deploy` lästes redan korrekt (`gio_menu_for` dolde "Tunnel"/
  "Nyckel"-menyposterna som förväntat) men gick bara att stänga av genom
  att redigera `settings.json` för hand — inget reglage i
  inställningsdialogen, till skillnad från de tre andra fälten (Docker/
  Kommandobibliotek/SFTP) som redan hade det. Motsvarar de två sista
  togglarna i `App/FeatureSettingsView.swift`s "Värdmenyn"-sektion.
  `show_snippets` förblir avsiktligt utan reglage — LinuxApp har ingen
  egen Snippets-vy att gömma (fältet finns bara kvar för att inte tappa
  en delad `settings.json`-fils övriga värden, samma motivering som
  redan gällde de andra fem fälten).
  - Ren GTK-limkod, ingen ny modul/inga nya tester. 188/188 `cargo test`
    fortsatt gröna, `cargo build`/`clippy` helt tysta.

- **Bitwarden-autentisering (`HostAuth::BitwardenItem`) i den NYA
  Rust/GTK4-`LinuxApp`** (2026-08-05, `LinuxApp/src/bitwarden.rs` +
  auth-väljare i `main.rs`): port av `Sources/SSHCore/BitwardenClient.swift`
  — men INTE bara paritet med en redan fungerande Apple-funktion.
  **Linux är faktiskt den ENDA plattformen där det här kan fungera i
  praktiken**: `App/AuthResolver.swift`s `resolveAuth` returnerar
  ALLTID `nil` för `.bitwardenItem` på båda Apple-plattformarna — iOS
  saknar `Foundation.Process` helt, och macOS-målets App Sandbox dödar
  `bw`-processen med ett okatchbart SIGTRAP, empiriskt verifierat på
  riktig macOS-hårdvara (se `AuthResolver.swift`s kommentar, som
  uttryckligen konstaterar att `bw` "faktiskt fungerar" på Linux-sidan).
  Ett tidigare felaktigt antagande i `ssh.rs`s modulkommentar grupperade
  `BitwardenItem` ihop med den genuint Apple-specifika `KeychainKey` som
  "saknar Linux-motsvarighet" — det stämde aldrig för `BitwardenItem`.
  - `std::process::Command::output()` (ingen manuell konkurrent
    pipe-läsning behövs, till skillnad från Swift-sidans egna
    `ProcessRunner` — Rusts `.output()` gör redan det säkert internt).
    Session skickas via `BW_SESSION`-miljövariabeln (INTE argv
    `--session`, som läcker via `/proc/*/cmdline`), `--nointeraction`
    förhindrar att `bw` hänger på ett interaktivt huvudlösenords-prompt
    Bastion inte har någon terminal att svara i. Bara den avslutande
    radbrytningen `bw` lägger till trimmas — ett riktigt lösenord med
    avsiktligt ledande/inre whitespace korrumperas INTE.
  - 6 tester porterade rakt av från `Tests/SSHCoreTests/
    BitwardenClientTests.swift` — riktiga körbara `/bin/sh`-fixturer,
    samma teknik som `TestSshd`. Ett genuint (om obskyrt) test-bara
    `ETXTBSY`-("Text file busy")-race hittades och fixades under
    verifieringen: `cargo test`s parallella trådar inom SAMMA process
    kan tillfälligt neka `execve()` på ett just skrivet skript om en
    ANNAN tråd forkar (t.ex. `TestSshd`) medan filhandtaget hunnit
    ärvas men inte stängts än — 100 % reproducerbart borta med
    `--test-threads=1`, alltså bevisligen inte ett fel i
    `fetch_password` själv. Löst med ett lokalt lås + en kort
    omförsöksloop specifikt i testkoden (aldrig i produktionsvägen).
  - Ny "Bitwarden"-post i auth-väljaren (från cert-auth-PR:en) med ett
    item-id/namn-fält, motsvarar `HostEditView`s
    `bitwardenItemIDText`-fält.
  - 188/188 `cargo test` gröna totalt, `cargo build`/`clippy` helt
    tysta.

- **Sektionerad värdlista (favoriter + taggrupper) + sökfält i den NYA
  Rust/GTK4-`LinuxApp`** (2026-08-05, `LinuxApp/src/host_grouping.rs` +
  `main.rs`): "Kvar" från föregående PR — den riktiga omstruktureringen
  av `refresh_list`s tidigare platta lista, port av
  `HostListModel.groups` + `HostListView.filteredGroups` i
  `App/HostListView.swift`.
  - Grupperingsalgoritmen EXAKT som Swift-sidan, inte en förenkling:
    favoriter (oavsett tagg) i en egen "★ Favoriter"-sektion FÖRST (bara
    om icke-tom), resten grupperat per tagg (alfabetiskt,
    skiftlägesokänsligt) — otaggade värdar hamnar i en "Övriga"-sektion,
    en värd med FLERA taggar förekommer i VARJE sin taggs sektion
    (medvetet, matchar Swift rakt av, inte en bugg). Varje sektions
    värdar sorteras på alias, skiftlägesokänsligt.
  - Nytt sökfält (`gtk::SearchEntry`) i sidopanelen: filtrerar på alias/
    värdnamn/användare/taggar, skiftlägesokänsligt — tomma sektioner
    (ingen träff i gruppen) faller bort helt, inte bara döms osynliga.
  - Grupperings-/filtreringslogiken bruten ut till en egen, GTK-fri
    modul (`host_grouping.rs`) just för att kunna TESTAS på riktigt —
    till skillnad från resten av `main.rs`s GTK-limkod, som bara
    verifieras via en lyckad `cargo build`. 8 nya tester (favorit-
    sektionens ordning/dedupliceringen mot taggsektioner, "Övriga"-
    fallback, multi-tagg-förekomst, skiftlägesokänslig sortering/
    sökning, tomma-sektioner-faller-bort).
  - Sökfiltrets tillstånd delas via en `Rc<RefCell<String>>` (samma
    mönster som `settings_store`/`snippet_store`) i stället för att
    trädas igenom `refresh_list`s ~10 anropsplatser som en vanlig
    parameter skulle krävt ändå — `refresh_list`/`show_host_dialog`/
    `show_settings_dialog`/`show_ssh_config_import_dialog`/
    `show_tailscale_discovery_dialog` fick alla den nya parametern,
    `#[allow(clippy::too_many_arguments)]` där antalet redan var åtta.
  - 182/182 `cargo test` gröna totalt, `cargo build`/`clippy` helt tysta.

- **Favoritmarkering + taggar (Host::is_favorite/tags), grunddata + redigering, i
  den NYA Rust/GTK4-`LinuxApp`** (2026-08-05, `main.rs`): ännu ett
  "fältet finns i datamodellen, ingen UI-koppling"-fynd — samma mönster
  som `color_tag` (samma PR-serie). Medvetet avgränsad till DATA +
  redigering + ograpperad visning i det här steget, INTE den fulla
  sektionerade listvyn Swift-sidan har (se "Kvar").
  - Stjärnknapp direkt i värdlistans rad — sparas OMEDELBART vid klick
    (ingen "spara"-knapp krävs), samma "Favorit"/"Ta bort favorit"-
    växling som `App/HostListView.swift`s context-meny. Ikonnamnen
    (`starred-symbolic`/`non-starred-symbolic`) är GNOME/Adwaitas
    standardnamn, samma som Nautilus/GNOME Webs bokmärkesknappar.
  - Taggfält (kommaseparerat) i värdredigeringsdialogen — EXAKT samma
    tolkning som Swift-sidans `save()`: dela på komma, trimma
    blanktecken, kasta tomma segment (ett kvarglömt avslutande komma
    ska inte ge en spöktagg).
  - Taggarna visas i värdlistans undertext (`"user@host:port · tag1,
    tag2"`) så de är synliga/redigerbara redan nu, men grupperar/
    filtrerar INTE listan än.
  - Ren GTK-limkod, ingen ny modul/inga nya tester. 174/174 `cargo test`
    fortsatt gröna, `clippy` tyst.
  - **Kvar**: den sektionerade listvyn `App/HostListView.swift` har
    (favoriter i en egen sektion överst, resten grupperat per tagg,
    otaggade i en "Övriga"-kategori) samt sökfältets tagg-matchning —
    en riktig omstrukturering av `refresh_list`s platta `gtk::ListBox`,
    inte en enkelradsändring, avsiktligt ett eget, senare steg.

- **Färgmärkning av värdar (`Host::color_tag`) i den NYA Rust/GTK4-
  `LinuxApp`** (2026-08-05, `main.rs`): fältet har funnits i datamodellen
  sedan starten (wire-kompatibelt med Swift-sidans `colorTag`, se
  `Host.swift`) men saknade helt UI-koppling i `LinuxApp` — varken en
  väljare i värdredigeringsdialogen eller en visuell markering i
  värdlistan. Samma "fältet finns, ingen använder det"-gapmönster som
  certifikatautentiseringen (#252) och auth-väljaren i värdredigerings-
  dialogen.
  - Sju namngivna färger (röd/orange/gul/grön/blå/lila/grå), samma
    paletturval som `App/HostColor.swift`s `HostColorPalette` — Adwaitas
    egna namngivna accentfärger som hex-värden, inte gissade.
  - GTK4 har (till skillnad från GTK3) ingen per-widget
    `style_context()`/`add_provider()` längre — en delad `CssProvider`
    med sju fasta `.host-color-<namn>`-klasser, laddad EN gång globalt
    vid appstart, är rätt väg för ett litet fast antal namngivna färger.
  - Värdlistan: en liten färgad cirkel som radens prefix, motsvarar
    `HostRow`s cirkel i `App/HostListView.swift` — osynlig helt om ingen
    färg är satt, inte en tom/grå platshållare.
  - Värdredigeringsdialogen: sju klickbara färgade cirklar, EXKLUSIVT val
    med manuell (inte `ToggleButton::set_group`) logik — trycker man på
    den REDAN valda färgen igen tas valet bort helt, samma beteende som
    Swift-sidans `HostColorPicker` (inte vanlig radioknapp-semantik, som
    inte tillåter att avmarkera).
  - Ren GTK-limkod, ingen ny modul/inga nya tester (samma typ av
    UI-verifiering via lyckad `cargo build` som tab-numreringsfixen).
    174/174 `cargo test` fortsatt gröna, `clippy` tyst.

- **OAuth2/PKCE-kontosynk, KÄRNAN, i den NYA Rust/GTK4-`LinuxApp`**
  (2026-08-05, `LinuxApp/src/oauth.rs`): port av
  `Sources/SSHCore/OAuthPKCE.swift` + `OAuthToken.swift` +
  `App/OAuthProviders.swift`, samt (delvis) den RIKTIGA HTTP-logiken i
  `App/OAuthAccountManager.swift`/`OAuthTokenStore.swift` — MEDVETET
  avgränsad till kärnan, inte hela funktionen i en enda PR (se "Kvar").
  - PKCE (RFC 7636): `pkce_verifier`/`pkce_challenge`, verifierad mot
    RFC:ns egen S256-testvektor rad för rad (inte bara "verifiern och
    utmaningen skiljer sig åt").
  - `OAuthTokenResponse`/`StoredOAuthToken` med samma 60 s förnyelse-
    marginal och samma "bevara gammal `refresh_token` om leverantören
    inte skickar en ny"-logik som Swift-sidan — 6 tester porterade rakt
    av från `Tests/SSHCoreTests/OAuthTokenTests.swift`.
  - Samma tre leverantörer/exakt samma registrerade värden (Dropbox
    `client_id` ifyllt, Google Drive/OneDrive tomma — "inte
    konfigurerade ännu", inte ett Rust-specifikt hål) som
    `App/OAuthProviders.swift`.
  - **RIKTIGT token-utbyte och token-förnyelse över HTTP** (`reqwest`,
    formulärkodning, JSON-svar) — testat mot en genuin lokal HTTP-server
    (samma teknik som `s3.rs`s `spawn_fake_s3_server`), inte mockat:
    verifierar de faktiska fälten i den skickade formulärkroppen
    (`grant_type`/`code`/`code_verifier` respektive
    `grant_type=refresh_token`), att ett icke-2xx-svar blir ett tydligt
    fel (inte en tyst godkänd tom token), och att `valid_access_token`
    både förnyar tyst vid utgången token OCH återanvänder en fortfarande
    giltig token UTAN att göra något HTTP-anrop alls (bevisat genom att
    inte ens starta en testserver i det fallet — ett anrop hade
    kraschat/timeoutat mot en icke-lyssnande port).
  - Lokal lagring (`~/.bastion/oauth_tokens.json`) — samma skyddsnivå
    som S3-kopplingarnas `secret_access_key`/WireGuard-privatnycklar
    redan har i den här klienten, INTE macOS Keychain (Linux har ingen
    universell motsvarighet över skrivbordsmiljöer).
  - **Kvar (medvetet, nästa steg)**: den INTERAKTIVA inloggningen —
    öppna en webbläsare mot `authorization_endpoint`, fånga
    redirect-URI:n med koden — är INTE med. Apple-sidan löser det med
    `ASWebAuthenticationSession`; Linux saknar en GTK-inbyggd
    motsvarighet, och valet mellan en lokal loopback-HTTP-lyssnare
    (RFC 8252) och att registrera `se.denied.bastion://`-schemat via
    `xdg-mime`/en `.desktop`-fil är ett öppet designval som inte avgörs
    här. Ingen UI-koppling i `main.rs` än av samma skäl — inget att
    klicka på förrän inloggningsflödet finns. 18 nya tester, 174/174
    `cargo test` gröna totalt, `cargo build`/`clippy` helt tysta (modulen
    är `#![allow(dead_code)]` tills nästa PR kopplar in den).

- **Numrerade fliktitlar för flera samtidiga sessioner mot SAMMA värd i
  den NYA Rust/GTK4-`LinuxApp`** (2026-08-05, `main.rs`,
  `unique_session_tab_title`): `AdwTabView` gav redan LinuxApp hela
  grundfunktionen `App/MultiSessionView.swift` beskriver (flera samtidigt
  anslutna sessioner, bakgrundsflikar rivs inte ner, flikbyte) — den enda
  verkliga luckan var `displayLabel`s specifika UX-regel: en andra/tredje
  session mot SAMMA värd (t.ex. via "Ny flik" i SFTP-/nyckeldistributions-
  vyerna, eller att dubbelklicka samma rad igen) fick tidigare en flik med
  EXAKT samma titel som den första, omöjlig att skilja åt i flikraden.
  - Räknar befintliga flikar (`AdwTabView::pages()`) vars titel är
    `alias` eller `alias (N)` innan en ny flik döps — första sessionen
    mot en värd förblir odekorerad, andra blir "(2)", tredje "(3)" osv.,
    samma numreringsprincip som Swift-sidans `displayLabel`.
  - Ingen ny modul/inga nya tester — ren GTK-limkod, verifierad via
    `cargo build` (156/156 `cargo test` fortsatt gröna, `clippy` tyst).

- **Systemöversikt (dashboard) i den NYA Rust/GTK4-`LinuxApp`**
  (2026-08-05, `LinuxApp/src/dashboard.rs`): fanns i Swift-sidan
  (`Sources/SSHCore/SystemProbe.swift` + `App/DashboardView.swift`) men
  aldrig porterad — genuint SAKNAD funktion (`grep`-sökning på
  "dashboard" i `LinuxApp/src/*.rs` gav noll träffar innan denna PR),
  till skillnad från cert-auth/terminalteman som var mer subtila luckor.
  - Samma agentlösa teknik som Swift-sidan: ETT sammansatt kommando
    (`echo @@SEKTION; kommando; …`) ger en hel ögonblicksbild (last,
    minne, disk, drifttid, OS, kärna, värdnamn, kärnantal, Docker) i EN
    round-trip — `@@`-markörer skiljer utdata åt, parsad med rena
    funktioner (sträng → struct), inte regex.
    `df -kP`/`/proc/loadavg`/`/proc/meminfo`/`/etc/os-release` läses
    rakt av, exakt samma källor som `SystemProbe.swift`.
    Docker-containrar sväljs tyst (`2>/dev/null`) om verktyget saknas —
    ingen felrad för det som förväntas kunna saknas.
    4 tester porterade rakt av från `Tests/SSHCoreTests/
    SystemProbeTests.swift` (samma fixtur, verklig Ubuntu-utdata).
  - Ny "Systemöversikt"-post i värdmenyn (host-åtgärder) — till skillnad
    från Docker/Snippets/SFTP/Tunnel/Nyckel har Swift-sidans
    `AppSettings.swift` INGEN `showDashboard`-togglingsbar inställning,
    så posten är alltid synlig, inte styrd av `FeatureToggles`. Öppnas
    som en egen flik (samma mönster som Docker-vyn) med en
    uppdateringsknapp.
  - **Kvar**: ingen auto-poll (Swift-sidans `DashboardModel.
    startPolling()`, 15 s intervall) än — bara manuell uppdatering via
    knappen. 156/156 `cargo test` gröna totalt, `cargo build`/`clippy`
    helt tysta.

- **Terminalfärgteman i den NYA Rust/GTK4-`LinuxApp`** (2026-08-05,
  `LinuxApp/src/terminal_theme.rs`): fanns i Swift-sidan
  (`App/TerminalTheme.swift`, 25 inbyggda teman) men Rust-terminalen
  (VTE4) körde bara vad GTK-temat råkade ge — inget eget färgval, inget
  motsvarande gap tidigare dokumenterat eftersom det inte är en "saknad
  funktion" i samma bemärkelse som t.ex. Telnet/S3, bara en aldrig
  ifylld detalj.
  - Alla 25 temans hex-värden avskrivna rad för rad från
    `TerminalTheme.swift` (Dracula, Nord, Solarized, Gruvbox, Monokai,
    One Dark, Tokyo Night ×2, Catppuccin ×4, Ayu ×2, Everforest, Rosé
    Pine, Kanagawa, Nightfox, Oxocarbon, m.fl.) — inte nya val.
  - `vte_terminal_set_colors` (bakgrund/förgrund/16-färgers ANSI-palett i
    ett anrop) + `set_color_cursor`/`set_color_highlight` separat, samma
    tre extra fält `TerminalView.swift` sätter utöver SwiftTerms egen
    motsvarighet till `set_colors`. Samma strikta "#RRGGBB"-parsning
    (svart som fallback vid fel längd/tecken) som Swift-sidans `HexRGB`.
  - **Till skillnad från** `settings::FeatureToggles`
    (`~/.bastion/settings.json`, delas/synkas mellan enheter): valt tema
    är en ren LOKAL preferens (`~/.bastion/linuxapp-terminal-theme.json`)
    — Swift-sidan sparar det i `UserDefaults`, aldrig i den synkade
    `AppSettings`-modellen, så en delad hemkatalog ska inte plötsligt
    synka ett rent UI-val mellan Mac och Linux.
  - Ny "Terminal"-preferensgrupp i inställningsdialogen (`adw::ComboRow`
    med alla 25 namn) — redan öppna flikar uppdateras direkt vid byte
    (`AdwTabView::pages()`, inget `nth_page` finns i libadwaita-API:t),
    nya sessioner skapas alltid via en delad `new_themed_terminal()`-
    hjälpfunktion som läser det sparade valet.
  - 5 nya tester (unika id:n, alfabetisk sortering, giltig 16-färgers
    palett per tema, hex-parsning inkl. svart-fallback, lagringens
    round-trip). 152/152 `cargo test` gröna totalt, `cargo build`/
    `clippy` helt tysta.

- **OpenSSH-certifikatautentisering i den NYA Rust/GTK4-`LinuxApp`**
  (2026-08-05, `LinuxApp/src/ssh.rs` + auth-väljare i `main.rs`):
  `HostAuth::CertificateFile` fanns redan som fält i datamodellen (wire-
  kompatibelt med Swift-sidans `certificateFile`) men föll i praktiken
  igenom till "autentiseringstypen stöds inte på Linux ännu" i
  `ssh.rs`s `authenticate`-matchning — precis samma gap-mönster som
  Tailscale/WireGuard/S3/Telnet/ssh-config-import, fast tidigare
  odokumenterat eftersom fältet redan fanns och SÅG portat ut.
  - russh har, till skillnad från swift-nio-ssh (vars SERVER-roll
    upptäcktes att INTE kunna ta emot cert-auth alls, se statusraden
    ovans notering — irrelevant för Bastion som alltid är klient, men
    det gjorde att Swift-sidans egna tester aldrig kunde bevisa en
    fullständig nätverksrundtur), FÖRSTKLASSIGT klientstöd:
    `Handle::authenticate_openssh_cert` + `russh::keys::
    load_openssh_certificate` — inget eget protokoll- eller
    ASN.1/binärformatarbete behövdes.
  - **Genuint bevisat mot en RIKTIG `sshd`** (`TrustedUserCAKeys`), inte
    bara en offline-verifiering av att erbjudandet byggs rätt — till
    skillnad från Swift-sidans engångsverifiering (som krävde en manuellt
    startad lokal `sshd` och aldrig byggdes in permanent, se statusraden
    ovan) är detta ett PERMANENT test i CI: tre riktiga
    `ssh-keygen -s`-signerade certifikat, tre utfall (giltigt cert mot
    betrodd CA + rätt principal → lyckas, fel principal → avvisas trots
    betrodd CA, obetrodd CA → avvisas trots giltig principal).
  - Ny auth-väljare i värdredigeringsdialogen (`show_host_dialog`) —
    fanns tidigare INGEN uttrycklig väg att sätta `KeyFile`/
    `CertificateFile` där alls (bara nyckeldistributionsflödet satte
    `KeyFile`, implicit); ett mindre, sidoupptäckt gap som stängdes
    samtidigt. Fyra lägen (SSH-agent/lösenord/nyckelfil/certifikat) med
    filväljare (`gtk::FileDialog`) för sökvägsfälten. `KeychainKey`/
    `BitwardenItem` (Apple-bara, synkade från en Apple-enhet) bevaras
    uttryckligen om värden redigeras utan att auth-läget ändras — samma
    opt-in-princip som nyckeldistributionens lösenordsborttagning.
  - 3 nya tester (`ssh::tests::certificate_auth_*`), 147/147 `cargo test`
    gröna totalt, `cargo build`/`clippy` helt tysta.

- **Importera `~/.ssh/config` i den NYA Rust/GTK4-`LinuxApp`** (2026-08-05,
  `LinuxApp/src/ssh_config.rs`): fanns redan i Swift/CLI-sidan
  (`Sources/SSHCore/SSHConfig.swift` + `HostStore.importSSHConfig`, se
  status­raden ovan) men aldrig porterad till Rust-om­skrivningen — samma
  gap-mönster som Tailscale/WireGuard/S3/Telnet.
  - Egen, beroendefri tokenizer/parser (ingen regex-motor) som stöder
    `Host`-block med jokertecken (`*`, `?`) och negation (`!`), `Key Value`/
    `Key=Value`/`Key = Value`-syntax, citerade värden och `#`-kommentarer.
    `Match`-block hoppas medvetet över (matchar aldrig) — exakt samma
    avgränsning som Swift-sidan gör.
  - Semantik enligt riktig OpenSSH, inte en förenkling: **första värdet
    vinner** per nyckel (senare `Host`-block kan inte skriva över ett
    tidigare matchat block), nycklar FÖRE första `Host`-raden gäller
    globalt, och `!mönster` exkluderar en värd även om ett senare
    positivt mönster (t.ex. en catch-all `*`) annars skulle matcha den.
    Jokerteckenmatchningen (`glob()`) är samma iterativa två-pekar-
    algoritm (med backtrack-markör för `*`) som Swift-sidan använder,
    inte en naiv `str::contains`-genväg.
  - `HostStore::import_ssh_config` importerar varje KONKRET alias (inga
    jokertecken/negationer, de kan inte bli en enskild host-post) som
    saknar `User` hoppas över (kan inte anslutas ändå utan användarnamn).
    Dedup vid ominmport: matchar på alias, importerar inte samma värd två
    gånger om man klistrar in samma config igen.
  - Ny "Importera"-knapp i sidopanelens header: en klistra-in-text-dialog
    (ingen filväljare — configen kan komma från vilken maskin som helst,
    inte nödvändigtvis en lokal `~/.ssh/config`), motsvarar
    `App/ImportConfigView.swift`. Visar antal importerade värdar efteråt.
  - 12 enhetstester porterade rakt av från `Tests/SSHCoreTests/
    SSHConfigTests.swift` + `ImportAndExecTests.swift` (exakt alias,
    andra mönstret på samma `Host`-rad, jokerteckensuffix/-prefix,
    `ProxyJump` via jokertecken, "första vinner", negation, okänt alias
    träffar catch-all, `=`-syntax, jokertecken-glob-tester separat,
    alias hoppar över jokertecken, import hoppar över anvädar­lösa/
    jokertecken-poster, dedup vid ominmport mot en riktig `HostStore` på
    disk). 144/144 `cargo test` gröna totalt, `cargo build`/`clippy`
    helt tysta.
  - **Kvar**: `Match`-block (t.ex. `Match host` / `Match exec`) stöds
    inte — samma medvetna avgränsning som Swift-sidan.

- **Seriell/USB-anslutning i den NYA Rust/GTK4-`LinuxApp`** (2026-08-05,
  `LinuxApp/src/serial.rs`): helt saknades — Termius har detta, Bastion
  saknade det helt (samma gap-lista Swift-sidans `Serial.swift`
  dokumenterar). Port av `Sources/SSHCore/Serial.swift`: rått termios-läge
  (`cfmakeraw`) mot t.ex. `/dev/ttyUSB*`/`/dev/ttyACM*` — mest relevant för
  hemmalabb-/nätverksutrustnings-konsolportar (samma "nätverkstekniker"-
  persona som Telnet).
  - Till skillnad från `ssh.rs`/`telnet.rs` (en bakgrundstråd,
    `tokio::select!`) används HÄR två separata trådar UTAN tokio alls:
    en seriell filbeskrivare är en vanlig blockerande syscall-baserad
    I/O-primitiv (`std::fs::File::read`/`write` fungerar direkt när
    porten är konfigurerad) — en dedikerad blockerande läs-tråd är
    enklare OCH effektivare än Swift-sidans `NIOPipeBootstrap`-väg (som
    behöver icke-blockerande I/O för att passa NIOs event-loop-modell,
    ett behov som inte finns här). Ren `libc`-FFI (`open`/`tcgetattr`/
    `cfmakeraw`/`cfsetispeed`/`cfsetospeed`/`tcsetattr`/`dup`) — samma
    raka syscall-nivå som Swift-sidan, inget mellanliggande
    `serialport`-kratbibliotek.
  - En delad `Arc<AtomicBool>`-flagga signalerar läs-tråden att avsluta
    när skriv-tråden märker att `input`-kanalen stängts (fliken stängd)
    — att stänga filbeskrivaren direkt från en ANNAN tråd medan en
    blockerande `read()` pågår på samma fd-nummer är odefinierat
    beteende i praktiken (fd-återanvändningsrisk), så cancellering görs
    kooperativt istället.
  - Ny "Seriell/USB"-knapp i sidopanelens header: ad-hoc
    anslutningsdialog (sökväg + baudhastighet, ingen auth — en fysisk
    port är inte en användarkontobar resurs, samma resonemang som
    Telnet/Quick Connect), motsvarar `App/SerialConnectView.swift`.
    `serial::available_paths()` föreslår kandidater
    (`/dev/ttyUSB*`/`/dev/ttyACM*`/`/dev/ttyS*`) i en väljare, men ett
    fritextfält tar alltid över om ifyllt.
  - **Testat GENUINT mot en riktig pseudo-terminal (PTY)**, inte en mock
    — `posix_openpt`/`grantpt`/`unlockpt`/`ptsname` (samma teknik som
    Swift-sidans `SerialPTYTests`, där begränsad till Darwin av ett
    lokalt toolchain-skäl, inte för att Linux saknar syscallen): en
    PTY-slav beter sig som en äkta seriell tty ur `open()`/
    `tcsetattr()`s perspektiv, så testet bevisar att hela vägen —
    öppna, konfigurera rått läge, läsa OCH skriva — fungerar mot en
    riktig enhet. **En genuin race hittades och fixades under
    verifieringen** (inte antagen i förväg): skrev testet till PTY-
    mastern INNAN bakgrundstrådens `configure_termios` hunnit köra,
    landade data i kärnans DEFAULT cooked-läge (eko/radbuffring
    påslagna) istället för det avsedda rå läget — synligt som en extra
    `\r` inskjuten framför `\n`. Fixat genom att testet väntar in
    `SerialEvent::Connected` innan något skrivs, körd 5 gånger i rad
    utan flakighet efteråt. Plus 3 enhetstester (icke-existerande
    sökväg, ogiltig/alla giltiga baudhastigheter). Port av
    `SerialTests.swift`/`SerialPTYTests.swift` rakt av. 132/132
    `cargo test` gröna totalt, `cargo build`/`clippy` helt tysta.
  - **Kvar**: Windows har en helt annan seriell-API (`SetCommState`, inte
    POSIX-termios) — medvetet inte täckt, samma avgränsning som Swift-
    sidan.

- **Snabbanslutning ("Quick Connect") i den NYA Rust/GTK4-`LinuxApp`**
  (2026-08-04, `LinuxApp/src/main.rs`): helt saknades — ingen väg att
  ansluta till en ad-hoc värd UTAN att först spara den i värdlistan.
  Motsvarar `App/QuickConnectView.swift` (Termius kallar detta "Quick
  Connect"). Ny knapp i sidopanelens header (bredvid "Lägg till värd"):
  värd/användare/port/lösenord-fält, bygger en `host::Host` bara i
  minnet (aldrig skickad till `HostStore::upsert`) och öppnar den direkt
  via `start_session` — samma terminalsession som en sparad värd får,
  bara utan att fylla värdlistan med en engångsanslutning.
  - Går direkt till `start_session`, inte via `open_session`s omväg
    genom `with_resolved_jump`/`prompt_password_then` — en ad-hoc-värd
    har per definition ingen `jump_host_id`, och lösenordet (om något)
    matas in i SAMMA formulär som resten av fälten, inte en separat
    efterföljande dialog (skiljer sig medvetet från hur en SPARAD
    `AskPassword`-värd hanteras, där lösenordet aldrig lagras och måste
    frågas efter varje gång).
  - Tomt lösenordsfält → `HostAuth::AgentDefault` (prova ssh-agent/
    standardnyckel); ifyllt → `HostAuth::AskPassword` med lösenordet
    skickat OBESKURET (trimning hade tyst korrumperat ett giltigt
    lösenord med inlednings-/avslutande blanktecken — samma cubic-fynd,
    PR #173, som Swift-sidan redan vaktar mot).
  - Ingen ny testbar logik (ren GTK-kablage ovanpå redan testad
    `host::Host`/`start_session`) — 128/128 `cargo test` oförändrat
    gröna, `cargo build`/`clippy` helt tysta.

- **S3 bucket-/objektbläddare i UI:t** (2026-08-04, `LinuxApp/src/main.rs`):
  slutförde "Kvar"-punkten från S3-kompatibel objektlagring nedan (samma
  dag) — resten av `S3Client`s redan portade+testade metoder
  (`create_bucket`/`delete_bucket`/`list_objects`/`put_object`/
  `get_object`/`delete_object`) kopplades in i en riktig bläddare, nådd
  via en ny "Bläddra"-knapp per rad i S3-anslutningslistan.
  - En flik per anslutning (samma "en flik per session/vy"-mönster som
    Docker/SFTP/Tunnel). `Rc<RefCell<Option<String>>>` håller "aktuell
    bucket" — `None` visar bucket-listan (rot), `Some(namn)` visar
    objekten i den bucket:en. Bara EN nivå djup — S3:s `Key` är platt
    (en nyckel som `mapp/fil.txt` är bara en vanlig nyckel, ingen riktig
    katalog), inget `prefix`-baserat undermapps-UI i v1.
  - Ny bucket/uppladdning/nedladdning använder `gtk::FileDialog`
    (`open_future`/`save_future`, GTK4:s moderna async-filväljare) —
    uppladdning läser den valda lokala filen och skickar den som ett
    objekt med samma filnamn som nyckel; nedladdning föreslår objektets
    nyckel som standardfilnamn.
  - `s3.rs` fick en generisk `spawn`-hjälpare (`FnOnce(S3Client) ->
    impl Future`) som ALLA `spawn_list_buckets`/`spawn_create_bucket`/
    `spawn_delete_bucket`/`spawn_list_objects`/`spawn_put_object`/
    `spawn_get_object`/`spawn_delete_object` bygger på — samma "egen
    bakgrundstråd med egen tokio-runtime, `reqwest` har ingen egen
    reaktor på GTK:s huvudloop"-mönster som resten av appen, men utan
    att duplicera tråd-uppstartskoden sju gånger. `spawn_test_connection`
    (redan klar) skrevs om att använda samma hjälpare.
  - Byggd och testad — inga nya varningar (`cargo build`/`cargo clippy`
    helt tysta för `s3.rs`, alla sju `S3Client`-metoder samt `S3Object`/
    `S3ConnectionStore::get` nu genuint använda, inga `#[allow(dead_code)]`
    kvar att motivera). 128/128 `cargo test` fortsatt gröna (ingen ny
    testkod i det här passet — bläddar-UI:t är GTK-kablage ovanpå redan
    testad backend, inte ny logik att testa isolerat).
  - **Kvar**: interaktiv klick-genom-bläddaren mot en riktig S3-tjänst
    (Xvfb-byggd+körd, inte klick-för-klick manuellt testad) — samma
    begränsning som redan dokumenterad för portvidarebefordran/SOCKS-UI:t.

- **S3-kompatibel objektlagring i den NYA Rust/GTK4-`LinuxApp`**
  (2026-08-04, `LinuxApp/src/s3.rs`): helt saknades i den nya
  Rust-omskrivningen. Byte-för-byte-port av `Sources/SSHCore/S3Client.swift`
  + `S3ConnectionStore.swift` — AWS Signature Version 4-signering, fungerar
  mot riktig AWS S3 OCH S3-kompatibla leverantörer (Ceph RGW m.fl.) eftersom
  SigV4 är en delad spec, inte en AWS-hemlighet.
  - **Signeringen verifierad mot EXAKT samma fixerade testvektor som
    Swift-sidan** (`sigv4_matches_verified_reference_vector`, härledd ur en
    oberoende Python-referensimplementation som fick ett genuint 200 OK mot
    Hostups riktiga S3-kompatibla tjänst) — samma indata gav samma
    `Authorization`-header byte för byte. Algoritmen är alltså bevisat
    korrekt portad, inte bara "ser rimlig ut".
  - HTTP via `reqwest` (`rustls-tls`, inget systemberoende OpenSSL), XML-
    svarstolkning via `quick-xml` (pull-baserad, samma SAX-liknande
    "spåra aktuell tagg + en `in_target`-flagga"-mönster som Swift-sidans
    `XMLParser`-delegat). Path-style URL:er (`https://endpoint/bucket/key`),
    inte virtual-hosted — samma val som Swift-sidan.
  - En redan percent-encodad sökväg/frågesträng (SigV4:s exakta
    encodningsregler) byggs ihop till URL:en som en FÄRDIG sträng och
    parsas med `Url::parse`, ALDRIG via `Url::set_path`/`set_query` — de
    mutatorerna hade percent-encodat en gång TILL (t.ex. `%20` →
    `%2520`), vilket bryter signaturen. Motsvarar varför Swift-sidan sätter
    `URLComponents.percentEncodedPath` direkt istället för `.path`.
  - `S3Client`: `list_buckets`/`create_bucket`/`delete_bucket`/
    `list_objects`/`put_object`/`get_object`/`delete_object` — alla
    portade och genuint testade (se nedan), men bara `list_buckets` (via
    en "Testa anslutning"-knapp) är kopplad till UI:t hittills.
  - `S3ConnectionStore`: persistent JSON-databas för sparade anslutningar
    (namn/endpoint/region/nycklar — nycklarna sparas i klartext, samma
    medvetna v1-avgränsning som `WireGuardProfile`s privatnycklar, ingen
    Keychain-motsvarighet på Linux än), samma CRUD-mönster som
    `WireGuardProfileStore`. Ny "S3-anslutningar"-knapp i sidopanelens
    header: lista + redigeringsdialog med en "Testa anslutning"-knapp som
    kör ett riktigt `list_buckets`-anrop mot de fält som STÅR i formuläret
    just nu (inte den senast sparade versionen — samma resonemang som
    `key_deploy`s "verifiera innan lösenordet tas bort").
  - Testat: den låsta SigV4-vektorn, XML-tolkning av tre riktiga
    svarsformat (fångade från Hostups tjänst, samma fixturer som Swift-
    sidan), Host-header-kantfall (port inkluderad/utelämnad beroende på
    schema/standardport — samma CodeRabbit-fynd, PR #90, som Swift-sidan
    redan fångat) + TVÅ genuina end-to-end-tester mot en riktig, minimal
    HTTP/1.1-server (rå TCP, inget ramverk): bevisar att HELA vägen —
    URL-byggande, signering, riktiga `reqwest`-anrop, XML-tolkning av
    svaret — fungerar ihop, inte bara `sign`/`parse_*` isolerat (en
    verifierar att `list_buckets` skickar en korrekt signerad begäran och
    tolkar svaret rätt, en att `put_object` skickar kropp OCH
    `Content-Type`-headern). Port av `S3ClientTests.swift`/
    `S3ConnectionStoreTests.swift` rakt av (utom det Hostup-specifika
    live-testet, som kräver riktiga kontouppgifter i miljön — samma skäl
    Swift-sidans motsvarighet hoppas över i CI). 128/128 `cargo test`
    gröna totalt.
  - **Kvar (ursprunglig text, historik)**: "en riktig bucket-/
    objektbläddare i UI:t — backend är redo och testad, bara ytlagret
    saknas, samma 'backend klart, UI en avgränsad uppföljning'-mönster
    som synkmotorn hade innan dess UI kom på plats." — ✅ KLART SAMMA DAG,
    se "S3 bucket-/objektbläddare i UI:t" ovan.

- **WireGuard-profiler i den NYA Rust/GTK4-`LinuxApp`** (2026-08-04,
  `LinuxApp/src/wireguard.rs`): helt saknades i den nya Rust-omskrivningen.
  Byte-för-byte-port av `Sources/SSHCore/WireGuardConfig.swift` +
  `WireGuardProfileStore.swift` — samma v1-avgränsning som Swift-sidan:
  parsning/lagring/redigering av `.conf`-profiler, INTE att faktiskt
  upprätta tunneln (kräver `wg`-binären + root, eller ett helt eget
  kryptoprotokoll utan den — separat, mycket större arbete, se
  "Native WireGuard/Tailscale — inget externt beroende" nedan).
  - Parsern hanterar precis det `wg(8)`/`wg-quick(8)`-formatet Swift-sidan
    redan verifierat: `[Interface]`/`[Peer]`-sektioner (skiftlägesokänsliga
    rubriker), `#`-kommentarer (även en rad som BÖRJAR med `#` — samma
    CodeRabbit-fynd, PR #79, som Swift-sidan redan vaktar mot), upprepade
    `Address`/`AllowedIPs`-rader som ackumuleras istället för att skriva
    över, flera `[Peer]`-sektioner.
  - `WireGuardProfileStore` — persistent JSON-databas,
    `~/.bastion/wireguard.json`, samma CRUD-mönster som `SnippetStore`
    (`camelCase`-fältnamn för framtida cross-platform-kompatibilitet,
    trunkerad/skadad fil ger ett fel istället för att tyst tömmas).
  - Ny "WireGuard"-knapp i sidopanelens header: en profillista (namn +
    sammanfattning: första adressen + antal peers), "+"/radklick öppnar en
    redigeringsdialog som visar/redigerar profilen som RÅ `.conf`-text
    (enklare och mer direkt begripligt för en användare som redan har
    filen, än ett fält-för-fält-formulär — samma designval som
    `App/WireGuardProfileView.swift`; `WireGuardConfig::parse` är
    förlåtande, så ogiltig text ger bara en tom/ofullständig profil, inte
    en krasch).
  - Testat: 12 parser-/serialiseringstester (samma fixturer/scenarier som
    `WireGuardConfigTests.swift` — round-trip genom `rendered()`, tomma
    valfria fält, kommentarhantering, ackumulerande adressrader) + 2
    lagringstester (upsert/get/delete/sortering, persistens mellan
    store-instanser, en FULL round-trip text→config→profil→JSON-på-disk→
    ny store→tillbaka till `.conf`-text) + en korrupt-fil-regression. Port
    av `WireGuardConfigTests.swift`/`WireGuardProfileStoreTests.swift` rakt
    av. 115/115 `cargo test` gröna totalt.
  - **Kvar**: fil-import (`.fileImporter` i Swift-sidan) — LinuxApp har
    bara klistra-in-textfältet, ingen egen filväljarknapp än. Profilerna
    är inte kopplade till någon specifik `Host` i UI:t (matchar Swift-
    sidan — en profil beskriver en VPN-anslutning, inte en SSH-värd).

- **Tailscale-värdförslag i den NYA Rust/GTK4-`LinuxApp`** (2026-08-04,
  `LinuxApp/src/tailscale.rs`): helt saknades i den nya Rust-omskrivningen
  (bara ett par statiska referensrader i Command Library — inte en riktig
  integration). Byte-för-byte-port av `Sources/SSHCore/TailscaleStatus.swift`
  (samma JSON-fältnamn, samma `suggestedHosts`-filtrering: bara online-
  peers med minst en Tailscale-IP, `DNSName`/MagicDNS föredraget framför
  `HostName` när det finns). Två källor, precis som Swift-sidan:
  - `tailscale::fetch_local` — kör `tailscale status --json` LOKALT
    (`std::process::Command`, ingen sandbox-begränsning på skrivbordet,
    till skillnad från iOS där `Foundation.Process` saknas helt).
    `tailscale::spawn_fetch_local` kör den på en egen bakgrundstråd — ett
    rakt anrop hade blockerat GTK:s huvudloop under hela processkörningen,
    samma resonemang som Swift-sidans `Task.detached`-kommentar i
    `TailscaleDiscoveryModel.fetch`.
  - `tailscale::fetch_remote` — kör samma kommando över en redan
    konfigurerad fjärrvärd via `ssh::run_command`, alltså AUTOMATISKT
    jump-host-medveten (samma anslutningsväg som allt annat
    engångskommando i appen, ingen egen kod behövdes för det).
  - Ny "Tailscale"-knapp i sidopanelens header, öppnar en dialog: välj
    källa (denna enhet eller en sparad värd) → "Hämta" → lista med
    föreslagna `värdnamn:adress`-rader, var och en med en "Lägg till"-
    knapp som öppnar den vanliga host-dialogen förifylld (alias=värdnamn,
    IP=adress) — Tailscale känner inte till SSH-användarnamnet, så det
    sista steget är alltid det vanliga redigeringsläget, samma designval
    som `App/TailscaleDiscoveryView.swift`.
  - Testat: 5 enhetstester (parsning av en RIKTIG `tailscale status
    --json`-fixtur — samma fångade utskrift som Swift-sidans
    `TailscaleStatusTests`, filtrering av offline/IP-lösa peers) + 2
    genuina process-tester (`fetch_local` mot ett riktigt, kortlivat
    `/bin/sh`-skript — inte mockat — både lyckad utskrift och en
    icke-noll-exitkod med stderr). Port av `TailscaleStatusTests.swift`
    rakt av. 101/101 `cargo test` gröna totalt.
  - **Kvar**: `fetch(over:)`-motsvarigheten (köra kommandot över en
    ANNAN, redan öppen SSH-session — t.ex. dashboard-datan) fanns bara
    som en optimering i Swift-sidan; LinuxApp öppnar en ny engångs-
    anslutning varje gång istället (`fetch_remote`), samma avvägning som
    resten av appens Docker-/kommandokörningar redan gör.

- **Telnet (RFC 854) i den NYA Rust/GTK4-`LinuxApp`** (2026-08-04,
  `LinuxApp/src/telnet.rs`): helt saknades i den nya Rust-omskrivningen —
  Telnet delar ingen kod med SSH-lagret (rå TCP, ingen kryptering, egen
  IAC-förhandling), så det fanns inget att av misstag hoppa över när
  `ssh.rs` porterades, bara ett helt eget spår som aldrig byggdes.
  Byte-för-byte-port av `Sources/SSHCore/Telnet.swift`s `TelnetIACFilter`-
  statmaskin (samma refusera-allt-strategi: `WILL`→`DONT`, `DO`→`WONT`,
  subförhandlingar kastas bort helt — enklast möjliga korrekta
  klientbeteende, servern faller tillbaka till rått NVT-läge). `telnet::
  spawn` körs på en egen bakgrundstråd med egen tokio-runtime, exakt
  samma `async_channel`-mönster som `ssh::spawn_shell` — och
  `start_telnet_session` (`main.rs`) återanvänder t.o.m. SAMMA
  `terminal.set_data("bastion-ssh-input", …)`-nyckel som SSH-sessioner,
  så den redan existerande generiska close-page-städningen
  (`SessionArea::new`) fungerar identiskt utan telnet-specifik kod där.
  - Ny "Telnet"-knapp i sidopanelens header (bredvid "Lägg till värd"),
    öppnar en AD HOC-anslutningsdialog (bara värd+port, INGEN auth —
    motsvarar `App/TelnetConnectView.swift`: Telnet-inloggning, om
    servern ens kräver någon, sker inuti själva terminalsessionen via en
    login-prompt, inte som ett separat handskakningssteg). Ingen sparning
    i värdlistan, medvetet — samma designval som Swift-sidan.
  - Testat: 8 enhetstester för IAC-statmaskinen (ren dataväg, escaped
    `0xFF`, WILL/DO-refusering, WONT/DONT kräver inget svar,
    subförhandling kastas bort, kommandon utan optionsbyte, fragmenterad
    förhandling över flera läsningar) + 1 genuint end-to-end-test mot en
    RIKTIG TCP-server som beter sig som en telnet-server (skickar en
    förhandling klienten ska refusera, läser klientens svar direkt av
    tråden — bevisar att hela vägen, inte bara filtrets logik i
    isolering, fungerar). Port av `TelnetTests.swift` rakt av. 87/87
    `cargo test` gröna totalt.

- **Wake-on-LAN i den NYA Rust/GTK4-`LinuxApp`** (2026-08-04,
  `LinuxApp/src/wake_on_lan.rs`): `Host.mac_address` fanns redan i
  datamodellen (för wire-format-kompatibilitet med App/Windows-synk) men
  lästes tidigare aldrig av något i den nya Rust-omskrivningen — samma
  mönster som `jump_host_id` hade innan ProxyJump-fixen nedan (den gamla
  SwiftCrossUI-`bastion-gui` hade redan Wake-on-LAN, se commit `06476d6`,
  men den är riven sedan arkitekturpivoten 2026-08-03 och funktionen
  porterades aldrig till den nya Rust-koden). Byte-för-byte-port av
  `Sources/SSHCore/WakeOnLan.swift` (samma magic-packet-format, samma
  felkontrakt — ogiltig MAC/port ger ett tydligt fel istället för att
  tyst tolkas fel eller skickas till fel port). `wake_on_lan::send` körs
  på en egen bakgrundstråd med egen tokio-runtime
  (`wake_on_lan::spawn_send`), samma mönster som `ssh::spawn_shell`/
  `run_command` — `glib::spawn_future_local` (GTK:s huvudloop) har ingen
  egen tokio-reaktor, ett rakt `.await` på `tokio::net::UdpSocket` hade
  panikat.
  - MAC-adress-fält i värdredigeringsdialogen (`show_host_dialog`),
    validerat VID SPARA (inte bara när "Väck" trycks) — en trasig adress
    sparad tyst skulle göra Wake-knappen deterministiskt trasig senare,
    samma resonemang som `App/HostEditView.swift`s `macValidationMessage`.
  - Ny "Väck (Wake-on-LAN)"-post i per-värd-menyn, synlig ENDAST när
    `host.mac_address` är satt (per-värd-egenskap, till skillnad från de
    andra menyposterna som styrs av globala `FeatureToggles`) — samma UX-
    regel som `App/HostListView.swift` (`if host.macAddress != nil`).
    Resultatet (lyckat/fel) visas i en modal dialog
    (`show_message_dialog`, en generalisering av den redan existerande
    `show_connect_error` från ProxyJump-arbetet).
  - Testat: 6 enhetstester (MAC-parsning i alla tre separatorformat, fel
    längd/tecken, magic-packet-byte-layout, port-gränsvärden) + 1 genuint
    end-to-end-test mot en RIKTIG UDP-lyssnare på loopback (bevisar att
    paketet faktiskt går ut på tråden med rätt innehåll, inte bara att
    byte-layouten stämmer i minnet) — port av `WakeOnLanTests.swift` rakt
    av. 87/87 `cargo test` gröna totalt.

- **ProxyJump (`ssh -J`) i den NYA Rust/GTK4-`LinuxApp`** (2026-08-04,
  `LinuxApp/src/ssh.rs`/`host.rs`): `Host.jump_host_id` fanns redan (för
  wire-format-kompatibilitet med Swift/Windows synk) men lästes tidigare
  aldrig av något i den nya Rust-omskrivningen — en synkad värd med en
  jump-host konfigurerad från App/(iOS/macOS) skulle tyst försöka ansluta
  DIREKT mot target och misslyckas (eller, värre, om target råkade vara
  nåbar direkt också: ansluta utan att gå genom den avsedda jump-hosten).
  - `HostStore::resolve_jump` (`host.rs`): löser upp `jump_host_id` mot en
    riktig `Host` — motsvarar `App/AuthResolver.resolveConnectionPlan`s
    kontrakt exakt. Bara ETT hopp stöds; en jump-host som SJÄLV har en
    `jump_host_id` avvisas explicit (`Err`, aldrig en tyst
    direktanslutning som hoppar över resten av kedjan) — samma
    säkerhetsprincip som Swift-sidans `jumpHost.jumpHostID == nil`-vakt.
    En jump-host-referens som pekar på ett ID som inte finns i databasen
    (borttagen, eller inte synkad än) avvisas likaså.
  - `ssh::connect_via_jump` (`ssh.rs`): ansluter+autentiserar mot
    jump-hosten (russh `connect_direct`, UTBRUTEN ur `connect_with_forwards`
    specifikt för att undvika en rekursiv-async-fn-cykel — E0733 — mellan
    `connect`/`connect_with_forwards`/`connect_via_jump`), öppnar en äkta
    `direct-tcpip`-kanal genom den (`channel_open_direct_tcpip`) mot
    target, och kör en HELT NY, oberoende SSH-handskakning
    (`client::connect_stream` på kanalens `ChannelStream`, targets EGEN
    TOFU-koll) ovanpå den byteströmmen — samma "SSH i SSH"-mönster som
    `SSHSession.connect(via:)` i SSHCore. Jump-hostens `Handle` behöver
    inte hållas vid liv explicit efter att kanalen öppnats — `Channel`
    håller sin egen klon av sessionens sändare (russh-dokumenterat), så
    russh:s bakgrundstråd för jump-anslutningen fortsätter servera
    tunneln även efter att `Handle`n gått ur scope.
    Jump-hosten autentiseras UTAN lösenordsprompt (samma begränsning som
    Swift-sidans `resolveAuth(for: jumpHost, password: nil)`) — bara
    nyckel-/agent-auth stöds för själva hoppet, target kan fortfarande
    fråga efter lösenord som vanligt.
  - `jump: Option<Host>` trädd genom HELA anropskedjan från `main.rs`s
    UI-lager (terminal, Docker, SFTP, portvidarebefordran/SOCKS,
    nyckeldistribution, kommandobibliotek/snippets — alla vägar som kan
    öppna en SSH-anslutning) ner till `ssh::connect`/`run_command`/
    `spawn_shell`, `port_forward::spawn_local_forward`/
    `spawn_remote_forward`, `socks_proxy::spawn_dynamic_forward`,
    `sftp::spawn`, `key_deploy::spawn_deploy_and_verify`. En trasig
    jump-host-konfiguration upptäcks INNAN någon anslutning ens försöks
    (`with_resolved_jump` i `main.rs`) och visas i en modal
    `adw::AlertDialog` — ansluter aldrig direkt mot target som tyst
    fallback.
  - Testat GENUINT end-to-end (`ssh::tests`, INGEN kortsluten
    loopback-gissning): TVÅ helt oberoende `TestSshd`-instanser (egna
    portar, värdnycklar, klientnyckelpar) — en lyckad anslutning kör ett
    riktigt kommando över den tunnlade sessionen och verifierar utdata;
    ett separat test bevisar att ett fel i JUMP-hostens autentisering
    pekar tydligt på jump-hosten; ett tredje bevisar att ett fel i
    TARGET-hostens autentisering (jump redan bevisat fungera) INTE
    felaktigt tillskrivs jump-hosten — samma distinktion som Swift-sidans
    `ProxyJumpTests`. Plus 4 nya `HostStore::resolve_jump`-tester
    (host.rs). 80/80 `cargo test` gröna totalt (73 innan, +4 resolve_jump
    +3 connect_via_jump).
  - **Kvar**: `WindowsApp`/`Android` har ingen egen SSH-implementation än
    (se "Fas D") så frågan är inte aktuell där. LinuxApps egen
    värdredigeringsdialog (`show_host_dialog` i `main.rs`) har INGEN
    UI-väljare för `jump_host_id` alls — bara körningsvägen (`ssh.rs`/
    `host.rs`) stöder det nu. En jump-host måste idag sättas från App/
    (Xcode-only) och synkas in för att LinuxApp ska kunna använda den;
    en egen "Anslut via"-väljare i LinuxApps host-dialog (samma som
    App/HostEditView.swift redan har) är en mindre, avgränsad
    uppföljning.

- **ECDSA-nyckelautentisering (P256/P384/P521), okrypterad** (2026-08-03,
  `SSHKeyParser.swift`/`SSHUserAuth.swift`): OpenSSH-privatnyckelparsern läser
  nu `ecdsa-sha2-nistp256/384/521` utöver Ed25519 — läser den råa privata
  skalären (SSH `mpint`, normaliserad till kurvans exakta byte-längd) och
  bygger en `NIOSSHPrivateKey` via `P256`/`P384`/`P521.Signing.PrivateKey
  (rawRepresentation:)`. Bekräftat via `NIOSSHPrivateKey`s källa att RSA INTE
  stöds alls av swift-nio-ssh på klientsidan (bara Ed25519 + dessa tre
  kurvor) — RSA är alltså en separat, egen lucka (se "Uppskjutet med
  avsikt"), inte samma sak som "krypterad nyckel". Testat mot RIKTIGA
  `ssh-keygen -t ecdsa -b 256/384/521`-nycklar (inte handskrivna fixtures):
  parsning + end-to-end-autentisering mot `LoopbackServer` för alla tre
  kurvorna, 7/7 gröna. Lösenfrasskyddade nycklar (alla typer, inklusive
  ECDSA) stöds fortfarande inte — separat KDF/cipher-lucka, oförändrad.
- **`ExternalBinaryFetcher` — generisk hämta+verifiera+cacha-hjälpare för
  externa binärer** (2026-08-03, `Sources/SSHCore/ExternalBinaryFetcher.swift`):
  första byggstenen för "Native WireGuard/Tailscale — inget externt beroende"
  (se det avsnittet nedan för den fulla motiveringen). Tar en URL + en känd
  SHA256-checksumma, laddar ner, verifierar checksumman mot bytesen INNAN
  något skrivs till disk (en fel/manipulerad nedladdning hamnar aldrig i
  cachen ens tillfälligt), cachar under en given katalog med `chmod 755`,
  och är idempotent (ett andra anrop med samma parametrar gör INGEN
  nätverkstrafik, verifierat explicit i testerna genom att peka det andra
  anropet mot en overkomlig URL som skulle kastat om cachen missades). En
  korrupt cachad fil (fel checksumma) tas bort och laddas ner på nytt
  istället för att litas på tyst.
  Medvetet WireGuard/Tailscale-AGNOSTISK (bara URL+checksumma+katalog in) —
  matchar roadmap-anteckningens egen slutsats att en sådan hjälpare "är värd
  att bygga ÅTERANVÄNDBAR för både WireGuard och Tailscale". Hör hemma i
  `SSHCore` eftersom mekaniken (nedladdning/SHA256/filcache) är genuint
  plattformsneutral — det är bara det FRAMTIDA valet av URL per plattform/
  arkitektur och själva tunneluppsättningen (wg-quick-motsvarighet,
  NetworkExtension på iOS) som hör hemma i UI-lagren, oförändrat.
  Testat mot en RIKTIG, taggpinnad (alltså oföränderlig) fil på
  `raw.githubusercontent.com` — inte ett mockat HTTP-svar: genuin nedladdning
  + checksumverifiering, cache-träff utan nätverksanrop (bevisat genom att
  peka på en overkomlig URL), avvisning av fel checksumma (aldrig skriven
  till disk), och återhämtning från en korrupt cache-post. 4/4 nya tester
  gröna (`ExternalBinaryFetcherTests.swift`), 290/290 totalt. Nätverksberoende
  — hoppar tydligt över (inte fel) om miljön saknar internetåtkomst.
- **ProxyJump (`ssh -J`)** (2026-07-06, `SSHSession.swift`): `connect(via
  jump:)` — istället för en ny TCP-anslutning öppnas en `direct-tcpip`-kanal
  FRÅN en redan uppkopplad jump-session till målet, och en helt egen,
  oberoende SSH-handskakning (eget `NIOSSHHandler`/`SSHUserAuth`/TOFU) körs
  direkt ovanpå den kanalen — "SSH i SSH", samma mönster som en riktig
  `ssh -J` på trådnivå.
  - `bastion-cli` läser `ProxyJump` ur `~/.ssh/config` automatiskt (fältet
    parsades redan sedan tidigare, `ResolvedHost.proxyJump`, men var aldrig
    kopplat till en riktig anslutning förut). Jump-hoppet återanvänder samma
    autentisering (miljövariabler/nyckelfråga) som huvudmålet
    (v1-förenkling).
  - **`-J <[user@]jumphost[:port]>`-kommandoradsflagg tillagt** (2026-08-03,
    `Sources/bastion-cli/main.swift`) — vinner över `ProxyJump` ur
    ssh-config om båda är satta, samma prioritetsordning som riktig
    `ssh -J`. Återanvänder exakt samma `resolveDestination()`/
    `connect(via:)`-väg som redan är bevisad end-to-end i
    `ProxyJumpTests.swift` — det nya är bara flaggparsningen. Verifierat
    genuint end-to-end mot TVÅ RIKTIGA lokala `sshd`-instanser (inte
    `LoopbackServer`): `bastion-cli -J` hoppade genom den ena och körde
    kommandot bevisligen på den ANDRA (separat `sshd`-process, egen
    värdnyckel). Fel målnyckel gav ett rent, snabbt auth-fel (exit 2) —
    inte den kända Windows-hänget (se ovan) — och saknade `-J`-argument gav
    ett tydligt användningsfel.
  - **Viktig arkitekturbegränsning, dokumenterad i kod**: en session öppnad
    via `connect(via:)` lever på JUMP-sessionens event loop-grupp, inte sin
    egen — måste därför stängas INNAN jump-sessionen stängs. Upptäckt
    empiriskt under testutveckling: fel ordning (stänga jump först) hängde
    hela testprocessen (`ERROR: Cannot schedule tasks on an EventLoop that
    has already shut down`), inte bara ett teoretiskt påstående.
  - 4 tester, inklusive en RIKTIG (inte ekande) test-jump-server som öppnar
    en genuin utgående anslutning till en separat, oberoende målserver
    (`makeRealDirectTCPIPForwarder` i `LoopbackServer.swift`) — bevisar att
    kedjningen faktiskt når ett verkligt, fristående SSH-mål, inte bara
    tunnlar rå bytes. Täcker: lyckad kedjning, fel lösenord för MÅLET
    (genom tunneln), oansluten jump kastar direkt, korrekt stängningsordning
    hänger inte.

- **Nyckelgenerering + export + fjärr-deploy till authorized_keys** (2026-07-06,
  `KeyManagement.swift`/`SSHKeyParser.swift`): kärnan för ett fullständigt
  "generera-nyckel-och-byt-bort-lösenord"-flöde.
  - `KeyGenerator.generateEd25519(comment:)` — ett helt nytt, slumpmässigt
    Ed25519-nyckelpar (`Curve25519.Signing.PrivateKey()`, samma
    `NIOSSHPrivateKey`-inpackning som redan användes för host-nycklar).
  - `OpenSSHPrivateKey.export(seed:comment:)` — skriver en okrypterad
    Ed25519-nyckel i riktigt OpenSSH-filformat (samma format `ssh-keygen`
    skapar), inversen av den redan befintliga `parse`-funktionen. Verifierad
    dubbelt: rundresa genom den egna (redan bevisade) decodern, OCH ett
    riktigt `ssh-keygen -y -f`-anrop mot den exporterade filen — den faktiska,
    kanoniska implementationen läser vår fil och räknar ut exakt samma
    publika nyckel, inte bara vår egen kod som testar sig själv.
  - `SSHSession.deployPublicKey(_:)` — lägger till en publik nyckelrad i
    fjärrsidans `~/.ssh/authorized_keys` över en redan autentiserad session
    (idempotent: `mkdir -p`/`chmod`/`grep -qxF || echo >>`, aldrig
    dubblettrader). Kommentaren (fri text) är inte ett smalt validerbart
    format som `DockerService`s namn-allowlist, så en riktig `shellQuoted`-
    escaping används istället — testad mot en RIKTIG `/bin/sh`-subprocess
    (inte bara egen escape-logik mot sig själv), inklusive skalmetatecken
    (`$() \`` ; & | > < \`) och en injektionsförsöks-sträng.
  - `SSHSession.verifyKeyAuthWorks(target:seed:knownHosts:)` — en tyst,
    separat anslutning med den nya nyckeln, stänger direkt utan att köra
    något kommando. Testad end-to-end mot `LoopbackServer` (lyckas) och mot
    en onåbar host (misslyckas rent, ingen hängning).
  - **Windows-stöd** (2026-07-06, `RemotePlatform`): `deployPublicKey(_:platform:)`
    tar nu ett `RemotePlatform`-argument (`.posix` default, `.windowsAdmin`,
    `.windowsStandard`) — upptäckt via RIKTIG verifiering mot en Windows
    Server 2025-VPS att Win32-OpenSSH har en avsiktlig säkerhetsregel:
    admin-konton IGNORERAR `~/.ssh/authorized_keys` helt, kräver den delade
    `C:\ProgramData\ssh\administrators_authorized_keys` med strikta ACL:er
    (`icacls`, bara SYSTEM+Administrators, ärvda rättigheter avstängda) —
    annars vägrar sshd använda filen. Windows-kommandot byggs som ett
    `powershell -EncodedCommand`-anrop (hela skriptet Base64/UTF-16LE-kodat)
    istället för att försöka escapa en fri kommentarsträng genom två
    nästlade skallager (SSH-exec-argumentet OCH cmd.exe/PowerShells egen
    citering) — base64 innehåller bara tecken som är säkra oquotade i cmd.exe.
    **Verifierat mot riktig extern hårdvara, inte bara enhetstester**: hela
    flödet (generera nyckel → `deployPublicKey(platform: .windowsAdmin)` →
    `verifyKeyAuthWorks`) kört i ett svep mot en riktig Windows Server 2025-
    VPS, autentiserade rent utan lösenord. Testnycklarna städades bort
    efteråt.
  - **`Host.platform`-fält** (2026-07-06): ✅ klart. `RemotePlatform`
    (`.posix`/`.windowsAdmin`/`.windowsStandard`) sparas nu per host-profil
    (bakåtkompatibel avkodning — gamla `host.json`-filer utan fältet faller
    tillbaka på `.posix`, samma mönster som `isFavorite`/`colorTag`). Egen
    `Picker` i `LinuxApp/HostEditView.swift` ("Fjärrsystem").
  - **LinuxApp-flödet klart** (2026-07-06, `KeyDeployView.swift`): ny
    "SSH-nyckel"-knapp i `HostDetailView`. Generera → deploya (`platform`
    läses från host-profilen) → tyst verifiera, i tur och ordning — checkbox
    "Byt till nyckel-auth" visas ENDAST efter lyckad verifiering (opt-in,
    aldrig automatiskt, matchar [[feedback_password_removal_scope]]).
    Bekräftelse skriver nyckeln till `~/.bastion/keys/<host-id>_ed25519`
    (0600) och byter `host.auth` till `.keyFile(path)` — LinuxApp har ingen
    Keychain (se `AuthResolver.swift`), så "ta bort lösenordet" betyder här
    bara att sluta FRÅGA efter det (`.askPassword` → `.keyFile`); LinuxApp
    sparade aldrig själva lösenordsvärdet till att börja med. Byggd + körd
    (Xvfb), rent utan krasch. **HISTORIK, inte aktuell status** — den här
    `KeyDeployView.swift` hörde till den gamla SwiftCrossUI/GTK4-`bastion-gui`,
    som är RIVEN sedan arkitekturbeslutet 2026-08-03 (se "Arkitekturbeslut").
    Motsvarigheten i den NYA Rust/GTK4-LinuxApp beskrivs separat nedan.
  - **Rust/GTK4-LinuxApp: SSH-nyckeldistribution, backend + UI klart**
    (2026-08-03, `LinuxApp/src/key_deploy.rs`): `generate_ed25519` (via
    `russh::keys::key::KeyPair::generate_ed25519`) → `deploy_command`
    (idempotent `~/.ssh/authorized_keys`-tillägg, samma logik som Swifts
    `deployPublicKeyCommandPOSIX` — bara POSIX, `Host.platform`/Windows-
    grenarna finns inte i LinuxApp) → `deploy_and_verify` öppnar en HELT NY,
    separat anslutning med bara den nya nyckeln för att bevisa att den
    fungerar, motsvarande `SSHSession.verifyKeyAuthWorks`. Nyckeln sparas
    som PKCS8 PEM (0600, `~/.bastion/keys/bastion_ed25519_<uuid>`) — samma
    `russh_keys::decode_secret_key` som redan läser OpenSSH-format läser
    PKCS8 lika bra, ingen egen OpenSSH-writer behövdes. Testat genuint
    end-to-end mot samma fristående test-sshd-mönster som `-L`/`-R`/`-D`:
    en riktig ny nyckel genereras, deployas, och en efterföljande anslutning
    som ENDAST litar på den nya nyckeln lyckas — plus en explicit kontroll
    att raden verkligen landade i den (isolerade) `authorized_keys`-filen,
    inte bara att verifieringen råkade lyckas av något annat skäl.
    GTK4-vyn ("Nyckel"-menyposten, bakom `show_key_deploy`) har
    Generera+deploya+verifiera i ETT klick, visar den publika raden, och en
    separat "Använd den nya nyckeln"-knapp (bara aktiv efter lyckad
    verifiering — opt-in, aldrig automatiskt, samma
    [[feedback_password_removal_scope]]-princip som Swift-sidan) som byter
    `host.auth` till den nya nyckelfilen via `HostStore::upsert`. Byggd och
    körd under Xvfb utan krasch.
    **"Klistra in befintlig nyckel"-flödet klart** (2026-08-04,
    `key_deploy::import_existing`): tolkar en klistrad OpenSSH-privatnyckel-
    PEM via `russh::keys::decode_secret_key`, motsvarande Swifts
    `KeyGenerator.fromExisting`/`KeyDeployModel.importExisting`. Avvisar
    tydligt lösenfras-skyddade nycklar (`Error::KeyIsEncrypted`) och
    icke-Ed25519-nycklar (RSA/ECDSA/…) — bara Ed25519 stöds, samma
    begränsning som Swift-sidan. Testat mot RIKTIGA `ssh-keygen`-genererade
    nycklar (okrypterad Ed25519, lösenfras-skyddad Ed25519, RSA), inte
    handkonstruerade PEM-strängar. GTK4-vyn fick ett textfält för att
    klistra in nyckeln + en egen "Importera + deploya + verifiera"-knapp
    som återanvänder samma deploy/verifiera/adoptera-flöde som
    generera-ny-knappen. Byggd och körd under Xvfb utan krasch.
    **`Host.platform`/Windows-mål klart** (2026-08-04,
    `key_deploy::deploy_command_for_host`/`deploy_command_windows`): `Host`
    hade redan ett `platform: RemotePlatform`-fält (för wire-format-
    kompatibilitet med Swift/WindowsApp) men det lästes tidigare aldrig av
    något i LinuxApp — `deploy_command` antog alltid POSIX. Windows-grenen
    bygger samma `powershell -EncodedCommand`-kommando (hela skriptet
    Base64/UTF-16LE-kodat, kringgår dubbel skal-citering helt) som Swifts
    `deployPublicKeyCommandWindows`: `.windowsAdmin` skriver
    `C:\ProgramData\ssh\administrators_authorized_keys` + låser ner ACL:er
    (`icacls`) eftersom Win32-OpenSSH annars vägrar filen helt för
    adminkonton, `.windowsStandard` skriver `$env:USERPROFILE\.ssh\
    authorized_keys` utan ACL-krav. Ny `base64`-direktberoende (fanns bara
    transitivt tidigare). Ny "Fjärrsystem"-väljare (Linux/macOS/Windows-
    adminkonto/Windows-standardkonto) i värdredigeringsdialogen.
    Testat: det genererade PowerShell-skriptet avkodas tillbaka och
    verifieras innehålla rätt sökväg/ACL-hantering/citerad nyckelrad (ingen
    riktig Windows-värd tillgänglig i den här sandlådan för ett fullt
    end-to-end-test — samma begränsning som WindowsApp/MainWindow.xaml.cs,
    se ROADMAP-posten om `wip/windowsapp-csharp`). Byggd och körd under
    Xvfb utan krasch, väljaren verifierad interaktivt (öppnar/visar alla
    tre alternativ korrekt).
  - **App/-flödet klart** (2026-07-08, `App/KeyDeployView.swift`): samma
    generera→deploya→verifiera-ordning, men lagrar nyckeln i Keychain
    (`.keychainKey`, samma ID-schema `host-key-<uuid>` som `HostEditView`
    redan använder för manuellt importerade nycklar) istället för en fil på
    disk — det mer iOS-idiomatiska valet, till skillnad från LinuxApp som
    saknar Keychain-åtkomst. Ny "SSH-nyckel"-post i `HostDetailView`s meny.
    Kan inte byggas/verifieras här (Xcode-only) — verifieras av
    `xcode.yml`-CI:t.
  - **"Klistra in befintlig nyckel och deploya"-klart** (2026-07-09):
    `KeyDeployModel.importExisting(pem:)` (LinuxApp OCH App/) — parsar
    klistrad OpenSSH PEM-text (`OpenSSHPrivateKey.parse`), avvisar
    tydligt om den inte är Ed25519 eller är lösenfras-skyddad. Ny
    SSHCore-funktion `KeyGenerator.fromExisting(seed:comment:)` (`throws`,
    till skillnad från `generateEd25519()` som alltid får ett giltigt frö
    från Curve25519 självt) härleder samma `GeneratedKeyPair`-form ur ett
    BEFINTLIGT frö — resten av flödet (deploy/verifiera/spara) återanvänds
    rakt av, ingen duplicerad logik. 3 nya SSHCore-tester (härledning
    matchar `generateEd25519`, avvisar fel frölängd, full export→parse→
    fromExisting-rundresa). UI: en "Klistra in befintlig nyckel istället"-
    knapp bredvid "Generera nyckel", visar en textruta + Importera/Avbryt.
- **App-ikon + launch screen**: `App/Assets.xcassets` (genererad från en SVG med
  `rsvg-convert`, opak PNG utan alfakanal enligt Apples krav — alla iOS- och
  macOS-storlekar) + en mörk `LaunchBackground`-färg som matchar ikonen.
  `ASSETCATALOG_COMPILER_APPICON_NAME` satt i `project.yml` för båda targeten.
- **macOS-target**: `Bastion-macOS` i `project.yml` (App Sandbox + utgående nätverk),
  terminalvyn plattformsvillkorad (`UIViewRepresentable`/`NSViewRepresentable`),
  app-guards `canImport(SwiftUI)`, `typealias Host = SSHCore.Host` i `Platform.swift`
  (undviker krock med `Foundation.Host` på macOS).
- **Auto-poll av dashboard**: `DashboardModel.startPolling()` hämtar direkt och
  sedan var 15:e sekund tills vyn stängs (`.task`-avbrott). Övergående fel under
  en periodisk uppdatering ersätter inte redan visad data — bara den första
  hämtningen kan visa felskärmen. UI visar senaste uppdateringstid + spinner.
- **Nyckelimport i appen**: `HostEditView` har ett "Importera nyckel"-läge —
  klistra in en OpenSSH-privatnyckel, den valideras direkt (`OpenSSHPrivateKey.parse`)
  och sparas i Keychain (aldrig i host-DB:n som synkas). Ny `HostAuth.keychainKey(id)`,
  löses upp i `AuthResolver`. Städas ur Keychain när värden tas bort eller
  auth-metoden byts bort.
- **Linux-GUI** (`bastion-gui`, SwiftCrossUI/GTK4): värdlista, dashboard med
  auto-poll, nyckelfil/lösenord/agent-auth, ssh-config-import.
  Byggd och startad (Xvfb) med en Swift 6.5-dev-snapshot — se README
  "Bygg Linux-GUI:t" för varför stabila 6.1.3 inte funkar än.
- **Linux-terminal** (`TerminalBuffer`/`TerminalGridView`/`TerminalSessionView`):
  bestående PTY-shell (miljö/cwd bevaras mellan kommandon, olikt engångs-`execute()`)
  med en egenskriven VT100/ANSI-tolk — markörflytt (CUU/CUD/CUF/CUB/CUP), radering
  (ED/EL), SGR-färg (16-färgspalett + bold), OSC-sekvenser (fönstertitel) sväljs
  utan att synas. 17 fristående tester (utan SwiftCrossUI-länkning) verifierar
  parsern, inklusive en verklig bugg som hittades under verifieringen: Swift
  grupperar `"\r\n"` till EN grafemkluster-`Character`, så tolkning måste ske
  per `Unicode.Scalar`, inte per `Character` — annars matchar CR/LF aldrig.
  **Riktig tangentbordsinmatning** (2026-08-02, `KeyEventBridge.swift`): ✅
  klart — löser punkten som tidigare stod under "Uppskjutet med avsikt".
  Antagandet där (kräver att gå under SwiftCrossUI direkt mot GTK:s
  event-controllers) stämde bara delvis: swift-cross-uis EGET `Gtk`-paket
  har redan en publik, kodgenererad `GtkEventControllerKey`-wrapper (samma
  mönster deras egen `Window.setEscapeKeyPressedHandler` redan använder) —
  men den wrapperns C-trampolin returnerar `Void` trots att `key-pressed`
  faktiskt är en `gboolean`-returnerande signal (`TRUE` stoppar vidare
  spridning, `_gtk_boolean_handled_accumulator` i GTK:s egen källkod) — en
  ABI-diskrepans som hittades under kodgranskning (CodeRabbit) och löstes
  genom att koppla signalen direkt via `g_signal_connect_data` med en EGEN,
  korrekt `gboolean`-returnerande trampolin, samma rå-signal-mönster som
  `GestureSwipeBridge.swift` redan använder för `GtkGestureSwipe` (som
  saknar en wrapper helt). Fokushantering (`gtk_widget_set_can_focus`/
  `set_focusable`/`grab_focus`) saknar publika Swift-wrappers, löst via
  samma `CGtk4Raw`-brygga.
  **Avsiktligt avgränsat**: text + navigeringstangenter (piltangenter/Tab/
  Esc/Backspace/Delete/Enter, `gdk_keyval_to_unicode` för allt skrivbart)
  — INTE Ctrl-kombinationer, som filtreras bort explicit (en `GdkModifier
  Type`-bitmask, CodeRabbit-fynd: annars hade fysisk Ctrl+C skickat en
  bokstavlig "c" till PTY:n) och även fortsatt täcks av de befintliga
  kontrollknapparna (Ctrl+C/Ctrl+D m.fl.).
  Kontrollknapparna (piltangenter/Home/End/PgUp/PgDn/Tab/Esc/Ctrl+C/Ctrl+D)
  och textfältet finns kvar som alternativ inmatningsväg. Fast 100×30
  storlek — ingen fönsterstorleks-driven `resize()` mot PTY:n än.
  **Ej verifierat interaktivt** (ingen riktig GUI-session eller
  testinfrastruktur för simulerade tangenttryck i det här repot) — bara
  `linuxapp-build` (kompilering + `TerminalBuffer`-parsertesterna) grönt.
- **SSHCore verifierat mot en riktig Linux-runner** (2026-08-02,
  `swiftpm-linux.yml`): ✅ klart. Påståendet "kärnan bygger på Linux och
  Apple" (CLAUDE.md/README) verifierades tidigare bara på en macOS-runner
  (`swiftpm-macos`) — ingen faktisk `swift build`/`swift test` av
  rot-paketet kördes någonsin på Linux i CI. Kör i den officiella
  `swift:6.1-noble`-Docker-avbildningen (rot-paketet beror inte på
  SwiftCrossUI, drabbas alltså inte av swift-mutex-kompilatorbuggen som
  tvingar `LinuxApp/` till en dev-snapshot). Hittade och fixade en riktig
  miljöbugg direkt: `ArchiveOperationsTests` (zip-rundresa) misslyckades
  med "remoteExit(status: 127)" — den minimala Docker-avbildningen saknade
  `zip`/`unzip`.
  **Kvarstående, dokumenterad flakighet** (upptäckt 2026-08-02, PR #223):
  `TerminalTeardownRaceTests.testConcurrentOpenShellAndCloseNeverCrashes`
  kraschade en gång på `swiftpm-linux` ("Cannot schedule tasks on an
  EventLoop that has already shut down" → "leaking promise"-fatal error i
  `EventLoopFuture.deinit`) — samma race-klass filen redan dokumenterar
  utförligt för macOS, men uppenbarligen med ett annat timingfönster på
  Linux (epoll) än macOS (kqueue), där samma test konsekvent gått grönt.
  En omkörning av EXAKT samma commit gick grönt utan ändringar — bekräftat
  intermittent, inte deterministiskt reproducerbart här och nu. Inte
  utrett vidare denna omgång (kräver djupare undersökning av den
  Linux-specifika event loop-avstängningsordningen, se `SSHSession.swift`s
  omfattande kommentarer om samma raceklass) — flaggat för framtida arbete
  snarare än en blind gissning.
- **`.deb`-paketering av `bastion-cli`** (2026-08-03, `linux-packaging.yml`):
  ✅ klart. Bygger release-varianten med `--static-swift-stdlib` (länkar
  Swift-runtimen — stdlib/Foundation/Dispatch — statiskt), paketerar
  `usr/bin/bastion-cli` som ett riktigt `amd64`-`.deb` via `dpkg-deb --build`,
  och installerar + kör det FAKTISKT byggda paketet (`dpkg -i` + ett
  smoke-test som förväntar exitkod 2 och "Användning:"-meddelandet från
  `main.swift`) — inte bara "det kompilerar".
  `--static-swift-stdlib` ger INTE en fullständigt statisk binär (bekräftat
  både via research och verkligt `readelf`-utfall i CI): glibc och,
  eftersom `SSHCores S3Client.swift` drar in `FoundationNetworking`/
  `FoundationXML` på Linux, även `libcurl`/`libxml2` förblir dynamiska
  beroenden. `Depends`-raden i kontrollfilen härleds därför FAKTISKT ur
  binärens egen `DT_NEEDED`-lista (`readelf -d`) — varje bibliotek slås upp
  mot sitt Debian-paket via `dpkg -S` — i stället för att gissas/hårdkodas.
  Två genuina buggar hittades och fixades under utvecklingen: containerns
  standardskal (`sh`/dash) saknar `set -o pipefail` (löst med `shell: bash`),
  och `ldconfig -p` svarar med den osolvade `/lib/...`-symlänken medan
  Ubuntus sammanslagna `/usr` gör att `dpkg -S` bara känner igen den
  kanoniska `/usr/lib/...`-vägen (löst med `readlink -f`). Version hämtas
  från senaste `v[0-9]*`-taggen (annars `0.0.0`); versionssträngen skickas
  via `env:` (inte direkt `${{ }}`-interpolering) för att inte vara sårbar
  för skalinjektion via en illvillig taggnamn.
  **Kvar**: bara `bastion-cli` — `bastion-gui` (GTK4-beroenden utöver
  Swift-runtimen) är ett separat, större paketeringssteg.
- **`.rpm`-paketering av `bastion-cli`** (2026-08-03,
  `linux-packaging-rpm.yml`): ✅ klart — RHEL/Fedora-halvan av samma
  backloggpunkt. Samma härledda-beroende-strategi som `.deb`-jobbet
  (binärens egna `DT_NEEDED`-lista via `readelf -d`, uppslaget mot
  RPM-paket via `rpm -qf` i stället för `dpkg -S`, `Requires`-raden byggd
  av DEN listan) — bara paketeringsverktyget bytt, ingen ny gissning.
  Bygger i den officiella `swift:6.1-rhel-ubi9`-containern (giltig
  image-tagg bekräftad direkt i CI, ingen omväg behövdes till skillnad
  från `.deb`-jobbets `sh`/dash- och symlänk-fällor). Spec-filen skrivs
  inline i workflowet (`%install` kopierar bara den redan byggda binären,
  inget `%build`-steg) med `%global debug_package %{nil}` för att slippa
  att `find-debuginfo.sh` letar efter debug-sektioner i en Swift-länkad
  binär. Installeras + körs på riktigt (`rpm -i` + samma smoke-test som
  `.deb`-jobbet) — inte bara "det kompilerar". Grönt på första försöket,
  inga CodeRabbit-fynd.
- **`.deb`-paketering av `bastion-gui`** (2026-08-03,
  `linux-packaging-gui.yml`): ✅ klart — GUI-halvan av backloggpunkten.
  Byggs INTE i `swift:6.1-noble`-containern som CLI-paketen — bastion-gui
  drar in SwiftCrossUI, vars kärnmodul kraschar stabila Swift 6.1.3
  (swift-mutex-buggen), så samma Swift dev-snapshot-hämtning som
  `linux-gui.yml` redan använder återanvänds rakt av. Samma härledda-
  beroende-teknik som `.deb`-jobbet för `bastion-cli` (`readelf -d` →
  `dpkg -S`) — bara en mycket LÄNGRE lista (hela GTK4/GLib/Pango/Cairo/
  GdkPixbuf-familjen i stället för bara libcurl/libxml2), eftersom
  tekniken är generisk och inte bryr sig om hur många delade bibliotek
  binären råkar länka mot.
  **CodeRabbit-fynd (Major)**: det första smoke-test-utkastet körde
  `dpkg -i` + start direkt på byggvärden — men samma värd hade precis
  installerat `libgtk-4-dev` som byggberoende, vilket redan lägger GTK4-
  runtimebiblioteken på disk. Ett ofullständigt/felaktigt `Depends` hade
  alltså kunnat passera testet ändå (fel sak bevisad). Löst genom att
  flytta install+körning till en HELT FRISK `ubuntu:24.04`-container
  (`docker run`) och byta `dpkg -i` mot `apt-get install ./paket.deb`,
  som faktiskt löser `Depends`-raden via apt i en miljö som aldrig haft
  `-dev`-paketen installerade — Xvfb-smoke-testet (starta binären, vänta
  5s, kontrollera att processen fortfarande lever) körs sedan INUTI den
  containern.
  **Uppföljande CI-fynd (upptäckt EFTER merge, ny PR samma dag)**: den
  ursprungliga `--static-swift-stdlib`-strategin (samma som `bastion-cli`)
  visade sig krascha på RIKTIGT i CI — `undefined reference to
  'swift_uloc_toLegacyKey'` m.fl. ICU-symboler när SwiftCrossUIs
  makro-/kompilatorplugin (`SwiftCrossUIMacrosPlugin`, körs på
  byggvärden, inte i den slutgiltiga binären) länkas mot dev-snapshotens
  `swift_static/`-bibliotek. Bekräftat (websökning) vara en känd
  buggkategori specifik för tarball-installerade Swift dev-snapshots —
  Docker-baserade toolchains (som `bastion-clis swift:6.1-noble`) har
  aldrig haft problemet. Löst genom att gå tillbaka till DYNAMISK länkning
  av Swift-runtimen och i stället BUNTA IHOP toolchainens egna `.so`-filer
  i paketet (`usr/lib/bastion-gui/`) med en RPATH satt via `patchelf`.
  **Ytterligare CodeRabbit-fynd (Major) på den lösningen**: en enkel
  genomsökning av bara bastion-guis EGNA direkta `DT_NEEDED`-lista räcker
  inte — de buntade Swift-biblioteken har SJÄLVA transitiva beroenden
  (t.ex. FoundationInternationalization → ICU) som aldrig syns där, och
  `DT_RUNPATH` är dessutom INTE transitivt till barn-bibliotek (ld.so(8):
  gäller bara objektets egna direkta beroenden). Löst med en riktig
  arbetskö (BFS) som även genomsöker varje buntat biblioteks egna
  `DT_NEEDED`, och `patchelf` på VARJE buntat bibliotek (inte bara
  huvudbinären) — alla ligger i samma katalog så `$ORIGIN` räcker för dem
  alla. Smoke-testet kompletterades med en precis `ldd`-baserad kontroll
  (pekar ut exakt vilket bibliotek som saknas) INNAN Xvfb ens startas.
  **Kvar**: `.rpm` för `bastion-gui` inte påbörjat. Bara `amd64`.
- **Linux-Docker-hantering**: `DockerView` (i `HostDetailView` via en knapp/sheet)
  lista/start/stopp/omstart/logg/shell — samma `DockerService` som iOS-appen.
  Shell öppnar en `TerminalSessionView` med `docker exec` som initialt kommando
  (nytt `initialCommand`-stöd i `TerminalController`).
- **Kontointegration, PKCE-kärna + Dropbox/Google Drive/OneDrive**: `OAuthPKCE`
  (SSHCore, plattformsoberoende) genererar verifier/challenge enligt RFC 7636
  — testad mot RFC:ns egen vektor (fångade ett eget transkriptionsfel i testet
  självt: `dbjftJeZ…` vs. rätta `dBjftJeZ…`, versalskillnad). `OAuthAccountManager`
  sköter den interaktiva inloggningen (`ASWebAuthenticationSession`),
  `OAuthTokenStore` Keychain-lagring + tyst förnyelse via `refresh_token`.
  Tre färdiga `SyncProvider`-implementationer (samma `SyncCrypto`-kryptering
  som `EncryptedFolderSyncProvider` — molntjänsten ser bara chiffertext):
  Dropbox (path-baserad), OneDrive (path-baserad via Graph), Google Drive
  (sök-först + multipart-upload mot `appDataFolder`, ingen path-API där).
  `SyncSettingsView` har transportval (mapp/Dropbox/Google Drive/OneDrive) +
  in-/utloggning per leverantör. **OBS**: allt utom PKCE-kärnan är Xcode-only
  och därför obyggt/otestat här — kräver ett riktigt klient-ID per leverantör
  (se README "Konton") för att verifieras.
- **Golden standard-repokonfiguration**: ✅ klart (verifierat 2026-07-07,
  sista biten tillagd). LICENSE (MIT), SECURITY.md, AGENTS.md, CLAUDE.md,
  issue-mallar (`bug_report.yml`/`feature_request.yml`) fanns redan —
  `.github/PULL_REQUEST_TEMPLATE.md` saknades, nu tillagd. Standardworkflows
  (auto-commit/label/merge/rebase/release, ci-autofix, security-alerts-sync)
  och branch-ruleset på `main` (required checks: xcodegen-and-build,
  swiftpm-macos, linuxapp-build, CodeRabbit) verifierade befintliga via
  `gh api repos/.../rulesets`.

## Uppskjutet med avsikt

- **Krypterade nycklar (lösenfras).** OpenSSH krypterar med `bcrypt_pbkdf`
  (Blowfish-baserad) + `aes256-ctr`. Varken bcrypt_pbkdf eller AES-CTR finns i
  swift-cryptos publika API, så det kräver egna implementationer av Blowfish +
  bcrypt_pbkdf + AES-CTR — säkerhetskritisk kod som förtjänar en egen
  genomgång med testvektorer, inte en snabb iteration. Parsern kastar
  `SSHKeyError.encrypted` tydligt tills dess. (ECDSA P256/P384/P521, OKRYPTERADE,
  stöds redan — se Status-tabellen; det här gäller bara lösenfrasskyddet.)
- **RSA-nyckelauth.** Blockerad av ett ANNAT skäl än krypterade nycklar: RSA
  finns inte alls bland `NIOSSHPrivateKey`s fall (bekräftat genom att läsa
  `NIOSSHPrivateKey.swift` i swift-nio-ssh — bara `.ed25519`/`.ecdsaP256`/
  `.ecdsaP384`/`.ecdsaP521`/`.secureEnclaveP256`). swift-nio-ssh saknar
  RSA-klientautentisering helt, inte bara okrypterad parsning av den —
  kräver antingen uppströms-stöd eller en egen RSA-signeringsimplementation
  ovanpå biblioteket. Parsern kastar `SSHKeyError.unsupportedKeyType`
  tydligt tills dess.
- **Ssh-agent-forwarding till fjärrserver** (`auth-agent@openssh.com`-
  kanaltypen, klassisk `ssh -A`) — **arkitektoriskt blockerad i
  swift-nio-ssh, inte något i Bastions egen kod** (verifierat 2026-07-07
  genom att läsa bibliotekets källkod, inte gissat).
  `SSHMessage.ChannelOpenMessage.ChannelType` (i `SSHMessages.swift`) har
  bara tre hårdkodade fall: `.session`, `.forwardedTCPIP`, `.directTCPIP`.
  `readChannelOpenMessage()`s `switch` på kanaltypssträngen har ett
  `default: throw NIOSSHError.unknownPacketType(...)` — ett inkommet
  `channel-open` av typen `auth-agent@openssh.com` (som en fjärrserver
  skulle skicka när NÅGOT på den servern vill använda den vidarebefordrade
  agenten) skulle alltså få biblioteket att kasta ett fel vid själva
  paketparsningen, inte ge en hanterbar "okänd kanaltyp, avvisa den här
  ENA kanalen"-väg. Till skillnad från globala förfrågningar (`tcpip-
  forward` m.fl.), som HAR en uttrycklig utökningspunkt
  (`GlobalRequestDelegate`, redan använd av `ServerRemoteForwardingDelegate`
  i testerna), finns ingen motsvarande delegate-typ för inkommande
  kanalöppningar av godtycklig typ. Inte fixbart utan att forka
  swift-nio-ssh. `SSHAgentClient.swift`s lokala (Unix-socket-mot-en-
  körande-agent) funktionalitet är opåverkad och fullt användbar.

Interaktiv shell finns i kärnan (`SSHSession.openShell`) och driver både
`App/TerminalView.swift` (SwiftTerm) och `LinuxApp`s `TerminalSessionView`.

## Backlog, fasindelad (uppdaterad 2026-07-06 — tvOS tillagd)

Se [VISION.md](VISION.md) "Tillägg efter den ursprungliga visionen" för
bakgrunden (konkurrentlandskap: Termius/Tabby/Termix/Magic Term/Conduit).
Strategin: UX-paritet med Termius väger tyngre än nya SSH-protokollfunktioner
för sin egen skull — det är UX:en folk betalar för, inte protokollet.

**Juridiskt:** undvik visuell/varumärkeslikhet med Termius i design — se
VISION.md "Design".

### Fas A — Få ut det som redan är byggt
Inget nytt att bygga, bara verifiera/lansera:
- Verifiera kontointegrationen i Xcode (Dropbox/Google Drive/OneDrive) med
  ett riktigt klient-ID.
- Få appen på en riktig iPhone — 🧩 påbörjad, se "Nästa steg" ovan (kontot
  aktivt, secrets satta, appen skapas nu i App Store Connect).

### Fas B — UX-paritet med Termius (det folk betalar för idag)
- **Port Forwarding**: 🧩 **lokal (`-L`) OCH fjärr (`-R`) klara i SSHCore**.
  - Lokal: `SSHSession.openLocalPortForward(bindHost:bindPort:targetHost:targetPort:)`,
    en lokal TCP-lyssnare som bryggar varje ansluten klient till en egen
    `direct-tcpip`-SSH-kanal. `close()` stänger både lyssnaren och alla
    aktiva tunnlar (CodeRabbit-fynd, PR #25/#61, se "Klart").
  - Fjärr (2026-07-06): `SSHSession.openRemotePortForward(bindHost:bindPort:targetHost:targetPort:)`
    — ber servern lyssna åt oss (`sendTCPForwardingRequest(.listen(...))`,
    ett globalt SSH-request, inte en kanal). Servern öppnar en
    `forwarded-tcpip`-kanal TILLBAKA till oss för varje anslutning; en
    delad, trådsäker tabell (`SSHSession.remoteForwards`, keyad på port)
    dirigerar varje inkommen kanal till rätt lokal `targetHost:targetPort`
    via `handleInboundForwardedChannel` (satt som `inboundChildChannelInitializer`
    vid `connect()`). Samma `GlueHandler`/`DirectTCPIPWrapperHandler` som
    lokal vidarebefordran, bara i motsatt riktning.
    Testservern (`LoopbackServer`) fick en riktig `GlobalRequestDelegate`-
    implementation (`ServerRemoteForwardingDelegate`/`ServerRemoteForwarder`,
    baserad på swift-nio-ssh:s eget `NIOSSHServer`-exempel) för att kunna
    bevisa hela vägen end-to-end (riktig extern TCP-anslutning → servern →
    SSH → klienten → riktig lokal TCP-ekoserver → samma väg tillbaka), inte
    bara en förenklad eko-kortslutning.
    **3 riktiga buggar hittade under just den här verifieringen** (skulle
    inte synts utan ett genuint end-to-end-test): (1) `DirectTCPIPWrapperHandler`
    sattes på fel kanal i `handleInboundForwardedChannel` (lokala TCP-
    anslutningen istället för SSH-kanalen) — kraschade direkt så fort riktig
    data flödade igenom. (2) `sendTCPForwardingRequest` är dokumenterat
    "inte trådsäker, får bara anropas på kanalens egen event loop", men en
    `async`-fortsättning garanterar inte det — måste skickas in explicit via
    `channel.eventLoop.execute { ... }` i både `openRemotePortForward` och
    `close()`. (3) Testserverns `stopListening()` kraschade med NIOs egen
    "BUG DETECTED"-skydd mot att anropa `.wait()` på en event loop-tråd.
  - **CLI-koppling för `-R`** (2026-07-06): ✅ klart i `bastion-cli`, symmetriskt
    med `-L` (samma `[bindHost:]bindPort:targetHost:targetPort`-syntax,
    samma `LocalForwardSpec`-parser återanvänd rakt av). `bastion-cli -R ...`
    öppnar fjärrtunneln, väntar på Ctrl+C, stänger rent.
  - **Dynamisk (`-D`, SOCKS5)** (2026-07-06): ✅ klart, `SOCKSProxy.swift`.
    En egen SOCKS5-handskakningshandler (RFC 1928, ackumulerar fragmenterade
    TCP-bytes tills ett helt ramverk kan avkodas) — stödjer IPv4/domännamn/
    IPv6 som måladress, ingen auth (bara `NO AUTHENTICATION REQUIRED`, lokal
    trådad tunnel). Målet klienten begär (godtyckligt, PER anslutning — det
    är hela poängen med "dynamisk" jämfört med `-L`s fasta mål) öppnas som
    en egen `direct-tcpip`-SSH-kanal, precis som `-L`. CLI: `bastion-cli -D
    [bindHost:]bindPort <host>`.
    **En riktig bugg hittad under end-to-end-verifieringen** (skulle inte
    synts utan ett genuint test — en handrullad SOCKS5-klient som begärde
    TVÅ olika mål i tur och ordning och verifierade att servern faktiskt
    fick rätt targetHost/targetPort för VARDERA, inte bara att data ekade):
    `pipeline.removeHandler(name:)` tar inte effekt omedelbart bara för att
    den anropas — data som klienten (korrekt, efter att ha läst CONNECT-
    svaret) skickade omedelbart därefter hann träffa den gamla handskaknings-
    handlern INNAN borttagningen faktiskt slagit igenom, och sväljdes tyst.
    Fix: handskakningshandlern vidarebefordrar (`context.fireChannelRead`)
    istället för att droppa allt som kommer in efter att den en gång blivit
    klar — oavsett om den formellt redan borttagen ur pipelinen eller inte.
  - **LinuxApp-GUI** (2026-07-06, `PortForwardView.swift`): ✅ klart, ny
    "Tunnlar"-knapp i `HostDetailView`. Väljer typ (lokal/fjärr/dynamisk) via
    `Picker`, fält för bindport + mål (mål döljs för dynamisk — SOCKS-
    klienten väljer det per anslutning), lista över aktiva tunnlar med
    "Stoppa"-knapp per rad. En delad `SSHSession` per vy-instans (samma
    mönster som `DockerModel`), stänger alla aktiva tunnlar + sessionen vid
    `onDisappear`. Byggd och körd (Xvfb) med Swift 6.5-dev-snapshot-
    toolchainen (se README "Bygg Linux-GUI:t" för varför stabil 6.1.3
    kraschar på ett känt, öppet kompilatorfel — inte relaterat till den
    här koden).
  - **App/-yta** (iOS/macOS): ✅ klart (2026-07-08, `App/PortForwardView.swift`)
    — samma modell/beteende som LinuxApp, native SwiftUI. Kan inte byggas/
    verifieras här (Xcode-only), verifierad av `xcode.yml`-CI:t.
- **Face ID/Touch ID-app-lås** — ✅ klart i App/. `AppLockManager` (LocalAuthentication,
  `.deviceOwnerAuthentication` — Face ID/Touch ID/lösenkod-fallback), låser vid
  bakgrund (`scenePhase`), egen inställningsyta (menyn i värdlistan, av som
  standard). `NSFaceIDUsageDescription` tillagd i Info.plist (krävs av iOS,
  annars kraschar appen vid första anropet). LinuxApp/Windows: ingen
  motsvarighet — plattformsspecifikt Apple-API.
- **Snippets med variabler** — ✅ klart, både App/ och LinuxApp. `Snippet`/
  `SnippetStore` i SSHCore (`{{namn}}`-variabler, testat inkl. en fångad
  regression: extraherad variabel trimmades men ersättningen letade efter
  den otrimmade nyckeln, så `{{ mellanslag }}` aldrig matchade). UI: knapp
  i värddetaljvyn, fyll i variabler, kör som startkommando i en ny terminal
  (samma `ConnectRequest.running(_:)`/`initialCommand`-mönster som Docker-
  shell). Ingen sync av snippets mellan enheter än (medvetet, v1).
- **Favoriter/färgkodning i host-listan** — ✅ klart, både App/ (`Host.isFavorite`/
  `colorTag` i SSHCore, `HostColorPicker`, egen "★ Favoriter"-sektion) och
  LinuxApp (samma fält, favoriter sorterade överst, "☆/★ Favorit"-knapp
  eftersom SwiftCrossUI saknar swipe-actions).
- **Sök i host-listan** — ✅ klart, både LinuxApp (`ContentView.swift`) och
  App/ (`HostListView.swift`, native `.searchable()`). Filtrerar alias/
  hostname/user/taggar i båda.
- **Flera samtidiga sessioner** — ✅ klart i App/ (iOS/macOS). `SessionManager`
  (`App/SessionManager.swift`) håller alla öppna sessioner; `MultiSessionView`
  presenterar dem som `TabView`-flikar — SwiftUI river inte ner overksamma
  flikars vyer vid växling, så en bakgrundad session förblir faktiskt
  ansluten utan egen livscykelkod. "Klar" (i `HostDetailView`) döljer bara
  flikväxlaren (`dismiss()`, sessionerna lever kvar); en ny meny-post
  "Stäng session" kopplar faktiskt från. Sista fliken stängd → tillbaka
  till värdlistan automatiskt. **Kvar**: äkta sida-vid-sida Split View
  (iPad/Mac) — bara flikväxling hittills, ingen samtidig visning av två
  terminaler. LinuxApp oförändrad (dess `NavigationSplitView` byter fortfarande
  ut hela detaljvyn vid nytt värdval — samma begränsning som App/ hade innan
  den här ändringen, inte adresserad än).
- **Kör kommando automatiskt vid anslutning** ("startup snippet", nytt,
  2026-07-07, ägarfråga) — ✅ klart. Nytt `Host.startupCommand: String?`
  (samma bakåtkompatibla `decodeIfPresent`-mönster som `isFavorite`/
  `colorTag`/`platform`). Skickas som `initialCommand` till den
  BEFINTLIGA `initialCommand`-mekanismen i `TerminalController`/
  `SSHTerminalController` (som redan fanns för Docker-shell/Snippets) —
  bara på den VANLIGA "öppna terminal"-vägen, inte de vägar som redan
  skickar sitt eget explicita kommando (annars skulle två kommandon
  köras i rad). LinuxApp + App/-UI: ett textfält "Kör automatiskt vid
  anslutning". 2 nya tester (Codable round-trip + bakåtkompatibilitet
  för gamla `host.json`-filer utan fältet), 226 gröna totalt.

### Fas C — Differentiatorer bortom Termius
- Docker-hantering ✅ redan klart (App + LinuxApp).
- Systemstatus/dashboard ✅ redan klart.
- **Tailscale-stöd**: ✅ statusparsning klar (2026-07-07, `TailscaleStatus.swift`)
  — `tailscale` installerades RIKTIGT lokalt (på användarens uttryckliga
  uppmaning, "installera och avinstallera sedan") för att äntligen få
  verifierbar grund istället för att gissa: en genuin `tailscaled` (v1.98.8)
  startades, `tailscale status --json` kördes på riktigt (`BackendState:
  NeedsLogin`, ingen inloggning gjordes — kräver riktiga kontouppgifter,
  olämpligt i en testkörning) och den RIKTIGA JSON-utskriften sparades som
  testfixtur. `Self`- och `Peer`-poster delar samma `PeerStatus`-Go-typ i
  Tailscales egen källkod (verifierat via källkodsläsning tidigare i natt),
  så fältnamnen som bekräftades via `Self` (HostName/DNSName/OS/
  TailscaleIPs/Online) gäller rimligen även `Peer`. `suggestedHosts`
  filtrerar till online-peers med minst en IP, föredrar `DNSName`
  (MagicDNS) över `HostName` när tillgängligt.
  **Kvarstående, dokumenterad begränsning**: Tailscale garanterar
  fortfarande INTE att formatet är stabilt mellan versioner — det här är
  verifierat mot v1.98.8 specifikt, inte en formell spec. 3 nya tester
  (riktig JSON-fixtur + en handkonstruerad peer-fixtur som återanvänder
  samma bekräftade fältnamn).
  **Värdförslag + LinuxApp-UI** (2026-07-07): ✅ klart —
  `TailscaleStatus.fetch(over:)` (SSH-remote, samma mönster som
  `SystemProbe.snapshot(over:)`) OCH `TailscaleStatus.fetchLocal()` (lokal
  `Process`-körning på maskinen appen själv exekverar på, som ssh-config-
  import läser en lokal resurs) — användaren väljer källa själv, inte
  appen (uttryckligt önskemål: "både som enskilda val eller kombinerade
  val så får användaren bestämma vad som är bekvämast för dom").
  `fetchLocal()` testad mot en RIKTIG, kortlivad processkörning (eget
  `/bin/sh`-skript, inte mockad), inklusive felvägen (icke-noll exitkod +
  stderr). `TailscaleDiscoveryView.swift` (LinuxApp): växlare "Denna
  maskin"/"Fjärrvärd", resultatlista med "Lägg till"-knapp per förslag —
  öppnar det vanliga redigeringsläget förifyllt med tailnet-adressen
  (Tailscale känner inte till SSH-användarnamnet, till skillnad från
  ssh-config-import). Byggd + körd (Xvfb), rent utan krasch. 2 nya
  tester, 190 gröna totalt.
  **App/-motsvarighet**: ✅ klart (2026-07-08, `App/TailscaleDiscoveryView.swift`)
  — `fetchLocal()` villkorsstyrd bort på iOS (`#if !os(iOS)`, `Foundation.
  Process` saknas i sandlådan där, dokumenterat i SSHCore-källan); iOS visar
  bara fjärrvärd-källan, macOS båda. Kan inte byggas/verifieras här
  (Xcode-only), verifierad av `xcode.yml`-CI:t.
  **Kvar**: `HostAuth`/host-listintegration djupare än "lägg till som ny
  värd" (t.ex. markera en befintlig värd som nåbar via ett specifikt
  tailnet) — oförändrat, ingen plattform har det än.
  **WireGuard fullständigt verifierat end-to-end, inklusive en riktig
  fungerande tunnel** (2026-07-07, rättar en felaktig tidigare slutsats):
  `wireguard-tools` installerades och `WireGuardConfig.swift`s
  `rendered()`-utdata testades mot en RIKTIG `wg-quick up` — ett genuint
  gränssnitt kom upp (`ip addr show` bekräftade rätt adress/MTU). Den
  tidigare uppfattningen ("CAP_NET_ADMIN blockeras av sandlådan") var
  FEL — roten var (1) `wireguard`-kärnmodulen inte laddad (`modprobe
  wireguard` löste det) och (2) en kommandonamnsbaserad spärr i den här
  specifika sandlådemiljön som blockerar `wg-quick` anropat DIREKT
  (`sudo wg-quick ...`) men inte via `sudo bash /usr/bin/wg-quick ...` —
  ingen verklig capability-begränsning.
  Byggde sedan en fullständig tunnel mp100 ↔ Windows Server-VPS
  (206.168.215.180, WireGuard för Windows installerat via winget): båda
  sidor kom upp och lyssnade korrekt (verifierat med `netstat`), men
  handskakningen nådde först inte igenom — roten var att VPS-leverantörens
  (Hostup AB) EGEN nätverksbrandvägg/security group framför maskinen
  blockerade den inkommande UDP-porten, utöver Windows egen brandvägg
  (som redan var öppnad). Efter att den porten öppnats i Hostups
  kontrollpanel gick handskakningen igenom direkt — **en riktig,
  fungerande krypterad tunnel bekräftad**: `ping`/`ping6` gav 0 % paketförlust
  åt båda hållen, både IPv4 (`10.99.2.1` ↔ `10.99.2.2`) och IPv6
  (`fd00:99:2::1` ↔ `fd00:99:2::2`), ~10 ms tur-och-retur. Både
  WireGuard-installationen på Windows-VPS:en (inkl. brandväggsreglerna
  och tunneltjänsten) och `wireguard-tools`/`tailscale` här togs bort
  igen efter testet.
- **WireGuard-profiler**: ✅ kärnan klar (2026-07-06, `WireGuardConfig.swift`)
  — v1 avgränsat till PROFILHANTERING (parsa/lagra/redigera/exportera
  `.conf`-text), INTE att upprätta tunneln (kräver `wg`-binären + root,
  eller ett helt eget WireGuard-protokoll om det byggdes utan den binären
  — separat, mycket större arbete). Formatet verifierat mot `wg(8)` och
  `wg-quick(8)` (man7.org), inte gissat: `[Interface]` (PrivateKey/Address/
  DNS/ListenPort/MTU/Table/PreUp/PostUp/PreDown/PostDown/SaveConfig/FwMark)
  + valfritt antal `[Peer]`-sektioner (PublicKey/PresharedKey/AllowedIPs/
  Endpoint/PersistentKeepalive). Skiftlägesokänsliga nycklar/sektions-
  rubriker (verkliga `.conf`-filer varierar), `#`-kommentarer, kommaseparerade
  listor, upprepade nycklar ackumuleras (flera `Address`-rader är tillåtet).
  9 tester, inklusive full round-trip (`parse -> rendered() -> parse` ger
  identiskt resultat) mot en realistisk exempelkonfiguration.
  **Lagring + LinuxApp-UI** (2026-07-06): ✅ klart. `WireGuardProfileStore`
  (JSON på disk, `~/.bastion/wireguard.json`, exakt samma mönster som
  `SnippetStore`). LinuxApp: `WireGuardProfileListView`/`WireGuardProfileEditView`
  — toppnivåknapp ("WireGuard" i sidopanelen, INTE per-värd som Snippets/
  Docker, eftersom en profil beskriver en VPN-anslutning, inte kopplad till
  en specifik SSH-värd). Redigering sker som rå `.conf`-text (klistra in,
  spara) snarare än ett fält-för-fält-formulär — enklare för en användare
  som redan har filen från sin VPN-leverantör/router. 3 nya store-tester
  (inkl. en full round-trip: text -> config -> lagrad JSON -> ny store-
  instans -> tillbaka till text, identiskt). 171 tester gröna totalt.
  Byggd + körd (Xvfb), rent utan krasch.
  **App/-motsvarighet**: ✅ klart (2026-07-08, `App/WireGuardProfileView.swift`)
  — samma modell, native SwiftUI. Kan inte byggas/verifieras här
  (Xcode-only), verifierad av `xcode.yml`-CI:t.
- **Native WireGuard/Tailscale — inget externt beroende** (nytt,
  2026-07-07, uttryckligt ägarönskemål — se VISION.md "En sak att
  prioritera högt": ska kännas komplett, inget proffs ska sakna något)
  — 🧩 hämta+verifiera-mekaniken PÅBÖRJAD (2026-08-03, se "Klart"), resten
  ren designanteckning för framtida arbete. Dagens läge
  (ovan) kräver ATT `wg`/`wg-quick` respektive `tailscale`/`tailscaled`
  redan är installerade separat av användaren — motsatsen till "fristående
  app" -löftet i README:s första rad. Målet: appen kan självständigt
  upprätta båda sortens tunnlar, utan att användaren installerar något.
  Plattformarna kräver TVÅ OLIKA arkitekturer, inte en gemensam lösning:
  - **Linux/Windows/BSD/macOS utanför Mac App Store** (Bastion distribueras
    redan fristående här, `.deb`/`.rpm`/direktnedladdning — se "Paketering"
    nedan): ladda ner OFFICIELLA, plattforms-/arkitekturmatchade
    förbyggda binärer (`wireguard-go` — WireGuards egen userspace Go-
    implementation, INGEN kärnmodul krävs, finns för i princip alla
    plattformar som EN binär — samt `tailscale`/`tailscaled`, som
    Tailscale själva distribuerar på samma sätt) vid första användning
    eller på begäran, verifiera checksum/signatur mot projektens egna
    publicerade värden (leverantörskedjesäkerhet — vi kör en nedladdad
    binär, samma tillitsnivå som ett `curl | sudo bash`-installations-
    skript om det görs fel), cacha under `~/.bastion/bin/<verktyg>/<version>/`,
    kör den direkt. LÖSER exakt ägarens konkreta önskemål: portabel (ingen
    separat installation), alltid senaste tillgängliga version, OCH ett
    versionsväljar-UI (fästa en specifik release — t.ex. vid en känd bugg
    i senaste versionen, eller ett företagskrav på en godkänd version).
  - **iOS (App Store-distribuerat, se `testflight.yml`/fastlane)**: KAN
    INTE ladda ner och köra godtycklig binärkod — App Store Review
    Guideline 2.5.2 förbjuder det uttryckligen, oavsett hur bekvämt det
    vore. Rätt arkitektur här är en `NetworkExtension`
    (`NEPacketTunnelProvider`) — ETT NYTT XCODE-TARGET, inte en byggflagga
    — med WireGuard/Tailscales Go-kod STATISKT inbyggd vid KOMPILERINGS-
    tillfället (inte nedladdad vid körning). Det är exakt samma arkitektur
    de OFFICIELLA WireGuard-  (github.com/WireGuard/wireguard-apple) och
    Tailscale-iOS-apparna (github.com/tailscale/tailscale, `ios/`-katalogen)
    använder — båda öppen källkod (MIT/BSD), värda att läsa som referens
    snarare än att uppfinna på nytt. macOS via Mac App Store (om Bastion
    någonsin distribueras dit) skulle ha samma begränsning; macOS UTANFÖR
    App Store (notariserad direktnedladdning) kan använda antingen vägen
    ovan (nedladdad binär) eller en NetworkExtension.
  Konsekvens att hålla i åtanke framöver: `SSHCore` förblir plattforms-
  neutral (ren SwiftNIO) — den här funktionen hör hemma i UI-lagren
  (`App/`, `LinuxApp/`, en framtida Windows-app), INTE i `SSHCore` självt,
  eftersom binärhantering/NetworkExtension är fundamentalt
  plattformsspecifikt på ett sätt SSH-kärnan inte är. Om något SSHCore-
  arbete framöver rör processhantering/nedladdning/binärverifiering
  generellt (t.ex. en delad "hämta+verifiera en extern binär"-hjälpare)
  är det värt att bygga den ÅTERANVÄNDBAR för både WireGuard och
  Tailscale, snarare än en engångslösning för det ena.
- **tvOS** (nytt, 2026-07-06) — ✅ grundspiken + Docker-vy klara, sync-
  implementation klar men ej verifierad (2026-07-21). Eget `Bastion-tvOS`-
  target (`App/TVApp/`), egen kod (delar inget med `App/`-roten, se
  filkommentaren där) — dashboard (värdlista + Wake-on-LAN), Docker-vy (samma
  `DockerService` som iOS/macOS, ingen Shell-knapp — Siri Remote kan inte
  driva en interaktiv terminal), riktig app-ikon/Top Shelf-bild, och synk mot
  Google Drive/OneDrive via OAuth device-flow (RFC 8628 — det enda OAuth-
  flödet som fungerar på tvOS, `ASWebAuthenticationSession` är
  `API_UNAVAILABLE` där). Dropbox stödjer INTE device-flow (bekräftat
  uppströms, "wontfix") och erbjuds därför inte som synktransport på tvOS.
  App Store-distribution: `testflight.yml`/`Fastfile` har nu en `tvos beta`-
  gren (samma mönster som iOS — `match` readonly + `latest_testflight_
  build_number` + `upload_to_testflight`, egen bundle-ID `se.denied.
  bastion.tv`). tvOS-distributionscertifikat + `match AppStore se.denied.
  bastion.tv`-profil är bootstrappade i bastion-certificates (2026-07-22,
  samma dag som iOS-certifikatet omgenererades efter att alla certifikat
  raderades manuellt i Apple Developer-portalen). **Kvar:** ett App Store Connect-app-
  record för `se.denied.bastion.tv` — verifierat (2026-07-22, 403 FORBIDDEN
  mot `POST /v1/apps`) att App Store Connect-API:t INTE stödjer att skapa
  app-record ALLS, oavsett nyckelroll (Admin/App Manager/Account Holder) —
  detta är inte ett rollproblem, bara en API-begränsning. Måste skapas
  manuellt i App Store Connect-webb-UI:t (Mina appar → + → Ny app).
  Google/Microsoft-kontoverifiering och device-flow-konfiguration med
  riktiga klient-ID:n, verifiering på riktig Apple TV-hårdvara (bara
  simulator hittills).
- **visionOS** (nytt, 2026-07-22) — lös idé, inte påbörjat, ingen
  prioritet satt än. Två separata effortnivåer, INTE jämförbart med
  tvOS-targetets omfattning rakt av (cubic-fynd på PR #199, korrigerat):
  ett vanligt fönsterläge kan i många fall fås "gratis" genom att bara
  lägga till Apple Vision-destinationen på befintliga iOS/iPad-targeten
  och kompilera om (enligt Apples egen dokumentation) — betydligt
  billigare än tvOS-targetets egna, separata kodbas. Skräddarsytt
  SwiftUI-arbete (likt tvOS-targeten) behövs bara för en RIKTIG spatial/
  immersiv upplevelse (3D-innehåll, flytande paneler i rummet osv.), inte
  för ett grundläggande fönster. Se VISION.md "Plattforms- och
  paketeringsmål, fullständigt".
- **Djup plattformsintegration** (nytt, 2026-07-22) — lös idé, inte
  påbörjat, ingen prioritet satt än. Uttryckt mål: Bastion ska kännas lika
  "hemma" på varje plattform som möjligt, inte bara en portad app. Konkreta
  förslag, ingen prioritetsordning fastställd:
  - **Widgets** (iOS/macOS hemskärm + Widgets-panel).
  - **Tillgänglighet** — respektera systemets Inställningar → Tillgänglighet
    (VoiceOver, Dynamic Type, Reduce Motion m.fl.), inte bara egna
    in-app-inställningar.
  - **Lokalisering baserad på systemspråk** — åtminstone som tillval utöver
    engelska (standard), inte hela översättningsomfånget bestämt än.
  - **Spotlight-sökning** (iOS/macOS) — sök fram en host direkt från
    systemsökningen.
  - **Siri Shortcuts / App Intents** — t.ex. "Anslut till [host]" som
    röstkommando eller Lås-skärm-widget.
  - **Live Activities/Dynamic Island** — visa pågående filöverföring eller
    långkörande kommando utan att ha appen öppen. Kräver egen utredning av
    "keep-alive utan att döda batteriet": iOS suspenderar/stänger vanliga
    nätverkssocklar i bakgrunden efter en kort tid oavsett — en hållen SSH-
    session kan INTE bara lämnas öppen i bakgrunden. Live Activities
    uppdateras via push (ActivityKit push-tokens) eller korta background-
    tasks (`BGTaskScheduler`), inte genom att hålla anslutningen vaken
    kontinuerligt — den arkitekturen (server-side-notis när kommandot är
    klart, snarare än att appen pollar) måste utredas separat innan detta
    är mer än en idé.
  - **Handoff** — starta en session på en enhet, fortsätt sömlöst på en
    annan.
  - **Windows Jump Lists/taskbar-progress**, **macOS meny-rad-extra**,
    **Linux `.desktop`-integration + systemnotiser** — plattformsspecifika
    motsvarigheter till samma idé.

  Huvudavvägning: varje integration multiplicerar underhållsbördan per
  plattform (redan 7+ plattformar i visionen) — värt att prioritera de som
  ger mest "känns hemma"-känsla per plattform snarare än att jaga alla
  samtidigt.
- **Anpassningsbara teman + skärm-/batterianpassning** (nytt, 2026-07-22)
  — lös idé, inte påbörjat, ingen prioritet satt än.
  - **Automatiskt mörkt/ljust tema** — följ systemets tema, inte en egen
    in-app-växel som kan hamna i otakt med OS-inställningen.
  - **Skärmanpassning** — Dynamic Type, säkra ytor (safe area), fungera
    bra i delad skärm/Stage Manager/olika fönsterstorlekar på desktop,
    inte bara en fast layout tänkt för en skärmstorlek.
  - **Batterimedvetenhet** — respektera systemets strömsparläge (t.ex.
    iOS Low Power Mode) genom att dra ner bakgrundssynk/pollingfrekvens,
    inte bara ett UI-tema-val.
  - **Skräddarsytt eget tema utan pixelputs** — arkitektera färgsättningen
    som en liten uppsättning semantiska "sektioner"/roller (bakgrund,
    accent, terminalfärger, m.fl.) som en användare kan nyansera i stort
    (byta några basfärger) istället för att behöva styla varje enskild
    UI-komponent för sig — en design-token-arkitektur, inte hårdkodade
    färger per vy.
- **Anslutnings-resiliens** (nytt, 2026-07-22) — lös idé, inte påbörjat,
  ingen prioritet satt än. EN sammanhängande arkitekturfråga i SSHCore
  (delas av alla plattformar), inte tre separata funktioner. Verifierat
  (2026-07-22) att inget av detta finns i koden idag — varken keep-alive,
  nätverksbytesdetektering eller reconnect-logik.
  - **Keep-alive**: ✅ klart (2026-08-02, `SSHShell.startKeepAlive()`/
    `stopKeepAlive()`). Bekräftat på nytt mot både 0.14.0 OCH den senare
    0.15.0-taggen (klonad käll­kod): fortfarande ingen publik generisk
    global request (bara `sendTCPForwardingRequest`), och inget publikt
    sätt att skicka en godtycklig KANALförfrågan heller (`SSHMessage.
    ChannelRequestMessage.RequestType.unknown` är internal, inte
    exponerad). Löst med den no-op-mekanism roadmapen redan pekade ut:
    en periodisk fönsterändring till SAMMA storlek som senast känd —
    ren no-op på Linux (kärnans `TIOCSWINSZ` skickar bara SIGWINCH vid
    en FAKTISK ändring, `tty_ioctl.c`), håller ändå NAT/brandväggars
    idle-timeout varm eftersom trafik faktiskt går över tråden.
    Delat tillstånd (senaste storlek, aktiva Task:en) i en
    `NIOLockedValueBox`, samma mönster som `PortForward.swift`/
    `SOCKSProxy.swift`. Kopplad in i alla tre GUI:erna (App/LinuxApp/
    WindowsApp) direkt efter att en shell öppnas, stoppas automatiskt av
    `shell.close()` — en CodeRabbit-granskning hittade att den INTE
    stoppades i normal-avslutnings-/catch-grenarna (keepAlive-Task:en
    kunde då hinna köra `triggerUserOutboundEvent` medan
    `chain.close()` rev ner event loop-gruppen under den, samma
    race-klass som redan dokumenteras i `SSHSession.swift`) — fixat i
    samma PR. 3 nya tester mot en riktig `LoopbackServer` (periodiska
    sändningar sker, `stopKeepAlive` stoppar dem faktiskt, `resize()`
    uppdaterar storleken keep-alive återanvänder).
    **Täcker bara "håll NAT-mappningen varm"** — dead-connection-
    detektering och återanslutning (nedan) är fortfarande inte
    påbörjade.
  - **WiFi ↔ mobildata-byte** — TCP-anslutningen dör vid nätverksbyte
    (annat interface = ny anslutning krävs). Kräver `NWPathMonitor`
    (Apple) eller motsvarande för att upptäcka bytet.
  - **Sömn/viloläge** — iOS stänger bakgrundssocklar oavsett (samma
    begränsning som Live Activities-punkten ovan); macOS/Linux-
    systemsömn bryter också anslutningen. Kräver detektering av "död
    anslutning" vid uppvaknande.
  - **Viktig begränsning (cubic-fynd på PR #199):** en ny transport-
    anslutning kan INTE bara ersätta den gamla "transparent" — SSH-kanaler
    och fjärrprocess-tillstånd (t.ex. en pågående interaktiv shell eller
    filöverföring) förloras när transporten bryts, oavsett hur snabbt en
    ny TCP-anslutning öppnas. Detta måste delas upp i två separata delar:
    (1) automatisk TRANSPORT-återanslutning (bara TCP/SSH-handskakningen)
    och (2) explicit ÅTERHÄMTNINGS-beteende per kanal/process (t.ex. varna
    användaren att en pågående shell-session dog och måste startas om,
    snarare än att låtsas den fortsätter sömlöst). Alla tre orsakerna
    (keep-alive-timeout, nätverksbyte, sömn) delar samma underliggande
    detekteringsbehov ("anslutningen är död"), men ÅTERHÄMTNINGEN skiljer
    sig åt beroende på vad som faktiskt pågick i kanalen.
- **Bugg-rapportering direkt i appen** (nytt, 2026-07-22) — lös idé,
  inte påbörjat, ingen prioritet satt än. Låt användaren skicka en
  buggrapport (med valfri skärmdump/loggutdrag) utan att lämna appen och
  utan att behöva ett GitHub-konto. Kräver ett beslut om backend/mottagare
  (t.ex. en enkel Cloudflare Worker-endpoint som skapar ett GitHub-issue å
  användarens vägnar) — inget sådant finns idag.
  **Säkerhet (cubic-fynd på PR #199):** en endpoint som håller GitHub-
  credentials och tar emot anonyma inskick kan missbrukas för att spamma
  repot eller tömma API-kvoter om den lämnas oskyddad — kräver autentisering
  /attestering (t.ex. App Attest på Apple-plattformar), rate limiting och
  missbruksskydd som en del av designen, inte en efterhandsfix.
  **Integritet (cubic-fynd på PR #199):** opt-in för diagnostik räcker INTE
  ensamt — inloggade credentials eller känsligt terminalinnehåll kan ändå
  finnas inbäddat i loggar/skärmdumpar även om användaren aktivt valt att
  skicka dem. Kräver en förhandsgranskning av innehållet FÖRE utskick plus
  automatisk hemlighets-redigering (secret redaction) av loggutdrag, inte
  bara en opt-in-kryssruta.
- **Command Library** — ✅ klart, både App/ och LinuxApp. `CommandLibrary`/
  `CommandLibraryEntry` i SSHCore — statisk referensdata (ingen egen lagring,
  till skillnad från `Snippet`), 27 kommandon över alla sju kategorier
  (Docker/Linux/Git/Cloudflare/Tailscale/WireGuard/systemd), var och en med
  beskrivning + valfritt exempel/dokumentationslänk. Kör ett kommando
  återanvänder Snippets variabelifyllning (`CommandLibraryEntry.asSnippet`).

### Fas D — De stora bitarna (ingen ändring i prioritet)
- **Android — INTE valfritt, uppdaterat 2026-07-07 (ägarbeslut, se
  VISION.md "Plattformar")**. Tidigare "kan vänta om resurserna är
  begränsade" — omvärderat: Bastions syfte är att ersätta Termius på
  bred front, ingen lucka som "skaver" ska finnas, och Termius egen
  Android-app är exakt den sortens lucka om Bastion saknar en.
  Sekvenserad EFTER iOS/macOS/Linux/Windows (redan i gång), inte för
  att den är mindre viktig utan för att den är den enda plattformen i
  hela backloggen som inte återanvänder `SSHCore` direkt via ett nytt
  Xcode-target/SwiftPM-paket — se VISION.md för de två realistiska
  vägarna (Skip-transpilering kontra en helt separat Kotlin-
  SSH-implementation) och avvägningen mellan dem.

  **Grundarbete klart (2026-07-07)**: valde Kotlin-native-vägen (inte
  Skip, som kräver macOS+Xcode+Android Studio — se VISION.md) för att
  kunna börja utan att vänta på Mac-åtkomst. `Android/` är ett eget
  Gradle/Kotlin-projekt (samma "eget paket"-princip som `LinuxApp/`),
  med Apache MINA SSHD som SSH-motor (samma princip som SSHCore bygger
  på swift-nio-ssh istället för att implementera protokollet från
  grunden). `BastionSshSession` (connect/run/close) är verifierad mot
  en riktig in-process SSH-server i `BastionSshSessionTest` — en äkta
  anslut+autentisera+kör-kommando+läs-utdata-runda, plus ett negativt
  test (fel lösenord avvisas). Ingen UI, ingen host-lagring, ingen
  nyckelbaserad auth eller jump host än — bara den minsta beviskärnan.
- **Telnet-stöd** (nytt, 2026-07-07, ägarfråga, se VISION.md) — ✅ klart.
  Helt separat från SSH (RFC 854, okrypterat, egen option-negotiation) —
  egen `TelnetSession`, inte en utökning av `SSHSession`. En egen
  protokollimplementation från grunden.
- **Paketering + BSD-täckning** (nytt, 2026-07-07, se VISION.md
  "Plattforms- och paketeringsmål, fullständigt"):
  `.deb`-paket för `bastion-cli` (Debian/Ubuntu) — ✅ klart (2026-08-03,
  se "Klart" → "`.deb`-paketering av `bastion-cli`"). `.rpm`-paket för
  `bastion-cli` (RHEL/Fedora) — ✅ klart (2026-08-03, se "Klart" →
  "`.rpm`-paketering av `bastion-cli`"). `.deb`-paket för `bastion-gui`
  — ✅ klart (2026-08-03, se "Klart" → "`.deb`-paketering av
  `bastion-gui`"). `bastion-gui` (SwiftCrossUI/GTK4) är sedan dess RIVEN
  (2026-08-03, se "Arkitekturbeslut") och ersatt av `LinuxApp` (native
  Rust/GTK4) — "`.rpm` för `bastion-gui`" är därför inte längre en giltig
  lucka, den appen finns inte kvar att paketera.

  **`.deb`-paketering av `bastion-linuxapp`** (2026-08-04,
  `linuxapp-packaging.yml`): ✅ klart — LinuxApp-motsvarigheten till
  `bastion-cli`s `.deb`, den faktiska nuvarande GUI-luckan efter
  arkitekturpivoten. Till skillnad från Swift-CLI:t (statisk
  Swift-runtime) länkas `bastion-linuxapp` DYNAMISKT mot systemets
  GTK4/GLib/libadwaita/VTE4 — men, till skillnad från den nu rivna
  `bastion-gui` (som behövde bunta ihop delade bibliotek + `patchelf`
  p.g.a. ett dev-snapshot-Swift-linkningsproblem), är detta REN Rust mot
  DISTRONS EGNA `.so`-filer, så samma enkla härledda-`Depends`-teknik som
  `bastion-cli`s `.deb`-jobb (`readelf -d` → `dpkg -S`) räcker rakt av —
  ingen bundling behövdes. Enda skillnaden mot mallen: `ldconfig -p`
  listar samma bibliotek i flera arkitekturvarianter (x32 FÖRE x86-64) på
  en Ubuntu-runner, så uppslaget måste matcha explicit mot `x86-64`-
  taggen (annars slås fel `.so`-sökväg upp) — och `readelf`s
  "Shared library:"-etikett är lokalberoende, så `LC_ALL=C` krävs för att
  inte bero på skalets standardlokal (upptäckt lokalt: en sv_SE-miljö gav
  "delat bibliotek:" i stället, vilket tystade hela beroendelistan). Ny
  `LinuxApp/packaging/bastion-linuxapp.desktop` (minimal, generisk
  `utilities-terminal`-ikon — ingen egen ikonresurs finns än). Installeras
  + körs (`apt-get install ./paket.deb` + `xvfb-run`, processen lever
  kvar efter 5s) i en HELT FRISK `ubuntu:24.04`-container, samma
  "fel sak bevisad annars"-lärdom som `bastion-gui`s CodeRabbit-fynd gav.
  Härledd `Depends` verifierad lokalt: `libc6, libadwaita-1-0,
  libglib2.0-0t64, libgtk-4-1, libvte-2.91-gtk4-0`.

  **`.rpm`-paketering av `bastion-linuxapp`** (2026-08-04,
  `linuxapp-packaging-rpm.yml`): ✅ klart (CI-verifierat, ej lokalt körbart
  i den här sandlådan — se nedan) — RPM-halvan av samma backloggpunkt,
  samma härledda-`Requires`-teknik (`readelf -d` → `rpm -qf` i stället för
  `dpkg -S`). Byggs i en `fedora:40`-container (INTE RHEL UBI9, som
  `bastion-cli`s `.rpm`-jobb använder — UBI9s begränsade repos saknar
  GTK4/libadwaita/VTE4-skrivbordspaketen; Fedora-paketnamnen `gtk4-devel`/
  `libadwaita-devel`/`vte291-gtk4-devel` bekräftade via websökning, inte
  gissade). Körs medvetet som `docker run fedora:40 …` i stället för
  jobbnivåns `container:`-nyckel: både build- och smoke-test-stegen
  behöver ett eget `docker run` (det senare i en HELT FRISK container,
  samma "fel sak bevisad annars"-princip som `.deb`-jobbet), och ett
  containeriserat jobb har ingen egen docker-daemon-åtkomst (ingen
  docker-in-docker utan extra uppsättning på GitHub-runners). Byggskriptet
  ligger i `LinuxApp/packaging/build-rpm.sh` (inte inline i YAML) för att
  undvika skör nästlad bash-i-YAML-citering. **Ej körbart lokalt i den här
  sandlådan**: `docker run` gav "permission denied" mot den lokala
  docker-sockeln (samma begränsning gällde redan `.deb`-jobbets
  container-baserade smoke-test) — skriptet är syntaxkontrollerat
  (`bash -n`) och YAML:et schemavaliderat, men den FAKTISKA `dnf install`/
  `rpmbuild`/smoke-test-körningen är overifierad tills CI kör den.

  **Kvar**: FreeBSD-bygge (Swift har
  community-toolchains där; LinuxApp/Rust har sin egen story att
  utreda), OpenBSD/NetBSD-undersökning (oklart om något av detta ens
  fungerar där än — måste verifieras mot en riktig installation innan
  något annat antas). Alla paketeringsworkflows (CLI och GUI) bygger idag
  bara `amd64` (CodeRabbit-fynd på `.deb`-jobbet för `bastion-cli`:
  föregående skrivning antydde felaktigt att ARM64/Raspberry Pi redan
  täcktes) — ARM64 kräver en egen körning på en ARM64-runner/toolchain
  och ett faktiskt testat artefakt innan det kan räknas som klart, inte
  bara att toolchainen i teorin stödjer arkitekturen.
- **Native filhanterare-integration + molnlagring som filkälla** (nytt,
  2026-07-07, se VISION.md "Native filhanterare-integration + molnlagring
  som filkälla") — inte påbörjat. Apple: `FileProvider`-ramverket
  (`NSFileProviderReplicatedExtension`) — Blink Shell gör redan detta,
  beprövad väg, kräver ett separat extension-target. Windows: egen
  WinFsp-filsystemsprovider backad av `SFTPClient` (`sshfs-win` bevisar
  konceptet men är en C/Cygwin-wrapper, inte återanvändbar rakt av).
  Molnlagring-som-filkälla kräver BREDARE OAuth-scope:er än de som redan
  finns för synk (app-mapp-avgränsade idag) + ny mappträd-bläddringskod.
  **AWS/S3-kompatibel klient**: ✅ klart (2026-07-07, `S3Client.swift`) —
  egen AWS SigV4-signering (canonical request + string-to-sign +
  HMAC-SHA256-kedja via `swift-crypto`), path-style URL:er, XML-parsning
  (`Foundation.XMLParser`/`FoundationXML` på Linux) av
  ListBuckets/ListObjects/Error-svarsformaten. `listBuckets`/`createBucket`/
  `deleteBucket`/`listObjects`/`putObject`/`getObject`/`deleteObject`.
  Signeringen verifierad på två sätt: en oberoende Python-referens-
  implementation fick ett genuint 200 OK mot Hostups riktiga
  S3-kompatibla tjänst (`s3.hostup.se`, Ceph RGW) med riktiga nycklar
  (region `us-east-1`, path-style bekräftat), och testerna låser en
  fixerad signeringsvektor mot regression. Ett riktigt, LIVE end-to-end-
  test (`testLiveRoundTripAgainstRealHostupS3`) skapar en bucket, laddar
  upp, listar, laddar ner, verifierar innehållet, städar upp — mot den
  genuina tjänsten, inte en mockad server. Hoppar tyst över (inte fail)
  om `HOSTUP_S3_*`-miljövariabler saknas (t.ex. i CI, som medvetet inte
  har dessa hemligheter). 6 nya tester, 196 gröna totalt (1 hoppad utan
  nycklar i miljön).
  **LinuxApp-UI + anslutningslagring** (2026-07-07): ✅ klart.
  `S3ConnectionStore` (JSON på disk, `~/.bastion/s3connections.json`,
  samma mönster som `WireGuardProfileStore` — nycklar i klartext, samma
  medvetna v1-avgränsning). `S3ConnectionListView`/`S3ConnectionEditView`/
  `S3BrowserView`: lista sparade anslutningar → bläddra buckets → objekt
  → ladda upp/visa/spara ändringar/ta bort. Innehåll som text (klistra
  in/redigera), inte en native filväljare — samma pragmatiska avgränsning
  som WireGuards råtextredigering och SFTP-filhanterarens textredigerare
  (SwiftCrossUI saknar en filväljar-API). Byggd + körd (Xvfb), rent utan
  krasch — men avslöjade en NY byggkomplikation: `FoundationXML` (draget
  in transitivt av `S3Client`) kräver `-Xlinker -rpath-link` utöver den
  redan dokumenterade `LD_LIBRARY_PATH`-workarounden för libxml2-
  kompatibilitet (se README "Om din toolchain-nedladdning..."). 4 nya
  store-tester, 205 gröna totalt.
  **App/-motsvarighet**: ✅ klart (2026-07-08, `App/S3ConnectionView.swift`)
  — samma modell, native SwiftUI. Kan inte byggas/verifieras här
  (Xcode-only), verifierad av `xcode.yml`-CI:t.
  **Kvar**: mappträd-bläddring för molnlagring-som-filkälla i stort (bredare
  OAuth-integrerad Dropbox/Drive/OneDrive-bläddring, separat från denna
  S3-specifika väg).
- **SFTP-filhanterare** — ✅ grundfunktionerna klara, både App/ och
  LinuxApp (`SFTPBrowserView`/`SFTPBrowserModel`): bläddra, navigera
  in/upp, ny mapp, döp om, ta bort. Mapp/fil skiljs via
  `SFTPFileAttributes.isDirectory` (läser POSIX-filtypsbitarna
  S_IFDIR/S_IFREG ur `permissions`-fältet — la till det efter att ha
  insett att den ursprungliga testservern bara satte behörighetsbitarna,
  inte typen, vilket hade gjort mapp/fil-särskiljning opålitlig).
  `SFTPProtocol.swift`: SFTP version 3-trådformatet (SSH_FXP_*), rent
  kodat/avkodat. `SFTPClient.swift`: öppnar en "sftp"-subsystem-kanal på
  en `SSHSession` (samma `DirectTCPIPWrapperHandler`-mönster som
  portvidarebefordran återanvänds för ByteBuffer<->SSHChannelData),
  INIT/VERSION-handskakning, id-baserad pending-request-tabell (en
  Swift-aktör — flera samtidiga förfrågningar över samma kanal är säkert).
  API: `realpath`/`stat`/`listDirectory`/`mkdir`/`rmdir`/`remove`/`rename`/
  `readFile`/`writeFile` (chunkad läsning/skrivning) + lägre nivå
  `openFile`/`read`/`write`/`closeFile`.
  42 tester totalt (26 rena protokoll-round-trip + 16 end-to-end mot en
  testserver backad av ett riktigt temp-directory — `FileManager`/
  `FileHandle`, inte bara protokolleko), inklusive ett samtidighetstest
  (10 parallella läsningar, verifierar att id-matchningen inte blandar
  ihop svar). **Ej gjort**: verifiering mot det RIKTIGA `sftp-server`-
  binärprogrammet (`/usr/lib/openssh/sftp-server` finns på den här
  maskinen) — testservern är min egen Swift-implementation av protokollet,
  inte OpenSSHs C-kod; att brygga ett riktigt underprocess-`sftp-server`
  via NIOPipeBootstrap + Foundation.Process är fragilt (dubbel fd-ägande
  mellan Foundation.Pipe och NIO) och sparat som ett eget, separat steg
  om djupare protokollkompatibilitet någonsin behöver verifieras.
  **chmod** (2026-07-06): ✅ klart — `SFTPClient.setPermissions(_:mode:)`
  (SSH_FXP_SETSTAT, samma `SFTPFileAttributes.permissions`-fält som redan
  fanns i tråd­formatet, bara ingen klientmetod förut) + en "chmod"-knapp
  i `LinuxApp/SFTPBrowserView.swift` (oktal textruta, t.ex. "644"). Test-
  servern (`ServerSFTPHandler`) svarade tidigare bara `opUnsupported` på
  SETSTAT — utökad till att faktiskt köra `chmod` på den riktiga bakomliggande
  filen, verifierat i testet genom att läsa tillbaka det RIKTIGA filläget
  från disk (`FileManager.attributesOfItem`), inte bara att servern svarade OK.
  **Textredigering** (2026-07-06, LinuxApp): ✅ klart — "Redigera"-knapp för
  filer (döljs för mappar), läser innehållet via befintlig `readFile`,
  visar i SwiftCrossUIs `TextEditor`, sparar via befintlig `writeFile`. Ingen
  ny SFTP-protokollkod behövdes — bara UI-orkestrering ovanpå redan testad
  läs/skriv-väg. En enkel giltighetskontroll (kodar tillbaka till UTF-8 och
  jämför bytelängd) vägrar öppna binärfiler som text istället för att visa
  korrupt/ersatt innehåll (`U+FFFD`) utan varning.
  **chown** (2026-07-07): ✅ klart, `SFTPClient.chown(_:uid:gid:)` —
  protokollagret (`SFTPFileAttributes.uid`/`gid`) var redan byggt och
  testat sedan tidigare, bara en bekvämlighetsmetod saknades. Kräver
  NUMERISKA UID/GID (SFTP v3 känner inte till användarnamn) — anroparen
  ansvarar för uppslagning. Verifierat mot den RIKTIGA filens ägarskap på
  disk (inte bara att servern svarade OK) — testservern (oprivilegierad)
  "byter" till processens egen uid/gid, det enda en icke-root-process får
  göra, men bevisar hela protokollvägen. `LoopbackServer`s SETSTAT-hantering
  utökad att applicera uid/gid, inte bara permissions. LinuxApp-UI: en ny
  "chown"-knapp bredvid "chmod", två textfält (UID/GID). 1 nytt test.
  **Zip/Tar** (2026-07-07): ✅ klart, `ArchiveOperations.swift` — SFTP
  version 3 har ingen egen arkivsemantik, så det här shellar ut till
  `tar`/`zip` över en vanlig exec-kanal (`SSHSession.run`), samma mönster
  som `DockerService.swift`. Sökvägar VALIDERAS INTE mot en whitelist
  (containerreferenser tål det, filnamn med mellanslag/unicode gör inte)
  — istället citeras varje sökväg individuellt (enkla citattecken,
  inbäddade `'` eskapade som `'\''`, standard POSIX-shell-säkert).
  Injektionssäkerheten är INTE bara antagen: ett test kör den citerade
  strängen genom en RIKTIG `/bin/sh -c` och verifierar att en filnamn-
  injektion (`"'; touch /tmp/...; echo '"`) tolkas som EN bokstavlig
  sökväg, inte ett avslutat citat + ett nytt kommando; ett annat test
  bevisar att en sådan injektion i `paths` genom hela vägen till
  `createTarGz` aldrig skapar bevisfilen. `LoopbackServer` fick en ny
  opt-in `realExec`-flagga (default `false`, rör inga befintliga
  exec-tester) som kör kommandot GENUINT via `Process` istället för det
  fejkade "ran: <kommando>\n"-ekot — krävs för att bevisa att `tar`/`zip`
  faktiskt skapar/packar upp RIKTIGA filer, inte bara att rätt
  kommandosträng skickades. Skapa-sedan-packa-upp-rundtur verifierad mot
  riktiga `tar`/`zip`-binärer för båda formaten. LinuxApp-UI: "Komprimera"
  (namnfält + zip/tar.gz-växel) och "Packa upp" (visas bara för kända
  arkivändelser: `.tar.gz`/`.tgz`/`.zip`) bredvid chmod/chown. v1
  avgränsat till EN fil/mapp åt gången (ingen flerval-UI i SwiftCrossUIs
  `List` ännu). 9 nya tester, 224 gröna totalt.
  **App/-paritet**: ✅ klart (2026-07-08) — textredigering (`App/SFTPBrowserModel.swift`/
  `SFTPBrowserView.swift`, binärt innehåll skrivskyddat, samma lärdom som
  S3-lagringsvyn), chmod/chown/komprimera/packa upp (kontextmeny per rad
  — long-press, inte fler swipe-actions — samt en enda enum-driven
  sheet-presentatör för chmod/chown/komprimera, samma CodeRabbit-lärdom
  applicerad proaktivt). Kan inte byggas/verifieras här (Xcode-only),
  verifierad av `xcode.yml`-CI:t.
  **Drag & Drop, App/ klart (2026-07-08)**: `.dropDestination(for: URL.self)`
  på filistan — släpp filer/mappar från Finder (macOS) laddar upp dem till
  den katalog som visas, via SAMMA redan öppna SFTP-anslutning som
  bläddringen använder (ingen ny session). Mappar laddas upp REKURSIVT
  (`mkdir` + rekursiv `contentsOfDirectory`-gång). `startAccessing
  SecurityScopedResource()`/`stop...` runt hela operationen — macOS App
  Sandbox ger tillfällig läsbehörighet för drop:ade filer utan egen
  entitlement (samma undantag som `NSOpenPanel`), verifierat mot
  `Bastion-macOS.entitlements` (bara `app-sandbox`+`network.client`, inget
  filbehörighets-entitlement finns eller behövs). `mkdir`-fel vid
  omuppladdning ignoreras medvetet (SFTP v3 saknar en egen "finns
  redan"-statuskod, den kom i v6 — se kodkommentar). Kan inte byggas/
  verifieras här (Xcode-only), verifierad av `xcode.yml`-CI:t.
  **LinuxApp-motsvarigheten klar** (2026-08-04, `sftp::upload_path_recursive`
  + `gtk::DropTarget` i `open_sftp_view`): den ursprungliga blockeraren
  (SwiftCrossUIs `Gtk`-paket saknade en färdig omslag för GTK4:s
  `GtkDropTarget`) gäller inte längre — gtk4-rs (den nya, native
  Rust-porten) har `DropTarget`/`gdk::FileList` direkt, ingen egen
  GObject/C-interop behövdes. Släpp av filer/mappar från filhanteraren rakt
  in i SFTP-vyns aktuella katalog laddar upp dem via SAMMA `SftpHandle`
  som resten av vyn använder; mappar laddas upp REKURSIVT (`mkdir` +
  rekursiv katalogvandring, samma `mkdir`-fel-ignoreras-medvetet-motivering
  som Swift-sidan — SFTP v3 saknar en "finns redan"-statuskod). Testat
  genuint end-to-end mot en fristående test-sshd: en lokal katalogstruktur
  (rot-fil + underkatalog med egen fil) laddas upp och läses tillbaka,
  bevisar både rekursionen och att innehållet överlever hela resan — inte
  bara ett plant enfilsfall. Byggd och körd under Xvfb utan krasch.
  **Kvar**: flerval för komprimering, förhandsvisning (t.ex. bilder),
  syntax highlighting (se separat post nedan).
- Inbyggd editor med syntax highlighting
- Plugin-system (Proxmox, TrueNAS, Unraid, Cloudflare, GitHub, Kubernetes)
- **Agent Forwarding**: ✅ agent-PROTOKOLLKLIENTEN klar (2026-07-07,
  `SSHAgentClient.swift`) — lista identiteter + begära signaturer från en
  KÖRANDE, LOKAL `ssh-agent` över `$SSH_AUTH_SOCK` (Unix-socket via NIO:s
  `ClientBootstrap.connect(unixDomainSocketPath:)`). Trådformatet
  verifierat mot `draft-miller-ssh-agent-09` (IETF). v1 avgränsat till
  klienten mot en LOKAL agent — INTE forwarding över en SSH-kanal till en
  fjärrserver än (`auth-agent@openssh.com`-kanaltypen, kräver att koppla
  ihop klientens ramning med en SSH-kanal istället för ett rått socket,
  separat nästa steg).
  3 tester mot en RIKTIG, självstartad `ssh-agent`-process (ingen
  fejkad testserver — agent-protokollet är redan minimalt, inget SSH
  inblandat) + en riktig nyckel tillagd med `ssh-add`: lista identiteter,
  begära en signatur och VERIFIERA den kryptografiskt (Curve25519) mot
  den riktiga publika nyckeln, samt att en okänd nyckelblob korrekt ger
  `SSH_AGENT_FAILURE`.
  **Genuin bugg hittad och fixad under testutvecklingen** (inte i
  produktionskoden — i testinfrastrukturen, men värd att dokumentera för
  framtida liknande tester): `Process.waitUntilExit()` (Foundation)
  HÄNGER på Linux när en långlivad demonprocess (`ssh-agent -D`) redan är
  startad via samma `Process`-bokföring i samma testprocess — trots att
  en vanlig `kill -TERM` fungerar perfekt utanför Foundation, och trots
  att `KeyManagementTests.swift` använder EXAKT samma `waitUntilExit()`-
  mönster för `ssh-keygen` utan problem (ingen samtidig bakgrundsdemon
  där). En känd kategori av swift-corelibs-foundation-kvirk med
  barnprocess-reaping vid flera samtidiga `Process`-instanser. Fixat
  genom att helt kringgå Foundations väntemekanism: rå `kill(2)`/
  `waitpid(2)` istället, för alla subprocess-anrop i testfilen.
  **Auth-wiring (agenten SOM inloggningsmetod, inte bara protokollklient)
  undersökt (2026-07-07) — arkitektoniskt blockerad**, samma kategori som
  kanal-forwarding ovan: `NIOSSHPrivateKey`s `backingKey` (`NIOSSHPrivateKey.
  swift`) är ett INTERNT enum med bara fyra fasta bakomliggande typer
  (`.ed25519`/`.ecdsaP256`/`.ecdsaP384`/`.ecdsaP521`, plus `.secureEnclaveP256`
  på Apple-plattformar) — ingen protokoll- eller delegate-baserad
  utökningspunkt för en EXTERN signerare. `sign(_ payload:)` mönstermatchar
  direkt mot dessa och signerar SYNKRONT i samma anrop `NIOSSHUserAuthentic
  ationOffer` byggs. En `ssh-agent`-signaturbegäran är i sig async (en
  separat Unix-socket-rundtur via `SSHAgentClient`) — det finns ingen väg
  att koppla in det utan att patcha swift-nio-ssh självt. PKCS11, YubiKey,
  Passkeys drabbas av samma begränsning (alla kräver en extern/asynkron
  signerare). `secureEnclaveP256Key`-fallet visar att biblioteket
  KONCEPTUELLT stödjer "nyckelmaterialet lämnar aldrig sin källa" — bara
  hårdkodat till just Apples Secure Enclave-API, inte generellt.
- **OpenSSH-certifikatautentisering** (nytt, 2026-07-05) — stöd för
  `ssh-keygen`-signerade/externt utfärdade SSH-certifikat som en egen
  `HostAuth`-variant, inte bara rå nyckel. De stora molnleverantörerna har
  konvergerat mot exakt den här modellen (identitetsleverantör utfärdar
  ett kortlivat cert efter inloggning, istället för statisk nyckel):
  Cloudflare Access (kortlivade SSH-cert via en app-specifik eller
  konto-CA — kräver `TrustedUserCAKeys` på målservern), Google Cloud
  (OS Login med certifikatbaserad autentisering, `gcloud compute ssh`),
  Microsoft Entra ID (SSH-certifikatautentisering efter inloggning) och
  AWS EC2 Instance Connect (kortlivad — ~60 s — nyckel push till
  instansmetadata; inte riktigt samma CA-cert-mekanism men samma
  grundidé om engångs-/kortlivad autentisering istället för en
  permanent nyckel). Ett generellt OpenSSH-certifikatstöd i SSHCore
  fångar alla fyra utan plattformsspecifik kod.
  **Parsning** (2026-07-07): ✅ klart, `OpenSSHCertificate.swift` —
  `ssh-ed25519-cert-v01@openssh.com` (nonce/publik nyckel/serial/typ/
  key id/principals/giltighetstid/critical options/extensions/CA-
  nyckelblob/signaturblob). v1 avgränsat till PARSNING, INTE
  signaturverifiering eller `SSHUserAuth`-wiring — att verifiera CA-
  signaturen KORREKT är säkerhetskritiskt på ett sätt ren parsning inte
  är och förtjänar en egen, försiktig genomgång (samma avgränsningsprincip
  som krypterade nycklar, se "Uppskjutet med avsikt"). Trådformatet
  verifierat mot OpenSSHs `PROTOCOL.certkeys`-spec OCH empiriskt mot
  RIKTIGA certifikat genererade lokalt med `ssh-keygen -s` (egen CA-
  nyckel, riktig signering) — avkodade byte-för-byte med ett fristående
  Python-skript och jämförda mot `ssh-keygen -L`s egen tolkning, inte
  gissat ur minnet. Nästlingsdetalj upptäckt just genom den empiriska
  koll­en: `force-command`s data-fält är i sin tur en nästlad SSH-sträng,
  inte en rå textbyte-sekvens. Bara Ed25519 stöds (matchar kodbasens
  nuvarande begränsning). 10 tester mot två riktiga certifikat (user +
  host, inkl. "giltig för alltid"-sentinelvärdena `0`/`UInt64.max`).
  **Signaturverifiering** (2026-07-07): ✅ klart, `verifySignature()`.
  Bara CA:er som signerar med `ssh-ed25519` stöds (samma Ed25519-
  avgränsning som resten av kodbasen) — RSA/ECDSA-signerande CA:er kastar
  `OpenSSHCertificateError.unsupportedSigningKeyType` tydligt istället för
  att gissa. Signerad data är en SLICE av originalblobet (allt fram till
  men inte inklusive `signatureBlob`), inte återkonstruerad ur avkodade
  fält — undviker risken att en återkonstruktion råkar vara "nästan rätt"
  och ge en falskt positiv verifiering. Verifierar BARA den kryptografiska
  signaturen, inte giltighetsperiod/principals/critical options — det
  ansvaret ligger hos anroparen. Verifierat mot samma RIKTIGA
  `ssh-keygen -s`-certifikat som parsningstesterna, PLUS ett genuint
  manipulationstest (flippar en byte i `publicKey`-fältet på en riktig
  signerad blob, bekräftar att verifieringen då korrekt ger `false` — inte
  bara att den lyckas för det oförändrade fallet, vilket ett buggigt
  "returnera alltid true" också hade klarat). 4 nya tester, 205 gröna
  totalt.
  **Auth-wiring** (2026-07-07): ✅ klart. Ny `SSHAuth.certificate(seed:,
  certificateLine:)` + `HostAuth.certificateFile(keyPath:, certPath:)` +
  `OpenSSHPrivateKey.loadCertificate(keyPath:certPath:)`. `SSHUserAuth`
  erbjuder certifikatet via swift-nio-ssh:s EGNA förstklassiga
  `NIOSSHCertifiedPublicKey`/`NIOSSHUserAuthenticationOffer.Offer.PrivateKey
  (privateKey:certifiedKey:)` — inget eget protokollarbete behövdes, biblioteket
  har redan fullt stöd för att en CLIENT ERBJUDER ett cert. LinuxApp
  (`HostEditView`) + App (Xcode-only) UI: ny "OpenSSH-certifikat"-autentiseringsväg
  med separata sökvägsfält för nyckel + certifikat.
  **Viktigt fynd under verifieringen** (empiriskt bekräftat genom att
  temporärt instrumentera den lånade paketkopian, inte gissat): swift-nio-ssh
  SERVER-rollen kan INTE ta emot certifikatbaserad publickey-auth —
  `readUserAuthRequestMessage()` (`SSHMessages.swift`) kollar det inkommande
  algoritmnamnet mot `NIOSSHPublicKey.knownAlgorithms`, som bara listar de
  fyra RÅA nyckeltyperna (`ssh-ed25519`/`ecdsa-sha2-nistp{256,384,521}`),
  ALDRIG `*-cert-v01@openssh.com`-varianterna — ett sådant erbjudande blir
  tyst `.publicKey(.unknown)` och avvisas INNAN
  `NIOSSHServerUserAuthenticationDelegate` ens nås (bekräftat: en
  handinstrumenterad körning visade att signeringen lyckas klient-sidan,
  men serverns `requestReceived` aldrig anropas). Detta är asymmetriskt —
  CLIENT-rollens erbjudande-stöd är fullständigt, bara SERVER-rollens
  mottagande saknas. Påverkar INTE Bastion i produktion (Bastion är alltid
  CLIENT, aldrig server — en riktig sshd hanterar certifikat fullt ut och
  standardmässigt), men gör att `LoopbackServer`-baserade tester inte kan
  bevisa en fullständig nätverksrundtur. Testerna
  (`OpenSSHCertificateAuthTests.swift`) verifierar istället EXAKT det
  Bastion kontrollerar som klient — att erbjudandet byggs/signeras korrekt
  och innehåller rätt certifikat — och verifierar SEDAN offline (samma
  `NIOSSHCertifiedPublicKey.validate(...)` en riktig sshd använder) att en
  korrekt implementerad server SKULLE acceptera det. 4 nya tester, 213
  gröna totalt.
  **Empiriskt bekräftat mot en RIKTIG `sshd` (2026-07-07, engångsverifiering,
  inte en permanent CI-test)**: `openssh-server` fanns installerat lokalt —
  startade en genuin `sshd` (unprivilegierad port, `TrustedUserCAKeys` mot
  en riktig CA, `sudo` krävdes bara för att binda som root, inget annat) och
  körde Bastions EGEN `SSHSession`/`SSHUserAuth` med `.certificate(seed:,
  certificateLine:)` mot den. Tre fall, alla med korrekt utfall: giltigt
  certifikat (rätt principal, betrodd CA) → `LYCKADES`, fel principal →
  `authenticationFailed`, obetrodd CA → `authenticationFailed`. Upphöjer
  "en riktig sshd SKULLE acceptera det" (ovan, resonerat + offline-
  verifierat) till genuint bevisat, inte bara resonerat. Byggdes inte in
  som permanent test — kräver root-processer + portgissning i CI för
  marginell extra säkerhet utöver de redan offline-verifierade testerna,
  en oproportionerlig skörhetsökning för den vinsten.
- Secure Enclave-bunden nyckellagring (i dag: vanlig Keychain)
- **256-färg + True Color i Linux-terminalen** — ✅ klart. `TerminalBuffer.applySGR`
  hanterade tidigare bara 16-färgspaletten (`SGR 30-37/40-47/90-97/100-107`).
  `SGR 38;5;n`/`48;5;n` (256-färgspaletten: 0-15 standard/ljusa, 16-231 en
  6×6×6-RGB-kub, 232-255 en gråskale-ramp) och `38;2;r;g;b`/`48;2;r;g;b`
  (True Color) tillagt. Krävde att `applySGR` skrevs om från en enkel
  `for`-loop till indexbaserad iteration, eftersom dessa koder konsumerar
  flera efterföljande parametrar atomiskt. Ingen dedikerad testfil finns
  för `TerminalBuffer` (upptäckt under arbetet — en tidigare sammanfattning
  påstod felaktigt 17 testfall; verifierat inte sant), så färgmatematiken
  verifierades manuellt (xterm-referensvärden: 196=röd, 46=grön, 21=blå,
  232/255=gråskale-ändpunkter) + byggd/körd (Xvfb) utan krasch.
  **Testtäckning tillagd (2026-07-08)**: `TerminalBuffer` hade INGEN
  testfil — README:ts "testad, se nedan" pekade på ingenting, och en
  tidigare sammanfattning påstod felaktigt 17 testfall. Nytt testmål
  `LinuxApp/Tests/bastion-guiTests/` (`.testTarget` som `@testable
  import`:ar `.executableTarget`:et direkt, ingen omstrukturering
  behövdes), 42 tester: markörflytt/radbrytning/scroll, CSI-radering
  (J/K, alla lägen), SGR (grundfärger/bold/reset/256-färg/True Color,
  xterm-referensvärdena 196=röd/46=grön/21=blå/232+255=gråskale-
  ändpunkter kodade som riktiga assertions istället för bara manuellt
  verifierade), trasiga/ofullständiga färgsekvenser (ska inte krascha),
  escape-sekvens delad över två `feed()`-anrop. Hittade i processen en
  smal, oadresserad kvirk: `newline()` (ren `\n`) återställer inte
  `cursorCol` (korrekt VT100-beteende i sig — `\n` ska bara flytta
  raden), men om markören redan står i "väntande radbrytning"-läge
  (`cursorCol == cols`, efter att exakt ha fyllt en rad) ger nästa
  tecken en extra oavsiktlig scroll. Träffar sällan i praktiken (riktiga
  SSH-PTY:er skickar nästan alltid `\r\n` ihop, termios `ONLCR`), men
  inte fixat — dokumenterat här istället för en oövervägd
  beteendeändring. `swift test` i `LinuxApp/` kräver samma
  `-rpath-link`-tillägg som `swift build` (se README).
  **Musstöd/ligaturer, undersökt och avfört (2026-07-08)**: BÅDA blockerade
  av samma klass SwiftCrossUI-begränsning. Musstöd: `onTapGesture` ger
  ingen klickposition alls (bara en tom callback), och det finns ingen
  `DragGesture`/scrollhjuls-API i det publika API:t — SGR-musrapportering
  kräver exakt kolumn/rad + tryck/släpp + knappnummer, ingetdera går att
  få ut. Ligaturer: `Font.Resolved.Identifier` har bara EN identifierare
  (`.system`) — inget sätt att välja ett namngivet typsnitt (t.ex. Fira
  Code) via den publika API:n. En kringgång EXISTERAR (`Gtk`-paketet som
  swift-cross-ui redan beror på exponerar `CSSProvider`/rå CSS, skulle gå
  att lägga till som eget beroende i `LinuxApp/Package.swift`), men kräver
  dels att gå runt SwiftCrossUIs abstraktion helt, dels ett overifierat
  antagande (att Pango faktiskt tillämpar `liga`/`calt`-ligaturer på en
  `GtkLabel` utan extra Pango-attributnivå-kod som inte är åtkomlig genom
  Swift-bindningen), dels ett installerat ligatur-typsnitt på målmaskinen
  — bedömt för bräckligt för att bygga blint. Kräver antingen en
  uppströms SwiftCrossUI-funktion (riktig gest-position-API, namngivet
  typsnitt-API) eller ett byte av renderingslager för att göra rätt.
  Terminalfärger i App/ (SwiftTerm) är opåverkade av allt ovan — SwiftTerm
  har redan eget stöd för både musrapportering och ligaturer.
Under Windows-portningen av LinuxApp-vyerna (commit 98f9931) upptäcktes att
`WinUIBackend` (SwiftCrossUI) saknar `BackendFeatures.Sheets` — alla 9
`.sheet()`-baserade popup-vyer i den porterade `ContentView.swift` kraschade
på Windows. Det ledde till en diskussion om vad ett rent Windows-native
alternativ hade sett ut, och slutsatsen blev ett definitivt beslut:
> "Det är bättre att göra ett gediget jobb från början och göra varje
> plattforms klient native. Och sammankopplingen mellan klienterna sker på
> annat sätt än att dela kod. [...] Skriv varje klient efter dess native
> plattform."
**Beslut:**
- `LinuxApp/` (SwiftCrossUI/GTK4) och `WindowsApp/` (SwiftCrossUI/WinUIBackend,
  commit 98f9931-portningen) är borttagna helt, inte frysta som referens.
  `.github/workflows/linux-gui.yml`/`windows-gui.yml` borttagna med samma
  commit (byggde bara de borttagna paketen).
- Ny `WindowsApp/`: C#/.NET + WinUI 3 + SSH.NET, byggs från grunden.
- Ny `LinuxApp/`: Rust + GTK4 (gtk4-rs) + russh/libssh2, byggs från grunden.
- `SSHCore` delas INTE längre av Windows/Linux — samma princip som redan
  gällde `Android/` (Kotlin + Apache MINA SSHD). `SSHCore` förblir kärnan
  bara för `App/` (iOS/macOS), eftersom Swift redan är native där.
- Sammankoppling mellan klienter (host-databas-synk m.m.) byggs vidare på
  befintligt synklager till ett formellt, klientoberoende protokoll/API —
  inte en helt ny tjänst, inte delad UI- eller SSH-kod.
**Varför:** uttalat produktmål (samma session) — inte att minimera kod/arbete,
utan att bygga den klient folk väljer framför alla andra, oavsett plattform,
utan att behöva jonglera flera olika leverantörers klienter för samma syfte.
Ett delat cross-platform UI-ramverk (SwiftCrossUI) släpar efter varje enskild
plattforms verkliga förmågor (Sheets-luckan var bara det senaste exemplet) —
motsatsen till målet.
**Status (2026-07-30):** `LinuxApp/` har nu en riktig grund: Host-datamodell +
HostStore wire-kompatibel med `Sources/SSHCore/Host.swift`/`HostStore.swift`
(verifierat mot en faktisk Swift-genererad `hosts.json`, inte gissat), en
GTK4/libadwaita HostList-UI (lägg till/redigera/ta bort), samt en riktig
SSH-anslutning via `russh` kopplad till en VTE4-terminalwidget — verifierat
end-to-end mot en levande lokal sshd.

**Kända begränsningar i SSH-lagret (dokumenterade i `src/ssh.rs`), näst på tur:**
- Bara `HostAuth::KeyFile` (utan lösenfras), `AgentDefault` (ssh-agent) och
  `AskPassword` stöds. `KeychainKey`/`CertificateFile`/`BitwardenItem` saknar
  Linux-motsvarighet.
- Ingen terminalstorlek-ombindning vid fönsterresize (fast 80x24).

**Klart samma dag, tredje pass: TOFU host-key-verifiering** —
`src/known_hosts.rs` portar `Sources/SSHCore/KnownHosts.swift` rakt av (samma
filformat, `~/.bastion/known_hosts`), `check_server_key` avvisar nu en
ändrad värdnyckel istället för att acceptera allt. Verifierat end-to-end mot
en levande sshd: dels en normal anslutning (lärde/litade på den riktiga
nyckeln, återanvände en post som redan fanns i `~/.bastion/known_hosts` från
tidigare Bastion-bruk på samma maskin — samma fil delas alltså redan
konceptuellt med Swift-sidan), dels en medveten manipulerad known_hosts-post
som korrekt gav ett avslag med ett förklarande felmeddelande.

**Klart samma dag, senare pass:** flikar (`AdwTabView`/`AdwTabBar`, en SSH-
session per flik) + touchscreen-svep mellan dem (`GestureSwipe`,
400px/s-tröskel) — motsvarar iOS MultiSessionView. Flikstängning (manuell
eller fjärrskalets EOF) stänger SSH-anslutningen rent.

**Medvetet UPPSKJUTET, inte glömt** (STATUS 2026-08-03, se nedan för vad som
sedan dess faktiskt byggts): Funktioner-inställningar (Docker valfritt m.m.)
kräver att Docker/Snippets/SFTP/portvidarebefordran-vyerna finns FÖRST i
LinuxApp. Vid skrivandets tillfälle fanns bara HostList+terminal. **Docker-,
Kommandobiblioteks/Snippets- och SFTP-vyerna (inkl. rättigheter/arkiv) är
sedan dess byggda** (se `src/main.rs`: `open_docker_view`/
`open_command_library_view`/`open_sftp_view` m.fl.) — den här texten är
alltså delvis inaktuell historik, inte aktuell status; kvarstår korrekt bara
för portvidarebefordran (se nästa stycke) och själva Funktioner-togglarna
(som fortfarande saknar UI trots att `settings::FeatureToggles` redan har
alla fälten, inklusive `show_port_forward`).

**Portvidarebefordran (lokal, `-L`), backend klar — UI ÄNNU INTE byggt**
(2026-08-03, `LinuxApp/src/port_forward.rs`): ny `spawn_local_forward`
öppnar en `direct-tcpip`-SSH-kanal per inkommen lokal TCP-anslutning via
russh (`channel_open_direct_tcpip` + `Channel::into_stream()` +
`tokio::io::copy_bidirectional`) — motsvarar
`SSHSession.openLocalPortForward` i SSHCore. Delar samma `ssh::connect()`-
hjälpare som redan används av den interaktiva shellen och engångskommandon.
Testat genuint end-to-end: en fristående, minimal `sshd`-instans (egen
konfigfil på en slumpad hög port — läser INTE `/etc/ssh/sshd_config`,
träffas alltså inte av den här kontots `DenyUsers`-restriktion, se
`bastion-cli -J`-verifieringen ovan för samma teknik) + en separat, oberoende
TCP-ekoserver som målet. Klientdata skickad genom den lokala vidarebefordrade
porten kom tillbaka genom HELA kedjan (lokal socket → SSH-kanal → sshd →
ekoserver → samma väg tillbaka), inte en kortsluten loopback-gissning.
1/1 nytt test grönt, 45/45 totalt (8 ignorerade, kräver riktig localhost-
sshd på port 22 specifikt — se `ssh.rs`/`sftp.rs`).
**GTK4-vy tillagd samma dag**: ny "Tunnel"-menypost (`host.forward`,
respekterar `show_port_forward`-togglen) öppnar en flik med fält för lokal
port/målvärd/målport + Starta/Stoppa, kopplad direkt mot
`spawn_local_forward`. Byggd och körd (Xvfb, riktig skärmdump) utan krasch
— samma verifieringsnivå som Docker-/SFTP-/Kommandon-vyerna. Interaktiv
klick-genom-menyn-verifiering (öppna tunnel-fliken på en riktig värd) INTE
gjord än, bara app-start med den nya kodvägen länkad in.
**Fjärr-portvidarebefordran (`-R`), backend + UI klart** (2026-08-03,
`LinuxApp/src/port_forward.rs`): ny `spawn_remote_forward` ber servern
lyssna åt oss via russh `Handle::tcpip_forward` (motsvarar
`SSHSession.openRemotePortForward` i SSHCore). Inkommande
`forwarded-tcpip`-kanaler dirigeras av en ny `ClientHandler::
server_channel_open_forwarded_tcpip` (`ssh.rs`) mot en delad
`RemoteForwards`-karta (port → mål), som slår upp target och bryggar en ny
lokal TCP-anslutning — samma princip som SSHCores `remoteForwards`-fält.
Testat genuint end-to-end mot samma fristående test-`sshd` som `-L`-testet:
en klient som ansluter mot porten SERVERN band åt oss (inte en lokal port vi
själva öppnade) når en oberoende TCP-ekoserver genom hela kedjan. GTK4-vyn
("Tunnel"-fliken) fick en "Riktning"-väljare (Lokal/`-L`, Fjärr/`-R`) som
delar samma fält och kopplar mot rätt bakomliggande funktion via ett nytt
`ActiveForward`-enum (håller `LocalPortForward`/`RemotePortForward` bakom
ett gemensamt handtag). Byggd och körd under Xvfb utan krasch. Interaktiv
klick-genom-menyn-verifiering av riktningsväljaren på en riktig värd INTE
gjord än, bara app-start med den nya kodvägen länkad in — samma
begränsning som redan gällde för `-L`-UI:t.
**Dynamisk portvidarebefordran (`-D`, SOCKS5), backend + UI klart**
(2026-08-03, `LinuxApp/src/socks_proxy.rs`): ny `spawn_dynamic_forward`
startar en lokal SOCKS5-proxy (RFC 1928, bara CONNECT/0x01) — motsvarar
`SSHSession.openDynamicPortForward`/`SOCKSProxy.swift`. Enklare än NIO-
versionen: tokio-strömmars sekventiella `read_exact` behöver ingen egen
pipeline-handler/buffertackumulator för fragmenterade TCP-läsningar.
Testat genuint end-to-end: en riktig SOCKS5-klient (handrullad på byte-
nivå, inte en biblioteksmock) förhandlar mot proxyn, väljer en oberoende
ekoserver SOM MÅL I FARTEN (det som gör "dynamisk" dynamisk, till skillnad
från `-L`/`-R`s fasta mål), och når den genom samma fristående test-`sshd`
som `-L`/`-R`-testerna. GTK4-vyn ("Tunnel"-fliken) fick en tredje
"Dynamisk (-D, SOCKS5)"-post i riktningsväljaren — Målvärd/Målport göms
automatiskt (meningslösa fält när målet väljs per SOCKS-anslutning), kopplad
via samma `ActiveForward`-enum (nu tre varianter: Local/Remote/Dynamic).
Byggd och körd under Xvfb utan krasch. Interaktiv klick-genom-menyn-
verifiering på en riktig värd INTE gjord än, samma begränsning som
`-L`/`-R`-UI:t.

**Portvidarebefordran (`-L`/`-R`/`-D`) i LinuxApp är nu funktionsmässigt
komplett** (bibliotek+UI, 2026-08-03) — matchar SSHCores tre lägen. Kvar
är bara interaktiv verifiering mot en riktig fjärrvärd (inte bara Xvfb-
uppstart) för alla tre.

**Klart samma dag, fjärde pass: formellt synkprotokoll, dokumenterat OCH
implementerat** — se [SYNC_PROTOCOL.md](SYNC_PROTOCOL.md). `LinuxApp/src/
sync.rs` porterar `SyncEngine.merge` + `FolderSyncProvider` från Swift,
`HostStore::sync` kopplar ihop dem. Verifierat med ett riktigt cross-
instans-test: två oberoende `HostStore`-instanser konvergerar via en delad
`FolderSyncProvider`-fil till samma tillstånd. Ingen UI för att välja/
konfigurera en synkmapp än (biblioteksnivå klart, inte ytnivå); krypterade
molntransporter (Dropbox/Drive/OneDrive) inte porterade.

**Klart samma dag, femte pass: `WindowsApp/`-scaffold (C#/.NET + WinUI 3 +
SSH.NET)** — `WindowsApp.csproj` (net8.0-windows10.0.19041.0,
`WindowsAppSDKSelfContained=true`, `Microsoft.WindowsAppSDK` 1.6 +
`SSH.NET`), minimal `App`/`MainWindow` (NavigationView-skal, ingen
HostList/SSH-koppling än — det är nästa steg, inte detta). Verifierat på
den lokala `bastion-winserver`-VM:en (Windows Server 2025, build 26100,
.NET SDK 8.0.423 installerat via winget): **`dotnet build` lyckas rent,
0 varningar/0 fel, första försöket** — självständig deployment bundlar
alla WinAppSDK/WinUI-DLL:er korrekt (verifierat: filerna finns faktiskt i
`bin/.../win-x64/`, inte bara antaget).

**RENDERING VERIFIERAD (2026-07-30, senare pass).** Körning via WinRM
(icke-interaktiv session, eller ens via `schtasks /it` från en
scheduled-task-kontext) ger antingen en krasch
(`Microsoft.UI.Xaml.dll`, `0xc000027b`) eller ett processfönster utan
`MainWindowHandle` — bekräftat samma klass session-isolationsproblem som
touchscreen-verifieringen ([[project-bastion-linuxapp-touchscreen-goal]]).
LÖSNINGEN: koppla upp mot VM:ens redan aktiva RDP-session (`rdp-tcp#0`,
session 2, `xfreerdp3` från denna Linux-värd) och skriva launch-kommandot
DIREKT i en redan öppen interaktiv terminal i den sessionen (via `xdotool`
mot den lokala Xvfb-skärmen som visar RDP-klienten) — INTE via WinRM eller
en scheduled task, båda kör i fel session/window-station trots
`/it`-flaggan. Resultatet: ett riktigt, korrekt renderat `Bastion`-fönster
(NavigationView + "Värdar"-menyalternativ + platshållartexten "Ingen
session öppen — värdlistan är inte kopplad in än." — exakt `MainWindow.xaml`
som skriven), skärmdump tagen som bevis, processen städad och den
tillfälliga `schtasks`-uppgiften borttagen efteråt.

**Klart samma dag, sjätte pass: `LinuxApp` Docker-vy** — port av
`Sources/SSHCore/DockerService.swift` (`docker.rs`: validering mot
shell-injektion, kommandobyggare, parsning — alla 5 Swift-testfallen
portade rakt av och gröna). UI: en "Docker"-flik per värd (öppnas via
menyn på värdraden) med containerlista, start/stopp/omstart/loggar/shell
per rad. "Shell" återanvänder den befintliga terminal-infrastrukturen
(`startup_command` skickar `docker exec -it ...` automatiskt in i en ny
flik) — ingen ny SSH-kod behövdes.

Ny `ssh::run_command` (engångs-exec utan pty, delar `connect()`-hjälpen
med den interaktiva shell-sessionen). Verifierat mot RIKTIGA levande
Docker-containrar på utvecklingsmaskinen (plex/maintainerr/fetcher/
watchtower) — men ENDAST läsande (`docker ps`), aldrig start/stopp/
omstart mot riktiga containrar i ett test.

**Klart samma dag, sjunde pass: `LinuxApp` Funktioner-inställningar
(Docker-delen)** — port av `Sources/SSHCore/AppSettings.swift`
(`settings.rs`, `~/.bastion/settings.json`, samma sex fält/wire-format som
Swift, inklusive `showSFTPBrowser`s versala SFTP-akronym som avviker från
serdes automatiska camelCase — verifierat mot en riktig `swift`-körning).
En inställningsknapp i sidopanelen öppnar en dialog med en Docker-toggle;
avstängd döljer den "Docker"-menyposten på alla värdrader direkt. De
övriga fem fälten (Snippets/Kommandobibliotek/SFTP/portvidarebefordran/
SSH-nyckeldistribution) round-trippar korrekt genom filen men saknar UI
än — de har ingen vy att gömma i LinuxApp (se ovan). Detta uppfyller det
uttryckligen namngivna kravet "Docker måste vara valfritt".

**Klart samma dag, åttonde pass: `LinuxApp` Kommandobibliotek + Snippets +
Funktioner-togglen för det** — port av `Sources/SSHCore/CommandLibrary.swift`
(`command_library.rs`, alla 30 statiska referenskommandon Docker/Linux/Git/
Cloudflare/Tailscale/WireGuard/systemd) + `Snippet.swift`/`SnippetStore.swift`
(`snippet.rs`, `~/.bastion/snippets.json`, `{{variabel}}`-rendering). UI: en
"Kommandon"-flik per värd listar båda; "Kör" fyller i variabler via en
dialog om det behövs, annars öppnar direkt en ny terminalflik med
kommandot som `startup_command` — samma återanvända mönster som
Docker-shell, ingen ny SSH-kod. Egna snippets går att lägga till/redigera/
ta bort. Funktioner-inställningen har nu även en Kommandobibliotek-toggle.

**Explicit verifierat denna session (adresserar tidigare påpekande):**
"exit måste avsluta sessionen" — ett nytt riktigt test
(`ssh::tests::typing_exit_in_the_shell_closes_the_session`) skriver
faktiskt `exit\n` i en levande SSH-shell och verifierar att
`SshEvent::Closed` kommer tillbaka (vilket `start_session` i `main.rs`
redan reagerar på genom att stänga fliken) — inte bara antaget från
tidigare commits.

**Klart samma dag, nionde pass: `LinuxApp` SFTP-bläddrare** — port av
App/SFTPBrowserModel.swifts kärnfunktioner via `russh-sftp`-cratet ovanpå
den befintliga `ssh::connect()`-hjälpen. `sftp.rs`: en bakgrundstråd med en
enda återanvänd `SftpSession` (samma `ensureClient()`-cache-princip som
Swift), kommandon via kanal (list/read/write/mkdir/remove_file/remove_dir/
rename). UI: "Filer"-flik per värd, mapp-först-sortering, textredigering
med binär-innehåll-detektering (UTF-8-avkodningsfel → skrivskyddad
platshållartext, samma säkerhetsmarginal som Swiftsidans `isBinary`-fält —
sparaknappen kan aldrig råka skriva över binärt innehåll med text). Ny
Filer-toggle i Funktioner-inställningen.

**Fälla hittad och fixad under verifiering:** `SftpSession::write()`
öppnar bara med `OpenFlags::WRITE` — misslyckas med "No such file" på en
NY fil, till skillnad från Swiftsidans `SFTPClient.writeFile` som alltid
kan skapa. Egen `write_file()`-hjälp med `CREATE|TRUNCATE|WRITE` löser det.

**Medvetet uppskjutet** (dokumenterat i `sftp.rs`, inte dolt): chmod/chown/
komprimera/packa upp (Swiftsidans `chmod`/`chown`/`compress`/`extract`) —
bara lista/navigera/ladda upp/ladda ner/ta bort/mkdir/döp om är porterat.

Verifierat END-TO-END mot en levande sshd, inte bara byggt: ett komplett
integrationstest (`full_round_trip_against_a_real_sftp_server`) kör mkdir
→ write → read → list → rename → remove_file → remove_dir i en egen
engångsmapp under `/tmp`, rör aldrig något annat på testmaskinen.

**Klart samma dag, tionde pass: `WindowsApp`-rendering verifierad** — se
uppdaterad status ovan i "Scaffolda ny WindowsApp/"-avsnittet. Nästa steg
för Windows-sidan är nu funktionsutveckling, inte längre toolchain-
verifiering.

**Klart samma dag, elfte pass: `WindowsApp` HostStore/HostList** — ny
`Bastion.Core` (plain net8.0-bibliotek, ingen WinUI-koppling): port av
`Host.swift`/`HostStore.swift`/`SyncEngine.swift`/`SyncProvider.swift` till
C#, samma wire-format som redan verifierat i `LinuxApp/src/host.rs`
(ReferenceDate-epok, HostAuth-kodning, platt tombstones-array). 11 xUnit-
tester körs DIREKT på Linux-utvecklingsmaskinen (separat .NET SDK
installerat lokalt) — ingen VM-omväg för denna del av verifieringen.
En riktig bugg hittades och fixades under testning: `ReferenceDate`
saknade `[JsonConverter(...)]`-attributet på själva typen. Cross-
instans-synktestet (`FolderSyncProvider`) portades också — SAMMA test
som Rust-sidan, nu i ett TREDJE språk, verifierar att synkprotokollet
verkligen är klientoberoende.

`MainWindow`: riktig HostList (`ListView` bunden mot `HostStore.All()`),
"Lägg till värd" via en native `ContentDialog`. Verifierat VISUELLT
end-to-end via samma xfreerdp+xdotool-teknik som render-verifieringen
([[reference-windows-vm-interactive-render-verification]]): lade till en
värd genom UI:t, den dök upp i listan, klick visade rätt platshållartext,
och `~/.bastion/hosts.json` på VM:en innehöll exakt rätt wire-format —
inte bara byggt, faktiskt kört och sett fungera.

**Klart samma dag, tolfte pass: `WindowsApp` SSH.NET-anslutning + riktig
terminal — FULLSTÄNDIGT visuellt end-to-end-verifierad.** Ny `SshSession`
(Bastion.Core, SSH.NET är rent C# — portabel, testad direkt på Linux mot
en riktig sshd, 15 xUnit-tester inkl. exit-stänger-sessionen via
`ShellStream.Closed` och TOFU-avvisning av ändrad värdnyckel). Terminalen
renderas med xterm.js i en `WebView2` (vendrat lokalt i `Assets/xterm/`,
ingen CDN) — WinUI 3 har ingen inbyggd VTE-motsvarighet.

Verifierat: klick på en riktig värd i `WindowsApp` kopplade upp mot en
levande sshd på värdmaskinen (via VM:ens virbr0-gateway, `192.168.122.1`),
visade en RIKTIG `berduf@mp100:~$`-prompt i xterm.js, tog emot inskrivet
`echo hello-from-windowsapp-terminal` och fick tillbaka rätt utdata, och
`exit` stängde sessionen rent ("Sessionen avslutades"). Hela kedjan —
tangentbord → `window.chrome.webview.postMessage` → `ShellStream.Write` →
riktigt fjärrskal → `ShellStream.DataReceived` → `window.feed(...)` →
xterm.js — fungerar. Detta var INTE bara byggt utan faktiskt kört och sett
fungera, med skärmdumpar som bevis (samma xfreerdp+xdotool-teknik som
tidigare render-verifiering). All testinfrastruktur (temporär nyckel i
authorized_keys, testfil på VM:en) städad efteråt.

KÄND BEGRÄNSNING (dokumenterad i SshSession.cs): bara nyckelfil (utan
lösenfras) och lösenord — SSH.NET saknar agent-protokollstöd, så
HostAuth.AgentDefault är inte porterat till WindowsApp. UI:t har heller
ingen auth-typväljare än (alla nya värdar skapas med AgentDefault, som
alltså inte fungerar i WindowsApp ännu — nästa steg).

**Klart samma dag, trettonde pass:** `WindowsApp` fick en auth-typväljare
i "Lägg till värd" (ComboBox: Nyckelfil/Lösenord — de enda två som
faktiskt fungerar, AgentDefault döljs medvetet eftersom SSH.NET saknar
agent-protokollstöd). Utan denna fix skapades nya värdar tyst med en
obrukbar auth-typ.

`LinuxApp` fick synk-UI: en ny `SyncConfig` (klientlokal — `~/.bastion/
sync-config.json`, medvetet SKILD från det delade `SyncState`-protokollet
eftersom vilken mapp man synkar mot är en per-enhet-inställning, inte
data att slå ihop). Inställningsdialogen har nu en "Synk"-sektion: "Välj
mapp…" (nativ `GtkFileDialog`) + "Synka nu" som kör `HostStore::sync` mot
en `FolderSyncProvider` i den valda mappen. Biblioteket var redan
verifierat (cross-instans-konvergenstestet); detta kopplar bara in ytan.
6 enhetstester (1 ny: `sync_config_round_trips_through_disk`), cargo
build/test rent, headless xvfb-run-körning utan krasch.

**Klart samma dag, fjortonde pass: `WindowsApp` Synk-UI — VISUELLT
end-to-end-verifierad.** `SyncConfig.cs` (Bastion.Core, 2 xUnit-tester) +
en ny Synk-knapp i sidopanelen öppnar en dialog med "Välj mapp…" (riktig
Windows `FolderPicker` via HWND-interop, `WinRT.Interop.WindowNative`) +
"Synka nu" (`HostStore.Sync` mot en `FolderSyncProvider`).

Verifierat via samma xfreerdp+xdotool-teknik: klickade Synk-knappen, en
RIKTIG native Windows-mappväljare öppnades, valde en testmapp, sökvägen
sparades och visades, "Synka nu" gav "Synkad", och den skrivna
`hosts.json` i testmappen hade exakt rätt `SyncState`-wire-format
(`{"hosts":[],"tombstones":[]}`) — samma protokoll som LinuxApp/Swift.
Testmapp + sync-config.json städade efteråt.

**Klart samma dag, femtonde pass: `LinuxApp` SFTP-utökningar
(chmod/chown/komprimera/packa upp)** — port av
`Sources/SSHCore/ArchiveOperations.swift` (`archive.rs`: samma
shell-citeringslogik, alla 6 Swift-testfallen portade rakt av inklusive
den RIKTIGA shell-injektionsverifieringen via `/bin/sh -c`) + chmod/chown
via `russh-sftp`s `FileAttributes`/`set_metadata` (`sftp.rs`).

UI: varje SFTP-rad fick en "Rättigheter/ägare"-knapp (oktalt läge +
UID/GID-fält), mappar fick en "Komprimera"-knapp (tar.gz), och
`.tar.gz`/`.tgz`/`.zip`-filer fick en "Packa upp"-knapp. Komprimera/
packa upp shellar ut till tar/zip via `ssh::run_command` (SFTP v3 har
ingen egen arkivsemantik) — infrastrukturen krävde en refaktorering
(`SftpContext`-struct som buntar handtag+host+lösenord) så
engångskommandona kan köras vid sidan av den öppna SFTP-sessionen.

Verifierat END-TO-END mot en levande sshd, inte bara byggt: chmod
verifierat via ett OBEROENDE `stat`-anrop (inte bara att SFTP-anropet
returnerade Ok), och ett komplett komprimera→ta-bort-original→packa-upp-
test som bevisar att filINNEHÅLLET faktiskt överlever hela resan.
37 enhetstester (7 nya: 6 archive.rs + chmod/chown + arkiv-roundtrip),
cargo build/test rent, headless xvfb-run-körning utan krasch.

**Klart 2026-07-30, sextonde pass: krypterade molntransporter (AES-256-GCM)
i alla TRE klienter, cross-språksverifierade byte-för-byte.**
`sync_crypto.rs` (LinuxApp) och `SyncCrypto.cs` (`WindowsApp/Bastion.Core`,
med .NETs inbyggda `Rfc2898DeriveBytes`/`AesGcm` — inga nya paket) portar
`Sources/SSHCore/SyncCrypto.swift`: PBKDF2-HMAC-SHA256 + AES-256-GCM,
samma "BSYNC1"-kuvert (magic + iterationer + salt + nonce||chiffertext||
tagg). `EncryptedFolderSyncProvider` i alla tre språk.

UI: både LinuxApp (`adw::SwitchRow` + `PasswordEntryRow` i synk-dialogen)
och WindowsApp (`ToggleSwitch` + `PasswordBox`) fick en "Kryptera"-växel
som byter `sync_now`-anropet mellan `FolderSyncProvider` (hosts.json,
klartext) och `EncryptedFolderSyncProvider` (hosts.enc, krypterad) —
rätt för en molnmapp (Dropbox/Drive/OneDrive) man inte litar på blint.

Cross-språksverifiering: körde RIKTIGA test mot varandras faktiska
kuvert i alla tre riktningar (Swift↔Rust, Rust↔C#) — genererade en
sealed blob med ett språks verkliga testkörning och läste den med ett
annat språks verkliga kod, inte bara "formatet ser rätt ut". Alla
tillfälliga cross-språk-testmetoder borttagna igen efter körning.
26 C#-tester (7 nya i `SyncCryptoTests.cs`), 44 Rust-tester (cargo
build/test rent), 283 Swift-tester (oförändrat, `swift test` rent).

WindowsApp-sidan VISUELLT verifierad också: `bastion-winserver`-VM:en
(mp100, 192.168.122.42) var faktiskt igång, koden synkades dit (WinRM +
base64-filöverföring, ingen git-klon där), `dotnet build
WindowsApp.csproj -r win-x64` byggde rent (kräver explicit RID för
`WindowsAppSDKSelfContained`, annars `error: requires a supported
Windows architecture` — miljöbegränsning, inte en kodbugg), och via
xfreerdp3+Xvfb+xdotool mot VM:ens redan aktiva RDP-session klickades
Synk-knappen, "Kryptera"-växeln slogs på LIVE och lösenfrasfältet dök
upp med texten "Dropbox/Drive/OneDrive — AES-256-GCM" — identiskt
beteende med LinuxApp:s `SwitchRow`/`PasswordEntryRow`. Processen
stoppades och VM-tillståndet återställdes efteråt.

Detta var den sista punkten på den ursprungliga "kvarstående"-listan —
krypterade molntransporter är nu en riktig, visuellt verifierad
användarfunktion i alla tre klienter (Swift/App, Rust/LinuxApp,
C#/WindowsApp).

**WindowsApp: TabView-flikskal + Docker-flik + Kommandobibliotek/Snippets-flik
(skrivet, kod-genomläst, INTE visuellt verifierat än).** `MainWindow` bytt
från en enkel-session-panel (platshållare ↔ EN terminal) till ett riktigt
`TabView`-flikskal (en flik per session/vy, stängbar), motsvarande LinuxApps
`AdwTabView`/iOS `MultiSessionView` — en förutsättning som saknades innan
Docker/Kommandon kunde portas utan att skapa en synligt annorlunda
interaktionsmodell än de andra klienterna.

- `DockerService.cs` (Bastion.Core): port av `DockerService.swift`/`docker.rs`
  — samma injektionsskydd, kommandobyggare, parsning. 21 nya tester.
- `Snippet.cs`/`SnippetStore.cs`/`CommandLibrary.cs` (Bastion.Core): port av
  `Snippet.swift`/`SnippetStore.swift`/`CommandLibrary.swift` (samma
  `{{variabel}}`-rendering som `snippet.rs`). 12 nya tester.
- `MainWindow`: värdradens "Mer"-meny fick "Docker"/"Kommandon". Docker-fliken
  (containerlista, start/stopp/omstart/loggar/shell) och Kommandon-fliken
  (snippets + inbyggt bibliotek, "Kör" med variabelifyllningsdialog om
  mallen har `{{variabler}}`, ny/redigera/ta bort-snippet) byggs imperativt
  i C# (samma stil som LinuxApps `main.rs`, inte XAML-databindning) —
  medvetet, eftersom referensimplementationen själv är imperativ.
- 56/56 `dotnet test` gröna (Bastion.Core + Bastion.Core.Tests).

**EJ verifierat**: `dotnet build` av själva `WindowsApp.csproj` kräver
Windows (WinUI:s XAML-kompilator, `XamlCompiler.exe`, är en Windows-binär —
`Exec format error` bekräftat vid försök på denna Linux-utvecklingsmaskin).
Kräver `bastion-winserver`-VM:en (se ovan, `192.168.122.42`,
`dotnet build WindowsApp.csproj -r win-x64` + xfreerdp3+xdotool-tekniken)
för att bekräfta att XAML:en faktiskt kompilerar och renderar rätt.

Kvar för full WindowsApp-paritet med LinuxApp: SFTP-bläddrare (inkl.
chmod/chown/arkiv), Funktioner-inställningar (dölj flikar via toggles),
fler HostAuth-typer (SSH.NET saknar agent-protokoll). "Rå tangentbordsinput"
togs tidigare upp här som en lucka men är det inte — `Assets/xterm/
terminal.html` använder redan xterm.js` `onData`-API, som internt
producerar KORREKTA byte-sekvenser för piltangenter/Ctrl-kombinationer/
funktionstangenter (samma väletablerade terminal-emulator som VS Code/
Hyper använder), inte en handskriven `onkeydown`-hantering som riskerar
att missa specialtangenter.

**WindowsApp: SFTP-bläddrare, grundpasset (skrivet, INTE visuellt
verifierat).** Port av `App/SFTPBrowserModel.swift`/`LinuxApp/src/sftp.rs`
— men ovanpå SSH.NETs INBYGGDA `SftpClient` istället för en egen SFTP-
protokollimplementation (Swift/Rust hade ingen färdig SFTP-klient att
återanvända, C#-sidan har det redan via samma SSH.NET-paket som redan
används för terminal/Docker/kommandon).

- `SftpBrowserSession.cs` (Bastion.Core): en öppen `SftpClient`-anslutning
  återanvänds för hela bläddringen (motsvarar Swiftsidans
  `ensureClient()`-cache / Rusts en-bakgrundstråd-per-flik — men
  SSH.NETs synkrona API behöver ingen egen tråd/kanal-aktör).
  List/ReadFile/WriteFile/CreateDirectory/RemoveFile/RemoveDirectory/Rename
  + `TryDecodeUtf8` (strikt UTF-8-validering, samma säkerhetsmarginal som
  Swiftsidans `isBinary`-fält — binärt innehåll ska aldrig tyst tolkas
  som text och riskera att sparas över). TOFU-verifieringen delas nu med
  `SshSession` via en gemensam `MakeHostKeyHandler`-hjälpare (refaktorering,
  ingen betydelseändring) eftersom `SshClient`/`SftpClient` båda ärver
  samma `BaseClient`-event.
- 3 nya tester (`TryDecodeUtf8`, som körs utan nätverk) + 2 nya
  SSH-gated integrationstester (samma `BASTION_TEST_SSH_KEY`-mönster som
  `SshSessionTests`, hoppas över utan nyckel).
  **Kunde INTE köras på riktigt i den här miljön**: `claude`-Linux-kontot
  på mp100 har `DenyUsers claude` i `sshd_config` — SSH till localhost är
  en avsiktlig, permanent gräns för det kontot (se
  [[feedback_claude_account_no_docker_no_berduf_home]]), inte en
  miljöbugg. En tillfällig testnyckel skapades, verifierades blockerad,
  och togs bort igen.
- `MainWindow`: "Filer"-menyalternativ, en flik per värd (upp/ny mapp/
  uppdatera-verktygsfält + lista, klick navigerar mapp eller öppnar en
  textredigerare, döp om/ta bort per rad) — samma UX som LinuxApps
  grundpass (utan chmod/chown/arkiv än, se ovan).
- 62/62 `dotnet test` gröna totalt (Bastion.Core + Bastion.Core.Tests).

**EJ portat i denna pass** (LinuxApps femtonde pass, senare arbete):
chmod/chown, komprimera/packa upp. SSH.NETs `SftpClient` saknar en
`GetCanonicalPath`/realpath-motsvarighet i den här versionen (verifierat
via reflektion mot den faktiska DLL:en, inte antaget) — behöver en annan
lösning (t.ex. ett engångskommando via `SshSession.RunCommand("pwd")`)
den dagen komprimera/packa-upp portas, som i sin tur (liksom i Rust/Swift)
shellar ut till tar/zip snarare än att använda SFTP-protokollet direkt.

**WindowsApp: Funktioner-inställningar (skrivet, INTE visuellt
verifierat).** Port av `AppSettings.swift`/`settings.rs`
(`FeatureToggles`, `~/.bastion/settings.json`) — samma fältnamn/wire-
format som Swift (`showSFTPBrowser` versalt, verifierat: System.Text.Json
gör ingen egen camelCase-omskrivning av ett redan explicit satt
`[JsonPropertyName]`, så ingen motsvarighet till Rusts `serde(rename)`-
fälla behövdes här). 4 nya tester.

Värdradens "Mer"-knapp bygger nu menyn DYNAMISKT i C# (istället för en
statisk `MenuFlyout` i XAML) utifrån aktuella togglar — motsvarar
LinuxApps `gio_menu_for`, som utesluter hela menyposten när en toggle är
av, inte bara döljer/inaktiverar en statisk post. Ny Funktioner-knapp
("⚙") i värdlistans verktygsfält öppnar en inställningsdialog med
Docker/Kommandobibliotek/Filer-togglar (portvidarebefordran/SSH-
nyckeldistribution har ingen vy att gömma än i WindowsApp, samma
avgränsning som LinuxApp dokumenterar).

63/63 `dotnet test` gröna totalt (Bastion.Core + Bastion.Core.Tests).

**WindowsApp: SFTP chmod/chown/arkiv (skrivet, INTE visuellt
verifierat).** Stänger gapet mot LinuxApps femtonde pass.

- `ArchiveOperations.cs` (Bastion.Core): port av `ArchiveOperations.swift`/
  `archive.rs` — `ShellQuote` (POSIX-shell-säker citering, samma
  RIKTIGA shell-injektionsverifiering som Rust-testet: kör faktiskt
  `/bin/sh -c` mot en konstruerad injektionssträng) + kommandobyggare för
  tar.gz/zip. 6 nya tester.
- `SftpBrowserSession.SetPermissions`/`SetOwner` (redan skrivna i
  grundpasset) kopplas nu in i UI:t via en "Rättigheter/ägare"-dialog
  (oktalt läge + UID/GID-fält).
- Komprimera/packa upp shellar ut via `SshSession.RunCommand` (SFTP v3 har
  ingen egen arkivsemantik) — en ny `ResolveRealPath`-hjälpare
  (`cd <väg> && pwd` över en engångs-exec-kanal) ersätter Swift/Rusts
  `SFTPClient.realpath`, eftersom SSH.NETs `SftpClient` saknar den
  motsvarigheten (bekräftat via reflektion tidigare i sessionen).
- UI: varje SFTP-rad fick en rättigheter-knapp, mappar fick en
  "Komprimera"-knapp, `.tar.gz`/`.tgz`/`.zip`-filer fick en
  "Packa upp"-knapp — matchar LinuxApps radlayout.

69/69 `dotnet test` gröna totalt.

**WindowsApp: hela ovanstående (SFTP/Funktioner/Docker/Snippets/
Kommandobibliotek/arkiv + multi-flik-UI) hämtat in, granskat, fixat och
delvis verifierat** (2026-08-04). Låg tidigare oreviderat och otestat på
en parkerad lokal gren (`wip/windowsapp-csharp`, aldrig pushad) — inget av
det ovan var faktiskt bekräftat innan den här omgången.

- **Bastion.Core-lagret (SftpBrowserSession/AppSettings/DockerService/
  Snippet/CommandLibrary/ArchiveOperations)**: byggt och `dotnet test`-
  verifierat GRÖNT (82/82), inklusive FYRA genuina end-to-end-tester mot
  en riktig, fristående `sshd`-process (ny `TestSshd`-hjälpare, samma
  teknik som `LinuxApp/src/port_forward.rs::TestSshd` — kringgår samma
  `DenyUsers claude`-spärr som redan dokumenterats för `ssh.rs`/`sftp.rs`s
  ignorerade tester). Fixat INNAN commit (fanns i den parkerade koden,
  aldrig tidigare granskat): `AppSettingsStore`/`SnippetStore` skrev
  icke-atomärt och kollapsade en korrupt fil till tysta standardvärden —
  samma mönster som redan fixats i `HostStore`/LinuxApp:s host.rs/
  settings.rs tidigare i PR #216, applicerat här också nu (ny delad
  `FsUtil.AtomicWrite`).
- **`MainWindow.xaml`/`.xaml.cs`-UI-lagret**: skrivet om till en riktig
  multi-flik-arkitektur (`TabView`, en `WebView2`/session per flik —
  ersätter den gamla enda-delad-`WebView2`-modellen helt, se `git log`
  för den commit som gjorde det). Manuellt granskad rad för rad (kan INTE
  byggas/köras här — `XamlCompiler.exe` är en Windows-binär, bekräftat
  `Exec format error` vid försök). Fyra buggar hittade och fixade under
  granskningen (ingen av dem tidigare känd/dokumenterad, upptäckta HÄR):
  1. "Synka nu" körde `_store.Sync(...)` synkront på UI-tråden — samma
     GTK/WinUI-frysningsbugg som redan fixats i LinuxApp/den gamla
     MainWindow.xaml.cs. Flyttad till `Task.Run`.
  2. Terminalflikens `EnsureCoreWebView2Async`/`Navigate` hade inget
     try/catch och läste aldrig `NavigationCompletedEventArgs.IsSuccess`
     — samma bugg som redan fixats i den gamla enda-session-koden,
     återintroducerad av omskrivningen. Samma fix (try/catch/finally,
     kollar IsSuccess) applicerad igen.
  3. En session som stängdes NORMALT (fjärrskalet avslutade sig självt,
     inte användaren som stängde fliken) disponerade ALDRIG den
     underliggande `SshSession`/socketen trots en kommentar som påstod
     motsatsen — en riktig resursläcka per avslutad session. Fixat:
     `OnSessionClosed` disponerar nu explicit, med en `ReferenceEquals`-
     kontroll mot `tab.Tag` (nollställs av BÅDA vägarna) som skydd mot en
     dubbeldisponering-race mot `OnTabCloseRequested`.
  4. Fyra av sex SFTP-skrivåtgärder i Filer-fliken (ny mapp/döp om/
     rättigheter/ta bort) körde SSH.NET-anropen direkt på UI-tråden
     istället för via `Task.Run` — bara läsa/skriva/lista/komprimera/
     packa-upp gjorde det redan rätt. Samma inkonsekvens-mönster som #1,
     fixat på alla fyra ställen.

**WindowsApp: ssh-agent-autentisering (`HostAuth.AgentDefault`) —
stänger den sista HostAuth-luckan.** SSH.NET har INGET inbyggt
agent-protokollstöd (bekräftat via reflektion mot den faktiska DLL:en
tidigare i sessionen) — `SshAgent.cs` (Bastion.Core) implementerar
draft-miller-ssh-agent-tråd­protokollet från grunden istället för att
vänta på ett tredjepartsbibliotek:

- `SshAgentClient`: ansluter via `SSH_AUTH_SOCK` (Unix-socket) eller,
  om den saknas och OS är Windows, `\\.\pipe\openssh-ssh-agent`
  (Win32-OpenSSHs standard-named-pipe). `RequestIdentities()`/`Sign()`
  över samma 4-byte-längdprefix-ramning som `SSHAgentClient.swift`/
  russhs agentklient. Bara Ed25519-identiteter listas (samma
  prioritering som resten av projektet) — RSA/ECDSA i agenten hoppas
  tyst över, ingen agentmix-krasch.
- `AgentHostAlgorithm`/`AgentPrivateKeySource`: kopplar in agenten i
  SSH.NET via `Renci.SshNet.Security.HostAlgorithm`/`IPrivateKeySource`
  — signeringen delegeras till agentprocessen, den privata nyckeln
  lämnar den aldrig.
- `SshSession.BuildAuthenticationMethod`: ny `HostAuth.AgentDefault`-
  gren bygger EN `AgentPrivateKeySource` PER identitet agenten har
  laddad och skickar alla till `PrivateKeyAuthenticationMethod`, som
  provar dem i tur och ordning — samma "provar ALLA laddade
  identiteter, inte bara den första"-beteende som redan gäller för
  LinuxApp/`ssh.rs` (en tidigare CodeRabbit-fix i den här sessionen).
  Tydliga svenska felmeddelanden om ingen agent hittas eller agenten
  saknar Ed25519-identiteter.
- Verifiering: 3 nya tester (`SshAgentTests.cs`) mot en RIKTIG,
  självstartad `ssh-agent`-process (`ssh-agent -a <sockel>` + `ssh-add`,
  ingen mock) — inklusive den avgörande `openssl pkeyutl -verify`-
  kontrollen, som HELT OBEROENDE av SSH.NET/min egen kod bevisar att
  agentens signatur faktiskt är giltig Ed25519 över exakt den skickade
  datan (och att den INTE verifierar mot manipulerad data). 85/85
  `dotnet test` gröna totalt (Bastion.Core + Bastion.Core.Tests).

Detta stänger "fler HostAuth-typer"-luckan från föregående pass —
WindowsApp har nu paritet med LinuxApp/App på alla tre auth-typer
(nyckelfil, lösenord, ssh-agent).

**Kvar**: det enda som faktiskt kräver Windows-hårdvara/en VM för att
stänga helt är en RIKTIG kompilering+körning av
`MainWindow.xaml`/`.xaml.cs` (WinUI3s XAML-kompilator finns bara på
Windows). Med Bastion.Core-lagret verifierat och UI-lagret manuellt
granskat rad för rad är risken låg, men "manuellt granskad" är inte
samma sak som "körd".
