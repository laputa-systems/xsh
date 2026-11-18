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

## Parity

The cohort is `lslocks`, `lsipc`, `lsmem`, `mountpoint`, and `namei`. Each
command runs natively in XSH. The installed utility is a test oracle and is
never the implementation. `coverage/` maps each command's behaviors to cases.

```sh
python3 dev/util-linux/check.py parity --command lslocks --case NAME
python3 dev/util-linux/check.py parity --commands lslocks,lsipc
python3 dev/util-linux/check.py parity --all --require-positive-boundaries
```

Private native cases run with 5 seconds per case for each command, and at
least 120 seconds; a single case is capped at 30 seconds.

A parity case fails when the reference or the candidate did not run, the
fixture inputs differ, an unexpected signal or timeout occurred, or no case
matched the selector. A skipped, unexecuted, or missing behavior is never a
pass. Output normalization is limited to named nondeterminism such as PIDs and
inode IDs, mapped by fixture identity. The exceptions are:

- `--version` output: the XSH banner `NAME (XSH core) VERSION` and the
  util-linux 2.42.4 banner are each asserted on their own.
- The stale `errno` suffix of the `lsmem` diagnostic for a rooted memory
  directory with no numeric entries.
- The thirteen `lsipc` invocations that crash the reference.

Parity, source checks, the harness self-test and `baseline --check` run
qualified verification products from
`.work/protected-products/util-linux-runtime-SOURCE` in the original checkout.
`SOURCE` is the latest commit that changed a Rust input: `src`, `crates`,
`third_party`, `tests/helpers`, `.cargo`, `Cargo.toml`, `Cargo.lock`,
`build.rs` or `rust-toolchain.toml`. The
directory's `receipt.json` records that commit and the SHA-256 of `xsh`, `xsht`
and `xsh-test-helper`. These commands refuse to run when a Rust input differs
from `SOURCE` or is untracked, when the receipt is missing or names another
commit, or when a product's hash differs from the receipt.

After a Rust change, build the musl verification products from the new source
and qualify them:

```sh
cargo build --locked --profile verification --target x86_64-unknown-linux-musl -p xsh -p xsht --bin xsh --bin xsht --bin xsh-test-helper
python3 dev/util-linux/check.py qualify-runtime --products "$PWD/target/x86_64-unknown-linux-musl/verification"
```

`qualify-runtime` copies the three products into the directory for the current
Rust source and writes the receipt. It does not replace a directory that
already qualifies other products; remove that directory to requalify. A
directory that holds the products but no receipt is qualified in place when it
is passed as `--products`. The command does not build the products and cannot
prove which source built them; build them from the checkout it names.

### Accepted divergences

The cohort follows util-linux 2.42.4 as built against musl, except for these
recorded differences. A review reports them only if their behavior changes.

- `lslocks`, `lsipc` and `lsmem` use XSH's shared option parser, which
  honours `POSIXLY_CORRECT` and stops permuting at the first operand, as glibc
  `getopt` does; musl ignores the variable. `mountpoint` and `namei` parse their
  own options and ignore it, as musl does.
- `mountpoint` needs procfs: without `/proc/self/mountinfo` it reports an
  error where util-linux 2.42.4 can fall back to `statmount`.
- `lslocks -Q` evaluates regular expressions in XSH's regex dialect, not POSIX
  ERE, with its own syntax-error text and number parsing. The BLOCKER column
  still names a holder whose row `-i` hides.
- When shared-memory segment sizes total at least 2^64 bytes, `lsipc -g`
  reports the full page count rather than wrapping the byte sum modulo 2^64.
- `lsipc` clamps POSIX object times before 1677-09-21 and after 2262-04-11,
  the ends of the nanosecond `Int` range that `fs.stat` and the time formatter
  share.
- User and group names that are not valid UTF-8 print with U+FFFD in `namei`
  and `lsipc`; the `user` and `group` modules return names as text.
- Display width follows XSH's terminal width rules, not musl `wcwidth`, for
  emoji sequences and the few codepoints where they differ, and the shared
  table escaping prints plane noncharacters such as U+1FFFE raw.
- Without System V procfs tables (when procfs is absent or mounted with
  `subset=pid`, or a table file is missing), `lsipc` lists no System V objects, and `lsipc -g` exits 1
  naming the unavailable tables instead of reading limits through `shmctl`,
  `msgctl` and `semctl` as util-linux does. When `/proc/sys` is masked,
  `lsipc -g` exits 1 naming the unreadable limit instead of using those syscalls.
- `lslocks` reads `/proc/locks` before each owner's descriptors; util-linux
  reads the descriptors first. An owner that exits between the two reads loses
  its COMMAND and HOLDERS attribution (`(unknown)`, or `(undefined)` for an OFD
  row, with empty HOLDERS), and an OFD lock it held drops out of `--pid`
  output, where util-linux keeps the row.
- Without a `/proc` directory, `lslocks` exits 255 without a diagnostic;
  util-linux reports `failed to open /proc` and exits 1.

### Review standard

The independent close review fails a command for a defect in behavior a user
or script relies on: a wrong answer or exit status, a crash, lost or corrupted
data, unsafe output, or a broken documented option. A byte-level difference in
diagnostic wording, layout or character classification against musl or
libsmartcols is recorded here as an accepted divergence or filed as a ticket;
it does not fail the review.

## Acceptance

`baseline --check` compares reference and candidate version output and samples
candidate peak RSS. Resource checks hold existing commands to the XSH
baseline product's peak RSS plus max(10%, 8 MiB); new cohort commands report
their RSS and CPU against the C reference without a limit, since the
interpreter alone exceeds that allowance over C. Resource and close checks build an isolated release
candidate from the clean source checkout. Verification products qualify
functional checks but cannot qualify release measurements.

Prepare a baseline from a clean XSH checkout before changing the candidate. The output directory must not exist. The adapter builds in the pinned image and records the source, image, target, release profile and product hashes.

```sh
python3 -B dev/util-linux/check.py prepare-baseline --output "$PWD/.work/tmp/util-baseline" --laputa-corpus PATH
```

Resource and close commands require that baseline and an explicit clean Laputa checkout:

```sh
python3 dev/util-linux/check.py resources --check --laputa-corpus PATH --baseline-products "$PWD/.work/tmp/util-baseline"
python3 dev/util-linux/check.py close --check --laputa-corpus PATH --baseline-products "$PWD/.work/tmp/util-baseline"
```

`PATH` must be an absolute canonical Git root with a non-symlink `pm/`
directory. The checkout must remain clean at the same commit and tree through
the command. These commands have no sibling-checkout fallback.

Close runs the cohort, positive boundary cases, focused regressions, and
resource checks. It also requires source-bound results from native, host,
repository, documentation, review, and cleanup gates. An independent reviewer
must provide a result bound to the frozen commit and tree through
`independent_review(result)` during the same invocation. The adapter consumes
the result once and does not save a review receipt.

`close --check` passed on 2026-11-18 at `a105f3d73`, with an independent review of
that commit and the release baseline built from `c21b814c1`: 227 parity
selectors, 5 positive boundaries and 13 required gates.

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
