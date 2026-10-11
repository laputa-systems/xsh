# XSH btop monitor

This is the fixed G1 monitor for the Linux data and default behavior inventoried
from btop v1.4.7. The collector, interval reducer, replay loader, terminal loop,
and renderer are XSH. It shows CPU, memory, network, and process panels using a
temporary ASCII layout.

## Running the monitor

`core/btop.xsh` takes no product options and does not read a configuration
file. The following harness flags choose a data source or output sink:

```text
xsh core/btop.xsh                         # live monitor
xsh core/btop.xsh -- --replay BUNDLE      # interactive replay
xsh core/btop.xsh -- --replay BUNDLE --headless
xsh core/btop.xsh -- --replay BUNDLE --headless --dump-model FILE.jsonl
xsh core/btop.xsh -- --replay BUNDLE --headless --dump-screen FILE.jsonl
xsh core/btop.xsh -- --frames N           # live, bounded diagnostic run
```

The launcher separator keeps its own arguments separate from applet arguments.
Unknown flags and operands fail with status 2. `--headless` requires `--replay`,
and dumps require `--headless`. `--frames N` is a positive count of samples
processed. Interactive mode presents one frame for each selected sample;
headless mode derives one model frame for each selected replay sample. A replay
ends when its last selected frame is processed; headless replay performs no
terminal access and does not wait between frames. Model dumps contain one JSON model per line.
Screen dumps contain one JSON record per line with the timestamp, dimensions,
minimum-size state, and rendered lines; their diagnostic canvas is 80 by 24.

The replay bundle has this schema:

```text
BUNDLE/
  manifest.json
  frames/0/proc/{stat,uptime,loadavg,meminfo,net/dev}
  frames/1/proc/{stat,uptime,loadavg,meminfo,net/dev}
  frames/N/proc/...
```

The manifest is JSON with `schema: 1`, positive `page_size_bytes` and
`clock_ticks_per_second`, a nonempty `source_id`, and a nonempty `frames` array
whose records each contain `at_ns`. Frame timestamps must be nonnegative and
strictly increasing. For each index, the collector reads that frame's rooted
`proc` tree; missing fields remain unavailable and never fall back to the live
host. Process directories can be added below each `proc` directory. Replay
timestamps and source identity are explicit because filesystem contents do not
encode either value.

## G1 behavior

The live collector samples once per second from a rooted view of `/proc`. The
first CPU, process, and network rates are shown as `warming`; an observed zero
counter or rate stays distinct from an absent, malformed, reset, or unavailable
source. CPU usage uses aggregate `/proc/stat` counters. Process CPU uses each
process's `utime + stime` over the aggregate CPU interval; RSS uses resident
pages times the injected page size. Rows sort by descending observed CPU, then
ascending PID, with non-observed rates after observed rates. A process's start
time is part of its identity, so a reused PID has no inherited rate.

Memory shows total, available, and `total - available`; swap shows observed
absence when both swap counters are absent. Network chooses the
name-sorted non-loopback interface, or loopback only when it is the only
interface, and displays byte totals and interval rates. The network source's
observed empty list remains distinct from a failed read.

Interactive mode needs usable terminal input and output. It buffers each full
frame, refreshes after the one-second sampling interval (or the replay's
relative timestamps), and redraws the previous frame after a resize. `q`, `Q`,
or the raw Ctrl-C byte exits without Return. Normal exits restore terminal
attributes, SGR styling, and cursor visibility. The minimum canvas is 80 by 24;
smaller sizes show a safe message. Signal-driven termination is not covered by
this G1 contract.

## Reader inventory

| Displayed field | Kernel input | Owner | Unit and kind | Bounds |
|---|---|---|---|---|
| Aggregate CPU usage | `/proc/stat` aggregate `cpu` row | `sys_cpu.parse_stat`, then `btop_rates.cpu_percent` | ticks, counter interval | Nonnegative signed 64-bit fields; incomplete source withheld |
| Uptime and load | `/proc/uptime`, `/proc/loadavg` | `sys_cpu.parse_uptime`, `sys_cpu.parse_loadavg` | seconds and load averages, gauges | Bounded reads; malformed values withheld |
| Total, available, used memory and swap | `/proc/meminfo` | `sys_mem.parse`, then `btop_rates.used_bytes` | bytes, gauges | kB-to-byte conversion checked against signed 64-bit range |
| Interface name, RX/TX totals and rates | `/proc/net/dev` | `sys_net.parse`, `sys_net.selected_interface`, `btop_rates.counter_rate` | bytes and bytes/second; totals are counters | Bounded read; nonnegative signed 64-bit counters |
| PID, UID, command, CPU and RSS | `/proc/[pid]/stat`, `status`, `statm` | `sys_proc.collect`, `system_report_live.parse_proc_stat`, `btop_rates.process_percent` | identity, ticks and bytes | PID/start-time race is withheld; RSS page conversion is checked |

The system-report and btop paths share the `/proc/meminfo` parser. Process
`stat` parsing also reuses `system_report_live.parse_proc_stat`; it retains
`utime + stime` for btop without changing system-report JSON v1.

## G1 evidence

Focused host release tests are kept with the implementation:

- `core/tests/test-btop-sources.xsh` covers raw readers, the B3 arithmetic
  literals, zero versus absent, truncation, process identity, and rate resets.
- `core/tests/test-btop-model.xsh` and `core/tests/test-btop-replay.xsh` cover
  deterministic reduction, process ordering and lifetime, strict replay times,
  missing fields, and frozen raw fixture digests.
- `core/tests/test-btop-render.xsh` covers field placement at 80 by 24 and
  120 by 40 and the below-minimum message.
- `core/tests/test-btop-cli.xsh` covers unknown flags, bounded headless dumps,
  and syscall traces for headless replay and live source collection.
- `tests/core_compat_extra/pty.rs` drives the real applet through a PTY and
  checks updates, resize, `q`, Ctrl-C, and terminal restoration.

These tests establish G1 behavior on host release binaries. Linux qualification
is recorded separately in `dev/btop-progress.json`.

## Current limits

This G1 renderer is intentionally temporary: it is ASCII, has no graphs, and
uses a fixed four-panel arrangement. Selection, filtering, process actions,
configuration, settings menus, GPUs, disks, sensors, and non-Linux live
collectors are not part of this G1 surface. The G2 terminal substrate, G3
telemetry slices, and G4a default layout extend this contract as they land.
