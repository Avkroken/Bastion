//! SSH-nyckeldistribution, motsvarigheten till `Sources/SSHCore/
//! KeyManagement.swift` + `App/KeyDeployView.swift`. Genererar ett nytt
//! Ed25519-nyckelpar, lägger till den publika raden i fjärrsidans
//! `~/.ssh/authorized_keys` (idempotent, samma kommando som Swift-sidan),
//! och verifierar sedan att nyckeln FAKTISKT fungerar genom en ny, separat
//! anslutning — precis som `SSHSession.verifyKeyAuthWorks` gör innan ett
//! lösenord får tas bort ur lagringen.
//!
//! Distribuerar mot POSIX- OCH Windows-fjärrsystem (`host.platform`,
//! samma tre-vägars `RemotePlatform`-uppdelning som
//! `Sources/SSHCore/KeyManagement.swift`) — LinuxApp hade fältet i `Host`
//! sedan tidigare men läste det aldrig här.

use crate::host::{Host, HostAuth};
use base64::Engine;
use russh::keys::PrivateKey;
use russh::keys::PublicKeyBase64;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[derive(Debug, PartialEq)]
pub struct GeneratedKeyPair {
    pub public_key_line: String,
    /// PKCS8 PEM — `russh_keys::decode_secret_key` (använd av `load_secret_key`,
    /// se `ssh::authenticate`s `HostAuth::KeyFile`-gren) läser detta formatet
    /// precis lika bra som `-----BEGIN OPENSSH PRIVATE KEY-----`, så samma
    /// `HostAuth::KeyFile(path)` fungerar oavsett vilketdera som skrevs.
    pub private_key_pem: String,
}

/// Genererar ett helt nytt, slumpmässigt Ed25519-nyckelpar. `comment`
/// bifogas den publika raden (samma konvention som `ssh-keygen -C`).
pub fn generate_ed25519(comment: &str) -> Result<GeneratedKeyPair, String> {
    // russh 0.62 bygger på `ssh_key`-typerna: `KeyPair::generate_ed25519()`
    // ersattes av `PrivateKey::random`, som tar en RNG explicit.
    let keypair = PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519)
        .map_err(|e| format!("kunde inte generera Ed25519-nyckel: {e}"))?;
    key_pair_to_generated(keypair, comment)
}

/// Bygger en `GeneratedKeyPair` ur en KLISTRAD, redan befintlig OpenSSH-
/// privatnyckel-PEM istället för att slumpa fram en ny — motsvarar Swift-
/// sidans `KeyGenerator.fromExisting`/`KeyDeployModel.importExisting`.
/// Bara OKRYPTERADE Ed25519-nycklar stöds (samma begränsning som
/// Swift-sidan): `Error::KeyIsEncrypted` ges ett eget, tydligt
/// felmeddelande istället för det generiska tolkningsfelet, och alla
/// andra nyckeltyper (RSA/ECDSA/…) avvisas explicit.
pub fn import_existing(pem: &str, comment: &str) -> Result<GeneratedKeyPair, String> {
    let keypair = russh::keys::decode_secret_key(pem, None).map_err(|e| match e {
        russh::keys::Error::KeyIsEncrypted => "lösenfras-skyddade nycklar stöds inte än".to_string(),
        other => format!("kunde inte tolka nyckeln: {other}"),
    })?;
    if !matches!(keypair.algorithm(), russh::keys::Algorithm::Ed25519) {
        return Err("bara Ed25519-nycklar stöds".to_string());
    }
    key_pair_to_generated(keypair, comment)
}

fn key_pair_to_generated(keypair: PrivateKey, comment: &str) -> Result<GeneratedKeyPair, String> {
    let public_key = keypair.public_key();
    let mut public_key_line =
        format!("{} {}", public_key.algorithm().as_str(), public_key.public_key_base64());
    if !comment.is_empty() {
        public_key_line.push(' ');
        public_key_line.push_str(comment);
    }

    let mut pem_bytes = Vec::new();
    russh::keys::encode_pkcs8_pem(&keypair, &mut pem_bytes).map_err(|e| e.to_string())?;
    let private_key_pem = String::from_utf8(pem_bytes).map_err(|e| e.to_string())?;

    Ok(GeneratedKeyPair { public_key_line, private_key_pem })
}

