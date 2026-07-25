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
| Krypterad nyckel (lösenfras) + RSA/ECDSA | ⬜ nästa steg (kastar tydligt fel nu) |
| Exec + strömmad stdout/stderr | ✅ testad |
| Exitkod-hantering | ✅ |
| Misslyckad auth utan att hänga | ✅ testad |
| Interaktiv shell + PTY (stdin/stdout, resize) | ✅ testad end-to-end |
| known_hosts / TOFU (SHA256-fingeravtryck, MITM-skydd) | ✅ testad, `~/.bastion/known_hosts` |
| ssh-config-parsing (`Host`-alias, jokertecken, `IdentityFile`) | ✅ testad, CLI slår upp alias |
| Host-databas (JSON, taggar, CRUD) | ✅ testad, `~/.bastion/hosts.json` |
| Dashboard-data (last/minne/disk/uptime/OS/Docker via SSH) | ✅ parser testad, ett kommando |
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
| Linux-GUI (`bastion-gui`, SwiftCrossUI/GTK4) | ✅ byggd och körd (Xvfb) + egen CI-lane (`linux-gui.yml`, required check) |
| Linux-terminal (VT100/ANSI-tolk, bestående PTY-shell) | ✅ 42 fristående parser-tester gröna (`LinuxApp/Tests/`), körd (Xvfb) — radvis input, inget musstöd (ingen rå key-/gest-position-API i SwiftCrossUI) |
| Linux-Docker-hantering (`DockerView`) | ✅ lista/start/stopp/omstart/logg/shell — motsvarar `App/DockerView.swift` |
| Portvidarebefordran (`PortForwardView`) | ✅ lokal/fjärr/dynamisk, starta/stoppa — LinuxApp (byggd+körd, Xvfb) OCH App/ (2026-07-08, Xcode-only) |
| ProxyJump (`ssh -J`) | ✅ `SSHSession.connect(via:)`, `bastion-cli` läser `ProxyJump` ur ssh-config automatiskt |
| WireGuard-profiler | ✅ parsning/serialisering + lagring — LinuxApp OCH App/-UI (2026-07-08, Xcode-only) |
| OpenSSH-certifikat | ✅ parsning + CA-signaturverifiering + `SSHUserAuth`/`HostAuth`-wiring (`.certificateFile`) — testad mot RIKTIGA `ssh-keygen -s`-certifikat, LinuxApp+App-UI klar |
| ssh-agent-protokollklient | ✅ `SSHAgentClient.swift`, testad mot en RIKTIG `ssh-agent` — 🚫 kanal-forwarding till fjärrserver BLOCKERAD (se ROADMAP) |
| Tailscale-värdförslag | ✅ `TailscaleStatus.swift` (fetch/fetchLocal) — LinuxApp OCH App/-UI (2026-07-08, Xcode-only; `fetchLocal` villkorsstyrd bort på iOS, `Foundation.Process` saknas där) |
| S3-kompatibel objektlagring | ✅ `S3Client.swift` + `S3ConnectionStore` — LinuxApp OCH App/-UI (2026-07-08, Xcode-only) |

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
   **Nästa steg nu när bygget faktiskt går grönt**: porta de riktiga
   vyerna från `LinuxApp/Sources/bastion-gui/` hit och testa på riktigt
   (VPS eller motsvarande) — inte påbörjat än.
4. Riktig rå tangentbordsinmatning i Linux-terminalen (kräver att gå under
   SwiftCrossUI mot GTK:s event-controllers direkt — se "Uppskjutet med avsikt").

## Klart

