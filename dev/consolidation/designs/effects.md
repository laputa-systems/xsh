# Design: socket calls need `net`

Status: approved by the owner 2026-10-11 (drafted 2026-10-10). Part of workstream 8 of
`../CAMPAIGN.md`. Carries decision D6.

## Problem

`Effect::from_module_call` charges every call in the `process`, `unix`, and
`linux` modules the `process` effect. `linux.socket`, `linux.connect`, and
`linux.sendto` are in `linux`, so XSH code can open a socket and send on it
inside `without net { ... }`, provided `process` is allowed. SPEC 9.6 says a
`without` region is "a claim the checker proves about XSH code". This is XSH
code, so the claim is false today.

That is separate from `run curl ...` inside `without net`, which the SPEC
names and allows: what an external program does is outside the claim.

## Contract

- A `linux` or `unix` function that creates, binds, connects, listens on,
  accepts from, sends on, or receives from a socket of an address family
  that can reach another host (`AF_INET`, `AF_INET6`, `AF_PACKET`) is charged
  both `process` and `net`. A function that takes the family as a run-time
  argument is charged both.
- Netlink sockets and `AF_UNIX` sockets stay `process` only. They reach the
  kernel and local processes, which `net` does not describe.
- No region becomes more permissive: `without process` excludes these calls
  as before, and `without net` now excludes them too.
- A proc with a declared clause that makes such a call and does not list
  `net` now reports `check.effect-violation`. Each one in this repository and
  Laputa gets `net` added to its clause; the integrator lists them in the
  handoff log. Inferred procs need no edit.
- Effects stay static claims. No run-time enforcement is added (D6).

The lane derives the list of functions from the registry by the first rule
and records it in the SPEC table in 9.6.

## Shape

`Effect::from_module_call` returns one effect today. It returns a small set,
since one call can now need two. Its two readers, the checker and lint, take
the set.

## Tests

- Rejected programs: `linux.socket` and `linux.connect` under
  `without net`, and in a proc declared `[process]`.
- Accepted: the same under `without fs`, and a netlink dump under
  `without net`.

## Relies on

- `Effect::from_module_call` maps the modules `process`, `unix`, and `linux`
  to `process`, with the exceptions it lists by function name.
- SPEC 9.6 states the `without` claim in the words quoted above.

## Out of scope

Any effect finer than the seven that exist; any run-time sandbox.
