# Approval Trust Boundary — Design Decision

**Status:** Proposed; implementation blocked on platform capability review  
**Date:** 2026-11-24  
**Scope:** Authenticated approval evidence, challenge freshness, and trusted-time ownership

## Decision summary

The existing approval challenge is a digest-binding and replay-store prototype, not proof that a human approved an action. A challenge ID, action digest, SQLite row, UI click, or signature produced by a key available to the same untrusted extension process is not sufficient authentication.

The Rust security authority must be the sole verifier and consumer of approval evidence. TypeScript may display the exact action and collect a user interaction, but it must not mint trusted evidence, choose the authoritative timestamp, or call a privileged executor directly. Python/provider output remains untrusted.

No production approval authenticator is selected in this decision. A cross-platform cryptographic scheme must not be implemented until each supported OS has a reviewed user-presence/authenticator mechanism and a defined same-user malware threat model.

## Assets and security goals

- Integrity of the exact action approved: action kind, normalized target, parameters, and relevant target-state digest.
- Authenticity and freshness of the approval assertion.
- Single-use challenge consumption under concurrent requests and process restarts.
- Integrity and confidentiality of approval/audit persistence.
- Clear binding between the displayed proposal, the user's deliberate approval, and the Rust-authorized operation.

## Trust boundaries and roles

| Component | Trust | Allowed responsibility |
|---|---|---|
| AI provider / Python service | Untrusted | Produce bounded proposals only |
| TypeScript extension / webview | Presentation boundary; not an authority | Display canonical proposal, request approval, relay opaque evidence |
| Rust authority process | Policy and verification authority | Create challenge, derive time, verify evidence, consume challenge, authorize operation |
| OS authenticator / protected credential service | Potential trust anchor, platform-specific | Provide reviewed user-presence or protected-key operation |
| SQLite approval store | Local persistence; not independently trusted against same-user tampering | Record challenge lifecycle and replay state |

The OS credential service is not automatically a human-approval proof. A key accessible to any process running as the same user can be invoked by malware in that user session. Platform-specific UI mediation and same-user threat assumptions must be explicit.

## Required protocol shape

1. Rust creates a cryptographically unpredictable challenge nonce and stores it with the canonical action digest, contract version, creation time, expiry, and a one-time state.
2. Rust sends the UI a display DTO derived from the exact canonical action and challenge. The UI must not be able to alter the action or expiry while returning approval evidence.
3. The user reviews the complete security-relevant action and deliberately approves or denies it.
4. A platform authenticator produces an assertion bound to the challenge nonce, action digest, relying-party/application identity, and user-presence/user-verification policy. The assertion format is opaque to TypeScript.
5. Rust validates authenticator identity, relying-party/application binding, nonce, digest, expiry, signature/counter semantics where applicable, and the required user-presence policy.
6. Rust atomically consumes the challenge using an immediate transaction. Any malformed, stale, replayed, mismatched, unsupported, or unverifiable assertion fails closed.
7. Immediately before a controlled write or process operation, Rust revalidates the target identity/content and policy. Approval does not freeze mutable filesystem state.

A denial is terminal for that challenge. Retries require a newly issued challenge. The UI cannot convert a denial into an allow result.

## Time ownership and freshness

- The timestamp passed by TypeScript, Python, or a provider is never authoritative.
- The Rust authority obtains the current time from its own clock interface. The interface exists to make clock behavior testable; callers cannot supply production time.
- Expiry is checked inside Rust at challenge verification and again at the authorization boundary.
- A monotonic clock is used for elapsed-time checks within one process lifetime. Persisted absolute expiry requires wall-clock time and therefore has a clock-rollback limitation across process or machine restart.
- Until a platform-backed rollback-resistant time source is selected, Rust must detect backward movement during its lifetime and fail closed. Across restart, wall-clock rollback remains a documented residual risk; short TTL alone does not solve it.
- Clock synchronization, suspend/resume, VM snapshots, and time changes require platform-specific tests and explicit policy.

## Authenticator selection gates

Before implementation, evaluate each supported platform separately (Windows, macOS, Linux):

- Can the platform produce a user-presence assertion through a trusted OS-owned prompt rather than a webview-supplied boolean?
- Is the private key inaccessible to arbitrary same-user processes, or is same-user malware explicitly outside the threat model?
- Can the assertion bind application identity, challenge nonce, and action digest?
- What are enrollment, recovery, device replacement, remote-desktop, headless, and locked-session behaviors?
- Can the Rust authority independently verify the assertion and revoke/re-enroll credentials?
- What happens when the platform facility is unavailable? Required behavior is deny/fail closed, not silently downgrade to a UI click or unsigned message.

Do not assume that a generic OS keychain, DPAPI, Keychain, Secret Service, or a software-held Ed25519 key alone proves user presence. Credential storage and user authorization are separate properties.

