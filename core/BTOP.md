# btop monitor

The XSH btop applet is a read-only Linux monitor for CPU, memory, network,
processes, mounted storage, and available sensor readings. It has no settings
file or configuration interface. Live collection reads Linux system sources;
replay renders data captured in a bundle.

## Run

Pass applet options after the XSH argument separator:

```sh
xsh core/btop.xsh
xsh core/btop.xsh -- --frames 3
xsh core/btop.xsh -- --replay BUNDLE
xsh core/btop.xsh -- --replay BUNDLE --headless --dump-model FILE.jsonl
xsh core/btop.xsh -- --replay BUNDLE --headless --dump-screen FILE.jsonl
```

`--frames N` requires a positive count and limits the samples processed.
`--headless` requires `--replay`; model and screen dumps require `--headless`.
Headless replay does not access a terminal or wait between frames. Model dumps
contain one JSON model per line. Screen dumps contain one JSON record per line
with the recorded frame time, dimensions, minimum-size state, and rendered
lines.

Interactive mode requires both stdin and stdout to be terminals. It enters raw
input mode and the alternate screen. Normal exit, including key or EOF exit,
and handled startup, input, replay, or write errors run cleanup that restores
input attributes and resets
terminal display modes. A process killed without running cleanup, or a dead
terminal transport that cannot receive its output, cannot be restored by the
applet.

Model dumps encode wide counters as decimal strings to avoid JSON number
precision loss. This includes process `start_ticks`, network byte counters, and
storage byte and sector counters. The top-level network fields describe the
deterministic default interface; the `interfaces` array carries the counters
and rates for each observed interface.

Replay bundles use schema 3 and contain a `manifest.json`, an
`observations.jsonl`, and rooted source trees under `frames/<index>/proc/` and
`frames/<index>/sys/`. The manifest pins the page size, clock rate, source ID,
terminal and locale inputs, dimensions, and increasing frame timestamps.
`observations.jsonl` records captured mount-capacity results for the matching
frame and mount. Replay uses only recorded inputs: missing data stays
unavailable, and it does not read live sources or query snapshot filesystems.

## Display and controls

The display includes CPU, memory, network, and process panels, with mounted
disk capacity and I/O in the memory panel. Temperature, CPU frequency, and
battery readings appear when their sources are available. Counter-based rates
are sampled every second. Temperature, CPU frequency, and battery readings
refresh every five seconds; mounted capacity refreshes every thirty seconds.
The first rate sample is marked `warming`. Missed sample deadlines coalesce,
and rates show a `gap` when an interval was skipped. The display uses the
terminal's capabilities and locale to choose colors and graph symbols. The
minimum canvas is 80 by 24; smaller terminals show a size message.

| Keys | Action |
|---|---|
| `q`, `Q`, Ctrl-C, or stdin EOF | Exit |
| Up/Down arrows | Select a process |
| Left/Right arrows, `r` | Change process sort; reverse its direction |
| `f` or `/` | Open the process filter |
| Enter, Escape | Accept or cancel the filter |
| Backspace, Delete | Remove one character or clear the filter |
| `c`, `%` | Toggle per-core CPU display; toggle process memory between bytes and percent of total memory |
| `e` | Toggle the process tree |
| `+`, `-`, Space | Expand, collapse, or toggle the selected node |
| `C`, `E` | Toggle the selected node's children; toggle all non-root parents |
| `b`, `n`, `z` | Cycle network interfaces; toggle totals between source counters and values since the current offset |
| `a`, `y` | Toggle automatic graph scaling; synchronize receive and transmit scales |
| `d`, `i` | Hide disk details; split the disk I/O graph into read and write |

The process list starts flat and sorted by CPU lazy. Process command lines are
read locally for the live display and filter only; they are not stored in
observations, replay bundles, or dumps. The applet does not read process
environments or perform process actions.

## Reader inventory

