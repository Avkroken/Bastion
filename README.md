# Bastion (arbetsnamn)

Fri, öppen, **fristående** SSH-klient — en app du laddar ner, inte något som
körs i en container. Varje plattform är en genuint native klient, skriven i
sitt eget språk/UI-ramverk (beslut 2026-07-29 — inget delat cross-platform
UI-lager). `SSHCore` ([SwiftNIO SSH](https://github.com/apple/swift-nio-ssh))
är kärnan för iOS/macOS, där Swift redan är native. Övriga plattformar har
egna native SSH-implementationer. Klienter kopplas samman via ett synkprotokoll
för host-databasen, inte via delad UI- eller SSH-kod.

> "Docker-stöd" = appen hanterar Docker-containrar på dina *fjärrservrar* via SSH.
> Appen själv körs aldrig i en container.

Se [VISION.md](VISION.md) för den fulla visionen (målgrupp, arkitektur,
funktionslista, utvecklingsplan) och [ROADMAP.md](ROADMAP.md) för aktuell
status mot den — den här filen är bara "hur man bygger och kör".

## Cross-platform & sync

| Plattform | Kärna | UI |
|-----------|-------|----|
| Plattform | Native stack |
|-----------|--------------|
| iOS/iPadOS | SwiftUI (`App/`) + SSHCore |
| macOS | SwiftUI (`App/`, delas med iOS) + SSHCore |
| Android | Kotlin/Gradle (`Android/`) + Apache MINA SSHD |
| Linux | Rust + GTK4 (`LinuxApp/`, under uppbyggnad) + russh/libssh2 |
| Windows | C#/.NET + WinUI 3 (`WindowsApp/`, under uppbyggnad) + SSH.NET |

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
LinuxApp/              UNDER OMBYGGNAD: Rust + GTK4 (gtk4-rs) + russh/libssh2,
                       native GNOME-klient. Ersätter det tidigare
                       SwiftCrossUI/GTK4-spåret (borttaget 2026-07-29, se
                       ROADMAP.md).
WindowsApp/            UNDER OMBYGGNAD: C#/.NET + WinUI 3 + SSH.NET, native
                       Windows-klient. Ersätter det tidigare
                       SwiftCrossUI/WinUIBackend-spåret (borttaget 2026-07-29,
                       se ROADMAP.md).
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

## Linux-GUI:t (`LinuxApp/`) och Windows-GUI:t (`WindowsApp/`)

Native klienter (arkitekturbeslut 2026-07-29, se ROADMAP.md för motivering
och historik): Linux är Rust + GTK4 (gtk4-rs) + russh, Windows är C#/.NET +
WinUI 3 + SSH.NET.

**Linux** (kräver `libgtk-4-dev`, `libadwaita-1-dev`, `libvte-2.91-gtk4-dev`,
`pkg-config`):

```sh
cd LinuxApp
cargo build
cargo test
cargo run
```

**Windows** (kräver Windows App SDK/WinUI 3 — bygger inte på Linux/macOS,
se `.github/workflows/`):

```powershell
cd WindowsApp
dotnet build
dotnet test Bastion.Core.Tests
dotnet run --project WindowsApp.csproj
```

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