## Required Rust interfaces (design target)

- `ApprovalAuthenticator`: verifies a platform assertion against a Rust-issued challenge and returns a typed authenticated principal plus verified user-presence properties. No boolean `approved=true` from the UI.
- `ApprovalClock`: authority-owned time source with wall-clock and monotonic readings; no production method accepts a caller-supplied `now`.
- `ApprovalChallengeStore`: persists nonce, action digest, authenticator binding, expiry, terminal state, and replay/counter state as required by the selected authenticator.
- `ApprovalVerifier`: orchestrates exact-action binding, authenticator verification, freshness, and atomic consumption; returns a narrow authorization capability rather than a general-purpose allow flag.

These are design names, not implemented or stable public APIs. Do not expose private keys to TypeScript or Python. Do not store raw biometric, PIN, or authenticator secrets in LOCO.

## Failure and downgrade policy

Fail closed for unknown authenticator types, missing enrollment, unavailable platform APIs, invalid signatures, wrong application identity, missing user presence, expired challenges, replay/counter rollback, clock rollback, DB corruption/locking, and action/target mismatch. There is no fallback from authenticated approval to a plain UI click.

The prototype challenge format and SQLite store are not production approval evidence. Existing callers that provide timestamps remain test/prototype-only until migrated.

## Threats not solved by this design alone

- Malware with the same privileges as the logged-in user may control the extension, inject into the Rust process, invoke OS authentication APIs, or tamper with local state depending on platform protections.
- A compromised OS/kernel, malicious administrator, debugger, or physical attacker may bypass local controls.
- A trusted prompt that displays only a generic “approve” message does not prove the user reviewed the exact action.
- Authentication does not prevent stale target state; the writer must revalidate immediately before mutation.
- SQLite WAL/database files need restrictive OS ACLs and a defined corruption/recovery policy; replay correctness is not tamper resistance.

## Implementation exit criteria

1. Written same-user attacker model and platform-specific authenticator decision.
2. Rust-owned challenge generation and clock; no production caller-supplied time.
3. Signed/attested assertion binds application, nonce, exact action digest, and verified user presence.
4. Tests for wrong principal, wrong app, altered action, nonce replay, expired assertion, counter rollback, clock rollback, process restart, and concurrent consumption.
5. Fail-closed behavior when the platform authenticator is absent or unavailable.
6. Security review of credential enrollment, recovery, revocation, IPC peer identity, and DB ACLs.
7. End-to-end test proves UI display → OS-owned user interaction → Rust verification → atomic consumption → target freshness check.

Until these criteria are met, the feature must remain explicitly labeled a prototype and must not authorize production privileged writes or command execution.


## Initial platform API survey — 2026-11-24 (research, not a selection)

Public platform documentation supports investigating a FIDO2/WebAuthn assertion flow, but does not yet establish a common implementation:

