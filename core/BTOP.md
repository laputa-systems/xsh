# XSH btop monitor

This is the fixed Linux monitor for the in-scope default behavior inventoried
from btop v1.4.7. The collector, interval reducer, replay loader, terminal loop,
process-list state, and renderer are XSH. It shows CPU, memory, network, and
process panels in a bounded terminal-cell layout. The memory panel includes
mounted-device capacity and block I/O. CPU temperature, frequency, and battery
status appear as CPU-panel readings when their kernel sources are observed.

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
Live process command lines are added only to the interactive display; they do
not enter model or screen dumps or replay observations.

The replay bundle uses manifest schema 3 and contains recorded terminal inputs,
rooted source frames, and mount-capacity results captured beside their frame:

```text
BUNDLE/
  manifest.json
  observations.jsonl
  frames/0/proc/{stat,uptime,loadavg,meminfo,net/dev}
  frames/0/proc/self/mountinfo
  frames/0/sys/class/block/...
  frames/1/proc/{stat,uptime,loadavg,meminfo,net/dev}
  frames/N/proc/...
```

The manifest is JSON with positive `page_size_bytes` and
`clock_ticks_per_second`, a nonempty `source_id`, a `display` record, and a
nonempty `frames` array whose records each contain `at_ns`. `display` contains
`term`, `colorterm`, `no_color`, `lc_all`, `lc_ctype`, `lang`, `rows`, and
`cols`; string fields are bounded to 256 bytes and dimensions to 1–2000 cells.
Frame timestamps must be nonnegative and strictly increasing. For each index,
the collector reads that frame's rooted `proc` tree; missing fields remain
unavailable and never fall back to the live host. Process directories can be
added below each `proc` directory. `observations.jsonl` has one record for each
valid `mountinfo` row, keyed by frame index, timestamp, sequence, mount ID,
device number, and decoded target; it carries the captured capacity state and
total, used, and user-available bytes as canonical decimal strings. Records are
bounded and must match the raw mount identities exactly. Replay never runs
`statvfs` on its snapshot tree.
Replay timestamps, source identity, display inputs, and capacity results are
explicit because filesystem contents do not encode them. Schemas 1 and 2 are
rejected because they do not pin all non-file inputs.

## Monitor behavior

The live collector samples from a rooted view of `/proc` on one-second absolute
monotonic deadlines. Interval rates use those monotonic sample timestamps;
wall-clock changes do not alter them. The first CPU, process, and network rates
are shown as `warming`; an observed zero counter or rate stays distinct from an
absent, malformed, reset, or unavailable source. CPU usage uses aggregate
`/proc/stat` counters. Process CPU direct uses
each process's `utime + stime` over the aggregate CPU interval; CPU lazy sorts by
the process's lifetime-average CPU and promotes recent high-use processes. RSS
uses resident pages times the injected page size. A process's start time is
part of its identity, so a reused PID has no inherited rate or display text.
The `c` key switches process CPU display and direct sorting from the all-core
percentage to the per-logical-core percentage. Model JSON rows include both
rates and the lifetime-average CPU rate used by CPU lazy sorting.

Collection classes use absolute deadlines: counters refresh each second, gauges
every five seconds, blocking capacity reads every thirty seconds, and inventory
is due at startup, after explicit invalidation, or after thirty seconds. CPU,
memory, network, and process counters run with the counter pass. Temperature,
CPU frequency, and battery readings reuse their last observation until the next
gauge deadline. Missed deadlines coalesce into one new observation. When a UID read is unavailable, a same-source
PID/start-time/command value remains visible for at most five seconds with a `~`
marker; after that it is unavailable. A command change, source change, process
exit, or PID reuse invalidates that cached UID.

When a counter deadline is skipped, interval rates show `gap` for that sample
and resume after the next complete interval. `network.interfaces` in model
dumps includes the derived counters and rates for each observed interface; the
top-level network values describe the pinned default interface. Network byte
counters and process start ticks are canonical decimal strings, while internal
rate math keeps them as signed 64-bit integers. Disk sector counters and mount
capacity bytes use the same decimal-string wire form.

CPU rates use the first eight aggregate `/proc/stat` counters, subtract guest
and guest-nice time to avoid double-counting them, and treat a decreasing
iowait counter as zero delta. Per-core rows keep sparse logical CPU IDs; a new
or returned core starts warming, and a reset core has no rate until its next
sample. Uptime and the three load averages remain observed only when their
complete source values parse.