| Values | Inputs | Owner functions | Nature | Sampling class and cadence | Units | Bounds and unavailable cases |
|---|---|---|---|---|---|---|
| Page size and clock rate | Host execution units | `system.execution_units` | Static for one live session | Session setup, once | Bytes per page; ticks per second | Both values must be positive before process RSS or CPU rates are derived |
| Aggregate and per-core CPU usage | `/proc/stat` `cpu` and `cpuN` rows | `sys_cpu.parse_stat`, `btop_rates.cpu_percent` | Dynamic counters | Counters, 1 s | CPU ticks and percent over the observed interval | Signed 64-bit nonnegative counters; sparse CPU IDs are retained; incomplete input withholds the rate |
| Uptime and load | `/proc/uptime`, `/proc/loadavg` | `sys_cpu.parse_uptime`, `sys_cpu.parse_loadavg` | Dynamic gauges | Counters, 1 s | Seconds and load averages | Bounded reads; malformed values are withheld |
| Total, available, used memory, and swap | `/proc/meminfo` | `sys_mem.parse`, `btop_rates.used_bytes` | Dynamic gauges | Counters, 1 s | Bytes | kB-to-byte conversion is checked against the exact JSON integer bound; missing inputs do not become zero |
| Raw memory categories (free, buffers, cache, active/inactive, dirty/writeback) | `/proc/meminfo` | `sys_mem.parse` | Dynamic gauges | Counters, 1 s | Bytes | Categories remain independent because their accounting overlaps |
| Interface name, RX/TX totals, and rates | `/proc/net/dev`, `/sys/class/net/[name]/ifindex` | `sys_net.collect`, `sys_net.selected_interface`, `btop_rates.counter_rate` | Dynamic counters and interface identity | Counters, 1 s | Byte counters and bytes per second | Bounded reads; rates require the same observed interface index across samples |
| PID, parent PID, thread count, UID, command, CPU, and RSS | `/proc/[pid]/stat`, `status`, `statm` | `sys_proc.collect`, `system_report_live.parse_proc_stat_for_monitor`, `btop_rates.process_percent` | Dynamic process inventory and counters | Counters, 1 s | Identities, counts, ticks, bytes, and percent | PID/start-time races are withheld; RSS page conversion is checked; cached UID expires after 5 s |
| Local process user label and command line | `/proc/[pid]/cmdline`; host account database | `sys_proc.collect_local_display` | Dynamic, display-only text | Counters, 1 s | User name and argument text | Command line is bounded to 4096 bytes; process identity is checked before and after; no environment is read or text captured for replay |
| Package and per-core temperatures | `/sys/class/hwmon`, `/sys/class/thermal` | `system_report_live.collect_monitor_sources`, `system_report_live.collect_sensors`, `btop_sensors.cpu_temperature`, `btop_sensors.core_mapping` | Dynamic gauges | Gauges, 5 s | Signed millidegrees Celsius | Bounded reads; malformed or missing values are withheld; duplicate core IDs are mapped deterministically |
| CPU frequency | `/sys/devices/system/cpu/cpufreq/policy*/scaling_cur_freq`, `/proc/cpuinfo` | `system_report_live.collect_monitor_sources`, `system_report_live.collect_frequency_policies`, `system_report_live.read_cpu_info`, `btop_sensors.frequency` | Dynamic gauges | Gauges, 5 s | Source kHz reduced to MHz | Positive readings only; supported CPUFreq modes are pinned and reads are bounded |
| Battery level, state, power, and time | `/sys/class/power_supply/*` type, presence, status, capacity, energy/charge, voltage/current, and power | `system_report_live.collect_monitor_sources`, `system_report_live.collect_power`, `btop_sensors.battery` | Dynamic gauges | Gauges, 5 s | Percent, microwatts, microamp-hours, and microvolts | Only present Battery/UPS devices qualify; unsupported or malformed values remain unavailable |
| Disk reads/writes and in-flight requests | `/sys/class/block/*/stat`, `diskseq` | `sys_block.collect`, `btop_disks.step` | Dynamic counters | Counters, 1 s | 512-byte accounting sectors, bytes per second, request count | Linux 11/15/17-field layouts; history requires disk sequence and device number; partitions inherit their parent disk sequence |
| Mounted filesystem capacity | `/proc/self/mountinfo` and rooted filesystem statistics | `sys_mount.collect`, `sys_mount.apply_captured_usage`, `btop_disks.step` | Dynamic inventory and blocking measurement | Inventory and Blocking, 30 s | Total, used, and user-available bytes | Mount escapes are decoded; shadowed, duplicate-target, and remote filesystems are skipped; replay uses captured capacity |

The scheduler reads CPU, memory, network, process, and block counters every
second. Sensor, frequency, power, and other telemetry gauges refresh every five
seconds. Mount inventory and capacity work share the thirty-second inventory
and blocking deadline. Replay uses recorded observations instead of live
system sources.