- **ProxyJump (`ssh -J`)** (2026-07-06, `SSHSession.swift`): `connect(via
  jump:)` — istället för en ny TCP-anslutning öppnas en `direct-tcpip`-kanal
  FRÅN en redan uppkopplad jump-session till målet, och en helt egen,
  oberoende SSH-handskakning (eget `NIOSSHHandler`/`SSHUserAuth`/TOFU) körs
  direkt ovanpå den kanalen — "SSH i SSH", samma mönster som en riktig
  `ssh -J` på trådnivå.
  - `bastion-cli` läser `ProxyJump` ur `~/.ssh/config` automatiskt (fältet
    parsades redan sedan tidigare, `ResolvedHost.proxyJump`, men var aldrig
    kopplat till en riktig anslutning förut) — inget eget `-J`-flagg på
    kommandoraden än. Jump-hoppet återanvänder samma autentisering
    (miljövariabler/nyckelfråga) som huvudmålet (v1-förenkling).
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
    (Xvfb), rent utan krasch.
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
  SwiftCrossUI saknar rå tangentbords-API, så inmatning är radvis via
  `TextField` + Enter; piltangenter/Home/End/PgUp/PgDn/Tab/Esc/Ctrl+C/Ctrl+D
  finns som knappar och skickas som rå bytes direkt (navigering i t.ex.
  `htop`/`less` fungerar, löpande texttangenttryckning gör det inte). Fast
  100×30 storlek — ingen fönsterstorleks-driven `resize()` mot PTY:n än.
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

- **Krypterade nycklar (lösenfras) + RSA/ECDSA.** OpenSSH krypterar med
  `bcrypt_pbkdf` (Blowfish-baserad) + `aes256-ctr`. Varken bcrypt_pbkdf eller
  AES-CTR finns i swift-cryptos publika API, så det kräver egna implementationer
  av Blowfish + bcrypt_pbkdf + AES-CTR — säkerhetskritisk kod som förtjänar en
  egen genomgång med testvektorer, inte en snabb iteration. Parsern kastar
  `SSHKeyError.encrypted` tydligt tills dess.
- **Rå tangentbordsinmatning i Linux-terminalen.** SwiftCrossUI har ingen
  key-event-API alls. En riktigt interaktiv terminal (piltangenter/Ctrl+C
  live, som SwiftTerm) skulle kräva en egen Cairo-ritad GTK-widget med GTK:s
  event-controllers direkt, utanför SwiftCrossUIs `View`-träd — större jobb,
  och kräver en osäker generisk-till-konkret cast (`GtkBackend.Widget` är
  faktiskt `Gtk.Widget`, så det är tekniskt möjligt men inte en ren, stödd
  API-väg). Radvis input + kontrollknappar täcker det mesta (se "Klart" ovan)
  tills vidare.
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
  — INTE PÅBÖRJAT, ren designanteckning för framtida arbete. Dagens läge
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
  - **Keep-alive** — swift-nio-ssh 0.14.1 (den version SSHCore bygger på)
    exponerar INTE godtyckliga globala SSH-förfrågningar publikt (cubic-
    fynd på PR #199) — den enkla "skicka en global request periodiskt"-
    planen går alltså inte att implementera som skriven idag. Kräver
    antingen ett uppströmsbidrag/API-utökning i swift-nio-ssh, eller en
    annan mekanism (t.ex. en no-op-kanalförfrågan om biblioteket
    exponerar någon sådan) — måste utredas mot faktisk bibliotekskod
    innan intervallet (25-60s, konfigurerbart) är mer än en idé.
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
  "Plattforms- och paketeringsmål, fullständigt") — inte påbörjat:
  `.deb`-paket (Debian/Ubuntu), `.rpm`-paket (RHEL/Fedora), FreeBSD-bygge
  (Swift har community-toolchains där), OpenBSD/NetBSD-undersökning
  (oklart om Swift ens fungerar där än — måste verifieras mot en riktig
  installation innan något annat antas). ARM64/Raspberry Pi täcks
  naturligt av samma Linux-bygge + `.deb`-paketering, förutsatt att
  toolchainen stödjer target-arkitekturen (gör den, för Linux ARM64).
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
  **Kvar**: LinuxApp-motsvarigheten (SwiftCrossUIs `Gtk`-paket saknar en
  färdig Swift-omslag för GTK4:s `GtkDropTarget`, till skillnad från
  `CSSProvider` — skulle kräva rå GObject/C-interop-kod, medvetet
  avvaktat tills det känns värt tiden), flerval för komprimering,
  förhandsvisning (t.ex. bilder), syntax highlighting (se separat post
  nedan).
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
