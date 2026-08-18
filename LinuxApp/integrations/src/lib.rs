//! Integrationer mot system som nås över SSH: Docker, Kubernetes och
//! Proxmox VE.
//!
//! VISION formulerar plugin-systemet som "alla plugins separata paket".
//! Det här är det paketet, och gränsen är inte kosmetisk: enda beroendet
//! är `serde_json` (för `docker compose ls`, som svarar med JSON). Ingen
//! GTK, ingen russh, inget filsystem. En integration får bygga
//! kommandosträngar och tolka utdata — inget annat, och nu är det
//! kompilatorn som ser till det i stället för en kodgranskning.
//!
//! # Varför gränsen dröjde till tre integrationer
//!
//! Med bara Docker fanns inget att generalisera FRÅN. Med Docker och
//! Kubernetes gick det att se vad som var gemensamt (hämta-och-rita-
//! skelettet, extraherat i `refresh_integration_list`) och vad som inte
//! var det (radbyggarna, 126 mot 187 rader). Proxmox blev provet: den
//! lades till utan att skelettet behövde ändras — 352 rader tillagda,
//! noll borttagna.
//!
//! Först då var mönstret mätt i stället för gissat, och först då fanns
//! det något att paketera.
//!
//! # Vad varje modul äger själv
//!
//! Valideringsreglerna, och de är olika av goda skäl:
//!
//! - [`docker`]: containernamn tillåter versaler och punkter, men
//!   image-referenser behöver dessutom `/`, `:` och `@` — två regler i
//!   samma modul, med ett test som visar att de skiljer sig
//! - [`kubernetes`]: RFC 1123-etiketter, alltså bara gemener, siffror och
//!   bindestreck — det API-servern faktiskt accepterar
//! - [`proxmox`]: heltal från 100, eftersom allt adresseras med VMID
//! - [`truenas`]: korta gemena ord (`cifs`, `nfs`), middlewares egna
//!   tjänste-id:n
//!
//! [`unraid`] är undantaget som bekräftar regeln: den är ren LÄSNING och
//! bygger inga kommandon med användarindata, så den behöver ingen
//! valideringsregel alls.
//!
//! Att pressa in dem i en gemensam regel hade gjort den till den
//! lösaste av dem, alltså sämst som injektionsskydd.

pub mod docker;
pub mod kubernetes;
pub mod proxmox;
pub mod truenas;
pub mod unraid;
