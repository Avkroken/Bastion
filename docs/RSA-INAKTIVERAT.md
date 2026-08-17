# RSA-nycklar är tillfälligt inaktiverade i Linux-appen

**Status:** avstängt sedan 2026-08-17. Gäller `LinuxApp/` (Rust/GTK4).
iOS/macOS, Android och Windows berörs inte.

## Vad som händer

Försöker du ansluta till en värd som är konfigurerad med en RSA-nyckel —
som nyckelfil, som OpenSSH-certifikat med RSA-användarnyckel, eller via
ssh-agent — avbryts anslutningen och du får en dialog som länkar hit.
Ed25519- och ECDSA-nycklar fungerar som vanligt.

## Varför

`rsa`-crate:n har [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071)
— Marvin-attacken, en timing-sidokanal som i värsta fall kan läcka den
privata nyckeln till någon som kan mäta tidsåtgången för många operationer.
Allvarlighetsgrad 5.9 (medium).

Advisoryn har **ingen rättad version**. Det går alltså inte att lösa genom
att uppgradera: `rsa` drogs in av `russh` 0.62.6, som är senaste släppet,
och uppströms har inte publicerat någon fix. Det enda sättet att få bort
den sårbara koden ur binären är att inte kompilera in den.

Därför byggs `russh` med `default-features = false`, vilket tar bort
`rsa`-featuren och hela crate:n ur beroendeträdet.

## Hur du går vidare

Skapa en Ed25519-nyckel och lägg till den på servern:

```sh
ssh-keygen -t ed25519 -C "din@epost"
ssh-copy-id -i ~/.ssh/id_ed25519.pub användare@värd
```

Ed25519 är dessutom snabbare och ger kortare nycklar än RSA, och stöds av
alla OpenSSH-versioner sedan 6.5 (2014).

Har du en server som *bara* accepterar RSA går den att nå från iOS/macOS-,
Android- eller Windows-klienten under tiden — begränsningen sitter i
Rust-beroendet, inte i protokollet.

## När slås det på igen

När `rsa` släpper en version som åtgärdar RUSTSEC-2023-0071, eller när
`russh` byter till en annan RSA-implementation. Då återställs:

- `LinuxApp/Cargo.toml` — ta bort `default-features = false` från `russh`
- `LinuxApp/src/ssh.rs` — de tre `is_rsa()`-kontrollerna i `authenticate`,
  och hash-förhandlingen via `best_supported_rsa_hash` för nyckelfiler
- `LinuxApp/src/main.rs` — `show_rsa_disabled_dialog`

Veckoskanningen i `.github/workflows/osv-scanner.yml` rapporterar
advisoryn så länge den är öppen, så bevakningen sköter sig själv.