/// Skriver PEM-materialet till en ny fil under `~/.bastion/keys/` med 0600 —
/// samma rättighetsnivå som `ssh-keygen` sätter på `~/.ssh/id_ed25519`.
/// Filnamnet innehåller ett UUID, inte värdaliaset, så två nycklar för olika
/// värdar (eller omgenererade nycklar för samma värd) aldrig krockar.
pub fn save_private_key(pem: &str) -> Result<String, String> {
    let dir = dirs::home_dir().ok_or("kunde inte hitta hemkatalogen")?.join(".bastion/keys");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;

    let path = dir.join(format!("bastion_ed25519_{}", uuid::Uuid::new_v4()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(pem.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Escapar en sträng säkert för inbäddning i ETT enkelcitat POSIX shell-
/// argument — samma teknik som Swift-sidans `shellQuoted`.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Bygger kommandot som lägger till `public_key_line` i
/// `~/.ssh/authorized_keys` — idempotent, skapar `~/.ssh` med rätt
/// rättigheter om den saknas. Egen funktion (inte inline i
/// `spawn_deploy_and_verify`) för att kunna testa den exakta strängen utan
/// en riktig SSH-anslutning — samma uppdelning som Swift-sidans
/// `deployPublicKeyCommandPOSIX`.
pub fn deploy_command(public_key_line: &str) -> String {
    deploy_command_at(public_key_line, "~/.ssh/authorized_keys")
}

/// Samma som `deploy_command`, fast mot en godtycklig sökväg — bara till
/// för `deploy_and_verify`s test, som MÅSTE skriva någon annanstans än den
/// riktiga körande kontots `~/.ssh/authorized_keys` (annars skulle testet
/// permanent förorena den här sandboxens riktiga SSH-konfiguration OCH
/// ändå misslyckas, eftersom `TestSshd`s isolerade `sshd` autentiserar mot
/// sin EGEN fristående `authorized_keys`-fil, inte kontots riktiga).
fn deploy_command_at(public_key_line: &str, path: &str) -> String {
    let quoted = shell_quote(public_key_line);
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("~/.ssh");
    format!(
        "mkdir -p {dir} && chmod 700 {dir} && touch {path} && \
         chmod 600 {path} && \
         (grep -qxF {quoted} {path} || echo {quoted} >> {path})"
    )
}

/// Vilket kommando som faktiskt distribuerar nyckeln — grenar på
/// `host.platform` (fältet fanns redan i `Host`, men lästes tidigare
/// aldrig av något i LinuxApp: `deploy_command`/`deploy_and_verify` antog
/// alltid POSIX). Windows OpenSSH har en avsiktlig säkerhetsregel som gör
/// att admin- och standardkonton måste hanteras helt olika — samma
/// tre-vägars uppdelning som `Sources/SSHCore/KeyManagement.swift`.
fn deploy_command_for_host(host: &Host, public_key_line: &str) -> String {
    match host.platform {
        crate::host::RemotePlatform::Posix => deploy_command(public_key_line),
        crate::host::RemotePlatform::WindowsAdmin => {
            deploy_command_windows(public_key_line, r"C:\ProgramData\ssh\administrators_authorized_keys", true)
        }
        crate::host::RemotePlatform::WindowsStandard => {
            deploy_command_windows(public_key_line, r"$env:USERPROFILE\.ssh\authorized_keys", false)
        }
    }
}

/// Bygger ett Windows-kommando som anropar `powershell -EncodedCommand` med
/// hela skriptet Base64/UTF-16LE-kodat — undviker helt att behöva escapa en
/// fri kommentarsträng genom TVÅ nästlade skallager (SSH-exec-argumentet
/// OCH cmd.exe/PowerShells egen citering). Base64 innehåller bara
/// `[A-Za-z0-9+/=]`, alla säkra oquotade i cmd.exe. Samma logik/wire-format
/// som Swift-sidans `deployPublicKeyCommandWindows` — verifierat mot
/// samma teststräng-approach (ingen riktig Windows-värd tillgänglig i den
/// här sandlådan, se testerna nedan).
///
/// `set_acl`: bara `.windows_admin` behöver `icacls`-låsningen — Win32-
/// OpenSSH vägrar annars filen helt. Standardkontots egen `.ssh`-mapp har
/// inga såna krav.
fn deploy_command_windows(public_key_line: &str, path: &str, set_acl: bool) -> String {
    let ps_quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let ps_key = ps_quote(public_key_line);

    // `path` kan innehålla `$env:USERPROFILE` (standardkontogrenen) — ett
    // enkelcitat PowerShell-argument är en LITERAL sträng, variabeln
    // expanderas då aldrig och nyckeln hade skrivits till en fil som
    // bokstavligen heter "$env:USERPROFILE\.ssh\authorized_keys" (CodeRabbit-
    // fynd). `Join-Path $env:USERPROFILE <resten enkelciterad>` expanderar
    // variabeln på RÄTT sida (PowerShell-uttrycket), medan resten av
    // sökvägen förblir en literal, injektionssäker sträng.
    let ps_expr = |p: &str| match p.strip_prefix(r"$env:USERPROFILE\") {
        Some(rest) => format!("(Join-Path $env:USERPROFILE {})", ps_quote(rest)),
        None => ps_quote(p),
    };
    let ps_path = ps_expr(path);
    let dir = path.rsplit_once('\\').map(|(d, _)| d).unwrap_or(path);
    let ps_dir = ps_expr(dir);

    let mut script = format!(
        "$ErrorActionPreference = 'Stop'\n\
         $key = {ps_key}\n\
         $path = {ps_path}\n\
         $dir = {ps_dir}\n\
         if (!(Test-Path $dir)) {{ New-Item -ItemType Directory -Path $dir -Force | Out-Null }}\n\
         if (!(Test-Path $path) -or -not (Select-String -LiteralPath $path -Pattern $key -SimpleMatch -Quiet)) {{\n    \
             Add-Content -Path $path -Value $key\n\
         }}"
    );
    if set_acl {
        script.push_str(
            "\n\nicacls $path /inheritance:r | Out-Null\n\
             icacls $path /grant SYSTEM:F /grant Administrators:F | Out-Null",
        );
    }

    let utf16: Vec<u8> = script.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    format!("powershell -NoProfile -NonInteractive -EncodedCommand {encoded}")
}

/// Distribuerar den nya nyckeln (via den BEFINTLIGA auth-metoden i `host`)
/// och verifierar sedan att den fungerar genom en HELT NY, separat
/// anslutning som bara använder den nya nyckeln — motsvarar
/// `SSHSession.verifyKeyAuthWorks`. Körs på en egen bakgrundstråd, samma
/// mönster som `port_forward`/`socks_proxy`.
pub fn spawn_deploy_and_verify(
    host: Host,
    password: Option<String>,
    public_key_line: String,
    new_key_path: String,
    jump: Option<Host>,
) -> async_channel::Receiver<Result<(), String>> {
    let (result_tx, result_rx) = async_channel::bounded(1);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("kunde inte starta tokio-runtimen för nyckeldistributionstråden");
        rt.block_on(async move {
            let result =
                deploy_and_verify(host, password, public_key_line, new_key_path, None, jump).await;
            let _ = result_tx.send(result).await;
        });
    });

    result_rx
}

async fn deploy_and_verify(
    host: Host,
    password: Option<String>,
    public_key_line: String,
    new_key_path: String,
    authorized_keys_path_override: Option<&str>,
    jump: Option<Host>,
) -> Result<(), String> {
    let command = match authorized_keys_path_override {
        Some(path) => deploy_command_at(&public_key_line, path),
        None => deploy_command_for_host(&host, &public_key_line),
    };
    let session = crate::ssh::connect(&host, password, None, jump.clone()).await?;
    let mut channel = session.channel_open_session().await.map_err(|e| format!("kunde inte öppna kanal: {e}"))?;
    channel
        .exec(true, command.as_bytes())
        .await
        .map_err(|e| format!("kunde inte köra distributionskommandot: {e}"))?;
    // Exitkoden IGNORERADES tidigare helt. Misslyckades distributions-
    // kommandot (skrivskyddad `~/.ssh`, full disk, saknad `mkdir`-rätt)
    // upptäcktes det visserligen ändå — verifieringsanslutningen nedan
    // faller då — men felmeddelandet blev missvisande: "deployades men
    // verifieringen misslyckades", när sanningen var att deployen aldrig
    // lyckades. Nu rapporteras det verkliga felet, med serverns egen
    // stderr när den finns.
    //
    // Läser tills kanalen är HELT stängd (`wait()` ger `None`) — bryter
    // varken på `ExitStatus` eller på `Eof`/`Close`:
    //
    // * `ExitStatus` duger inte som slutvillkor (samma lärdom som i
    //   `ssh::run_command_on_session`): utdata kan fortfarande ligga kvar
    //   i kön när den kommer, och här behövs stderr till felmeddelandet.
    // * `Eof`/`Close` duger heller inte HÄR — empiriskt verifierat mot en
    //   riktig `sshd` att de anländer FÖRE `exit-status` i det här
    //   flödet, så ett break där tappade exitkoden helt (först tolkat som
    //   "kommandot lyckades"). `exit-status` är en kanalFÖRFRÅGAN, inte
    //   data, och får legitimt komma efter EOF — till skillnad från
    //   utdata, som per definition inte kan det.
    //
    // `COMMAND_TIMEOUT` skyddar mot en server som aldrig stänger kanalen.
    let mut exit_status: Option<u32> = None;
    let mut stderr = Vec::new();
    let drain = async {
        while let Some(msg) = channel.wait().await {
            match msg {
                russh::ChannelMsg::ExitStatus { exit_status: code } => exit_status = Some(code),
                russh::ChannelMsg::ExtendedData { ref data, .. } => stderr.extend_from_slice(data),
                _ => {}
            }
        }
    };
    tokio::time::timeout(crate::ssh::COMMAND_TIMEOUT, drain)
        .await
        .map_err(|_| format!("distributionskommandot svarade inte inom {}s", crate::ssh::COMMAND_TIMEOUT.as_secs()))?;

    if let Some(code) = exit_status
        && code != 0
    {
        let detail = String::from_utf8_lossy(&stderr);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            format!("distributionskommandot misslyckades (exitkod {code}) — nyckeln deployades INTE")
        } else {
            format!("distributionskommandot misslyckades (exitkod {code}): {detail} — nyckeln deployades INTE")
        });
    }

    // Ny, HELT SEPARAT anslutning som bara litar på den nya nyckeln — bevisar
    // att den fungerar innan anroparen (GTK-vyn) erbjuder att byta värdens
    // lagrade auth-metod och ta bort lösenordet, precis som Swift-sidans
    // "verifiera innan lösenordet tas bort"-resonemang. Samma jump-host (om
    // någon) som deploy-anslutningen ovan — target nås fortfarande GENOM
    // samma hopp, bara auth-metoden på target har bytts.
    let mut verify_host = host;
    verify_host.auth = HostAuth::KeyFile(new_key_path);
    crate::ssh::connect(&verify_host, None, None, jump).await.map(|_| ()).map_err(|e| {
        format!("nyckeln deployades men verifieringen misslyckades — lösenordet är INTE borttaget: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_has_the_expected_ssh_ed25519_line_shape() {
        let pair = generate_ed25519("bastion-test").unwrap();
        assert!(pair.public_key_line.starts_with("ssh-ed25519 "), "fick: {}", pair.public_key_line);
        assert!(pair.public_key_line.ends_with(" bastion-test"));
        assert!(pair.private_key_pem.starts_with("-----BEGIN PRIVATE KEY-----"));
    }

    #[test]
    fn two_generated_keys_are_never_the_same() {
        let a = generate_ed25519("").unwrap();
        let b = generate_ed25519("").unwrap();
        assert_ne!(a.public_key_line, b.public_key_line);
    }

    /// Genererar en RIKTIG OpenSSH Ed25519-nyckel via `ssh-keygen` (inte en
    /// handkonstruerad sträng) och "klistrar in" dess PEM-text — bevisar att
    /// `import_existing` faktiskt kan tolka en nyckel skapad av ett annat
    /// verktyg, inte bara sitt eget `generate_ed25519`-format.
    #[test]
    fn import_existing_parses_a_real_ssh_keygen_generated_ed25519_key() {
        let dir = std::env::temp_dir().join(format!("bastion-import-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let key_path = dir.join("id_ed25519");
        let status = std::process::Command::new("ssh-keygen")
            .args(["-q", "-N", "", "-t", "ed25519", "-f"])
            .arg(&key_path)
            .status()
            .unwrap();
        assert!(status.success());
        let pem = std::fs::read_to_string(&key_path).unwrap();
        let expected_public_line = std::fs::read_to_string(dir.join("id_ed25519.pub")).unwrap();
        let expected_key_part = expected_public_line.split(' ').nth(1).unwrap();

        let imported = import_existing(&pem, "imported-comment").unwrap();
        assert!(imported.public_key_line.starts_with("ssh-ed25519 "));
        assert!(imported.public_key_line.contains(expected_key_part), "den importerade nyckeln ska matcha ssh-keygens .pub-fil");
        assert!(imported.public_key_line.ends_with(" imported-comment"));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn import_existing_rejects_a_passphrase_protected_key_with_a_clear_message() {
        let dir = std::env::temp_dir().join(format!("bastion-import-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let key_path = dir.join("id_ed25519");
        let status = std::process::Command::new("ssh-keygen")
            .args(["-q", "-N", "hemlis", "-t", "ed25519", "-f"])
            .arg(&key_path)
            .status()
            .unwrap();
        assert!(status.success());
        let pem = std::fs::read_to_string(&key_path).unwrap();

        let result = import_existing(&pem, "");
        assert_eq!(result, Err("lösenfras-skyddade nycklar stöds inte än".to_string()));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn import_existing_rejects_a_non_ed25519_key() {
        let dir = std::env::temp_dir().join(format!("bastion-import-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let key_path = dir.join("id_rsa");
        let status = std::process::Command::new("ssh-keygen")
            .args(["-q", "-N", "", "-t", "rsa", "-b", "2048", "-f"])
            .arg(&key_path)
            .status()
            .unwrap();
        assert!(status.success());
        let pem = std::fs::read_to_string(&key_path).unwrap();

        let result = import_existing(&pem, "");
        assert_eq!(result, Err("bara Ed25519-nycklar stöds".to_string()));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn saved_private_key_round_trips_through_russh_keys_loader() {
        let pair = generate_ed25519("").unwrap();
        let path = save_private_key(&pair.private_key_pem).unwrap();
        let loaded = russh::keys::load_secret_key(&path, None).unwrap();
        let loaded_public = loaded.public_key();
        assert_eq!(loaded_public.public_key_base64(), pair.public_key_line.split(' ').nth(1).unwrap());
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn deploy_command_is_idempotent_and_quotes_the_key_line() {
        let cmd = deploy_command("ssh-ed25519 AAAA it's-a-comment");
        assert!(cmd.contains("mkdir -p ~/.ssh"));
        assert!(cmd.contains("grep -qxF"), "ska kolla om raden redan finns innan den lägger till den");
        // Enkelcitatet i kommentaren ska vara escapat, inte avslutat kommandot i förtid.
        assert!(cmd.contains("it'\\''s-a-comment"));
    }

    /// Avkodar en `powershell -EncodedCommand`-sträng tillbaka till klartext
    /// för att kunna kontrollera INNEHÅLLET, inte bara att kommandot råkar
    /// börja med rätt prefix.
    fn decode_powershell_script(command: &str) -> String {
        let encoded = command.strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ").unwrap();
        let utf16_bytes = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        let utf16: Vec<u16> =
            utf16_bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        String::from_utf16(&utf16).unwrap()
    }

    #[test]
    fn deploy_command_windows_admin_writes_administrators_authorized_keys_with_acl() {
        let cmd = deploy_command_windows(
            "ssh-ed25519 AAAA bastion@test",
            r"C:\ProgramData\ssh\administrators_authorized_keys",
            true,
        );
        assert!(cmd.starts_with("powershell -NoProfile -NonInteractive -EncodedCommand "));
        let script = decode_powershell_script(&cmd);
        assert!(script.contains(r"C:\ProgramData\ssh\administrators_authorized_keys"));
        assert!(script.contains("Add-Content -Path $path -Value $key"));
        assert!(script.contains("icacls $path /inheritance:r"), "adminkontot måste låsa ner ACL:erna");
        assert!(script.contains("ssh-ed25519 AAAA bastion@test"));
    }

    #[test]
    fn deploy_command_windows_standard_skips_the_acl_lockdown() {
        let cmd = deploy_command_windows("ssh-ed25519 AAAA bastion@test", r"$env:USERPROFILE\.ssh\authorized_keys", false);
        let script = decode_powershell_script(&cmd);
        assert!(!script.contains("icacls"), "standardkontot ska INTE låsa ner ACL:er");
    }

    /// `-SimpleMatch` gör redan sökningen LITERAL — att ändå köra `$key`
    /// genom `[regex]::Escape` (TIDIGARE kod) bäddar in bakstreck i en
    /// kommentar som t.ex. `key[prod]`, vilket gör att det escapade
    /// mönstret ALDRIG matchar den riktiga, oescapade raden i filen.
    /// Idempotensen hade alltså varit trasig för varje nyckelkommentar med
    /// hakparenteser eller andra regex-specialtecken — samma nyckel hade
    /// lagts till på nytt vid varje körning (CodeRabbit-fynd).
    #[test]
    fn deploy_command_windows_uses_a_literal_simplematch_not_a_regex_escaped_one() {
        let cmd = deploy_command_windows("ssh-ed25519 AAAA key[prod]", r"C:\path\authorized_keys", false);
        let script = decode_powershell_script(&cmd);
        assert!(!script.contains("[regex]::Escape"), "fick: {script}");
        assert!(script.contains("-Pattern $key -SimpleMatch"), "fick: {script}");
    }

    /// `$env:USERPROFILE` fick TIDIGARE inbäddas i ett enkelcitat PowerShell-
    /// argument, som är literalt — variabeln expanderades aldrig, och
    /// nyckeln hade skrivits till en fil som bokstavligen HETER
    /// "$env:USERPROFILE\.ssh\authorized_keys" i stället för användarens
    /// riktiga hemkatalog (CodeRabbit-fynd). `Join-Path $env:USERPROFILE …`
    /// (utanför citattecken, i PowerShell-uttrycket) expanderar variabeln
    /// korrekt.
    #[test]
    fn deploy_command_windows_standard_expands_userprofile_via_join_path() {
        let cmd = deploy_command_windows("ssh-ed25519 AAAA bastion@test", r"$env:USERPROFILE\.ssh\authorized_keys", false);
        let script = decode_powershell_script(&cmd);
        assert!(
            script.contains(r"Join-Path $env:USERPROFILE '.ssh\authorized_keys'"),
            "$env:USERPROFILE måste expanderas via Join-Path, inte bäddas in i ett literalt enkelcitat, fick: {script}"
        );
        assert!(
            !script.contains(r"'$env:USERPROFILE"),
            "$env:USERPROFILE får aldrig hamna INUTI ett enkelcitat (då expanderas det aldrig), fick: {script}"
        );
    }

    #[test]
    fn deploy_command_windows_escapes_embedded_single_quotes_powershell_style() {
        // PowerShell fördubblar enkelcitat (''), till skillnad från POSIX-skalets '\''.
        let cmd = deploy_command_windows("ssh-ed25519 AAAA it's-a-comment", r"C:\path\authorized_keys", false);
        let script = decode_powershell_script(&cmd);
        assert!(script.contains("it''s-a-comment"), "fick: {script}");
    }

    #[test]
    fn deploy_command_for_host_dispatches_on_platform() {
        let mut host = Host::new("t".into(), "h".into(), "u".into());

        host.platform = crate::host::RemotePlatform::Posix;
        assert!(deploy_command_for_host(&host, "ssh-ed25519 AAAA").starts_with("mkdir -p"));

        host.platform = crate::host::RemotePlatform::WindowsAdmin;
        assert!(deploy_command_for_host(&host, "ssh-ed25519 AAAA").starts_with("powershell"));

        host.platform = crate::host::RemotePlatform::WindowsStandard;
        assert!(deploy_command_for_host(&host, "ssh-ed25519 AAAA").starts_with("powershell"));
    }

    #[tokio::test]
    async fn deploy_and_verify_reaches_a_real_sshd_and_the_new_key_actually_authenticates() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };

        let mut host = Host::new("keydeploy-test".into(), "127.0.0.1".into(), whoami_user());
        host.port = sshd.port as i64;
        host.auth = HostAuth::KeyFile(sshd.client_key_path());

        let pair = generate_ed25519("bastion-keydeploy-test").unwrap();
        let key_path = save_private_key(&pair.private_key_pem).unwrap();

        // Isolerad sökväg (TestSshd:ns EGEN authorized_keys-fil), inte den
        // riktiga kontots `~/.ssh/authorized_keys` — se
        // `deploy_command_at`s dokumentationskommentar för varför.
        let authorized_keys_path = sshd.dir.join("authorized_keys").to_string_lossy().into_owned();
        let result = deploy_and_verify(
            host,
            None,
            pair.public_key_line.clone(),
            key_path.clone(),
            Some(&authorized_keys_path),
            None,
        )
        .await;
        assert!(result.is_ok(), "deploy+verify misslyckades: {result:?}");

        // Bevisa att raden verkligen hamnade i authorized_keys på riktigt —
        // inte bara att verifieringsanslutningen råkade lyckas av någon
        // annan anledning (t.ex. den ORIGINALA test-nyckeln fortfarande
        // liggandes kvar i samma fil).
        let authorized_keys = std::fs::read_to_string(sshd.dir.join("authorized_keys")).unwrap();
        assert!(authorized_keys.contains(&pair.public_key_line), "den nya nyckeln hittades inte i authorized_keys");

        std::fs::remove_file(key_path).ok();
    }

    /// Samma fristående test-sshd-teknik som `port_forward`/`socks_proxy`,
    /// men `authorized_keys` pekar på HEMKATALOGENS `~/.ssh/authorized_keys`
    /// (inte en fristående fil) eftersom `deploy_command` alltid skriver dit
    /// — testet kör alltså mot den riktiga `$HOME` för den här sandboxens
    /// `claude`-användare, samma sätt som övriga `TestSshd`-baserade tester
    /// redan gör (ingen isolerad hemkatalog per sshd-instans).
    struct TestSshd {
        child: std::process::Child,
        port: u16,
        dir: std::path::PathBuf,
    }

    impl TestSshd {
        fn start() -> Option<Self> {
            let dir = std::env::temp_dir().join(format!("bastion-keydeploy-sshd-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).ok()?;

            let host_key = dir.join("hostkey");
            let status = std::process::Command::new("ssh-keygen")
                .args(["-q", "-N", "", "-t", "ed25519", "-f"])
                .arg(&host_key)
                .status()
                .ok()?;
            if !status.success() {
                return None;
            }

            let client_key = dir.join("client_key");
            let status = std::process::Command::new("ssh-keygen")
                .args(["-q", "-N", "", "-t", "ed25519", "-f"])
                .arg(&client_key)
                .status()
                .ok()?;
            if !status.success() {
                return None;
            }
            let client_pub = std::fs::read_to_string(dir.join("client_key.pub")).ok()?;
            // Den fristående sshd:ns EGEN authorized_keys, i testkatalogen —
            // inte den riktiga kontots `~/.ssh/authorized_keys`. Bara den
            // ORIGINALA test-nyckeln finns här från start; `deploy_command`
            // lägger till den NYA genererade nyckeln i samma fil (eftersom
            // `AuthorizedKeysFile` pekar hit, inte `~/.ssh/authorized_keys`
            // relativt en riktig hemkatalog).
            std::fs::write(dir.join("authorized_keys"), client_pub).ok()?;

            let port = crate::test_support::reserve_port()?;

            let config_path = dir.join("sshd_config");
            std::fs::write(
                &config_path,
                format!(
                    "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\n\
                     PubkeyAuthentication yes\nPasswordAuthentication no\nUsePAM no\nStrictModes no\n\
                     AllowTcpForwarding yes\nPidFile {}\n",
                    host_key.display(),
                    dir.join("authorized_keys").display(),
                    dir.join("pid").display()
                ),
            )
            .ok()?;

            let mut child = std::process::Command::new("/usr/sbin/sshd")
                .args(["-f"])
                .arg(&config_path)
                .args(["-D", "-e"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok()?;

            if !crate::test_support::wait_until_listening(&mut child, port) {
                let _ = std::fs::remove_dir_all(&dir);
                return None;
            }
            Some(TestSshd { child, port, dir })
        }

        fn client_key_path(&self) -> String {
            self.dir.join("client_key").to_string_lossy().into_owned()
        }
    }

    impl Drop for TestSshd {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn whoami_user() -> String {
        std::env::var("USER").unwrap_or_else(|_| "test".into())
    }

    /// Ett distributionskommando som MISSLYCKAS ska rapportera det
    /// verkliga felet — inte det missvisande "deployades men
    /// verifieringen misslyckades" som exitkoden tidigare ignorerades
    /// till förmån för.
    ///
    /// Framkallas genom att peka `authorized_keys` på en sökväg under en
    /// befintlig FIL (`.../authorized_keys/omöjlig`) — då kan varken
    /// `mkdir -p` eller omdirigeringen lyckas, och kommandot avslutar
    /// med nollskild kod, precis som vid en skrivskyddad `~/.ssh` på en
    /// riktig server.
    #[tokio::test]
    async fn a_failing_deploy_command_reports_the_real_error_not_a_verification_failure() {
        let Some(sshd) = TestSshd::start() else {
            eprintln!("hoppar: kunde inte starta en test-sshd i den här miljön");
            return;
        };

        let mut host = Host::new("keydeploy-fail-test".into(), "127.0.0.1".into(), whoami_user());
        host.port = sshd.port as i64;
        host.auth = HostAuth::KeyFile(sshd.client_key_path());

        let pair = generate_ed25519("bastion-keydeploy-fail-test").unwrap();
        let key_path = save_private_key(&pair.private_key_pem).unwrap();

        // `authorized_keys` ÄR en fil — att använda den som KATALOG går inte.
        let impossible = sshd.dir.join("authorized_keys").join("omöjlig").to_string_lossy().into_owned();
        let err = deploy_and_verify(host, None, pair.public_key_line, key_path.clone(), Some(&impossible), None)
            .await
            .expect_err("ett misslyckat distributionskommando ska ge Err");

        assert!(
            err.contains("distributionskommandot misslyckades"),
            "felet ska peka ut distributionen, inte verifieringen, fick: {err}"
        );
        assert!(
            err.contains("deployades INTE"),
            "felet ska vara tydligt med att nyckeln inte kom fram, fick: {err}"
        );

        std::fs::remove_file(key_path).ok();
    }
}
