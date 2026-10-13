##! Holds the fixed values extracted for every in-scope upstream option.
##!
##! The monitor exposes no configuration file or tuning interface.

## Carries one pinned setting name and its typed value.
export type Default = {key: Str, value: Any}

const ROUNDED_CORNERS = true
const TERMINAL_SYNC = true
const GRAPH_SYMBOL = "braille"
const GRAPH_SYMBOL_CPU = "default"
const GRAPH_SYMBOL_MEM = "default"
const GRAPH_SYMBOL_NET = "default"
const GRAPH_SYMBOL_PROC = "default"
const SHOWN_BOXES = "cpu mem net proc"
const UPDATE_MS = 2000
const PROC_SORTING = "cpu lazy"
const PROC_REVERSED = false
const PROC_TREE = false
const PROC_COLORS = true
const PROC_GRADIENT = true
const PROC_PER_CORE = false
const PROC_MEM_BYTES = true
const PROC_CPU_GRAPHS = true
const PROC_LEFT = false
const PROC_AGGREGATE = false
const CPU_GRAPH_UPPER = "Auto"
const CPU_GRAPH_LOWER = "Auto"
const CPU_INVERT_LOWER = true
const CPU_SINGLE_GRAPH = false
const CPU_BOTTOM = false
const SHOW_UPTIME = true
const CHECK_TEMP = true
const CPU_SENSOR = "Auto"
const SHOW_CORETEMP = true
const CPU_CORE_MAP = ""
const TEMP_SCALE = "celsius"
const BASE_10_SIZES = false
const SHOW_CPU_FREQ = true
const FREQ_MODE = "first"
const CLOCK_FORMAT = "%X"
const DISKS_FILTER = ""
const MEM_GRAPHS = true
const MEM_BELOW_NET = false
const SHOW_SWAP = true
const SWAP_DISK = true
const SHOW_DISKS = true
const ONLY_PHYSICAL = true
const USE_FSTAB = true
const SHOW_IO_STAT = true
const IO_MODE = false
const IO_GRAPH_COMBINED = false
const IO_GRAPH_SPEEDS = ""
const SWAP_UPLOAD_DOWNLOAD = false
const NET_DOWNLOAD = 100
const NET_UPLOAD = 100
const NET_AUTO = true
const NET_SYNC = true
const NET_IFACE = ""
const BASE_10_BITRATE = "Auto"
const SHOW_BATTERY = true
const SELECTED_BATTERY = "Auto"
const SHOW_BATTERY_WATTS = true

## Returns the pinned process sort used by the initial view.
export pure process_sorting() -> Str { PROC_SORTING }

## Returns whether process sorting starts reversed.
export pure process_reversed() -> Bool { PROC_REVERSED }

## Returns the pinned tree-view default.
export pure process_tree_enabled() -> Bool { PROC_TREE }

## Returns the pinned child-resource aggregation default.
export pure aggregate_process_children() -> Bool { PROC_AGGREGATE }

## Returns whether process memory starts in byte units.
export pure process_memory_in_bytes() -> Bool { PROC_MEM_BYTES }

## Returns whether process CPU starts normalized per logical core.
export pure process_per_core_enabled() -> Bool { PROC_PER_CORE }

## Returns whether mounted disk details are visible in the default memory box.
export pure disks_enabled() -> Bool { SHOW_DISKS }

## Returns whether disk I/O is shown as separate read and write graphs.
export pure disk_io_mode_enabled() -> Bool { IO_MODE }

## Returns the pinned automatic network-scaling default.
export pure network_auto_enabled() -> Bool { NET_AUTO }

## Returns the pinned synchronized network-scale default.
export pure network_scales_synchronized() -> Bool { NET_SYNC }

## Returns the fixed download graph limit in mebibits per second.
export pure network_download_mibits() -> Int { NET_DOWNLOAD }

## Returns the fixed upload graph limit in mebibits per second.
export pure network_upload_mibits() -> Int { NET_UPLOAD }

