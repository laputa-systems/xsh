# XSH btop monitor

This is the fixed Linux monitor for the in-scope default behavior inventoried
from btop v1.4.7. The collector, interval reducer, replay loader, terminal loop,
process-list state, and renderer are XSH. It shows CPU, memory, network, and
process panels using a bounded ASCII layout.

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

The replay bundle uses manifest schema 2 and contains recorded terminal inputs
as well as rooted source frames:

```text
BUNDLE/
  manifest.json
  frames/0/proc/{stat,uptime,loadavg,meminfo,net/dev}
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
added below each `proc` directory. Replay timestamps, source identity, and
display inputs are explicit because filesystem contents do not encode them.
Schema 1 bundles are rejected because they do not pin display capability inputs.

## Monitor behavior

The live collector samples from a rooted view of `/proc` on one-second absolute
monotonic deadlines. The first CPU, process, and network rates are shown as `warming`; an observed zero
counter or rate stays distinct from an absent, malformed, reset, or unavailable
source. CPU usage uses aggregate `/proc/stat` counters. Process CPU direct uses
each process's `utime + stime` over the aggregate CPU interval; CPU lazy sorts by
the process's lifetime-average CPU and promotes recent high-use processes. RSS
uses resident pages times the injected page size. A process's start time is
part of its identity, so a reused PID has no inherited rate or display text.
Model JSON rows include the lifetime-average CPU rate used by CPU lazy sorting.

Collection classes use absolute deadlines: counters refresh each second, gauges
every five seconds, blocking capacity reads every thirty seconds, and inventory
is due at startup, after explicit invalidation, or after thirty seconds. The
current live readers run with the counter pass. Missed deadlines coalesce into
one new observation. When a UID read is unavailable, a same-source
PID/start-time/command value remains visible for at most five seconds with a `~`
marker; after that it is unavailable. A command change, source change, process
exit, or PID reuse invalidates that cached UID.

CPU rates use the first eight aggregate `/proc/stat` counters, subtract guest
and guest-nice time to avoid double-counting them, and treat a decreasing
iowait counter as zero delta. Per-core rows keep sparse logical CPU IDs; a new
or returned core starts warming, and a reset core has no rate until its next
sample. Uptime and the three load averages remain observed only when their
complete source values parse.

Memory shows total, available, and `total - available`; swap shows observed
absence when both swap counters are absent. Network chooses the
name-sorted non-loopback interface, or loopback only when it is the only
interface, and displays byte totals and interval rates in bytes per second
with binary KiB/s and MiB/s labels. The network source's observed empty list
remains distinct from a failed read.

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

The system-report and btop paths share the `/proc/meminfo` parser. Process
`stat` parsing also reuses `system_report_live.parse_proc_stat`; it retains
`utime + stime` for btop without changing system-report JSON v1.

## Host release evidence

Focused host release tests are kept with the implementation:

- `core/tests/test-btop-sources.xsh` covers raw readers, the B3 arithmetic
  literals, zero versus absent, truncation, process identity, and rate resets.
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
  seconds apart. On Alpine x86_64 musl, input-to-present p95 was 1.032 ms and
  p99 was 1.181 ms; replay sampling stayed on its separate recorded interval.
- `rg -n -i btop src crates stdlib` found no monitor-specific native code.

These tests establish host release behavior. Linux qualification is recorded
separately in `dev/btop-progress.json`.

## Current limits

The renderer remains ASCII, has no graphs, and uses a fixed four-panel
arrangement. Process actions, configuration, settings menus, GPUs, disks,
sensors, and non-Linux live collectors are outside this monitor surface.
