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
