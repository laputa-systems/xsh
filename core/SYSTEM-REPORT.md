# system-report

`core/system-report.xsh` collects a bounded Linux inventory from the
process-visible system view and renders text or JSON v1. It reads rooted
`/proc`, `/sys`, and `/etc` sources and starts no child processes. A report is a
bounded set of reads, not an atomic snapshot or a claim about the physical
host.

## Command

```text
system-report [--json] [--full] [--section NAME] [--sensitive] [--from FILE]
```

`--section` accepts `identity`, `cpu`, `memory`, `pci`, `usb`, `storage`,
`network`, `sensors`, `power`, `firmware`, `kernel`, `processes`, or `devices`.
Live collection always retains identity and collects only the requested
section plus data needed to resolve its relationships. A saved-report
projection follows the same rule. `--json` emits one JSON v1 document;
`--full` expands text output. `--sensitive` includes supported identifying
values. `-V` and `--version` print the schema version.

Live collection is supported on Linux. `--from FILE` reads at most 16 MiB,
validates a saved v1 JSON report, and renders it without collecting host data.
Invalid or truncated input fails instead of being treated as a partial report.
Valid partial reports exit 0 with observation states and collection issues.
Usage, invalid replay, and unsupported live collection use distinct error
variants; unhandled failures retain their source error and exit with runtime
failure status 3.

## Collection and report states

`core/lib/system_report_live.xsh` collects from a caller-owned filesystem root
and maps source records into `SystemReport` in
`core/lib/system_report.xsh`. The `core/lib/sys_*.xsh` modules own parsers for
kernel interfaces; the report module owns JSON encoding, redaction, and text
rendering. `collect_from_root` skips filesystem-capacity queries by default.
Live collection enables capacity for eligible local filesystems only when the
mount is not shadowed or automounted: a pathname can resolve to a different
overmount or trigger an automount. Rooted `statvfs` uses an `O_PATH` descriptor
to avoid reading target data or blocking on special files. Credentials and
unknown mount options are removed during collection and replay, including
sensitive output.

Observation states distinguish values that are observed, absent, unsupported,
not requested, redacted, malformed, truncated, raced, disappeared, or unreadable.
Section status is separate from item values: a successful empty enumeration
is complete, while a failed nested read can make an otherwise enumerated
section partial. Issues retain the affected section and field. A zero is an
observed value; missing or invalid data remains unavailable rather than being
filled with an inferred value. Truncated sources never contribute a parsed
prefix. Integer values must stay within JSON's exact integer range
(`9,007,199,254,740,991`); out-of-range measurements remain unavailable with
an issue.

Process records contain no per-process command-line arguments or environment.
The kernel section can include the system kernel command line. Text output
escapes terminal control, bidi, and line-separator characters from source data.

## JSON v1 and privacy

`SystemReportJson` defines the v1 root fields and sections; enum values use
stable lowercase strings. The decoder rejects unsupported schema versions,
unknown state spellings, and malformed present fields. It accepts selected
older v1 layouts with missing optional fields, leaving those values null or
unsupported rather than guessing them.

Text and JSON output redact by default, including replay output. Redaction
withholds source paths and identifying names or addresses while retaining
numeric hardware IDs and indexed relationships. Sensor labels are redacted;
numeric readings and their units remain. Redaction reduces exposure but does
not guarantee anonymity. `--sensitive` restores supported identifying values,
but cannot restore mount sources or options already removed because they may
contain credentials or other sensitive text.
`KernelSection.command_line` is redacted by default; `--sensitive` preserves the
complete bounded `/proc/cmdline` bytes, which may contain secrets.

PCI, USB, block-device, mount, and sensor relationships use indexes into the
same report's arrays. This keeps links resolvable when identifying names and
addresses are redacted. Sensor values retain source units and unknown channels
remain raw; battery charge and energy measurements are kept separate rather
than inferred from one another.

## Verification

Run `cargo dev system-report-check [OPTIONS]` from the repository root. Linux
verification uses the pinned `xsh-test` image from `Dockerfile.test`; do not
substitute another toolchain, image, or libc. The checker validates
`dev/system-report-coverage.json`. Deterministic rooted fixtures do not inspect
live devices:

```sh
cargo dev system-report-check --run-fixtures \
  --xsh-bin /absolute/path/to/xsh \
  --xsht-bin /absolute/path/to/xsht \
  --cargo-bin /absolute/path/to/cargo
```

`--no-subprocess` traces report invocations with `strace` and checks that they
start no child process or read outside the allowlist. Live comparison flags
check the current Linux view against independent references. Binary and script
arguments must be absolute; for example:

```sh
cargo dev system-report-check --no-subprocess \
  --xsh-bin /absolute/path/to/xsh \
  --script /absolute/path/to/core/system-report.xsh
cargo dev system-report-check --compare-command-line \
  --xsh-bin /absolute/path/to/xsh \
  --script /absolute/path/to/core/system-report.xsh
```

Capture and replay are separate operations. Run both in the pinned image; a
replay is scoreable only when its saved sources and independent reference are
complete and unchanged. Eligibility comes from the coverage manifest and
independent references, not from the candidate's unsupported claims. A source
missing from the container is unavailable, never an exact host match.
Before creating `.work/tmp`, ensure `.work/` is ignored through
`.git/info/exclude`.

```sh
mkdir -p .work/tmp
cargo dev system-report-check --capture-cpu-bundle "$PWD/.work/tmp/cpu-capture"
cargo dev system-report-check --replay-cpu-bundle "$PWD/.work/tmp/cpu-capture"
```

Keep capture bundles private and untracked because raw system sources may
contain sensitive values.

The deterministic XSH coverage is in
`tests/xsh/system-report.xsh` and
`tests/xsh/system-report-collect.xsh`; per-interface parser tests are in
`core/tests/test-sys-*.xsh`. The harness and its tests are in
`dev/system_report_check.xsh` and `dev/tests/test-system-report-check.xsh`.
Repository test gates are documented in [`docs/TESTING.md`](../docs/TESTING.md).
