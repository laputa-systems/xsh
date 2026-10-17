# Util-linux acceptance adapter

The oracle is util-linux 2.42.4-r3 in the campaign's pinned x86-64 Linux image.
`inventory.tsv` lists the 117 installed executable paths owned by packages
whose APK origin is `util-linux`. The APK source commit is
`087ea48145e24307010ba91f403c5ff8f402568c`.

The [package build recipe](https://github.com/alpinelinux/aports/blob/087ea48145e24307010ba91f403c5ff8f402568c/main/util-linux/APKBUILD)
disables translations, uuidd, kill, chfn and chsh. The non-bootstrap build
enables runuser, Python bindings and libeconf. Login utilities are a separate
package absent from this image. Runtime capability proofs qualify kernel and
namespace operations separately from these build features.

The inventory separates cohort membership from implementation disposition.
`partial` means existing native command code without a full reference proof.
`absent` means no native implementation was established at this baseline.
Neither disposition grants parity credit. Outside-cohort commands remain
inventoried without joining this campaign's behavior denominator.

`python3 dev/util-linux/check.py inventory --check` compares the live package
inventory and all five exact reference banners against the recorded inventory.
`python3 dev/util-linux/check.py harness --self-test` runs lease and cleanup
controls, a real TERM-during-container control, and exact native comparison
controls. Missing evidence fails acceptance; it cannot produce an empty pass.

The adapter resolves shared leases through the original Git common directory.
Every Docker execution holds the original shared performance guard and exclusive
native guard. Acquisition is nonblocking. A busy guard exits 75. Scheduling
priority belongs to the coordinator's dispatch protocol.

Unresolved container cleanup writes the existing native lock's quarantine
prefix and its shared cleanup marker before releasing guards. The adapter
preserves the lock inode and retains the affected scratch directory.

Native tests use `support.uu` scenes and `support.util_linux` reference capture.
Comparisons preserve stdout and stderr bytes, arguments, statuses and explicit
fixture-state snapshots. No identifiers or whitespace are normalized. Reference
and candidate observe the same fixture. The sole provenance exception asserts
each implementation's exact version banner independently.

Protected consolidation products are reusable only while all three product
hashes and the Rust, Cargo and helper inputs match their source revision.
Native execution checks that qualification before and after each case.
The source and product container mounts are read-only. Owned scratch uses `/t`.

`baseline --check` executes all five reference and candidate version paths,
accounts for missing candidate commands explicitly, and samples candidate peak RSS.
The selector has a 30-second execution bound. Final timing gates remain separate.

Signal-aware `observe_reference` and `observe_candidate` preserve the distinction
between a signal and its shell exit-code convention. The exact native control
distinguishes SIGTERM from ordinary exit 143. The private runner proves changed
IPC, mount and PID namespaces, fresh procfs, and two unprivileged identities
with zero effective capabilities.

Lane admission requires the common controls and that lane's own positive boundary
proofs. Rooted memory and private bind proofs pass. Lock and IPC fixture lifetime
qualification includes normal release and cancellation. Full cohort acceptance
still requires every integrated command comparison and closing gate.


`resources --check` measures binary size, source size, peak RSS, command and
startup CPU, and build CPU. Baseline and candidate products must use comparable
release configurations; verification products fail qualification before sampling.
Readiness checks bracket every timing pair. Only the readiness helper's busy
status defers timing; other errors fail. A deferred measurement prevents closure.
Build execution also requires the original exclusive Cargo guard.

`close --check` executes the cohort, positive boundaries, focused regressions,
and resource checks, and requires source-bound results from the full native,
host, repository, documentation, review, and cleanup callbacks. Missing callbacks
and changed source fail closure.

Known fast functional regressions may use `native_batch`: at most four distinct
exact selectors share one container bounded to 30 seconds. Each framed case must
report exactly one pass and no skips. Batch results provide functional evidence;
resource measurements use their own observed process samples.
