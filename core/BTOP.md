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

| Values | Sources |
|---|---|
| CPU, uptime, load | `/proc/stat`, `/proc/uptime`, `/proc/loadavg` |
| Memory and swap | `/proc/meminfo` |
| Network | `/proc/net/dev` and interface data under `/sys/class/net` |
| Processes | `/proc/[pid]/stat`, `status`, `statm`, and `cmdline` |
| Sensors, frequency, and power | `/sys/class/hwmon`, `/sys/class/thermal`, `/sys/devices/system/cpu`, `/proc/cpuinfo`, `/sys/class/power_supply` |
| Block I/O and mounted capacity | `/sys/class/block`, `/proc/self/mountinfo`, and guarded filesystem-stat observations |

Replay reads these recorded observations rather than live system sources.

## Campaign completion summary

The monitor is a fixed, read-only Linux implementation with schema-3 replay.
Host release gates are complete. The pinned Linux ARM64 lanes did not reach
tests because Docker could not execute the ARM64 container on the x86_64 runner;
Linux ARM64 is outside the supported product scope. Battery and sensor behavior
is fixture-tested because no live battery hardware was available.
