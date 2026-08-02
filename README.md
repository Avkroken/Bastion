# Bastion (arbetsnamn)

Fri, öppen, **fristående** SSH-klient — en app du laddar ner, inte något som
körs i en container. Byggd på [SwiftNIO SSH](https://github.com/apple/swift-nio-ssh)
så att **samma kärna kör på iOS, macOS, Linux och Windows**; bara det tunna
UI-lagret är plattformsspecifikt. All affärslogik ligger i den testade kärnan
(`SSHCore`) — verifierad på Linux, byggbar på Apple.

> "Docker-stöd" = appen hanterar Docker-containrar på dina *fjärrservrar* via SSH.
> Appen själv körs aldrig i en container.

Se [VISION.md](VISION.md) för den fulla visionen (målgrupp, arkitektur,
funktionslista, utvecklingsplan) och [ROADMAP.md](ROADMAP.md) för aktuell
status mot den — den här filen är bara "hur man bygger och kör".

## Cross-platform & sync

| Plattform | Kärna | UI |
|-----------|-------|----|
| iOS/iPadOS | ✅ | SwiftUI (`App/`) — fas 1 |
| macOS | ✅ | SwiftUI (delas med iOS) |
| Linux | ✅ (byggd/testad här) | CLI + GUI (SwiftCrossUI/GTK4) — se toolchain-kravet nedan |
| Windows | ✅ (Swift finns på Windows) | GUI senare (WinUIBackend, otestad) |

**Sync utan inloggning:** host-databasen slås ihop deterministiskt mellan enheter
(`SyncEngine`, last-write-wins + gravstenar för raderingar). Transporten är en
enkel fil i en synkad mapp (`FolderSyncProvider`) — peka på iCloud Drive, Dropbox,
Syncthing eller en Git-mapp. Ingen server, inget konto.

**End-to-end-krypterat:** `EncryptedFolderSyncProvider` krypterar hela nyttolasten
på enheten med **AES-256-GCM**, nyckel härledd ur en lösenfras via
**PBKDF2-HMAC-SHA256** (verifierad mot kända testvektorer). Molntjänsten ser bara
chiffertext; fel lösenfras eller manipulerad fil upptäcks och avvisas. Så oavsett
om filen ligger i Dropbox, Google Drive, OneDrive eller iCloud är innehållet
oläsbart för alla utom dina enheter.

### Konton (Dropbox/Google/OneDrive/iCloud)
Två vägar, och de utesluter inte varandra:
1. **Synkad mapp (finns nu):** peka på en mapp som tjänstens egen app redan synkar
   (iCloud Drive, Dropbox, Syncthing, Git). Inget OAuth i appen, funkar direkt.
2. **Kontointegration (Dropbox klar, Google/OneDrive samma mönster):** logga in
   mot Dropbox via OAuth2 + PKCE (`ASWebAuthenticationSession`) och skriv filen
   direkt via deras API, mot en app-scopad mapp (aldrig hela kontot).

iCloud har ingen egen kod än — det fungerar redan i dag via väg 1 (peka
"Synkad mapp" på iCloud Drive-mappen), men kräver att användaren själv hittar
och pekar ut den. En native CloudKit/ubiquity-container-integration (slipper
peka ut mappen manuellt) är en möjlig framtida förbättring, inte byggd.

Oavsett väg är nyttolasten E2E-krypterad, så leverantören är bara dum lagring.

**Vill du använda kontoinloggning?** Klientkoden är klar för alla tre — Dropbox,
Google Drive, OneDrive — men kräver att DU registrerar en app hos leverantören;
det kan inte kodas i förväg:

| Leverantör | Utvecklarkonsol | Scope | Redirect URI |
|---|---|---|---|
| Dropbox | [App Console](https://www.dropbox.com/developers/apps) | `files.content.write` + `files.content.read` (App folder) | `se.denied.bastion://oauth/dropbox` |
| Google Drive | [Cloud Console](https://console.cloud.google.com/apis/credentials) | `drive.appdata` | `se.denied.bastion://oauth/googledrive` |
| OneDrive | [Azure-portalen (App registrations)](https://portal.azure.com) | `Files.ReadWrite.AppFolder offline_access` | `se.denied.bastion://oauth/onedrive` |

Klistra in respektive klient-ID i `App/OAuthProviders.swift` (t.ex.
`OAuthProviders.dropbox.clientID`) — inget annat behöver ändras.

## Layout

```
Sources/SSHCore/       Ren SwiftNIO — bygger på Linux OCH Apple
  SSHSession.swift       Anslut, execute() -> AsyncThrowingStream, run(), close()
  SSHUserAuth.swift      Klient-autentisering (lösenord / Ed25519-frö / OpenSSH-certifikat)
  SSHKeyParser.swift     OpenSSH-privatnyckelparser (~/.ssh/id_ed25519)
  SSHShell.swift         Interaktiv PTY-shell: send/resize + strömmad utdata
  ExecHandler.swift      Barnkanal: ByteBuffer <-> SSHChannelData, strömmar utdata
  PortForward.swift      Lokal (ssh -L, direct-tcpip) + fjärr (ssh -R, forwarded-tcpip) portvidarebefordran
  GlueHandler.swift      Bryggar två Channel-pipelines rakt igenom (från swift-nio-ssh:s exempel)
  HostKeyValidator.swift TOFU-validering + SHA256-fingeravtryck
  KnownHosts.swift       Lagring av sedda värdnycklar (MITM-skydd)
  SSHConfig.swift        ~/.ssh/config-parser (alias, jokertecken, IdentityFile)
  Host.swift             Sparad värd (metadata + taggar, inga hemligheter)
  HostStore.swift        Persistent host-databas (JSON, trådsäker)
  Snippet.swift          Sparat kommando med {{variabler}} + rendering
  SnippetStore.swift     Persistent snippet-databas (JSON, ingen sync ännu)
  CommandLibrary.swift   Statiskt referensbibliotek (Docker/Linux/Git/Cloudflare/Tailscale/WireGuard/systemd)
  SystemProbe.swift      Dashboard: ett SSH-kommando -> SystemSnapshot (parser testad)
  SFTPProtocol.swift     SFTP-trådformat (SSH_FXP_*, version 3) — kodning/avkodning, inget kanal-I/O
  SFTPClient.swift       SFTP-klient: subsystem-kanal + handskakning + id-baserad request/response-matchning (aktör)
  DockerService.swift    Docker: lista/start/stopp/omstart/logg (injektionssäkert)
  ArchiveOperations.swift  tar/zip-arkiv över exec-kanal (skapa/packa upp, injektionssäkert)
  SyncEngine.swift       Deterministisk merge (LWW + gravstenar) för sync
  SyncProvider.swift     Synktransport (mapp/iCloud/Dropbox/Syncthing/Git)
  SyncCrypto.swift       E2E-kryptering (AES-256-GCM + PBKDF2) + krypterad provider
  OAuthPKCE.swift        PKCE-kärna (RFC 7636) för kontointegration — testad, plattformsoberoende
  SSHTypes.swift         SSHTarget, SSHAuth, SSHChunk, SSHError, HostKeyInfo
  KeyManagement.swift    Nyckelgenerering (Ed25519) + fjärr-deploy till authorized_keys (POSIX/Windows)
  WireGuardConfig.swift  WireGuard .conf-parser/serialiserare (profilhantering, inte tunnel-upprättning)
  WireGuardProfileStore.swift  Persistent WireGuard-profildatabas (JSON, samma mönster som SnippetStore)
  OpenSSHCertificate.swift  ssh-ed25519-cert-v01-parser + CA-signaturverifiering
  SSHAgentClient.swift   ssh-agent-protokollklient över $SSH_AUTH_SOCK (lista identiteter, begär signaturer)
  TailscaleStatus.swift  `tailscale status --json`-parser + `fetch(over:)`/`fetchLocal()` (fjärr via SSH resp. lokal process)
  S3Client.swift         S3-kompatibel objektlagring (AWS SigV4-signering) — egna nycklar, inget OAuth
  S3ConnectionStore.swift  Persistent S3-anslutningsdatabas (JSON, samma mönster som WireGuardProfileStore)
Sources/bastion-cli/   Tunn CLI runt SSHCore (bevisar mot riktig server)
Tests/SSHCoreTests/    In-process SSH-server + end-to-end-test (ingen extern server)
App/                   XCODE-ONLY: iOS+macOS-appen (SwiftUI, delad kod) + XcodeGen-spec
  project.yml            XcodeGen → Bastion.xcodeproj (targets: Bastion iOS, Bastion-macOS)
  Platform.swift         Plattformsskillnader iOS/macOS samlade (Host-alias, nav-hjälpare)
  BastionApp.swift       @main
  HostListView.swift     Värdlista grupperad på tagg, anslut/redigera/ta bort
  SessionManager.swift   Håller alla samtidigt öppna sessioner (flikväxlare, se MultiSessionView)
  MultiSessionView.swift TabView mellan flera anslutna värdar — overksamma flikar hålls anslutna i bakgrunden
  HostEditView.swift     Lägg till / ändra värd
  HostDetailView.swift   Dashboard vid öppning + knapp till terminal
  DashboardView.swift    Renderar SystemSnapshot (last, minne, disk, Docker)
  DockerView.swift       Containerlista med start/stopp/omstart/logg/shell
  SnippetListView.swift  Sparade snippets — kör en (fyll i variabler) som startkommando
  SnippetEditView.swift  Lägg till/ändra ett snippet, visar upptäckta {{variabler}} live
  CommandLibraryView.swift Bläddra referensbiblioteket, kör via samma variabelifyllning som Snippets
  SFTPBrowserModel.swift SFTP-anslutningens livscykel (lazy connect, samma mönster som DockerModel)
  SFTPBrowserView.swift  Bläddra/navigera/ny mapp/döp om/ta bort över SFTP
  SessionView.swift      Aktiv session → terminalvyn (valfritt startkommando)
  TerminalView.swift     SwiftTerm kopplad till SSHCore.SSHShell (UIViewRepresentable/NSViewRepresentable)
  AuthResolver.swift     Delad SSHAuth-uppbyggnad
  Keychain.swift         Hemligheter (sync-lösenfras) i Keychain
  SyncSettingsView.swift Synkmapp/molnval, lösenfras, in/utloggning, "Synka nu"
  ImportConfigView.swift Klistra in ssh-config för att importera värdar
  OAuthProviders.swift   Dropbox/Google/OneDrive-config (klient-ID tomt tills du fyller i det)
  OAuthToken.swift       Token-modell (access/refresh/utgång)
  OAuthTokenStore.swift  Keychain-lagring + tyst förnyelse (inte MainActor — anropas synkront)
  OAuthAccountManager.swift Interaktiv PKCE-inloggning (ASWebAuthenticationSession) — OBS ej byggd här
  DropboxSyncProvider.swift SyncProvider mot Dropbox (krypterat, som EncryptedFolderSyncProvider)
  GoogleDriveSyncProvider.swift SyncProvider mot Google Drive (appDataFolder, sök+multipart-upload)
  OneDriveSyncProvider.swift    SyncProvider mot OneDrive (Graph API, path-baserad som Dropbox)
  Info.plist             Endast iOS-target (macOS genererar sin egen Info.plist)
LinuxApp/              EGET SwiftPM-paket (se "Bygg Linux-GUI:t" — varför det inte ligger i rot-paketet)
  Package.swift          .package(path: "..") mot roten för SSHCore, + SwiftCrossUI
  Sources/bastion-gui/   Linux-GUI, SwiftCrossUI (GTK4)
  BastionGUIApp.swift    @main, beror på GtkBackend direkt (se kommentar i Package.swift)
  ContentView.swift      NavigationSplitView: värdlista + dashboard/kommando
  HostListModel.swift    Host-databas-wrapper (samma HostStore som App/)
  HostEditView.swift     Lägg till/ändra värd — agent/lösenord/nyckelfil (ingen Keychain här)
  ImportConfigView.swift Klistra in ssh-config för import
  HostDetailView.swift   Lösenordsgrind + dashboard + terminal för vald värd
  DashboardView.swift    Samma auto-poll-modell som App/, SwiftCrossUI-vyer
  TerminalBuffer.swift   Egen VT100/ANSI-tolk (markör, SGR-färg, radering) — 42 tester i Tests/bastion-guiTests/
  TerminalGridView.swift Renderar buffern som hopslagna Text-körningar (ingen Canvas i SwiftCrossUI)
  TerminalSessionView.swift Bestående PTY-shell + radvis input + kontrollknappar (piltangenter/Home/End/PgUp/PgDn/Ctrl+C/Tab/Esc)
  DockerView.swift       Docker: lista/start/stopp/omstart/logg/shell — motsvarar App/DockerView.swift
  SnippetListView.swift  Sparade snippets — motsvarar App/SnippetListView.swift
  SnippetEditView.swift  Lägg till/ändra ett snippet — motsvarar App/SnippetEditView.swift
  CommandLibraryView.swift Bläddra referensbiblioteket — motsvarar App/CommandLibraryView.swift
  SFTPBrowserView.swift  SFTP-filhanterare (modell + vy i samma fil) — motsvarar App/SFTPBrowser{Model,View}.swift
  PortForwardView.swift  Portvidarebefordran (lokal/fjärr/dynamisk -L/-R/-D), starta/stoppa — ingen App/-motsvarighet än
  KeyDeployView.swift    Generera+deploya+verifiera SSH-nyckel, byt host till nyckel-auth efter grönt ljus
  TailscaleDiscoveryView.swift Föreslå värdar ur ett tailnet (lokalt eller via en sparad fjärrvärd), lägg till som ny värd
  S3BrowserView.swift    Bläddra S3-kompatibel objektlagring: buckets → objekt → ladda upp/ner (text)/ta bort
  S3ConnectionEditView.swift  Lägg till/ändra en S3-anslutning (endpoint/region/nycklar)
  AuthResolver.swift     Som App/, men `.keychainKey` ger nil (ingen Keychain på Linux)
WindowsApp/            EGET SwiftPM-paket, samma mönster som LinuxApp/ — WinUIBackend istället för GtkBackend
  Package.swift          .package(path: "..") mot roten för SSHCore, + SwiftCrossUI/WinUIBackend
  Sources/bastion-gui/   Windows-GUI, SwiftCrossUI (WinUIBackend) — medvetet minimal första version
  BastionGUIApp.swift    @main + en enkel ContentView (visar antal sparade värdar) — bevisar pipelinen, riktiga vyerna porteras hit senare
```

## Bygga & testa (Linux eller macOS)

```sh
swift build
swift test
```

Testerna startar en riktig SSH-server i processen på en slumpport och kör hela
klientvägen mot den — ingen extern server eller några hemligheter krävs.

## Kör mot en riktig server

```sh
swift build
BASTION_PASSWORD='...' ./.build/debug/bastion-cli user@host:22 "uname -a; docker ps"
# nyckel (rått 32-byte Ed25519-frö som hex):
BASTION_ED25519_HEX='...' ./.build/debug/bastion-cli user@host "systemctl status"
# alias ur ~/.ssh/config (User/HostName/Port/IdentityFile hämtas därifrån):
./.build/debug/bastion-cli myserver "docker ps"
```

Autentiseringsordning i CLI:t: `BASTION_KEY_FILE` > `BASTION_ED25519_HEX` >
`BASTION_PASSWORD` > `IdentityFile` (ssh-config) > `~/.ssh/id_ed25519` > lösenordsfråga.

## Bygg Linux-GUI:t (`bastion-gui`, i `LinuxApp/`)

Eget SwiftPM-paket, medvetet skilt från roten (`.package(path: "..")` för
`SSHCore`) — annars skulle rotens `swift build`/`swift test` dra in hela
SwiftCrossUI-grafen och krascha på stabil toolchain (se nästa stycke).

**Kräver en Swift-toolchain nyare än 6.1.3.** Stabila Swift 6.1.3 kraschar
med ett bekräftat, öppet kompilatorfel
([swiftlang/swift#80759](https://github.com/swiftlang/swift/issues/80759)) när
den bygger SwiftCrossUIs `swift-mutex`-beroende — inget fel i den här koden.
Verifierat löst i en Swift 6.5-dev-snapshot (2026-07-02) — se
`.github/workflows/linux-gui.yml` för hur den hämtas (`download.swift.org`).
(`apt install swiftlang` finns INTE på en vanlig Ubuntu 24.04 — "swift" i
Ubuntus repon är OpenStack Swift, ett helt orelaterat paket; upptäckt när
`swiftpm-linux.yml`, som bygger rot-paketet — SSHCore/bastion-cli, opåverkat
av swift-mutex-buggen — försökte samma sak.)

```sh
apt-get install libgtk-4-dev pkg-config   # GTK4-headers, en gång
cd LinuxApp
swift build --product bastion-gui         # med en toolchain där buggen är fixad
./.build/debug/bastion-gui
```

`swift test` funkar likadant (`bastion-guiTests`, `@testable import bastion_gui`
— ett testmål kan importera ett `.executableTarget` direkt, ingen omstrukturering
till ett bibliotekstarget krävs). Om `S3Client`/`FoundationXML` är inbyggt, se
`-rpath-link`-tillägget nedan — det behövs för både `build` och `test`.

Beror på `GtkBackend` direkt i stället för `DefaultBackend` (se kommentar i
`LinuxApp/Package.swift`) — `DefaultBackend`s plattformsvillkorade
`WinUIBackend`-gren byggdes ändå på en tidig SwiftPM-snapshot och krävde
Windows-headers som saknas på Linux. `GtkBackend` beror bara på
`SwiftCrossUI` + `Gtk` + `CGtk`.

### Om din toolchain-nedladdning är byggd för en äldre Ubuntu-version
En `ubuntu24.04`-snapshot på en nyare Ubuntu (t.ex. 26.04, som saknar
`libxml2.so.2` — bara `libxml2-16`) ger `error while loading shared libraries:
libxml2.so.2` VID KÖRNING. Lös genom att peka `LD_LIBRARY_PATH` på en mapp
med den gamla `.so`:n (extraherad ur ett arkiverat `libxml2`-paket, aldrig
`dpkg`-installerad systemvitt):

```sh
LD_LIBRARY_PATH=/path/to/compat-libs swift build --product bastion-gui
```

Sedan `S3Client.swift` (`FoundationXML`) drogs in transitivt (2026-07-07)
misslyckas dessutom LÄNKNINGEN på samma sätt (`undefined reference to
xmlBufferFree@LIBXML2_2.4.30` m.fl.) — `LD_LIBRARY_PATH` räcker inte
ensamt där, GNU `ld.bfd` behöver även `-rpath-link` för att slå upp
`libFoundationXML.so`s TRANSITIVA libxml2-beroende under själva länkningen:

```sh
LD_LIBRARY_PATH=/path/to/compat-libs swift build --product bastion-gui \
  -Xlinker -rpath-link -Xlinker /path/to/compat-libs
```

## Bygg Windows-GUI:t (`bastion-gui`, i `WindowsApp/`)

Eget SwiftPM-paket, samma mönster/skäl som `LinuxApp/`. Använder
`WinUIBackend` istället för `GtkBackend`. Medvetet minimal första version
(`ContentView` visar bara antal sparade värdar) — verifieras hittills
bara via CI (`.github/workflows/windows-gui.yml`, `windows-latest`-runnern),
ingen lokal Windows-maskin att bygga/köra på ännu.

```powershell
cd WindowsApp
swift build --product bastion-gui
```

Kräver [Swift för Windows](https://www.swift.org/install/windows/) samt
WinUIBackends beroenden (Windows SDK 10.0.17763 vid kompilering,
WindowsAppSDK-runtime vid körning) — se SwiftCrossUIs egen
["Setting up a development environment on Windows"](https://swiftpackageindex.com/stackotter/swift-cross-ui/documentation/swiftcrossui/windows-development)-guide.

## Bygg appen (på en Mac)

`App/project.yml` genererar **två** Xcode-mål ur samma delade SwiftUI-kod:
`Bastion` (iOS, fas 1) och `Bastion-macOS` (fas 2, App Sandbox + utgående
nätverk). Xcode-projektet genereras med [XcodeGen](https://github.com/yonaskolb/XcodeGen)
— så projektet hålls i textform och kan versionshanteras.

```sh
brew install xcodegen
cd App
xcodegen generate
open Bastion.xcodeproj
```

I Xcode: välj ditt team under **Signing & Capabilities**, välj target (`Bastion`
eller `Bastion-macOS`) och kör på simulator/enhet eller Mac. SwiftTerm och
SSHCore dras in automatiskt som paketberoenden till båda targeten.

### Väg till App Store
1. Byt `PRODUCT_BUNDLE_IDENTIFIER` i `project.yml` till ditt eget (t.ex. `se.dittnamn.bastion`).
2. Sätt signeringsteam (app-ikon och launch screen finns redan, se `Assets.xcassets`).
3. Höj `MARKETING_VERSION`, arkivera (**Product → Archive**) och ladda upp via Organizer.
4. Öppen källkod-appar godkänns — se bara till att licens (MIT/Apache) och ev.
   tredjepartslicenser (SwiftNIO, SwiftTerm) listas i appen.

### TestFlight utan en egen Mac (GitHub Actions)

`.github/workflows/testflight.yml` (manuell knapp, `workflow_dispatch`) bygger,
signerar och laddar upp till TestFlight direkt från en macOS-runner —
ingen lokal Mac behövs för det här steget. Signering är helt automatisk
(App Store Connect API-nyckeln ger Xcode rätt att skapa/hantera certifikat
och provisioning profile själv, se `App/fastlane/Fastfile`).

Kräver ett aktivt Apple Developer Program-medlemskap och fyra secrets under
**Settings → Secrets and variables → Actions**:

| Secret | Var det kommer ifrån |
|---|---|
| `APP_STORE_CONNECT_TEAM_ID` | Developer-portalen → Membership (10 tecken) |
| `APP_STORE_CONNECT_KEY_ID` | App Store Connect → Users and Access → Integrations → App Store Connect API (skapa en nyckel med minst "App Manager"-roll) |
| `APP_STORE_CONNECT_ISSUER_ID` | Samma sida som ovan |
| `APP_STORE_CONNECT_KEY_CONTENT` | Den nedladdade `.p8`-filens innehåll, base64-kodat: `cat AuthKey_XXXXXXXXXX.p8 \| base64` (fungerar likadant på macOS och Linux, till skillnad från `pbcopy`) — klistra in utskriften som secret-värdet |

Kör sedan workflowet manuellt (fliken **Actions** → *TestFlight-uppladdning* →
**Run workflow**). Byggnumret hämtas automatiskt från senaste TestFlight-build
+ 1, ingen manuell versionshantering behövs.

Appens affärslogik ligger i den testade kärnan (`SSHCore`); `App/`-lagret är tunn
SwiftUI-glue. Så länge kärnan är grön är appen mest layout att putsa i Xcode.

## Terminalvyn (Xcode)

`App/TerminalView.swift` kopplar `SSHCore.SSHSession` till
[SwiftTerm](https://github.com/migueldeicaza/SwiftTerm). Lägg till SwiftTerm som
paketberoende i ett Xcode-app-target — den byggs inte av SwiftPM på Linux (kräver
UIKit/AppKit). En interaktiv shell använder en PTY-kanal (backlog); vyn visar
exec-utdata idag för att bevisa datavägen till skärmen.

Se [ROADMAP.md](ROADMAP.md) för status, nästa steg och avsiktligt uppskjutna delar.

## Licens

MIT (se `LICENSE`). Alla valda beroenden (SwiftNIO, SwiftNIO SSH, swift-crypto,
SwiftCrossUI, SwiftTerm) är Apache 2.0 / MIT — kompatibla.
