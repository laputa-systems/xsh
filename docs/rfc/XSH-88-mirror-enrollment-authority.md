# XSH-88: Mirror enrollment authority

Status: accepted

## Summary

Require authority over an account before registering a passkey for it. Use an
expiring, single-use authorization issued by the mirror operator for the first
credential. Require an authenticated session for the same user when adding
another credential. Keep the username allowlist as an eligibility check.

Implementation belongs in `../laputa/mirror/`. This draft records the decision
needed by [XSH-88](../../TODO.md); it changes no running service.

## Motivation

The reviewed [registration handlers][handlers] accept an allowlisted username,
reuse its existing user record, and verify a new WebAuthn ceremony without
checking prior account authority. Successful registration creates a browser
session. The [token endpoint][tokens] accepts that session and can issue a
publisher token even when the account already has tokens.

WebAuthn proves possession of the newly registered key and binds the ceremony
to the relying party. It does not establish that this caller may add a key to
the chosen account. The documented [Cloudflare ingress][ingress] routes the
public hostname to the mirror without defining an Access policy. An external
policy may exist, but this review did not inspect deployment state or exercise
the service. Runtime exploitation remains unproven.

Publisher authority can change the unsigned developer index and disrupt
delivery. Production clients still require valid offline snapshot signatures
and the bound index digest under the [PM trust contract][trust]. Enrollment
must preserve that separate trust boundary.

## Design

### Authorize both ends of registration

| Account state | Required authority |
|---|---|
| No credential | Operator-issued authorization bound to this user |
| Existing credential | Authenticated session for this user |
| Lost credentials | Explicit operator recovery; no anonymous enrollment fallback |

Create a pending registration only after checking that authority. Store the
target user, challenge, expiry, and authority identity together. At verification,
check the WebAuthn response and recheck the authority and its binding before
persisting the credential. A session for another user is insufficient.

Consume the pending challenge on every successful registration, including
additional credentials. Make that consumption, any bootstrap-token consumption,
and credential persistence one database transaction. Concurrent verification
must have at most one successful consumer. Failure must not leave a usable
credential attached without its corresponding authorization being consumed.

### Bootstrap through operator access

Add a local administrative operation that issues a high-entropy authorization
for one allowlisted user with a short expiry. Persist only its verifier, and
deliver the secret through the operator's existing administrative channel.
Submit it in the registration request body; keep it out of URLs and logs.
An absent or expired authorization fails closed. There is no shared permanent
enrollment password and no first-visitor-wins window.

The server may permit several challenges to be prepared from one authorization,
but only one credential commit may consume it. Reissuing or revoking an
authorization invalidates its pending uses. Existing authenticated users add
credentials through the same bound challenge flow, without bootstrap access.

Keep ordinary login and publisher-token scope unchanged. Update the mirror
setup and account-management documentation with the implemented bootstrap and
recovery procedure before enabling enrollment again.

### Acceptance

Use a local mirror fixture and test authenticator. Verify authorized first
enrollment and same-user additions, then reject anonymous registration for an
existing user, cross-user sessions, absent or expired bootstrap authority,
revoked authority, replayed challenges, and concurrent verification of the same
authorization. A failed transaction must leave neither a credential nor a
browser session that grants account access. Retain the existing production
snapshot signature and index-digest rejection checks.

## Alternatives considered

- **Local credential provisioning only:** avoids a bootstrap bearer secret but
  requires a suitable local WebAuthn enrollment workflow. Prefer this if the
  operator does not need remote first enrollment.
- **Identity-provider access policy:** can add protection at ingress, but makes
  correct account enrollment depend on deployment policy outside this server.
  The application would still need a verified identity-to-user binding.
- **Username allowlist or empty-account check:** identifies eligible accounts
  but does not establish who owns them. Neither closes the first-enrollment
  takeover path.

## Open questions

1. **Bootstrap channel:** use a local command that issues a one-use authorization
   for remote enrollment, or provision the credential locally. Recommend the
   one-use authorization because it retains browser enrollment without adding
   an identity-provider dependency.
2. **Existing account rollout:** determine whether the deployed credential and
   token inventory is trusted. Recommend operator review before reopening
   enrollment; revoke uncertain credentials or tokens explicitly. The source
   review establishes no past compromise and authorizes no automatic deletion.
3. **Additional-key session age:** accept any current session or require recent
   WebAuthn authentication. Recommend recent authentication, with the lifetime
   chosen alongside the existing session policy.

## Decision

Accepted by the owner on 2026-11-13. A local operator command issues a
one-use, user-bound, expiring authorization for first enrollment. Browser
registration consumes that authorization and the pending challenge atomically
with credential persistence. Additional keys require recent same-user WebAuthn
authentication. Existing credentials and tokens are not revoked automatically;
the rollout lists them for operator review.

[handlers]: https://github.com/laputa-systems/laputa/blob/bfe60a9ac40f55369c6e33bd0d80ae5e96a04f3a/mirror/src/webauthn_handlers.rs#L26-L158
[tokens]: https://github.com/laputa-systems/laputa/blob/bfe60a9ac40f55369c6e33bd0d80ae5e96a04f3a/mirror/src/webauthn_handlers.rs#L480-L516
[ingress]: https://github.com/laputa-systems/laputa/blob/bfe60a9ac40f55369c6e33bd0d80ae5e96a04f3a/infra/mirror/cloudflare.sh#L197-L209
[trust]: https://github.com/laputa-systems/laputa/blob/bfe60a9ac40f55369c6e33bd0d80ae5e96a04f3a/docs/PM.md#L394-L432
