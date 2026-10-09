# XSH Core Compatibility Campaign

Status: Wave 1 in progress on branch `campaign-utils`; this resume uses one
integrator and GPT-6 Luna at xhigh for every subagent. Scope widened on
2026-10-04 from coreutils parity to the full systems-core surface below. Lane strategy and the
integrator protocol are in [`LANES.md`](LANES.md); harness usage is in
[`README.md`](README.md).

## Active resume (2026-10-09)

- Local `campaign-utils` now includes `fs-misc`, `perm`, native byte/hash, GNU
  patch classification, `legacy-buckets`, `stat-du-df`, `printf-env`, `proc-a`,
  `checksums`, and `bytes-enc` (last code merge `78256c56`; the parity manifest records 101 of
  106 in-scope utilities present). The local
  `origin/campaign-utils` ref is still at `5de29706`; push after the campaign
  evidence is refreshed. Pinned references are uutils
  `e7c9f3194280835c4487c2945c68d5f01ccacc8d` and GNU coreutils 9.12.
- Setup is complete on native Linux x86_64. Docker is not used. The workspace
  uses `nightly-2026-09-15`, the `x86_64-unknown-linux-musl` target, and mold
  3.0.0 from the SHA-256-verified x86_64 GitHub release. Bootstrap commands,
  checksum, host prerequisites, and GNU dependency notes are in
  [`README.md`](README.md#setup). GNU 9.12 is prepared with ACL, capability,
  and Linux xattr support.
- Merged utility slices: `fs-basic` **188 / 203**; `fs-misc` **295 / 304**;
  `perm` **106 / 149**; `stat-du-df` **166 / 237**; and `ls dir vdir split`
  **314 / 365** with no regression among the earlier passing test IDs. The
  `legacy-buckets` lane removed all five non-uutils discard buckets. Native
  byte/hash and GNU patch classification primitives and public API wiring are
  merged. The API surface now records **369 functions, 393 overloads, and 799
  queryable items**; SHA-224 and SHA-384 APIs are implemented and their tests,
  generated docs, and docs project checks pass. Exact remaining utility
  failures and their causes are in `gaps.json`.
- `printf-env` is merged at **225 / 252** (env 79/100, printf 146/152), with
  native tests and ratchets passing. `proc-a` is merged at **103 / 125** (kill
  47/50, nice 11/13, nohup 12/13, stdbuf 11/20, timeout 22/29); all **57**
  native tests pass, including a fix for the PTY test runner's intermittent
  SIGHUP. `text-a2` started from **17 / 663** across nine applets; its first
  full slice measured **454 / 663** (cut 70/83, expand 43/46, fmt 27/37, fold
  93/101, nl 34/67, paste 19/27, pr 25/84, tr 101/174), with gaps under active
  work; follow-ups brought nl to **67/67** and paste to **25/27** (native
  paste tests 4/4), while pr/fmt/tr remain active. `bytes-enc` has implemented
  base32, base64, basenc, od, and dd; native tests pass and its full slice is
  **193 / 311** (base32 15/16, base64 24/25, basenc 38/38, od 33/80, dd
  83/152). The `dd` FIFO seek case now passes. The lane is continuing od/dd
  compatibility work. `checksums` is merged over
  the shared byte/hash APIs and its ten native tests pass. Its first slice was
  **220 / 507**; a follow-up is **304 / 507** (md5sum 37/38,
  sha1sum 10/10, sha224sum 8/8, sha256sum 13/13, sha384sum 8/8, sha512sum 8/8,
  b2sum 17/18, cksum 191/391, sum 12/13). Remaining parser, verification, and
  output-format gaps are under active work. The integrator owns shared results
  and baselines; lane runs use scratch outputs.
- The last full uutils run after cp/mv integration measured **2,813 / 5,974**
  and exposed 187 regressions. The shared `FsStat` and special-file read fixes
  are at `d0ac7d7c`; cp fixes at `f06c924b`; mv/ln fixes at `e4efc7ed`;
  `fs-basic` at `aa8a0183`. A corrected full run is pending the active lanes and
  current integration changes.
- `wc --files0-from=-` now reads NUL-delimited filenames incrementally through
  the new `io.stdin_until` primitive. The focused wc slice passes **59 / 59**,
  including progressive input, directory diagnostics, output errors, and
  `--debug`. The docs check passes, including all three tour project tests; its
  test-runner argument boundary was corrected during this resume.
- Earlier verified results remain: date/dircolors **183 / 185** (two French
  month punctuation differences documented against GNU/glibc), and GNU against
  uutils **573 passed, 85 skipped, 58 failed, 3 harness errors** out of 719.
  The release filesystem stdlib tests pass **27**, with one reflink skip; this
  includes FIFO reads, zero-sized `/proc` files, and a `/dev/zero` guard. The
  optimized `cargo test --release -p xsh` still crashes in rustc/LLVM
  ScalarEvolution while compiling the library test binary; release app builds
  succeed.

## Historical handoff (2026-10-05, superseded by Active resume above)

Read this first, then `CLAIMS.md`. Verified against the repository when written;
anything not stated as verified is marked.

### Where the campaign stands

- **Wave 0 is complete** and the merge of fresh `origin/master` is done (the branch
  was rebased onto master before this session; master has nothing the branch lacks).
- **Merged lanes:** `w0-cli`, `w0-gnu-lib`, `sysreport-extract`, `native-fs`,
  `trivial`, `text-a1`, then this session's `native-proc-tty` (process, tty, session,
  termios, utmp primitives and `io.flush_stdout`), `ls` (`ls dir vdir`, rewritten),
  `tty-misc` (`stty more uptime users who pinky`) and a partial `proc-a` (`kill`,
  `nice` only); and, from the other integrator session (`01Tqp2`), `text-b1`
  (`uniq comm join split csplit tsort ptx shuf`) and `text-b2` (`seq wc numfmt
  factor expr`). Also `[` is now an alias of `test` (`core/[.xsh` is gone).
- **Scoreboard** (full uutils run on the merged head `5358d323` plus docs, Linux
  glibc host, [`results/`](results/)): **2186 / 5974 passing**, 4 excluded per test
  ID (up from 1000 at the start of the session); **73 of 106** in-scope utilities have
  an applet. Surface inventory ([`surface.json`](surface.json)): 125 commands, 2
  present. The GNU suite on the pinned uutils is unchanged (571 / 719); the XSH side
  of the GNU differential has still not been run.
- **One regression is on the books:** `test_uniq::test_obsolete_skip_fields_not_read_after_double_dash`
  passed in the other session's last committed results and fails on the merged head.
  Probably the runtime request that `xsh` swallows a first `--` after the script path
  (`src/entrypoints/xsh.rs:124`); not investigated. The `ls` merge also lost
  `test_ls_proc_self_fd_no_errors` (eager directory reads; waived in `gaps.json`) and
  `sleep case_7_single_quote` was excluded (uutils quotes an apostrophe argument
  differently from GNU `quote()`).