CPU temperatures reuse the system-report hwmon and thermal-zone readers. The
package channel prefers `Package id`, `Tdie`, or `SoC Temperature` labels, then
a CPU-named sensor, then the first deterministic sensor; duplicate logical core
labels yield one reading. Signed millidegrees and observed zero values are
preserved, while malformed or missing channels stay unavailable. The default
empty `cpu_core_map` maps sensor channels from `/proc/cpuinfo` physical core IDs;
if topology is incomplete, SMT sibling or evenly spread logical-ID mapping is
used. CPU frequency uses the first positive CPUFreq policy reading by default
and falls back to the first `cpu MHz` value from `/proc/cpuinfo`; the reducer
also supports the pinned
range, lowest, highest, and average modes. Battery selection prefers a present
Battery over a UPS, uses capacity or an energy/charge ratio, and derives status,
power, and remaining time from the exported attributes. Battery and sensor
readings follow the five-second gauge cadence.

Memory shows total, available, and `total - available`; swap shows observed
absence when both swap counters are absent. Network chooses the
name-sorted non-loopback interface, or loopback only when it is the only
interface, and displays byte totals and interval rates in bytes per second
with binary KiB/s and MiB/s labels. The network source's observed empty list
remains distinct from a failed read.
`b` and `n` cycle through observed interface names in sorted order. Selecting a
different interface starts its network graph at that interface's current
sample instead of carrying the previous interface's history. `z` toggles the
displayed byte totals between source counters and values since the current
interface's offset; it does not alter interval rates.

Block statistics refresh with the one-second counter sample. Read and write
sector deltas use Linux's fixed 512-byte accounting sectors, independent of
physical sector size; `in_flight` remains the current request-count gauge. A
mounted device number resolves to one block row, and counters are never summed
across a disk, its partitions, or stacked devices. Disk history uses the
device's `diskseq` and major/minor pair; a partition inherits its containing
disk's sequence. If that identity is unavailable, the row retains capacity
but its I/O rates stay unavailable. Mount capacity refreshes every thirty
seconds, keeps total, used, and user-available bytes separate, and comes from
captured statvfs results during replay. The model carries `capacity_age_ns` for
consumers that need the age of a reused capacity observation.

If collection finishes after multiple deadlines, the monitor records the
skipped intervals and starts one fresh sample instead of running a catch-up
burst. Replay uses recorded display inputs and frame timestamps without
consulting live sources. The default process view uses CPU lazy sorting; the
sort cycle is PID, name, command line, threads, user, memory, CPU direct, and
CPU lazy. Each mode follows its pinned ascending or descending default, and
`r` reverses it. The live display reads bounded `/proc/[pid]/cmdline` bytes
between matching process-stat identity checks, replaces NUL argument
separators with spaces, and falls back to the process name when the read is
unavailable. It never reads `/proc/[pid]/environ`; command-line text stays out
of observations, model JSON, screen dumps, and replay. `q`, `Q`, the raw
Ctrl-C byte, or stdin EOF exits without Return. Outside the filter editor,
arrows select a process by PID and start time, Left/Right changes the sort,
and `f` or `/` opens the filter editor. The filter searches PID, UID, process
name, resolved user, and the live command line. Enter accepts, Escape cancels,
Backspace removes one character, and Delete clears the filter. Bracketed paste
stays inert. Resize preserves the selected process identity and redraws the
current view.

The process list starts flat, sorted by CPU lazy, with child aggregation off.
`e` switches to a process tree while keeping the selected sort order; tree
siblings use that same order with PID as the stable tie-break. `+` expands the
selected node, `-` collapses it, Space toggles it, and `C` toggles the selected
node's immediate child nodes. `E` toggles every non-root parent between
expanded and collapsed. Collapse state is tied to PID and start time, so a
reused PID does not inherit another process lifetime's view state. When child
aggregation is enabled in the fixed model defaults, CPU and resident memory
are accumulated from the complete process tree before filtering and viewport
clipping.

The `%` key switches the process memory column from bytes to a percentage of
total physical memory. `a` toggles automatic network graph scaling and `y`
uses one shared scale for receive and transmit. `d` hides or shows mounted disk
details. The default disk view shows each visible device and mount with used
and total capacity plus read/write byte rates; `i` switches from a combined
I/O graph to separate read and write graphs. The graphs follow the first
identified mounted device by disk sequence and device number. The selected
process identity stays visible across updates and sort changes, so the
read-only list follows it without a separate follow mode. These controls change
only the current view;
the monitor has no settings menu or configuration file. Other upstream controls
outside the read-only contract, including process actions, menu keys,
panel-layout keys, and alternate vim bindings, have no effect.

Terminal input and alternate-screen modes are owned by one XSH session and are
restored on normal return, key exit, EOF, handled write failure, partial start,
and runtime error. A process killed by a signal or a dead terminal transport
cannot run XSH cleanup. The minimum canvas is 80 by 24; smaller sizes show a safe
message.