The system-report and btop paths share the meminfo, sensor, frequency,
power-supply, block, mount, and process-stat readers. Block counters retain
signed 64-bit values for monitor rate math; system-report JSON v1 withholds
counters above its exact-number bound. The monitor uses the shared process
parser's signed 64-bit start-time policy while preserving the system-report
JSON bound. Monitor battery fields remain private to the live collector and do
not widen system-report JSON.

## Host release evidence

The archived host release gates completed through G4a. The baseline passed
404/404 host tests. The final release build passed; completion checks passed
for btop (121/121), input (7/7), UI (9/9), renderer (8/8), graphs (5/5), PTY
(11/11), and repository contracts (5/5). Native-origin checks passed for
BusyBox (638/638), GNU (514/514), and uutils (5653/5653); the documentation
check passed 3 checks and the campaign-ledger test passed 1. Coverage
assertions recorded 4 G0 passes plus one externally blocked lane, then 29 G1,
25 G2, 54 G3, and 24 G4a passes, with no failed assertions. The optional
reference build was not attempted.

The pinned Linux ARM64 baseline and final lanes did not reach tests: Docker
could not execute the ARM64 `cargo` binary on the x86_64 runner without ARM64
binfmt support. No suite failure set or time-to-suite-start was measured.
Linux ARM64 is outside the supported product scope; ARM64 macOS remains in
scope. All Linux evidence is therefore host-only. No live battery hardware was
available, so battery and sensor behavior was fixture-tested only.

Archived performance observations are gate evidence, not speedup claims:

- The process-join median was 8.163 ms at 1,000 processes and 16.623 ms at
  2,000 (seed 424242, five interleaved repeats), a 2.04x ratio within the 3.0x
  maximum gate. The record does not include a spread or host-load guard.
- The 1,000-process model/render/diff profile at 120 by 40, with 20 samples,
  measured p95 2.631 s and p99 2.692 s against a 50 ms p95 target. This target
  was missed; the record has no range or host-load guard.
- A PTY probe with 100 key events at 80 by 24 and a 60 s replay interval
  measured input-to-present p95 2.955 ms and p99 3.014 ms. It has no baseline
  or range, so it does not establish a change in latency.

## Run the preloaded profile

The native test suite keeps behavior and scaling assertions. The 20-sample
preloaded profile runs separately as `dev/perf/btop-profile.xsh` because it
prints performance evidence without asserting behavior or latency. Run it from
the repository root with the release `xsh` binary and the Linux test image
available. The command checks host readiness before the profile and checks
again afterward, even if the profile times out or fails:

```sh
set -e
repo=$(pwd -P)
profile_status=0
post_status=0
perf-gate-ready
timeout 120s docker run \
  --rm --init --platform linux/amd64 --network none \
  --cpus=1 --memory=4g --memory-swap=4g \
  --tmpfs /tmp:rw,nodev,exec,mode=1777 \
  -v "$repo:/work" \
  -v "$repo/target/x86_64-unknown-linux-musl/release/xsh:/main-xsh:ro" \
  -w /work \
  -e TMPDIR=/work/.work/tmp \
  -e TARGET=x86_64-unknown-linux-musl \
  xsh-test@sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c \
  /main-xsh dev/perf/btop-profile.xsh || profile_status=$?
perf-gate-ready -1 || post_status=$?
test "$profile_status" -eq 0
test "$post_status" -eq 0
```

`perf-gate-ready` must exit successfully before you run the profile. The
release `xsh` binary must target `x86_64-unknown-linux-musl`. The image digest
pins the `Dockerfile.test` environment for `linux/amd64`.

## Campaign completion summary

The fixed, read-only Linux monitor now has the pinned default panels, graphs,
palette, process tree, sorting and view controls, mounted-disk capacity and
I/O, and schema-3 replay. It has no settings file or configuration interface.
The archived G0–G4a host gates passed their behavioral assertions; the Linux
ARM64 blocker and performance-target miss remain as described above.

The historical follow-on plan deferred process actions and detail views until
the G4a gate passed; multi-battery and power edges, ZFS, and GPU collection
until G4b plus eligible hardware or an explicit decoder-only decision; and
endurance, fault injection, and bounded fuzzing until after G4a. It also
required Linux product qualification before live macOS collection, since
replay alone does not qualify a live collector. These were recorded follow-on
decisions, not a current commitment: the completion ledger says no further
in-scope product work remains.

## Current limits

Linux ARM64 has no qualification from this run, live battery behavior has not
been verified, and the 120-by-40 model/render/diff p95 target remains unmet.
GPU collection, process actions and detail views, and non-Linux live btop
collectors are outside this monitor surface.
