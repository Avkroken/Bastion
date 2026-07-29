# Synkprotokoll

Formell specifikation för hur Bastion-klienter (`App/`, `Android/`,
`LinuxApp/`, framtida `WindowsApp/`) synkar host-databasen utan att dela
UI- eller SSH-kod (se [ROADMAP.md](ROADMAP.md), arkitekturbeslut
2026-07-29). Detta protokoll är den enda kopplingen mellan klienterna —
allt annat (SSH-implementation, UI-ramverk) är helt fristående per
plattform.

Referensimplementationer:
- Swift: `Sources/SSHCore/Host.swift`, `HostStore.swift`, `SyncEngine.swift`,
  `SyncProvider.swift`.
- Rust: `LinuxApp/src/host.rs`, `LinuxApp/src/sync.rs`.

Ingen central server. Protokollet är avsiktligt enkelt: en JSON-fil +
en deterministisk merge-funktion + en utbytbar transport.

## 1. Wire-format (`SyncState`)

```
SyncState {
    hosts: [Host],
    tombstones: [UUID: Date]   // se §1.3 för det platta array-formatet
}
```

`Host` — se `Sources/SSHCore/Host.swift` för hela fältlistan (`id`, `alias`,
`hostName`, `user`, `port`, `tags`, `auth`, `isFavorite`, `colorTag`,
`platform`, `startupCommand`, `jumpHostID`, `macAddress`, `modifiedAt`).
Innehåller ALDRIG hemligheter — bara metadata (lösenord/nycklar hör hemma i
Keychain/agent/lokal disk per klient).

### 1.1 Datum

Kodas som **sekunder sedan 2001-01-01T00:00:00Z** (Swifts
`timeIntervalSinceReferenceDate`), INTE Unix-epok. Verifierat empiriskt mot
en riktig `swift`-körning (inte antaget) — se testet
`reference_date_matches_swift_epoch` i `host.rs`. En klient som kodar Unix-
epok direkt kommer producera datum ~31 år fel och förlora merge-ordningen.

### 1.2 Enum-associerade värden (`HostAuth`)

Swifts synteserade `Codable` kodar ett enum-case som ett enda-nyckel-objekt:

| Case | JSON |
|---|---|
| `.askPassword` (ingen payload) | `{"askPassword": {}}` |
| `.keyFile(path)` (ett omärkt värde) | `{"keyFile": {"_0": path}}` |
| `.certificateFile(keyPath:certPath:)` (märkta värden) | `{"certificateFile": {"keyPath": ..., "certPath": ...}}` |

En klient som avviker från detta (t.ex. `{"keyFile": path}` direkt) kan
inte avkoda vad andra klienter skrev.

### 1.3 `tombstones` är en PLATT ARRAY, inte ett objekt

`Dictionary<UUID, Date>` kodas av Swift som `[k1, v1, k2, v2, ...]` eftersom
`UUID` inte är en giltig Codable-objektnyckel:

```json
{"hosts": [], "tombstones": ["00000000-0000-0000-0000-000000000001", 5.0]}
```

INTE `{"tombstones": {"...": 5.0}}`. Se testet
`sync_state_tombstones_are_flat_not_object` i `host.rs`.

## 2. Merge-algoritm (last-write-wins)

Deterministisk, kommutativ, idempotent — säker att köra i valfri ordning
och upprepat mellan valfritt antal klienter, utan server som koordinerar:

1. Samma värd (`id`) på båda sidor: nyaste `modifiedAt` vinner.
2. En radering (gravsten i `tombstones`) vinner om den är **minst lika ny**
   som värdens senaste redigering; annars "återupplivas" värden (en nyare
   redigering slår en äldre radering).
3. Resultatet innehåller den nyaste versionen av varje `id` som fanns i
   ENTINGEN sida, som antingen en levande `Host` eller en gravsten — aldrig
   båda.

Se `SyncEngine.merge` (Swift) / `sync::merge` (Rust) för referens-
implementationen, och `sync::tests` i `LinuxApp/src/sync.rs` för de tre
kärnfallen (nyare redigering vinner, gravsten vinner, återupplivning) plus
ett kommutativitetstest.

## 3. Transport (`SyncProvider`)

```
trait SyncProvider {
    fn pull() -> SyncState?     // None = inget synkat än
    fn push(state: SyncState)
}
```

En full synkrunda (`HostStore.sync(with:)` / `HostStore::sync`):

```
remote = provider.pull() ?? SyncState()
merged = merge(localState, remote)
persist(merged)               // lokalt
provider.push(merged)
```

### 3.1 `FolderSyncProvider` — referenstransporten

Enklaste och mest portabla transporten: en JSON-fil i en mapp som något
ANNAT redan synkar mellan enheter — Syncthing, en klonad Git-mapp, en
krypterad delad disk. Ingen inloggning, ingen egen server. Implementerad
identiskt i Swift (`FolderSyncProvider`) och Rust
(`sync::FolderSyncProvider`) — verifierat med ett riktigt cross-instans-test
(`two_independent_stores_converge_through_a_shared_folder_provider`) där två
oberoende `HostStore`-instanser konvergerar via samma delade fil.

### 3.2 Molntransporter (Dropbox/Google Drive/OneDrive) — Swift-sidan, inte porterat än

Swift-sidan har egna OAuth-baserade `SyncProvider`-implementationer för
molnlagring. Dessa krypterar `SyncState` innan uppladdning (se
`SyncCrypto.swift`, PBKDF2 + AEAD) eftersom molnleverantören inte är
betrodd på samma sätt som en lokal/synkad mapp. **Detta är en transport-
specifik utbyggnad, inte en ändring av kärnprotokollet** — vilken transport
som helst kan lägga till kryptering utan att `SyncState`-formatet eller
merge-algoritmen ändras. Inte porterat till `LinuxApp` än.

## 4. Vad en ny klient måste implementera för att delta

1. Läsa/skriva `SyncState` exakt enligt §1 (verifiera mot en riktig fixtur
   från en ANNAN klient — gissa inte formatet).
2. Implementera merge-algoritmen i §2 (eller återanvänd `sync::merge` om
   klienten redan länkar mot Rust-koden).
3. Välja minst en `SyncProvider`-transport — `FolderSyncProvider` är den
   enklaste startpunkten och kräver ingen inloggning.

## 5. Känt kvarstående arbete

- `LinuxApp` har `sync.rs`/`host.rs` (protokollet + `FolderSyncProvider`)
  men ingen UI för att välja/konfigurera en synkmapp än — biblioteks-nivå
  klart, inte yt-nivå.
- Krypterade molntransporter (§3.2) är inte porterade till `LinuxApp`.
- `WindowsApp` har ännu ingen implementation alls (scaffold inte påbörjad).
