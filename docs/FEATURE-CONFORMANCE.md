# Cross-platform feature conformance

This matrix is the lightweight product-level conformance view required by the Bastion product program in #389. `ROADMAP.md` remains the detailed implementation history and source for planned work; this file is intentionally compact and should be updated when a focused product PR changes a row below.

## Status legend

- **Yes** — the normal user workflow is implemented and has repository evidence at the appropriate layer.
- **Partial** — useful implementation exists, but the normal workflow or platform parity is incomplete.
- **Gap** — a known material capability is not implemented for that platform.
- **N/A** — intentionally not applicable to that platform.
- **Verify** — implementation exists or is documented, but production-use verification is still an explicit roadmap gate.

A screen or code path alone is not enough for **Yes**. When evidence is incomplete, use **Partial** or **Verify** rather than inferring parity.

## Core workflow matrix

| Capability | Apple (iPhone/iPad/macOS) | Android | Linux | Windows | Evidence / intentional difference |
|---|---|---|---|---|---|
| Native client and SSH session | Yes | Yes | Yes | Yes | Native platform stacks are maintained separately; repository CI has platform-specific gates. |
| Password authentication | Yes | Yes | Yes | Yes | Core SSH authentication is implemented across the maintained clients. |
| SSH key authentication | Partial | Partial | Partial | Yes | Swift and Linux currently have material encrypted-key/RSA limitations; key formats and secure-storage integration are platform-specific, and Android parity remains incomplete. |
| Host-key verification / known hosts | Yes | Yes | Yes | Yes | Host-key verification is a security boundary and must not be weakened for parity. |
| Host organization, tags and search | Yes | Partial | Yes | Yes | Linux and Windows have explicit grouping/search implementations; keep Android conservative until its complete normal workflow is verified. |
| SSH config import | Yes | Partial | Yes | Partial | Shared semantics include `Include`, `Match`, `ForwardAgent`, `RemoteCommand` and `ProxyJump`; platform import UX differs. |
| Interactive terminal | Yes | Yes | Yes | Yes | Terminal engines are intentionally native/platform-specific rather than one shared UI/runtime. |
| Connection liveness / silent-death detection | Yes | Partial | Yes | Yes | Windows uses response-bearing `keepalive@openssh.com` probes; Swift and Linux have their own tested mechanisms. Android requires separate parity verification. |
| System/server dashboard | Yes | Partial | Yes | Yes | Dashboard field sets are maintained per native client; Android parity remains to verify. |
| Docker workflows | Yes | Partial | Yes | Yes | Normal list/action/log/shell workflows exist on the principal desktop/Apple implementations. |
| Port forwarding | Yes | Partial | Yes | Partial | Local/remote/dynamic forwarding is implemented in established stacks; verify remaining native UX parity before promoting all platforms to Yes. |
| ProxyJump | Yes | Partial | Yes | Partial | Shared product behavior exists, but platform UI and integration coverage differ. |
| SFTP / file management | Partial | Partial | Partial | Partial | Keep as Partial until each platform's normal browse/transfer/edit workflow is demonstrated and documented. |
| Snippets / command library | Yes | Partial | Yes | Partial | Capability exists; native UX parity is not yet demonstrated across all four columns. |
| Encrypted synchronization | Yes | Partial | Yes | Partial | LWW/tombstone merge and AES-256-GCM/PBKDF2 are established; provider/native UX parity remains platform-specific. |
| Native secure storage / biometrics | Yes | Partial | Partial | Partial | Use each platform's native secure facility; do not manufacture parity by sharing secret storage through a cross-platform abstraction. |
| Keyboard shortcuts / command palette | Yes | Partial | Yes | Partial | Desktop power-user workflows are platform-specific; Android touch UX is intentionally different where appropriate. |
| Accessibility and adaptive native layout | Partial | Partial | Partial | Partial | This remains a continuous conformance requirement rather than a one-time implementation checkbox. |
| Packaging / release validation | Verify | Verify | Verify | Verify | Apple has `.github/workflows/testflight.yml`; Linux has dedicated packaging workflows; Android and Windows have platform build CI. Keep all four at **Verify** until successful release-path validation is documented rather than inferring it from workflow presence. |

## Known verification gates

1. OAuth account/provider integration on Apple still requires the real Xcode/provider-client-ID verification described in `ROADMAP.md`.
2. Secret material and credentials must never be added merely to make a matrix row pass; native secure-storage and external-provider verification remain real gates.
3. A platform row moves to **Yes** only when its normal user workflow is usable, tested at the appropriate layer, and documented.
4. Intentional platform differences should be described in the evidence column rather than hidden as apparent parity.

## Maintenance rule

Every focused PR under #389 that materially changes one of these capabilities should update this matrix in the same PR. When code and this matrix disagree, inspect the implementation and tests first, then correct the documentation rather than treating the matrix as executable truth.
