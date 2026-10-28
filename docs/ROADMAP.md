# XSH roadmap

Work proceeds in this order:

1. **G0 release candidate:** qualify the existing supported contract on
   headless Linux x86_64 against Laputa's G0 baseline. See [G0](#g0-release-candidate).
2. **Pyramid:** complete scoped verification, including accurate selection,
   results, source and product binding, resource accounting, and representative
   workloads. See [the Pyramid campaign](../pyramid-campaign.md).
3. **Inference:** complete general type, row, and effect inference and its
   compiler and corpus acceptance. See [the inference campaign](../full-inference-campaign.md).
4. **Lean:** establish a connected compiler proof and execution correspondence,
   then continue the full-core obligations. See [the Lean campaign](../xsh-lean.md).

The Pyramid, inference, and Lean campaigns stay paused until G0 is decided.
When inference changes a Pyramid input or contract, rerun the affected checks.
Lean's source-bound correspondence uses the qualified source available after
the preceding work. Model proofs, finite witnesses, and Rust correspondence
remain distinct evidence.

The Deser cutover remains in place. Its performance thresholds are waived;
outstanding correctness and platform qualification are not passes.

## G0 release candidate

G0 stabilizes `master`. It admits only these changes:

- correctness, data-loss, and resource-ownership repairs;
- fixes that Laputa's G0 consumers need;
- test and documentation corrections;
- verification repairs without which a result cannot be trusted.

New APIs, codecs, language changes, speculative optimization, and verification
features wait for a later milestone.

An inference failure belongs to G0 only when it breaks a documented language
guarantee or a Laputa program. Other inference limits stay with the inference
milestone.

XSH-53 and XSH-54 (supervised DHCPv4 and syslog) add APIs on `master` under the
owner's campaign authorization; they are outside the frozen G0 candidate.

Performance and resource gates are deferred for G0. XSH-1 and XSH-3 stay open
and do not block it. The owner also waived the quiet-host requirement on
2026-10-28; no G0 claim depends on a timing.

The graphical profile is outside G0, so Laputa's Muon build blocker does not
gate it. macOS, Linux aarch64, and Asterinas are outside the G0 claim.

The candidate is one immutable XSH commit with its xsh, xsht, xshi, and core
script products. G0 is complete when:

1. every failing selector in the latest full native run has a recorded cause, a
   focused reproducer, and a repair or an explicit out-of-scope reason;
2. one combined run on the candidate products has no unexplained required
   failure and no unaccounted required skip;
3. Laputa's check, test, and PM, xinit, and system suites pass with those
   products, including recipes that actually rebuild rather than reuse cached
   artifacts;
4. Laputa's full G0 qualification passes with the same products, and its seed,
   kernel, and profile are recorded.

A result counts only for the executable and source it ran. Any later executable
change reruns the affected qualification.

### G0 status

Of the 59 failures in the last recorded full native run, 47 still reproduced
on `master`; all 47 are fixed. A full native run on `f39195a4e`
then had 3 failures, introduced by the temporary-root fix and since repaired,
and 215 skips. The 64 skips above the recorded run need user namespaces and
pass in a privileged container. All 72 util-linux private-namespace tests
pass in that lane's namespace.
The remaining skips need root, external fixtures, or tools absent from the
image.

With verification products from `387259402`, Laputa `d4f8a03` passes
`make check`, `test-xinit`, `test-linux`, and `tests/integration`. `test-pm`
and `test-system` each have one failure that Laputa's frozen XSH snapshot
reproduces: Laputa `2549eb0` added `ca-certificates` to tailscale's runtime
dependencies without updating `tests/pm/repository_keys.xsh` and
`tests/system/profile.xsh`. Criterion 3 still needs release products and
recipes that rebuild rather than reuse cached artifacts.

Criterion 2 is met for candidate `b268b5007`. A privileged combined run on
`8d7330542`, whose products and tests are identical to the candidate's, passed
14,588 tests with 151 skips and one incomplete; the private-namespace lane on
the same products passed 72 of 72. The incomplete gate test saw commits land on
the checkout during the run and passes on a quiet tree. Of the skips, 72 are the
lane's tests; the other 79 need root, external network or TLS fixtures, or
tools absent from the image.

Laputa `make verify` with the candidate's release seed (`b268b5007`, clean,
x86_64) found no XSH defect. It found eight Laputa ones, now fixed through
Laputa `9485988`: the mirror's git dependency lost between fetch and build,
installer modules shadowing `/usr/bin/trust`, a partition table in a shape
`linux.write_partition_table` stopped accepting on 2026-10-06, a fixed CI
disk too small for the target root, a root partition ending in an unusable
ext4 group, `ca-certificates` missing from the target root, PM fixture
repositories without the trust sources, and the dwl proof cancelling its
compositor before the screenshot. After a clean rebuild of all 84 packages
(kernel `3c602079…`), `check`, `test`, `profile-build`, `profile-test`
(`qemu-dwl-foot`), `mirror-test` and the no-op rebuild pass. The installer
partitions, installs and boots the target, then fails its SSH smoke check:
the target's login shell is `/bin/xshi`, which the default seed builds as the
documented stub, so remote commands cannot run. `mirror-test` also needs a
host `jq`. Criterion 3 is met; criterion 4 waits on the login-shell decision
and one uninterrupted `make verify`.