Cell-aware TUI operations use Unicode 16.0.0 width data, with ambiguous-width
characters narrow. Hostile terminal controls, line separators, and bidi controls
are replaced before display. Surfaces are bounded to 2000 rows/columns and
200,000 cells. A committed frame changes only after the complete update is
written; partial output forces the next redraw to start from a cleared screen.
Unchanged frames write zero content bytes. PTY tests reconstruct the emitted
terminal subset and compare a delta update with a fresh full frame.

Capability selection uses the captured `TERM`, `COLORTERM`, `NO_COLOR`, locale,
and terminal dimensions. `TERM=dumb`, an empty `TERM`, or `NO_COLOR` disables
color; a truecolor marker selects truecolor, a `256color` marker selects 256
colors, and other usable terminals select 16 colors. UTF-8 locale detection
selects Unicode symbols; other locales select ASCII symbols. A canvas smaller
than 80 by 24 displays a safe size message.

The default layout places CPU across the top at 32% of terminal height. The
lower area puts Memory / Disks in the left 55% and Processes on the right; the
Network panel replaces the lower 60% of the left column. The memory panel
reserves a disk section even when no device rows are available. CPU, memory,
network, temperature, and the selected-device I/O graphs retain up to 600
source samples independently of the visible width; resize changes only the
viewport. Braille graphs are used with UTF-8 terminals and ASCII marks
otherwise. Graph gaps preserve time positions and autoscaling includes every
retained spike.

The built-in panel palette is defined in truecolor. It maps to the nearest
xterm 256-color cube or grayscale entry, then to the nearest standard 16-color
entry; `NO_COLOR`, an empty `TERM`, and `TERM=dumb` omit foreground colors.

## Reader inventory

| Displayed field | Kernel input | Owner | Unit and kind | Bounds |
|---|---|---|---|---|
| Aggregate and per-core CPU usage | `/proc/stat` `cpu` and `cpuN` rows | `sys_cpu.parse_stat`, then `btop_rates.cpu_percent` | ticks, counter interval | Nonnegative signed 64-bit fields; sparse IDs retained; incomplete source withheld |
| Uptime and load | `/proc/uptime`, `/proc/loadavg` | `sys_cpu.parse_uptime`, `sys_cpu.parse_loadavg` | seconds and load averages, gauges | Bounded reads; malformed values withheld |
| Total, available, used memory and swap | `/proc/meminfo` | `sys_mem.parse`, then `btop_rates.used_bytes` | bytes, gauges | kB-to-byte conversion checked against the exact JSON integer bound |
| Raw memory categories (free, buffers, cache, active/inactive, dirty/writeback) | `/proc/meminfo` | `sys_mem.parse` | bytes, gauges | Categories remain independent because their accounting overlaps |
| Interface name, RX/TX totals and rates | `/proc/net/dev`, `/sys/class/net/[name]/ifindex` | `sys_net.collect`, `sys_net.selected_interface`, `btop_rates.counter_rate` | bytes and bytes/second; totals are counters | Bounded reads; rates require matching observed interface indices across samples |
| PID, PPID, thread count, UID, command, CPU and RSS | `/proc/[pid]/stat`, `status`, `statm` | `sys_proc.collect`, `system_report_live.parse_proc_stat`, `btop_rates.process_percent` | identity, counts, ticks and bytes | PID/start-time race is withheld; RSS page conversion is checked; cached UID expires after five seconds |
| Process user label and local command line | `/proc/[pid]/cmdline`; UID resolved through the host account database | `sys_proc.collect_local_display` | display-only user name and argument text | 4096-byte bounded command line; stat identity checked before and after; no environment read or replay capture |
| Package and core temperatures | `/sys/class/hwmon/*/temp*_input`, labels, and `/sys/class/thermal/thermal_zone*/temp` | `system_report_live.collect_sensors`, `btop_sensors.cpu_temperature`, `btop_sensors.core_mapping` | signed millidegrees Celsius, gauge | Existing rooted system-report parser; malformed or missing values are withheld; duplicate core IDs are collapsed deterministically |
| CPU frequency | `/sys/devices/system/cpu/cpufreq/policy*/scaling_cur_freq`, then `/proc/cpuinfo` `cpu MHz` | `system_report_live.collect_frequency_policies`, `read_cpu_info`, `btop_sensors.frequency` | kHz source reduced to MHz, gauge | Positive readings only; pinned CPUFreq modes; bounded source reads |
| Battery level, state, power, and time | `/sys/class/power_supply/*` type, present, status, capacity, energy/charge, voltage/current, and power | `system_report_live.collect_power`, `btop_sensors.battery` | percent, microwatts, microamp-hours, microvolts, gauge | Only present Battery/UPS devices; values remain absent or malformed when the source cannot support a result |
| Disk reads/writes and in-flight requests | `/sys/class/block/*/stat` and `diskseq` | `sys_block.collect`, `btop_disks.step` | 512-byte accounting sectors; bytes/second; `in_flight` is a request-count gauge | Linux 11/15/17-field layouts; future fields are reported; history requires the disk sequence and device number; partitions inherit their parent disk sequence |
| Mounted filesystem capacity | `/proc/self/mountinfo` and rooted filesystem statistics | `sys_mount.collect`, `sys_mount.apply_captured_usage`, `btop_disks.step` | total, used, and user-available bytes, gauges | Mount escapes are decoded; shadowed, duplicate-target, and remote filesystems are not queried; results refresh on the 30-second blocking schedule and replay uses captured results |