- **Known failures on a clean tree, not regressions:** native suite on Linux
  2459 pass / 10 fail before this session's merges (the `linux-modules` and two mime
  stdlib tests, `test-system-report-check ... traced_live_and_replay`, three `px`
  showcase tests, the two pattern-lint tests, `tempdir` removal-on-exit); rerun
  `target/release/xsht test` to refresh it. `make docs` was last run before the
  `tty-misc`, `proc-a` and text lanes landed: run
  `target/release/xsh dev/main.xsh docs` (not `make docs`, whose `cargo dev` alias
  builds a debug `xsh` that overflows its stack on Linux), then `make docs-check`
  equivalents, and commit. Not verified after this session's last merges.
- **The lanes are all merged or released.** No lane worktree, lane branch or lane
  target dir remains except `/home/user/targets/main` (a release build of the head
  before `kill`/`nice`, safe to delete). `cp` and `mv-ln` were started and lost to a
  worker restart with nothing committed; `nohup`, `timeout` and `stdbuf` (the rest of
  `proc-a`) were never started.

### Two integrators share this branch

At the time, a second integrator was working in parallel and had the claims
listed in `CLAIMS.md`. That parallel workflow has ended; treat those old
reservations as historical until verified against current branches and
worktrees. Current model and host setup are stated in Active resume above.

### Next steps, in order

1. Fetch, merge `origin/campaign-utils`, rebuild (`cargo build --release -p xsh --bin
   xsh -p xsht --bin xsht`; never `--bins`, which triggers the extra thin-LTO links),
   regenerate docs, run the native suite and the full uutils run, compare with
   `compare.py`, commit results and `parity.py` output.
2. Investigate the `uniq` regression above.
3. Work through [`requests.md`](requests.md): the remaining exclusion candidates
   (verify each against GNU 9.12 source after `run-gnu.sh prepare`; the GNU source was
   never fetched this session, so none of the `ls`, `tty-misc` or text-lane candidates
   is verified), the `gaps.json` entries, and the `fs.stat` wiring for `test -ef`.
4. Continue Wave 1 in `lanes.json` start order for whatever `CLAIMS.md` leaves free.
   `python3 dev/compat/lanes.py brief LANE` renders a brief with live counts. The
   rest of `proc-a` (`nohup timeout stdbuf`) now has its primitives: `process.wait_timeout`,
   `kill_group`, `set_signal_action`, `priority`, `Status.shell_code`; `nohup` must use
   `process.run` with an ignored HUP and a `stdout` path because `unix.exec` ignores plan
   redirections. `native-bytes-hash` still gates `bytes-enc` and `checksums`.
5. Wave 2 and 3 follow the tables in `LANES.md` and the phases below; the XSH side of
   the GNU differential starts in Wave 2.

### How lanes ran this session (lessons)

- Historical lanes ran from a different host layout. Current worktrees are
  under `/workspace/xsh-lanes`; use the generated brief and current model policy.
- A worker restart (container restart) ends every lane and background suite at once and
  loses uncommitted lane work; lane branches are never pushed. Tell lanes to commit
  after every utility, and consider pushing finished lane branches.
- Script lanes use the shared release binaries; a native build into a separate
  `CARGO_TARGET_DIR` avoids replacing the binary under a running suite. The full suite
  takes 25-30 minutes with other work running.

### Runtime requests (none implemented; full lists with test IDs in `requests.md`)

- Non-UTF-8 argv is rejected by `xsh` before the script runs.
- Stdout is buffered to exit; `io.flush_stdout()` now exists but write errors, EPIPE
  and SIGPIPE are still not observable by default (`process.set_signal_action("PIPE",
  "default")` opts in): blocks the broken-pipe and `/dev/full` tests.
- `src/entrypoints/xsh.rs:124` swallows a first `--` after the script path.
- `main`'s `Int` return does not set the exit status (use `exit(n)`); no `Path` to
  `Bytes` accessor; no incremental stdin read; no FIFO read; no lazy `fs.children`.
- Missing APIs: `user.groups(name)`, `user.login_name()`, `system.hostid()`, ordered
  `getgroups`, online CPU count, u64 integers, `gecos` in `user.lookup`, a timezone
  database primitive, `dns.canonical`, sub-second boot time.
- `date +%99999999999c` writes 2 GiB (assigned to the `date` lane).

### Standing constraints

- Current campaign subagents must use GPT-6 Luna at xhigh effort, as specified
  above.
- The owner's non-negotiables: no accepted option silently ignored; the denominator
  never shrinks; no compatibility command parses another's text output; uutils is an
  oracle and reference, never a dependency.
