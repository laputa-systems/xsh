# Util-linux acceptance adapter

The oracle is util-linux 2.42.4-r3 in the pinned x86-64 Linux image.
`inventory.tsv` records the 117 executable paths owned by packages with APK
origin `util-linux`; the source package commit is
`087ea48145e24307010ba91f403c5ff8f402568c`. The pinned
[APKBUILD](https://github.com/alpinelinux/aports/blob/087ea48145e24307010ba91f403c5ff8f402568c/main/util-linux/APKBUILD)
defines which utilities and features are present.

The inventory separates cohort membership from implementation disposition:
`partial` means native code without full reference proof, and `absent` means
no native implementation was established at that inventory baseline. Neither
grants parity credit, and outside-cohort commands do not enter the behavior
denominator. The inventory still marks `lsipc` absent; its
[behavior map](coverage/lsipc.md) documents the mixed-selector aggregate as
behavior-only. Thirteen approved oracle-crash vectors receive candidate
assertions without oracle-parity credit; their enclosing selector is behavior
evidence only. The other 36 vectors remain exact comparisons.

## Checks

```sh
python3 dev/util-linux/check.py inventory --check
python3 dev/util-linux/check.py harness --self-test
python3 dev/util-linux/check.py source --check core/lsmem.xsh core/tests/test-lsmem.xsh
```

The inventory check compares installed package paths and exact reference
banners. The harness self-test covers lease and cleanup controls, TERM during a
container, and native comparison. Signal controls distinguish SIGTERM from its
shell exit-status convention. Missing evidence fails; it cannot produce an
empty pass.

Native comparisons preserve stdout and stderr bytes, arguments, status, and
fixture state. Reference and candidate see the same fixture. Only version
banners are checked independently. The native contract and regression
selectors live in `core/tests/` and `dev/util-linux/acceptance.py`.

## Acceptance

`baseline --check` compares reference and candidate version output and samples
candidate peak RSS. Resource and close checks build an isolated release
candidate from the clean source checkout. Verification products qualify
functional checks but cannot qualify release measurements.

Resource and close commands require an explicit clean Laputa checkout:

```sh
python3 dev/util-linux/check.py resources --check --laputa-corpus PATH
python3 dev/util-linux/check.py close --check --laputa-corpus PATH
```

`PATH` must be an absolute canonical Git root with a non-symlink `pm/`
directory. The checkout must remain clean at the same commit and tree through
the command. These commands have no sibling-checkout fallback.

Close runs the cohort, positive boundary cases, focused regressions, and
resource checks. It also requires source-bound results from native, host,
repository, documentation, review, and cleanup gates. An independent reviewer
must provide a result bound to the frozen commit and tree through
`independent_review(result)` during the same invocation. The adapter consumes
the result once and does not save a review receipt. No accepted current-source
baseline exists yet.

Timed operations use `perf-gate-ready` before and after each timing pair. A busy
host defers timing and prevents closure. Docker work holds the shared
performance guard and exclusive native guard; lock acquisition is nonblocking.
Unresolved container cleanup retains its scratch and fails the command.

`native_batch` groups at most four exact functional selectors in one bounded
container. Every selector must pass exactly once with zero skips.
`native_private_batch` runs at most four declared cases in fresh IPC, mount,
and PID namespaces; each case must also pass with zero skips. These batches
provide functional evidence, not per-case resource samples. Full cohort
acceptance still requires every integrated comparison and closing gate.

The private boundary proof also requires fresh procfs and two unprivileged
identities with zero effective capabilities. The five cohort commands report
failed stdout writes as `NAME: write error` and exit 1; default SIGPIPE still
terminates by signal, while ignored SIGPIPE uses the write-error path.
