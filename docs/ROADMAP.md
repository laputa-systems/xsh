# XSH roadmap

Work proceeds in this order. Each live doc holds the campaign's status, resume
instructions, and open items.

| # | Campaign or milestone | Status | Live doc | Next step or reason |
|---|---|---|---|---|
| 1 | G0 release candidate | done | [G0](#g0-release-candidate) | Tagged `xsh-g0-rc1` at `b268b5007`. |
| 2 | Pyramid: scoped verification | active | [pyramid-campaign.md](../pyramid-campaign.md) | Record the P0 timing, memory and size baselines in a quiet window; that is the last open item. |
| 3 | Inference: general type, row, and effect inference | active | [full-inference-campaign.md](../full-inference-campaign.md) | Close the two open G3 items, then reduce annotations in G4. |
| 4 | Lean: connected compiler proof | paused | [xsh-lean.md](../xsh-lean.md) | Paused by the owner on 2026-10-15. Resumes only on an explicit order, after the source freeze. |
| 5 | Performance and deployment | paused | [CAMPAIGN-PERFORMANCE-DEPLOYMENT.md](CAMPAIGN-PERFORMANCE-DEPLOYMENT.md) | Paused on 2026-10-25. G0 defers its performance obligations. XSH-1, XSH-3, XSH-14 and XSH-19 stay open in [TODO.md](../TODO.md). |
| 6 | Deser cutover | done, qualification open | [TODO.md](../TODO.md) | Correctness and platform qualification are open items there. |
| 7 | Supervised DHCPv4 and syslog (XSH-53, XSH-54) | active | [TODO.md](../TODO.md) | Tracked there and in the [XSH-53 RFC](rfc/XSH-53-supervised-dhcp-and-syslog.md). |
| 8 | util-linux cohort, btop, system-report | done | [util-linux README](../dev/util-linux/README.md), [BTOP.md](BTOP.md), [SYSTEM-REPORT.md](SYSTEM-REPORT.md) | The util-linux resource rerun is XSH-1. |

When inference changes a Pyramid input or contract, rerun the affected checks.
Lean's source-bound correspondence uses the qualified source available after
the preceding work. Model proofs, finite witnesses, and Rust correspondence
remain distinct evidence.

The Deser cutover remains in place. Its performance thresholds are waived;
outstanding correctness and platform qualification are not passes.

## G0 release candidate

G0 is complete. The candidate is the single immutable XSH commit `b268b5007`,
tagged `xsh-g0-rc1`, with its xsh, xsht, xshi, and core script products.

G0 stabilizes `master`. It admits only these changes:

- correctness, data-loss, and resource-ownership repairs;
- fixes that Laputa's G0 consumers need;
- test and documentation corrections;
- verification repairs without which a result cannot be trusted.

New APIs, codecs, language changes, speculative optimization, and verification
features wait for a later milestone. An inference failure belongs to G0 only
when it breaks a documented language guarantee or a Laputa program.

XSH-53 and XSH-54 (supervised DHCPv4 and syslog) add APIs on `master` under the
owner's campaign authorization. They are outside the frozen G0 candidate.

A result counts only for the executable and source it ran. Any later executable
change reruns the affected qualification.

### Scope exclusions

- Performance and resource gates are deferred. XSH-1 and XSH-3 stay open and do
  not block G0. The owner waived the quiet-host requirement on 2026-10-28, and
  no G0 claim depends on a timing.
- The graphical profile is outside G0, so Laputa's Muon build blocker does not
  gate it.
- macOS, Linux aarch64, and Asterinas are outside the G0 claim.

### Recorded result

All four completion criteria hold for `b268b5007`.

1. **Native failures.** Every failing selector in the last full native run has a
   recorded cause, a focused reproducer, and a repair or an out-of-scope reason.
2. **Combined run.** A privileged run on `8d7330542`, whose products and tests
   are identical to the candidate's, passed 14,588 tests with 151 skips and one
   incomplete. The incomplete gate test saw commits land on the checkout during
   the run and passes on a quiet tree. Of the skips, 72 are the
   private-namespace lane's tests, which pass 72 of 72 on the same products. The
   other 79 need root, external network or TLS fixtures, or tools absent from
   the image.
3. **Laputa suites.** Laputa's check, test, PM, xinit, and system suites pass
   with the candidate's products, with recipes that rebuild rather than reuse
   cached artifacts.
4. **Laputa qualification.** An earlier `make verify` with the candidate's
   seed found no XSH defect and eight Laputa defects, fixed through Laputa
   `9485988`. One uninterrupted run at Laputa `7d550bc` then passed every step.

Identities of the passing run:

- **Seed:** XSH `b268b5007`, clean, `release, lto=false`, x86_64-musl. Product
  sha256 prefixes: `xsh` `abb9e5cffd31e9ec`, `xsht` `4e964e4271999830`, `xshi`
  `f6355b0fca563977`.
- **Kernel:** artifact `3c602079f1ed…`, rebuilt from clean.
- **Profile:** `qemu-dwl-foot` generation `d37ec0bbb131…`.