- **Windows:** Microsoft's native Win32 WebAuthn API documents Windows Hello platform authentication and external FIDO2 security keys, with a minimum of Windows 10 version 1903. This is a viable candidate for a Rust-native adapter, subject to API version, RP identity, user-verification flags, assertion parsing, and packaging review. Sources: [Microsoft WebAuthn API](https://learn.microsoft.com/en-us/windows/win32/webauthn/-webauthn-portal), [WebAuthn API reference](https://learn.microsoft.com/en-us/windows/win32/api/_webauthn/).
- **macOS:** Apple's LocalAuthentication framework provides OS-mediated biometric/passcode evaluation and can gate Keychain item access. The documented API's high-level result is a pass/fail authentication result; that alone is not a portable, signed assertion binding an exact LOCO action digest. A keychain item protected by user presence may be a building block, but a separate cryptographic operation and application/action binding must be demonstrated. Sources: [Local Authentication](https://developer.apple.com/documentation/localauthentication), [Accessing Keychain items with Face ID or Touch ID](https://developer.apple.com/documentation/localauthentication/accessing-keychain-items-with-face-id-or-touch-id).
- **Linux:** The Credentials for Linux `libwebauthn` project describes WebAuthn/FIDO2 support over pluggable transports and a D-Bus portal direction, but its published feature table marks TPM 2.0 platform authentication as planned. This is not evidence of a universally installed OS-owned platform authenticator. Availability, portal support, distribution packaging, and user-presence semantics need independent validation. Source: [Credentials for Linux libwebauthn](https://github.com/linux-credentials/libwebauthn).

**Research conclusion:** WebAuthn/FIDO2 is a promising protocol candidate because it can bind a fresh challenge to a relying-party identity and return a verifiable assertion. It is not yet approved as LOCO's cross-platform authenticator. Native platform API parity, origin/RP identity strategy for a local desktop application, user-verification policy, transaction-display limitations, and Linux availability remain open. A generic FIDO assertion generally proves authenticator participation for an RP challenge; it does not necessarily display or cryptographically attest the full human-readable command/file diff. LOCO must display the exact action in its own trusted UI boundary and must not overstate what the authenticator attests.

No dependency, native API, key enrollment flow, or production code is selected by this survey.


## Rust-owned clock implementation checkpoint — 2026-11-24

PR #83 introduces a process-wide Rust wall-clock guard and changes the SQLite store API so challenge issuance and consumption obtain time internally. Challenge issuance accepts a bounded TTL (1–300 seconds) and computes absolute expiry inside the store; consumption no longer accepts a caller-supplied timestamp. Backward wall-clock movement during the running process returns an error and fails closed.

This does **not** solve wall-clock rollback across process or machine restart. The persisted expiry is still Unix wall time. It also does not make challenge IDs unpredictable, authenticate a user, or protect the SQLite file from same-user tampering. The caller currently supplies the challenge ID; cryptographically random nonce generation and platform-authenticator assertions remain separate work.

The clock abstraction has deterministic tests for backward movement, expiry, range overflow, and clock failure. Exact-head cross-platform CI is required before treating this checkpoint as validated.


## Cryptographic challenge nonce checkpoint — 2026-11-24

PR #84 changes the store to generate a fresh 256-bit challenge identifier from the operating system random source using `getrandom`. Callers no longer choose the challenge ID. The returned identifier is 64 lowercase hexadecimal characters and remains bound in SQLite to the canonical action digest and store-owned expiry.

This improves unpredictability and prevents caller-selected ID collisions, but is not itself authenticated approval. The nonce is not a signature, the UI must treat it as opaque, and a future platform authenticator must bind its signed assertion to this nonce, the relying-party/application identity, and the action digest. Entropy-source failure fails closed. The database is still not protected from same-user tampering by this change.


## Process-restart invalidation checkpoint — 2026-11-24

PR #85 adds a random process epoch to the approval database. On authority startup, Rust rotates the active epoch. Challenges are stored with the epoch that issued them; consumption requires both the issuing store handle and persisted singleton runtime epoch to match. Starting a new authority process therefore invalidates all outstanding challenges from the previous process, even if the wall clock moved backward while the application was stopped. A stale still-running store handle also fails closed after another authority process takes over.

This is a single-active-authority design: opening a second authority against the same database intentionally revokes the first authority's outstanding challenges. It does not protect against an attacker who can rewrite the SQLite database, forge its epoch rows, or compromise the authority process. OS ACLs, IPC peer identity, and a reviewed authenticator remain required.


## Monotonic in-process challenge deadlines — 2026-11-24

The follow-on approval-store change keeps a process-local monotonic deadline for each challenge issued by the current authority. Issuance records both the existing Unix expiry (for the persisted record and diagnostics) and a deadline derived from a monotonic elapsed-time source. Consumption requires both checks to pass. The monotonic deadline is held only in memory; a challenge without a matching in-memory deadline is rejected. The process epoch from PR #85 continues to invalidate all challenges across restart, so no monotonic clock value is persisted or compared across processes.

This narrows reliance on wall-clock time during a process lifetime, but does not make the wall clock trusted against reboot/OS-level manipulation, does not protect SQLite against same-user tampering, and does not authenticate user approval. The challenge store remains a prototype and cannot authorize production writes or commands.


## Platform authenticator capability review — 2026-11-24

**Outcome: no cross-platform authenticator is selected.** The public platform APIs provide useful building blocks, but they do not yet establish one equivalent, portable transaction-approval contract for LOCO. Implementations remain blocked on the application identity/RP-ID strategy, transaction-display threat model, enrollment/recovery model, and adversarial same-user process analysis.

### Windows

Microsoft documents the Win32 WebAuthn API as supporting Windows Hello platform credentials and external FIDO2 security keys. The documented runtime floor is Windows 10 version 1903 and later. WebAuthNAuthenticatorGetAssertion is a native assertion ceremony with a window handle for system UI, RP ID, client data, and assertion options; the options include an explicit user-verification requirement. This is the strongest directly documented native-app assertion path in the reviewed set.

Implementation gates:
- Set user verification to required and reject assertions that do not satisfy the requested policy; verify the RP ID hash, client-data challenge, expected client-data origin/type, signature, credential identity, and signed UP/UV flags using a reviewed WebAuthn verifier.
- Decide a stable RP ID and native-app client-data origin strategy. Do not use an arbitrary hostname or localhost without a standards and platform compatibility review.
- Confirm the supported API version and Windows build behavior for the exact assertion fields and user-verification policy LOCO requires. Keep a runtime capability check and fail closed if unavailable.
- Treat the OS prompt as presence/verification mediation, not proof that the OS authenticator independently displayed the full command or file diff. LOCO must render the canonical action separately and bind the signed client-data challenge to the Rust-generated nonce and action digest.

### macOS

Apple LocalAuthentication (LAContext) can evaluate biometric or device-passcode policies and returns success/failure to the app. Security framework access-control flags include userPresence, biometryCurrentSet, and privateKeyUsage; Apple documents combining access control with a Secure Enclave private key so signing occurs within the Secure Enclave and use can require biometric/passcode authorization.

This is a promising native protected-key building block, but it is not a WebAuthn assertion API with an RP ID and standard authenticator-data UP/UV fields. LOCO would need a separately specified signed envelope containing at least the challenge nonce, canonical action digest, protocol version, and application identity. The enrolled public key and access-control policy must be managed by the native app; key access groups, code-signing identity, app updates, key replacement, biometric enrollment changes, passcode fallback, and recovery all need explicit treatment. The app-provided localizedReason is a system authentication prompt reason, not a trusted display of a complete transaction; do not claim transaction confirmation merely because LocalAuthentication succeeded.

### Linux

Yubico libfido2 supports FIDO U2F/FIDO2 communication with external devices over USB or NFC and provides assertion-signature verification. Its documented platform support includes Linux, macOS, and Windows, but that portability describes the library/device protocol, not a uniform built-in Linux platform authenticator. Linux packaging also has native dependencies (including libcbor, OpenSSL 3, zlib, and libudev) and may require device-access udev rules. The tooling exposes explicit user-presence and user-verification requests/checks.

This provides a viable optional roaming-key adapter candidate. It does not guarantee that a Linux workstation has a compatible device, a uniform desktop-owned prompt, or a native biometric/TPM assertion service. No universal Linux platform-authenticator API is selected here. Missing device, denied device permissions, unsupported CTAP capabilities, absent UP/UV, or verifier failure must result in denial; never downgrade to a UI click, a Secret Service lookup, or a software key without an equivalent reviewed presence boundary.

### Cross-platform protocol requirements

The WebAuthn standard distinguishes user presence (UP) from user verification (UV). UP indicates an authenticator performed a presence test; UV indicates local authorization of the authenticator operation, potentially by PIN, password, or biometric. LOCO's privileged actions should require UV, and should require UP where the selected authenticator supports it. Rust must verify the signed authenticator data, RP ID hash, expected client-data challenge/type/origin, credential public key, and assertion signature; it must not trust a UI-supplied verified=true flag. WebAuthn UV is not a guarantee of a unique natural person's identity, and neither UP nor UV guarantees the authenticator displayed LOCO's full action summary.

### Decision and next gate

1. Define LOCO's supported OS/version matrix and whether external FIDO2 keys are a required Linux fallback or optional capability.
2. Specify the exact action-approval envelope and desktop RP/application identity strategy before selecting WebAuthn versus a macOS protected-key envelope.
3. Define enrollment, credential rotation/revocation, recovery, multiple local users, remote desktop, locked/headless session, and authenticator-unavailable behavior.
4. Threat-model same-user malware that can call native APIs, control the extension/webview, or spoof app UI. Identify what each OS prompt and protected-key boundary actually prevents.
5. Prototype each adapter independently with captured assertion fixtures and negative tests; no adapter may issue authorization directly. Rust performs all cryptographic verification and atomic challenge consumption.
6. Until these gates are reviewed, all three platform paths remain design candidates only; the Rust authority must fail closed when authenticated evidence is unavailable.

### Primary platform references

- Microsoft, [WebAuthn API overview and runtime requirements](https://learn.microsoft.com/en-us/windows/win32/webauthn/-webauthn-portal) and [GetAssertion](https://learn.microsoft.com/en-us/windows/win32/api/webauthn/nf-webauthn-webauthnauthenticatorgetassertion).
- Microsoft, [GetAssertion options and user-verification requirement](https://learn.microsoft.com/en-us/windows/win32/api/webauthn/ns-webauthn-webauthn_authenticator_get_assertion_options).
- Apple, [LocalAuthentication LAContext](https://developer.apple.com/documentation/localauthentication/lacontext), [Secure Enclave key protection](https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave), and [SecAccessControl flags](https://developer.apple.com/documentation/security/secaccesscontrolcreateflags).
- Yubico, [libfido2 capabilities, platform support, and dependencies](https://github.com/Yubico/libfido2) and [assertion UP/UV behavior](https://github.com/Yubico/libfido2/blob/main/examples/README.adoc).
- W3C, [Web Authentication Level 3: UP, UV, RP ID, and assertion verification](https://www.w3.org/TR/webauthn-3/).

This review is a capability survey, not a certification or security audit of any platform. Native OS versions, API behavior, entitlement/signing requirements, and hardware configurations must be tested on supported release targets before implementation.