The system-report and btop paths share the `/proc/meminfo`, sensor, frequency,
power-supply, block, mount, and process `stat` readers. Block counters retain
signed i64 values for monitor rate math; system-report JSON v1 continues to
withhold counters above its exact-number bound. The monitor uses the shared
process parser's i64 start-time policy and retains `utime + stime` without
changing system-report JSON. Monitor battery fields remain private to
`system_report_live` and do not widen system-report JSON.

## Host release evidence

Focused host release tests are kept with the implementation:

- `core/tests/test-btop-sources.xsh` covers raw readers, the B3 arithmetic
  literals, zero versus absent, truncation, process identity, and rate resets.
- `core/tests/test-btop-sensors.xsh` covers signed and duplicate temperature
  channels, thermal fallback, all pinned temperature and frequency modes,
  battery selection and derived values, gauge-cache reuse, and visible CPU
  panel output.
- `core/tests/test-btop-model.xsh` and `core/tests/test-btop-replay.xsh` cover
  deterministic reduction, process lifetime rates, strict replay times,
  missing fields, and frozen raw fixture digests.
- `core/tests/test-btop-process-tree.xsh` covers orphan, self-parent, cycle,
  PID-reuse parent edges, deterministic adjacency, and descendant aggregation.
- `core/tests/test-btop-render.xsh` covers field placement at 80 by 24 and
  120 by 40 and the below-minimum message.
- `core/tests/test-btop-cli.xsh` covers unknown flags, bounded headless dumps,
  and syscall traces for headless replay and live source collection.
- `tests/core_compat_extra/pty.rs` drives the real applet through PTYs and
  checks updates, resize, `q`, Ctrl-C, EOF, write failure, partial startup, and
  terminal restoration. The runtime-error case uses the existing `ptytest`
  semantic terminal model.
- `core/tests/test-btop-loop.xsh` and `test-btop-ui.xsh` cover absolute sample
  deadlines, coalesced work, identity selection, every pinned process sort and
  direction, CPU-lazy spike promotion, local-text filtering, and resize.
- `core/tests/test-btop-performance.xsh` records seeded process-join scaling,
  bounded process and device identity churn, optional-source degradation, and
  model/render/diff work. On Alpine x86_64 musl release, the 1,000-to-2,000
  process join median ratio was 2.03× (9.342 ms to 18.945 ms); the 1,000-row
  120 by 40 model/render/diff p95 was 2.886 s, above the 50 ms target.
- `core/tests/test-btop-schedule.xsh` covers source-class deadlines,
  invalidation, coalescing, and checked monotonic bounds. The model tests cover
  UID expiry, exec and credential changes, PID reuse, exit eviction, and churn.
- `core/tests/test-btop-input.xsh`, `test-btop-terminal.xsh`, and
  `test-btop-surface.xsh` cover chunked input, capability detection, terminal
  ownership, geometry, cell-safe widgets, and delta recovery.
- `tests/xsh/stdlib/tui.xsh` and `tests/xsh/api-tool.xsh` pin the public cell
  width, clipping, padding, slicing, and sanitization contracts.
- `tests/xsh/stdlib/tui.xsh::test_tui_public_apis_compose_a_generic_table_input_and_overlay`
  runs a small filtered table app from piped input using only public `tui`
  operations for its display.
- `tests/core_compat_extra/pty.rs` uses a bounded VT reconstructor to compare a
  real delta with a fresh full frame and drive interactive sorting, filtering,
  selection, and resize.
- The release PTY latency probe sent 100 sort events between replay frames 60
  seconds apart. The final run on Alpine x86_64 musl measured input-to-present
  p95 2.955 ms and p99 3.014 ms; replay sampling stayed on its separate
  recorded interval.
- `rg -n -i btop src crates stdlib` found no monitor-specific native code.

These tests establish host release behavior. Linux qualification is recorded
separately in `dev/btop-progress.json`.

## Current limits

Battery behavior is fixture-tested only; live hardware battery behavior has
not been verified.
GPU, process details and mutations, configuration menus, signal-driven
terminal restoration, and non-Linux live collectors are outside this monitor
surface.
