# CI och build

PR-CI producerar kontexterna `CI / android`, `CI / windows`, `CI / linux`, `CI / swift-linux`, `CI / apple` och `scope-policy`.

De fem plattformsgaterna kör verifiering för respektive plattform. `scope-policy` verifierar filscope för namngivna `platform/*`-grenar och `core/swift`; vanliga kortlivade grenar har inget särskilt filscope.

CLI- och LinuxApp-paketering för `.deb` och `.rpm` kör kompletterande build- och install-smoke-verifiering.

`TestFlight` är manuellt och separat från PR-CI. Signering och App Store Connect-data kommer endast från Actions secrets.

`.github/workflows/osv-scanner.yml` kör kompletterande dependency scanning.
