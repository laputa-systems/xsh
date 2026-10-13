# system-report

`system-report` collects a bounded inventory from the process-visible Linux
system and renders text or JSON v1. It reads system sources without starting
child processes. A report is a set of observations, not an atomic snapshot or
a description of the physical host.

## Run

```text
system-report [--json] [--full] [--section NAME] [--sensitive] [--from FILE] [-h|--help] [-V|--version]
```

`--section` accepts `identity`, `cpu`, `memory`, `pci`, `usb`, `storage`,
`network`, `sensors`, `power`, `firmware`, `kernel`, `processes`, or `devices`.
Live collection retains identity and gathers the requested section plus data
needed for its relationships. With `--from FILE`, the command validates and
renders a saved v1 JSON report without collecting host data; it reads at most
16 MiB. `--section` also projects a saved report.

`--json` emits one JSON v1 document. `--full` expands text output. `-V` and
`--version` print the schema version. Valid partial reports still succeed and
carry section status, field observation states, and collection issues; missing
or invalid values are not inferred.

## Privacy and limits

Output is redacted by default, including when replaying a saved report.
`--sensitive` restores supported identifying values. Mount credentials and
unknown mount options are removed during collection and replay and cannot be
restored. The kernel command line can contain secrets and is included only with
`--sensitive`. Process records omit per-process command lines and environment
variables. Text output escapes terminal controls and other unsafe characters
from collected values.

The report describes only data visible to the current process. Collection may
race with system changes, and unavailable or malformed sources remain marked
as such instead of being represented as observed values.
