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

The JSON v1 observation states are `observed`, `absent`, `unsupported`,
`permission_denied`, `not_requested`, `redacted`, `malformed`, `disappeared`,
`raced`, `truncated`, `range_failure`, and `read_failure`. Numeric identifiers
and relationship indexes are bounded by the exact-integer limit
`9,007,199,254,740,991`. Relationship indexes refer to positions in arrays in
the same report—for example, a PCI parent index addresses `pci.functions`, and
a mount's block-device index addresses `storage.devices`. Redaction preserves
these links; an index is not a kernel identifier.

Replay accepts older schema v1 reports with selected later fields omitted. It
fills missing namespace observations as `unsupported`, treats absent CPU idle
and thermal trip indexes as unknown, fills omitted USB runtime and sensor
channel fields as null, and marks a missing device-class entry name
`unsupported`. It also maps legacy CPU frequency `requested_current_khz` to
`scaling_current_khz` and normalizes the older power-cap zone shape. Conflicting
old and new frequency fields are invalid.

## Privacy and limits

Output is redacted by default, including when replaying a saved report.
`--sensitive` restores supported identifying values. Mount credentials and
unknown mount options are removed during collection and replay and cannot be
restored. The kernel command line can contain secrets and is included only with
`--sensitive`. Process records omit per-process command lines and environment
variables. Redaction reduces exposure but does not guarantee anonymity. Text
output escapes terminal controls and other unsafe characters from collected
values.

Mount capacity is queried only for eligible local filesystem types when the
mount table is valid and the mount and its ancestors have unique, unshadowed
targets. Overmounts and shadowed ancestors are skipped because resolving a
mount path could reach a different filesystem. Automount filesystems such as
`autofs` are ineligible, avoiding a capacity lookup that could trigger an
automount.

The report describes only data visible to the current process. Collection may
race with system changes, and unavailable or malformed sources remain marked
as such instead of being represented as observed values.