- Do not run formatters or autofixers (AGENTS.md).
- Build only `xsh` and `xsht` (the owner's request this session), not the other
  workspace binaries.
- The suite lock must not leak to children: `run-uutils.sh` runs its children with the
  lock descriptor closed.

### Native Linux x86_64 bootstrap

Use the host directly; Docker is outside the campaign setup. Follow the
versioned toolchain and mold installation steps in [`README.md`](README.md#setup),
then, from the repository root:

```sh
source dev/compat/native-env.sh
git clone https://github.com/uutils/coreutils ../ref/uutils-coreutils
git -C ../ref/uutils-coreutils checkout e7c9f3194280835c4487c2945c68d5f01ccacc8d
export UUTILS_ROOT=$PWD/../ref/uutils-coreutils UUTESTS_THREADS=3
cargo build --locked --release -p xsh --bin xsh -p xsht --bin xsht
cargo install cargo-nextest --locked
dev/compat/run-uutils.sh
dev/compat/run-gnu.sh prepare
dev/compat/run-gnu.sh uutils
```

Build only the `xsh` and `xsht` campaign binaries. Sourcing
`native-env.sh` selects the workspace-local pinned Rust toolchain and mold
3.0.0 linker; it rejects non-Linux or non-x86_64 hosts. The first
`run-uutils.sh` after a new checkout or `PATH` change recompiles the uutils
test crate once. Details are under "Environment notes".

## Mission

XSH becomes the software-defined Linux systems core for Laputa: one typed
language and native substrate that owns the kernel-ABI-facing userland. A
Laputa system with XSH installed does not carry coreutils, util-linux,
procps-ng, kmod, pciutils, usbutils, findutils, grep, the compression CLIs,
dosfstools, smartmontools, nvme-cli, iproute2, ethtool, iw or similar small
distro packages merely because conventional software expects their executable
names. Traditional source-package boundaries are packaging history, not an
architectural constraint.

### Ownership rule

Implement in XSH when the software is primarily

```text
kernel ABI + sysfs/procfs/netlink/ioctl parsing + conventional CLI presentation
```

Package externally when it contains a deep independent implementation: kernel
and firmware; filesystem repair engines (`e2fsck`); GPU drivers and Mesa;
nftables semantics; ptrace syscall-decoding databases (`strace`); perf's PMU
and profile machinery; packet protocol decoders (`tcpdump`); Git, Make and
rsync (protocols and languages). Libraries that third-party binaries link
against (`libcap`, `libacl`, `libattr`, `libcurl`) may still be packaged later
as ABI compatibility for external software; that is orthogonal to owning the
command surface.

Optimize for removing whole categories of traditional distro package
dependencies through one coherent, typed Linux systems substrate, not for the
number of command names implemented.

### Objectives

1. Match the complete practical Linux command surface of `uutils/coreutils`
   with comparable behavioral compatibility.
2. Measure that with uutils' own integration, GNU and BusyBox suites rather
   than a bespoke suite.
3. Own the rest of the kernel-ABI userland listed in
   [`surface.json`](surface.json): util-linux, procps, kmod, pciutils,
   usbutils, findutils, grep, sed/awk/diffutils, the compression family
   (including zstd), dosfstools, block/mount/partition tools, file
   attribute/ACL/capability tools, namespace and process control, storage
   health (`smartctl`, `nvme`), network control and diagnostics (`ip`, `ss`,
   `ethtool`, `iw`, `dig`, `nc`, `curl`), `cpio` and EFI boot variables.
4. Implement commands as thin XSH applets over typed native APIs; add Rust
   primitives where syscalls, byte semantics, performance or kernel interfaces
   demand it.
5. Never fake a command: an accepted option works, and unsupported behavior
   fails explicitly.
6. Use canonical tools (dosfstools, smartmontools, nvme-cli, util-linux,
   ethtool, iw, xz, zstd, ...) only as test references, never as runtime
   dependencies.

Two constraints govern every decision:

> Laputa should not need dozens of tiny Unix packages merely because
> conventional software expects familiar executable names.

> Do not reproduce BusyBox's worst failure mode: a command name present while
> important semantics quietly do nothing.

## Architecture

### Two interfaces

- **Native XSH APIs** (`fs`, `process`, `unix`, `linux`, `system`, `hash`,
  `compression`, `archive`, `dns`, `net`, `cli`, ...) expose semantics as typed
  records and operations. They live in `src/modules/` and are documented
  through the registry behind `xsht api`.
- **Compatibility CLIs** (`core/*.xsh`) translate conventional argv into those
  APIs and print conventional output.

One compatibility command never parses another's text output. Shared data
flows through typed collectors:

```text
BAD:   lsusb -> run system-report -> parse human output
GOOD:  typed USB inventory -> { system-report, lsusb }
```

The same holds for PCI, block devices, processes, network state, mounts and
modules. `system-report`'s collectors (`core/lib/system_report_*.xsh`, about
11.7k lines) are extracted into reusable typed modules without weakening its
bounded reads, redaction, safety, or capture/replay guarantees. Presentation
commands may show local detail by default where convention expects it;
`system-report`'s share-safe redaction stays its own policy.

### One model per kernel ABI

The main architectural risk of this scope is duplicating kernel parsing in
dozens of applets. Each kernel ABI gets exactly one typed collector or
controller; applets, `system-report`, Laputa boot and install scripts and user
programs are renderers over it. Target domains (the split follows the existing
module architecture; these names are targets, not required spellings):

`block`, `mount`, `partition`, `fs` (FAT first), `device`, `pci`, `usb`,
`storage_health`, `nvme`, `net`, `wifi`, `process` (including sampling),
`namespace`, `capability`, `xattr`, `acl`, `efi`, `module`, `power`, `sensor`.

- A command reads `/proc`, `/sys`, netlink or ioctl state only through its
  domain's API. An applet that needs a field the domain lacks adds it to the
  domain through the owning lane, never a private reader.
  [`check_kernel_reads.py`](check_kernel_reads.py) ratchets this for top-level
  applets.
- Point-in-time inventory (`system-report`) and time-series sampling (`top`,
  `vmstat`, `iostat`, `pidstat`, `watch`) are different APIs. Sampling has
  explicit sample types: stable process identity (pid plus start time), CPU
  ticks, RSS/VSS, I/O counters, context switches, state, threads, system CPU
  totals, memory, paging and block-I/O counters. It is not built by abusing
  the report collector.
- Mutating operations (partition tables, `wipefs`, `mkfs`, EFI variables,
  `ethtool`/`iw` set, `setcap`, namespace and privilege transitions) validate
  explicitly, act only on a target the caller named, and are tested only
  against loop files, namespaces and synthetic fixtures (Gate 7).

### Shared applet foundation (revised: Wave 0, before any fan-out)

`core/README.md` currently keeps applets self-contained and usage errors
local. The compatibility campaign needs one exception, because uutils and GNU
tests assert exact option grammar, diagnostics and exit codes, and ninety
applets must not each reinvent them:

- **GNU option grammar in `cli`** (native, `src/modules/cli.rs`). Today
  `cli.applet`/`cli.parse` reject repeated options (`duplicate argument at
  argv[N]`; GNU `ls -l -l` is valid, last value wins), do not resolve
  unambiguous long-option abbreviations (`--all` from `--al`), and report
  errors in XSH style. Add a GNU mode: short-option bundling, attached and
  separate values, optional arguments (`--color[=WHEN]`), `--opt=value`,
  abbreviation with ambiguity errors, argument permutation with
  `POSIXLY_CORRECT` and a leading `+` disabling it, `--`, repeated options,
  numeric-option shapes (`head -5`, `nice -n -5`), and an explicit
  `unsupported` declaration that fails with a clear diagnostic.
- **GNU diagnostics and exit codes** (`core/lib/gnu.xsh` plus any native
  support). `PROG: message`, `PROG: cannot access 'X': No such file or
  directory` (with GNU quoting of names), `Try 'PROG --help' for more
  information.`, per-utility usage-error exit status (1 for most, 2 for `ls`,
  `cmp`, `diff`, `grep`; 125 for `env`, `nice`, `nohup`, `timeout`,
  `stdbuf`, `chroot`), `--help`/`--version` handling, and SIGPIPE/EPIPE
  behavior on stdout.
- **Invoked-name access** for applets. Aliases (`dir`, `vdir`, `[`, `egrep`,
  `gunzip`, `zcat`, ...) are installed as symlinks; the applet reads the name
  it was invoked as (the script path the kernel passes) to choose defaults,
  help and diagnostics. Expose this cleanly in the runtime if it is not
  already available.
- **Byte-stream helpers** so binary data never passes through `Str`.

`core/README.md` is updated to describe this shared layer as an audited
command family, matching its existing rule for `lib/auth.xsh` and
`lib/text_input.xsh`.

## Decisions

- **Diagnostic wording is GNU's.** uutils builds on clap and about 60 of its
  roughly 6,000 integration tests assert clap wording (`error: unexpected
  argument`, `a value is required for`, `invalid value`). XSH follows
  GNU `getopt_long` and coreutils wording (`invalid option -- 'x'`,
  `unrecognized option '--foo'`, `cannot access 'X': No such file or
  directory`). Tests that assert clap wording are excluded per test ID with
  category `clap-wording`, never per module; the GNU differential covers the
  GNU wording.
- **Execution phrase.** Usage errors end with `Try 'PHRASE --help' for more
  information.` PHRASE is `$XSH_EXECUTION_PHRASE` when set, else the invoked
  name (the basename of the script path as the kernel passed it; aliases are
  symlinks, so `dir` sees `dir`). The uutils adapter sets it to
  `"<adapter path> <util>"`, the multicall form uutils' `usage_error` helper
  expects.
- **Alias table shape.** `aliases.json` holds `{name, target}` entries, because
  XSH decodes a JSON object as a record and cannot schema-check an open map.
- **Expanded-scope inventory.** [`surface.json`](surface.json) lists every
  command beyond the uutils set with phase, domain and test-only reference
  tool. The count is pinned in `upstream.lock.json` and only grows.

## Phase 0: upstream baseline (done)

- Pinned uutils/coreutils `e7c9f3194280835c4487c2945c68d5f01ccacc8d`
  (`0.12.0-289-ge7c9f3194`, 2026-10-04) in
  [`upstream.lock.json`](upstream.lock.json). Checkout lives outside the tree
  via `UUTILS_ROOT`.
- GNU coreutils 9.12, pinned transitively by uutils' `util/fetch-gnu.sh` at
  that commit; fetched from GitHub releases into `GNU_ROOT`, never vendored.
- [`parity.py`](parity.py) derives the utility list from the pinned
  `Cargo.toml` (`feat_os_unix`, expanded recursively, filtered to `src/uu/*`
  crates) and writes [`../coreutils-parity.json`](../coreutils-parity.json).
  `parity.py --check` fails when the manifest is stale.
- Denominator: **108 upstream utilities = 106 in scope + 2 SELinux-gated**
  (`chcon`, `runcon`, tracked with `capability: "selinux"` and still counted).
  Every `src/uu` crate is accounted for except the `checksum_common` library.
- Baseline presence: **41 of 106** in-scope utilities have an XSH applet; 65
  are missing. XSH has 58 applets in total (17 are outside the uutils set:
  `fd`, `getty`, `host`, `ifdown`, `ifup`, `ip`, `mdev`, `passwd`, `pstree`,
  `rev`, `rg`, `strings`, `su`, `system-report`, `tar`, `tree`, `which`).
- The denominator never shrinks. Exclusions are per test ID, categorized and
  explained, never per module.

Manifest fields per utility: upstream commit (top level), utility, capability
gate, XSH implementation path, present, native test file, uutils test count,
uutils integration pass/fail/skip/excluded, GNU differential, BusyBox
results, excluded test IDs, known semantic gaps.

## Phase 1: harness (Wave 0)

### A. uutils integration suite (Gate 3)

- [`xsh-uutests`](xsh-uutests) implements the multicall contract
  `BINARY UTILITY ARGS...` by exec'ing the staged applet. It is a dispatcher,
  never a second implementation; argv, stdio bytes, exit status, cwd,
  environment and signals pass through untouched (verified with a stand-in
  interpreter, including empty arguments, `--`, and NUL bytes on stdin). A
  missing applet exits 127 with a diagnostic.
- [`stage.py`](stage.py) installs `core/` in its released shape (suffix
  dropped, `lib/` adjacent, aliases as symlinks, shebang pointing at the built
  `xsh`) and writes the stage's `applets.json`. Suites test the installed
  shape, not the source tree.
- [`run-uutils.sh`](run-uutils.sh) builds the uutils `tests` target with
  `--features feat_os_unix` (the test crate requires
  `CARGO_BIN_EXE_coreutils` and per-utility features, so uutils
  implementations are compiled but never exercised), makes the uutils
  `coreutils` binary non-executable as a tripwire for anything that bypasses
  `UUTESTS_BINARY_PATH`, runs `cargo nextest` with
  `UUTESTS_BINARY_PATH=xsh-uutests`, `LC_ALL=C`, `TZ=UTC`, and converts the
  JUnit report with [`results.py`](results.py) into
  `results/uutils-integration.json`. Partial runs (named utilities) update
  only their entries.
- Cross-utility calls such as `scene.ccmd("touch")` also go to XSH. Fix
  widely used cheap applets first; their failures cascade.
- [`host-deps.json`](host-deps.json) lists the 28 utilities whose tests spawn
  host programs directly (`sh`, `strace`, `setfacl`, `unshare`, `locale`,
  ...) or compare against a host GNU tool. These are explicit, not
  accidental: in the clean environment they come from XSH or are excluded
  with category `host-oracle`.

Baseline against the current XSH (2026-10-04, pinned uutils `e7c9f31`, 5,978
tests across 106 utilities, 22.6 minutes at 2 threads): **539 pass, 5,439 fail**
(14 of those timed out; 9.0%). The 41 utilities that already have an applet pass
483 of 3,277 tests. The other 56 passes belong to the 65 utilities without an
applet: "command not found" satisfies every expects-failure test, so those are
vacuous, and `results.py` records `"applet": false` for them so `compare.py`
does not count them as regressions when the applet arrives. Dominant failure
causes seen so far: unsupported GNU options (`unknown argument at argv[0]`),
non-UTF-8 argv rejected by `xsh`, wording and exit-status mismatches, buffered
stdout (no EPIPE or write-error behavior), and large-input throughput.

### B. GNU suite differential (Gate 4)

[`run-gnu.sh`](run-gnu.sh) prepares one GNU 9.12 tree with uutils'
`util/build-gnu.sh`, then runs the identical tests twice, changing only the
`PATH` entry that `tests/local.mk` prepends: the pinned uutils multicall
directory, then the XSH stage's `gnu-bin/`. `gnu-bin/` has every GNU program
name; names XSH lacks are copies of `false`, so nothing falls through to host
GNU binaries (uutils uses the same trick). Because both sides share one
patched harness, uutils' patches cannot bias the differential.

Patch policy (revised): the brief asks to separate plumbing, accepted
semantic divergences and uutils wording patches. That classification
matters for absolute GNU conformance, not for the uutils-parity milestone,
so it is a tracked task in [`gnu-patches.json`](gnu-patches.json) (15 patches
plus `build-gnu.sh`'s sed rewrites, all `unclassified` today) rather than a
precondition for running the differential.

Results: `results/gnu-uutils.json` (cached per uutils commit),
`results/gnu-xsh.json`, and `results/gnu-differential.json` with the four
cells globally and per test directory. `uutils pass / xsh fail` is the
blocker list. GNU groups some tests under `misc/`; the per-utility rollup
maps those by test name when the manifest ingests them.

GNU baseline against the pinned uutils (2026-10-04, GNU 9.12 via uutils'
`build-gnu.sh`, run unprivileged in a user namespace, 21 minutes): 719 tests,
**571 pass, 46 fail, 101 skip, 1 error**
([`results/gnu-uutils.json`](results/gnu-uutils.json)). The XSH side
(`run-gnu.sh xsh`) is not run yet: with most applets absent nearly every test
would fail, so the four-cell differential starts in Wave 2.

### C. BusyBox suite

After A and B are green enough to be useful, adapt uutils' BusyBox route
(`GNUmakefile` `busytest`). BusyBox behavior never overrides the uutils/GNU
contract where the projects intentionally differ.

### D. Native tests remain

`core/tests/test-<applet>.xsh` stay the fast regression suite and cover
XSH-specific boundaries. Upstream suites supplement them.

### E. Ratchets

- [`check_ignored_options.py`](check_ignored_options.py) (Gate 6): fails on any
  new discard bucket (`ignored`, `unused`, `compat`, `noop`), on a bucket that
  gains options, and on a stale baseline. Baseline: 16 buckets in chgrp,
  chmod, chown, cp, df, du, getty, hostname, ifdown, ifup, ln, ls, mv, pstree,
  touch, which ([`ignored-options-baseline.json`](ignored-options-baseline.json)).
  It can only shrink.
- Test ratchet: the integrator rejects a merge that turns any previously
  passing uutils test into a failure (compared against the committed
  `results/uutils-integration.json`).

## Phase 2: complete uutils Linux surface

The authoritative list is `totals.missing_in_scope` in the manifest, never a
handwritten list. Notes:

- Names that are xshi builtins (`echo`, `kill`, `true`, `false`, `test`) still
  need external applets for `env CMD`, `execve`, `PATH` lookups and programs
  that bypass the shell. Provide `[` for `test`.
- Share implementations through aliases in [`aliases.json`](aliases.json):
  `arch` over `uname` machinery, `dir`/`vdir` over `ls` with different
  defaults, the checksum family (`md5sum`, `sha*sum`, `b2sum`; `cksum` and
  `sum` share the native hash layer), `true`/`false`. Preserve each command's
  argv, help and output.
- `stdbuf` needs an LD_PRELOAD-style buffering shim in uutils; on musl that is
  unavailable (uutils drops it from `feat_require_unix_musl`). Decide with
  evidence: implement over a native mechanism or record a capability
  exclusion; never ship a no-op.
- `pinky`, `users`, `who`, `uptime` (user count) read utmp; Laputa's utmp
  story decides between real support and explicit failure.

Quality bar per command: options and their interactions, stdout, stderr, exit
status, binary safety, symlink follow/no-follow, permissions, ownership,
timestamps, hardlinks, sparse files, recursion, filesystem boundaries, broken
symlinks, material races, TTY behavior, signals, numeric parsing and
overflow, empty input, `-` operands, multiple operands and error continuation.
When POSIX/GNU is unclear, the pinned uutils implementation and tests are the
specification. uutils is a reference and oracle, never a cargo dependency;
reusing a small MIT-licensed algorithm is acceptable with attribution.

The no-ignored-options rule applies to every existing applet before it counts
as passing: implement or reject each discarded option.

## Native API growth

Add primitives instead of contorting scripts. Expected areas:

- **fs**: complete nofollow metadata, ownership/mode preservation, ns
  timestamps (`utimensat`), hardlinks, FIFOs and device nodes, sparse files
  (`SEEK_DATA`/`SEEK_HOLE`), efficient copy (`copy_file_range`, reflink),
  xattrs if `cp -a` requires them, statvfs and mount identity, atomic and
  no-clobber operations (`RENAME_NOREPLACE`, `O_EXCL`).
- **process**: signals, process groups and sessions, priority, terminal
  association, identity, wait with timeout, rlimits, stdio and environment
  control (for `nice`, `nohup`, `timeout`, `setsid`, `ps`, `pgrep`, `pkill`).
- **unix**: termios, PTYs and sessions (`stty`, `tty`, `getty`, `login`).
- **bytes**: streaming byte I/O for base encodings, checksums, `dd`, `od`.
- **hash**: one native layer for the checksum family; add missing algorithms
  (BLAKE2b with variable length, CRC for `cksum`, BSD/SysV `sum`).
- **compression**: existing gzip/bzip2/xz/lzma codecs back Phase 5.
- **block/mount/partition**: `BLKRRPART`, `BLKGETSIZE64`, sector sizes,
  read-only state, discard, flush, `FITRIM`, `FIFREEZE`/`FITHAW`, signature
  probing and removal, partition reread (Phase 7A).
- **fs (FAT)**: typed FAT12/16/32 parse, build and check (Phase 7B).
- **xattr, acl, capability**: typed representations, including
  `security.capability` revisions (Phase 7C).
- **storage_health, nvme**: ATA pass-through, `SG_IO`, NVMe admin commands,
  identify, SMART and logs, self-test control (Phase 8).
- **namespace, process control**: `setns`, `unshare`, affinity, scheduler class,
  I/O priority, rlimits, uid/gid and supplementary groups, capabilities,
  securebits, `no_new_privs`, parent-death signal, and the structured process
  and system sampling layer (Phases 3D and 9).
- **net control**: ethtool netlink, nl80211, rtnetlink mutation, real ICMP
  (Phase 3E and 10).
- **efi**: typed efivarfs variables (Phase 11).
- **compression**: zstd, and streaming reader/writer forms for every codec
  (Phase 5).
- **archive**: cpio read/write (Phase 11).


## Phase 3: practical Linux surface beyond coreutils

Required, built over typed collectors and existing `linux.*` APIs:

- **3A hardware/inventory**: `lscpu`, `lspci` (`-n -nn -k -v -vv -D -t -s -d`;
  numeric output without a database, names from `/usr/share/hwdata/pci.ids`
  when present, never embedded), `lsusb` (`-t -v -d -s -D`, `usb.ids` when
  present), `rfkill`, `sensors`. Block and mount inventory (`lsblk`, `blkid`,
  `findmnt`) is Phase 7A.
- **3B util-linux control**: `swapon`, `swapoff`, `mkswap`, `dmesg`, `sysctl`,
  `hwclock`, `flock`, `setsid`, `pivot_root`, `switch_root`, `reboot`,
  `poweroff`, `halt` over `linux.swapon`, `linux.dmesg`, `linux.sysctl_*`,
  `linux.hwclock`, `linux.pivot_root`, `linux.switch_root` and friends. The
  mount family and loop devices are Phase 7A.
- **3C kmod**: `lsmod`, `modinfo`, `modprobe`, `insmod`, `rmmod`, `depmod` over
  `linux.modules`, `linux.modinfo`, `linux.modprobe`, `linux.depmod`,
  `linux.insmod`, `linux.rmmod`, with kmod-compatible argv and output.
- **3D procps and observability**: `ps` (`-e -f -ef aux -o -p --ppid --sort`,
  mainstream Linux forms tested comprehensively), `pgrep`/`pkill`/`pidof`/
  `killall` over one typed selector, `pstree`, `free` (`-h -m -g -s`), `lsof`
  over `linux.open_files()` grown to files, pipes, sockets, devices,
  cwd/root/exe, `fuser`, and the sampling tools `top`, `watch`, `vmstat`, with
  `iostat` and `pidstat` if the sampling primitives are shared cleanly. All of
  them sit on one native structured sampling layer (see "One model per kernel
  ABI"), never on per-command `/proc` readers.
- **3E networking**: `ip` grows to `link show/set up/down`, `address
  show/add/del`, `route show/add/del`, `rule show/add/del`, `-4 -6 -j -o`,
  via native rtnetlink mutation APIs next to `linux.network_dump()`; `ss`
  (TCP/UDP/listening/process); `ping`/`ping6` with real ICMP. Legacy
  `ifconfig`/`route`/`arp` only if cheap. Not `tc` or `bridge`. Device control
  (`ethtool`, `iw`) and diagnostics are Phase 10.
- **3F udevadm**: honest `info`, `trigger`, `settle`, `monitor` over sysfs and
  XSH's uevent machinery; no operation reports success without effect.
- **3G accounts**: `login` sharing `lib/auth.xsh` with `passwd`, `su`, `getty`
  (which already execs `login`); `getent` for passwd, group, hosts, and
  services/protocols where present. No NSS empire.

## Phase 4: script-ecosystem commands

`grep`/`egrep`/`fgrep` (shared lower-level search primitives, not `rg`; `-E
-F -G -i -v -w -x -n -H -h -l -L -c -q -r -R -e -f --color`, `-P` only if
truly supported), `find` (`-name -iname -type -path -regex -size -mtime
-newer -user -group -perm -maxdepth -mindepth -prune -print -print0 -exec
-execdir -delete` with boolean composition), `xargs` (byte/NUL-safe `-0 -n -L
-P -I -r`, correct exit codes), `sed` (a real parser and addressing model;
uutils/sed and its GNU shim as reference and tests), `awk` (credible POSIX
awk, Rust parser/runtime), `cmp`, `diff`, `patch` over the native diff/patch
modules.

Revised: `sed` and `awk` share nothing with coreutils and are each multi-week
efforts, so they start in Wave 1 as long-running lanes.

## Phase 5: compression

`gzip`/`gunzip`/`zcat`, `bzip2`/`bunzip2`/`bzcat`, `xz`/`unxz`/`xzcat`,
`lzma`/`unlzma`/`lzcat` and `zstd`/`unzstd`/`zstdcat` as one family: thin XSH
presentation layers over a common native streaming implementation. Do not
package the traditional implementations to get familiar binaries.

- XSH already has native gzip, bzip2, xz and lzma codecs. zstd joins the native
  compression layer with full frame encode and decode. Prefer a good pure-Rust
  implementation if it has full frame support and acceptable performance;
  otherwise a narrowly contained implementation dependency is acceptable.
  Correctness outranks "pure Rust". The decision is recorded with benchmark
  evidence in this file before any dependency lands.
- Required semantics: stdin/stdout streaming, multiple files, file replacement,
  `-c -d -k -f`, compression levels, integrity testing (`-t`), concatenated
  streams where the format permits them, original name and timestamp where the
  format records them, each tool's documented exit codes, and binary-safe
  operation.
- `compression.*` is refactored as needed so the CLIs share one streaming
  reader/writer implementation; no whole file passes through memory.
  Benchmark large streams against the canonical tools with `bench/`.

## Phase 6: small high-value commands

`file` (deterministic subset: ELF, shebangs, archives and compression, text
encodings, common image/container formats; no libmagic database), `ldd`
(inspect the ELF interpreter and dependencies on musl without executing
untrusted binaries), `clear`, `reset`.

## Phase 7: storage and filesystems

### 7A block, mount and partition

Turn `linux.block_devices`, `linux.blkid`, `linux.partition_table`,
`linux.write_partition_table`, `linux.loop_attach/detach/list` and
`linux.fsck` into a coherent practical storage environment: `lsblk` (`-a -b -d
-f -J -l -n -o -p -r`, relationships from typed identity, not names), `blkid`,
`findmnt` (mountinfo first: `TARGET`, `--json`, `--types`, `--source`,
`--target`; `--fstab` after), `mount`, `umount`, `losetup`, `blockdev`,
`wipefs`, `partx`, `partprobe`, `fstrim`, `fsfreeze`, and `sfdisk`/`fdisk`
with their existing syntax, not a bespoke one (`sfdisk --json` is the machine
interface). GPT and MBR both, correctly.

New native primitives: `BLKRRPART`, `BLKGETSIZE64`, logical and physical sector
size, read-only state, discard, block-device flush, `FITRIM`,
`FIFREEZE`/`FITHAW`, partition reread, signature probing and removal.
`wipefs` understands and selectively erases known signatures; it is never
"zero the start of the disk".

### 7B FAT (replaces Laputa's handwritten FAT tooling)

A typed FAT module (parse and manipulate), then `mkfs.fat`/`mkfs.vfat`,
`fsck.fat`/`fsck.vfat` and `fatlabel`. Real FAT12/FAT16/FAT32 handling, not
just the geometry the Laputa installer happens to generate.

- `mkfs`: FAT type selection, automatic layout, volume label and ID, sector
  size, cluster sizing, reserved sectors, number of FATs, regular files and
  block devices, and reproducible images when explicitly requested.
- `fsck`: an actual checker, not a header sanity check. Validate and, where
  appropriate, repair the boot sector and BPB, backup boot sector, FAT copies,
  cluster chains, loops, cross-links, lost clusters, directory entries, `.` and
  `..`, invalid cluster references, long-filename chains, free-space
  accounting, FAT32 FSInfo and dirty/error flags. Conventional noninteractive
  modes serve installation and recovery workflows; destructive repair is
  explicitly controlled.
- `laputa-fs` is migrated onto this module. Only a genuinely Laputa-specific
  helper (such as `fat-put`) may remain there, or it migrates too.

### 7C attributes, ACLs, capabilities

`lsattr`/`chattr` over the existing native file-attribute support;
`getfattr`/`setfattr`, `getfacl`/`setfacl` and `getcap`/`setcap` over typed
xattr, ACL and capability domains. File capabilities understand and validate
the `security.capability` xattr versions (revision, flags, permitted and
inheritable sets, root id) rather than treating the payload as opaque bytes.

## Phase 8: storage health

`smartctl` as a serious surface over typed APIs, not a text parser: ATA/SATA
and NVMe first, SCSI/SAS once the transport abstraction is clean. New native
Linux functionality: ATA pass-through, `SG_IO`, NVMe admin commands, identify
data, SMART data and logs, health information, self-test control.

Surface: `smartctl DEVICE`, `-i`, `-H`, `-A`, `-a`, `-x`, `-l error`,
`-l selftest`, `-t short|long`, `-s on|off`, `-j`. No `smartd`: Laputa needs no
further background daemon. The read-only inventory is reusable by
`system-report`, which reports normalized health and identify data while
`smartctl` exposes the lower-level raw fields.

`nvme` inspection over the same typed NVMe transport: `list`, `id-ctrl`,
`id-ns`, `smart-log`, `error-log`, `self-test-log`, `fw-log`. Read-only first.
Mutating admin operations (format, firmware activation) arrive only with
explicit validation and strong tests, never merely to claim coverage.

## Phase 9: namespaces and process control

`nsenter`, `unshare`, `lsns`, `taskset`, `chrt`, `ionice`, `prlimit`,
`setpriv` as thin applets over reusable typed XSH APIs: `setns`, `unshare`,
namespace fd discovery, CPU affinity, scheduler class and priority, I/O
priority, rlimits, uid/gid transitions, supplementary groups, Linux
capabilities, securebits, `no_new_privs` and parent-death signals. The logic
lives in the `process`, `namespace` and `capability` domains, not in the
applets.

## Phase 10: network device control and diagnostics

- `ethtool`: modern netlink API first, ioctl only where required. `DEV`, `-i`,
  `-k`, `-K DEV FEATURE on|off`, `-S`, `-g`, `-G`, `-c`, `-C`, `-a`,
  `--show-eee`; inspection and common controls first.
- `iw`: a typed nl80211 API and `iw dev`, `dev DEV info|link|scan`, `phy`,
  `phy PHY info`, `reg get|set CC`. No association or authentication
  (`wpa_supplicant` stays separate); `wireless-regdb` stays external.
- Diagnostics over the existing `dns` and `net` modules: `traceroute`,
  `tracepath`, `dig`, `nslookup`, `nc` (a clean TCP/UDP client and listener,
  not every historical netcat incompatibility; no `socat` clone). `host`
  already exists.
- HTTP transfer: a focused `curl` (`-L -f -s -S -o -O -I -X -H -d
  --data-binary -u --connect-timeout --max-time --retry --cacert -k`, correct
  stdin, stdout and file streaming), and a small `wget` over the same
  implementation if worthwhile. Not every curl feature.

## Phase 11: boot

- `cpio` with `newc` first, then `crc` and `odc`; `-o -i -t -p -d -m -u -v
  --null`; binary-safe streaming. Reading and writing live in the archive
  module, not only in the CLI.
- `efibootmgr` over a typed EFI-variable domain on efivarfs: `efibootmgr`, `-v`,
  `-c ...`, `-b XXXX -B`, `-o`, `-n`. Writes are extremely careful: synthetic
  efivarfs fixtures first, then QEMU/OVMF, before any real-host mutation. The
  Laputa installer is an eventual consumer. No D-Bus firmware stack.

## Phase 12: clock

Laputa's clock model stays deliberately simple: RTC, then `linux.hwclock()`,
then `linux.set_system_clock()`, or the explicit boot epoch when present. No
NTP, chrony or ntpd in this campaign. Maintain and complete `date` and
`hwclock` and their typed APIs.

## Non-goals

External by design (a deep independent implementation): the kernel and
firmware, `e2fsck` and other filesystem repair engines, GPU drivers and Mesa,
nftables semantics, `strace`, `perf`, `tcpdump`, Git, Make, rsync, browsers,
databases, PipeWire, ALSA playback, `wpa_supplicant`, Tailscale, SSH, full
`curl` feature parity.

Not introduced merely because conventional distros ship them: systemd, D-Bus,
elogind, polkit, NetworkManager, a udev daemon, FUSE, chrony, ntpd, smartd,
udisks, upower, fwupd. A daemon is not added when a direct kernel interface and
an on-demand XSH command suffice.

Rule: command veneers and kernel/userspace inspection and control belong in
XSH when XSH owns the structured capability; independent application or
protocol semantics stay packages.

## Semantics and style rules

- Never fake success. No accepted semantic flag is discarded; unsupported
  behavior fails clearly.
- Output is conventional; typed records belong in the native API. Support
  established JSON options (`ip -j`, `lsblk -J`, `findmnt --json`).
- `LC_ALL=C` and `TZ=UTC` are the deterministic baseline. UTF-8 is still
  handled correctly; full i18n does not block the milestone.
- Linux first. Coreutils applets stay cross-platform where the API already
  is; Linux-only commands report unsupported cleanly elsewhere.

## Performance

Cold start, measured on the 4-core reference host with the release `xsh`
(2026-10-04, 50 sequential invocations, wall time per invocation): empty
script 9.5 ms, `print "x"` 10.6 ms, staged `core/cat` 13.6 ms, staged
`core/basename` 19 ms, host `/bin/cat` 3 ms. A suite of 6,000 invocations
therefore spends about 1-2 minutes in startup; cold start does not bound suite
runtime or make per-applet scripts non-viable, so no native multicall applet
entry is needed for startup. Throughput on large inputs is the real risk: the
uutils `cat` tests drove `core/cat` to 100% CPU and over 1 GB RSS and timed out.

Measure hot utilities (`cat`, `cp`, `dd`, `grep`, `sort`, `wc`, `head`,
`tail`, `find`, `xargs`, `ls`, `du`, checksums, `base64`, compression)
against uutils with the existing `bench/` infrastructure. Revised: measure
interpreter cold start first. Every suite invocation and every script call
pays it; if it dominates, the fix is a native multicall applet entry, not
per-script tuning. Prefer a small applet plus a fast native primitive over
rewriting applets in Rust.

## Packaging

`dev/release.xsh::package_core` already ships every `core/*.xsh`. Add a
deterministic applet manifest (applets, aliases, libraries; the shape
`stage.py` writes as `applets.json`) to the release artifact so Laputa can
materialize `/usr/bin` links from it instead of its hand-maintained list.
Aliases (`[`, `dir`, `vdir`, `egrep`, `fgrep`, `gunzip`, `zcat`, `mkfs.vfat`,
`fsck.vfat`, ...) come from [`aliases.json`](aliases.json) with no duplicated
sources. Laputa itself
changes only if needed to prove the interface.

## Verification gates

1. **Native**: project Rust tests, `xsht check`, `core/tests` pass.
2. **Inventory**: every in-scope uutils utility implemented or carrying an
   accepted capability/platform rationale in the manifest.
3. **uutils integration**: all applicable `tests/by-util` tests pass through
   `UUTESTS_BINARY_PATH`; exclusions are per test ID, categorized, explained.
4. **GNU differential**: zero `uutils pass / xsh fail`, reported globally and
   per utility with all four cells.
5. **Options**: automated comparison of each uutils command's option surface
   with XSH's (help-text parsing is one input, not proof of semantics); gaps
   appear in the manifest.
6. **No ignored semantic options**: ratchet at zero buckets.
7. **Practical Linux suite**: deterministic tests for every command in
   [`surface.json`](surface.json). Families with a canonical implementation get
   a differential harness against it (dosfstools, smartmontools, nvme-cli,
   util-linux, ethtool, iw, xz, zstd, procps, kmod, ...), used only as a
   test/reference dependency; where output carries volatile identifiers or
   counters, compare parsed semantics, not unstable text. Destructive and
   system operations run only in sandboxes: loop files, mount, user and
   network namespaces, veth pairs, synthetic sysfs/procfs/efivarfs fixtures,
   QEMU virtual disks and NVMe devices, QEMU/OVMF for EFI, `scsi_debug` and
   nvme loop facilities when safe. Never mutate the developer's real block
   devices, firmware variables, network interfaces or SMART state.
8. **Clean smoke**: a Linux image with XSH first on `PATH` and no GNU
   coreutils, util-linux, procps, findutils, grep, sed, gawk, diffutils or
   kmod, running real workflows: file manipulation, archives, checksum
   verification, find/xargs, grep/sed/awk pipelines, process, mount/block,
   network, module and hardware inspection, partitioning, FAT creation and
   checking, compression round trips, SMART/NVMe inspection against QEMU
   devices, namespace and process control, boot-adjacent commands.
9. **One model per kernel ABI**: [`check_kernel_reads.py`](check_kernel_reads.py)
   ratchet; a new `/proc` or `/sys` reader in an applet fails the build.
10. **Surface inventory**: `surface.json` entries are only ever added;
   `parity.py --check` fails when the count drops below the pin in
   `upstream.lock.json`, and every command is implemented or carries an
   accepted capability/platform rationale.

## Deliverables

Implementation; pinned reference manifest; uutils adapter; GNU differential
runner; per-command parity report; native tests for new primitives and
applets; documentation (what "XSH core compatibility" means, baseline
revision, rerunning the suites, unsupported capability areas, downstream
discovery and installation); and a final report: applets before/after,
uutils coverage, uutils integration pass rate, GNU differential, intentional
gaps, extra Linux utilities, new native APIs.

## Sequence (revised)

Wave 0 is serial and must land before fan-out; later waves are parallel lanes
defined in [`LANES.md`](LANES.md). Native domain lanes land before the applet
lanes that consume them.

| Wave | Work |
|---|---|
| 0 | parity manifest and adapter (done); toolchain and suite build; baseline uutils run of current XSH; `cli` GNU mode; `lib/gnu` diagnostics; invoked-name access; byte helpers; ratchets wired into `make check`; applet manifest in release |
| 1 | existing-applet repair (ignored buckets, GNU diagnostics); missing cheap coreutils; native fs, process/tty and bytes/hash primitives; text and checksum families; `system-report` collector extraction; `sed` and `awk` start |
| 2 | difficult coreutils finish; GNU differential to zero blockers; native domains for block/partition, process sampling, compression streaming (+zstd), xattr/ACL/capability; compression CLIs; hardware/kmod/util-linux wrappers; procps and sampling tools; `ip`/`ss`/`ping`; grep/find/xargs; diff/cmp/patch; login/getent/udevadm |
| 3 | block, mount and partition tools; FAT module and dosfstools surface; attributes/ACL/capabilities; namespace and process control; storage health (`smartctl`, `nvme`); `ethtool`, `iw`, diagnostics and HTTP; `cpio`; `efibootmgr`; Phase 6 commands |
| 4 | BusyBox route; Gate 5 option comparison; Gate 8 clean smoke image; performance pass; final report |

## Environment notes

- **Current execution scope:** native Linux x86_64 only, with no Docker.
  Source `dev/compat/native-env.sh` after installing the pinned toolchain and
  mold. It checks the host architecture, requires mold 3.0.0, and sets
  `RUSTFLAGS` so Cargo links with mold. The release product build is
  `cargo build --release --locked -p xsh --bin xsh -p xsht --bin xsht`.
  See [`README.md`](README.md#setup) for the verified mold release SHA-256 and
  local tool layout. `run-gnu.sh prepare` uses a temporary compatibility copy
  of the pinned uutils helper because this revision puts external
  `libstdbuf.so` under its Cargo build output rather than `target/release/deps`;
  it also repairs a previously emptied factor-test list before a repeated
  `autoreconf`. The reference checkout stays unchanged. This host has no system
  package installation; GNU build tools and development headers are in the
  workspace-local `../.tools/gnu-env` prefix. ACL and capability support are
  enabled in the prepared GNU tree.
- The repository pins `nightly-2026-09-15`; XSH uses no `#![feature]`, so
  stable 1.97 builds it when the pinned toolchain is unreachable
  (`RUSTUP_TOOLCHAIN=stable`). Do not commit a toolchain change for this.
- Cloud sessions need network access to crates.io (`index.crates.io`,
  `static.crates.io`), `static.rust-lang.org`, GitHub release downloads, and,
  for the `xsh-test` image, Docker Hub and `dl-cdn.alpinelinux.org`. The
  default **Trusted** environment level covers crates.io, the Rust
  distribution host and Docker Hub; add anything else as a custom domain.
- The reference host has 4 cores, about 15 GB RAM, a 14.3 GB memory cgroup
  shared by every process the session starts (builds, lanes and suites), and
  about 27 GB of disk. First full runs showed what that costs: a runaway
  applet and a test process buffering gigabytes each got `cargo-nextest`
  OOM-killed. The uutils runner therefore caps each test process and each
  applet (address space, 4 GiB and 3 GiB), limits nextest threads, and refuses
  to publish results from a run nextest did not finish.
- The uutils test crate embeds the build-time `PATH` (`env!("PATH")`) and its
  `build.rs` lists the gitignored `docs/tldr.zip` as a rerun trigger; either
  one changing makes cargo recompile the whole test crate (five minutes).
  `run-uutils.sh` pins `PATH` and creates an empty placeholder.
- The uutils framework runs each command with a cleared environment, so the
  staged native XSH dispatcher finds its stage from `argv[0]` rather than from
  an environment variable. It execs the applet directly and applies the stage's
  per-applet address-space limit.
- Native-lane features need hardware or kernel facilities this VM lacks
  (QEMU/OVMF, NVMe devices, `scsi_debug`, privileged namespaces): those tests
  are written against synthetic fixtures here and run for real in the
  `xsh-test` image or a privileged CI lane.
- This campaign run follows the user's native-host instruction: Linux x86_64
  host runs are the evidence recorded here. `Dockerfile.test` is not used for
  this run; do not generalize its result counts to other architectures.