## Returns every fixed setting exactly once for ledger cross-checking.
export pure records() -> List[Default] {
  [
    {key: "rounded_corners", value: ROUNDED_CORNERS},
    {key: "terminal_sync", value: TERMINAL_SYNC},
    {key: "graph_symbol", value: GRAPH_SYMBOL},
    {key: "graph_symbol_cpu", value: GRAPH_SYMBOL_CPU},
    {key: "graph_symbol_mem", value: GRAPH_SYMBOL_MEM},
    {key: "graph_symbol_net", value: GRAPH_SYMBOL_NET},
    {key: "graph_symbol_proc", value: GRAPH_SYMBOL_PROC},
    {key: "shown_boxes", value: SHOWN_BOXES},
    {key: "update_ms", value: UPDATE_MS},
    {key: "proc_sorting", value: PROC_SORTING},
    {key: "proc_reversed", value: PROC_REVERSED},
    {key: "proc_tree", value: PROC_TREE},
    {key: "proc_colors", value: PROC_COLORS},
    {key: "proc_gradient", value: PROC_GRADIENT},
    {key: "proc_per_core", value: PROC_PER_CORE},
    {key: "proc_mem_bytes", value: PROC_MEM_BYTES},
    {key: "proc_cpu_graphs", value: PROC_CPU_GRAPHS},
    {key: "proc_left", value: PROC_LEFT},
    {key: "proc_aggregate", value: PROC_AGGREGATE},
    {key: "cpu_graph_upper", value: CPU_GRAPH_UPPER},
    {key: "cpu_graph_lower", value: CPU_GRAPH_LOWER},
    {key: "cpu_invert_lower", value: CPU_INVERT_LOWER},
    {key: "cpu_single_graph", value: CPU_SINGLE_GRAPH},
    {key: "cpu_bottom", value: CPU_BOTTOM},
    {key: "show_uptime", value: SHOW_UPTIME},
    {key: "check_temp", value: CHECK_TEMP},
    {key: "cpu_sensor", value: CPU_SENSOR},
    {key: "show_coretemp", value: SHOW_CORETEMP},
    {key: "cpu_core_map", value: CPU_CORE_MAP},
    {key: "temp_scale", value: TEMP_SCALE},
    {key: "base_10_sizes", value: BASE_10_SIZES},
    {key: "show_cpu_freq", value: SHOW_CPU_FREQ},
    {key: "freq_mode", value: FREQ_MODE},
    {key: "clock_format", value: CLOCK_FORMAT},
    {key: "disks_filter", value: DISKS_FILTER},
    {key: "mem_graphs", value: MEM_GRAPHS},
    {key: "mem_below_net", value: MEM_BELOW_NET},
    {key: "show_swap", value: SHOW_SWAP},
    {key: "swap_disk", value: SWAP_DISK},
    {key: "show_disks", value: SHOW_DISKS},
    {key: "only_physical", value: ONLY_PHYSICAL},
    {key: "use_fstab", value: USE_FSTAB},
    {key: "show_io_stat", value: SHOW_IO_STAT},
    {key: "io_mode", value: IO_MODE},
    {key: "io_graph_combined", value: IO_GRAPH_COMBINED},
    {key: "io_graph_speeds", value: IO_GRAPH_SPEEDS},
    {key: "swap_upload_download", value: SWAP_UPLOAD_DOWNLOAD},
    {key: "net_download", value: NET_DOWNLOAD},
    {key: "net_upload", value: NET_UPLOAD},
    {key: "net_auto", value: NET_AUTO},
    {key: "net_sync", value: NET_SYNC},
    {key: "net_iface", value: NET_IFACE},
    {key: "base_10_bitrate", value: BASE_10_BITRATE},
    {key: "show_battery", value: SHOW_BATTERY},
    {key: "selected_battery", value: SELECTED_BATTERY},
    {key: "show_battery_watts", value: SHOW_BATTERY_WATTS},
  ]
}
