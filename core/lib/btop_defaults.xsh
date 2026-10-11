##! Holds the fixed values extracted for every in-scope upstream option.
##!
##! The monitor exposes no configuration file or tuning interface.

## Carries one pinned setting name and its typed value.
export type Default = {key: Str, value: Any}

const ROUNDED_CORNERS: Bool = true
const TERMINAL_SYNC: Bool = true
const GRAPH_SYMBOL: Str = "braille"
const GRAPH_SYMBOL_CPU: Str = "default"
const GRAPH_SYMBOL_MEM: Str = "default"
const GRAPH_SYMBOL_NET: Str = "default"
const GRAPH_SYMBOL_PROC: Str = "default"
const SHOWN_BOXES: Str = "cpu mem net proc"
const UPDATE_MS: Int = 2000
const PROC_SORTING: Str = "cpu lazy"
const PROC_REVERSED: Bool = false
const PROC_TREE: Bool = false
const PROC_COLORS: Bool = true
const PROC_GRADIENT: Bool = true
const PROC_PER_CORE: Bool = false
const PROC_MEM_BYTES: Bool = true
const PROC_CPU_GRAPHS: Bool = true
const PROC_LEFT: Bool = false
const PROC_AGGREGATE: Bool = false
const CPU_GRAPH_UPPER: Str = "Auto"
const CPU_GRAPH_LOWER: Str = "Auto"
const CPU_INVERT_LOWER: Bool = true
const CPU_SINGLE_GRAPH: Bool = false
const CPU_BOTTOM: Bool = false
const SHOW_UPTIME: Bool = true
const CHECK_TEMP: Bool = true
const CPU_SENSOR: Str = "Auto"
const SHOW_CORETEMP: Bool = true
const CPU_CORE_MAP: Str = ""
const TEMP_SCALE: Str = "celsius"
const BASE_10_SIZES: Bool = false
const SHOW_CPU_FREQ: Bool = true
const FREQ_MODE: Str = "first"
const CLOCK_FORMAT: Str = "%X"
const DISKS_FILTER: Str = ""
const MEM_GRAPHS: Bool = true
const MEM_BELOW_NET: Bool = false
const SHOW_SWAP: Bool = true
const SWAP_DISK: Bool = true
const SHOW_DISKS: Bool = true
const ONLY_PHYSICAL: Bool = true
const USE_FSTAB: Bool = true
const SHOW_IO_STAT: Bool = true
const IO_MODE: Bool = false
const IO_GRAPH_COMBINED: Bool = false
const IO_GRAPH_SPEEDS: Str = ""
const SWAP_UPLOAD_DOWNLOAD: Bool = false
const NET_DOWNLOAD: Int = 100
const NET_UPLOAD: Int = 100
const NET_AUTO: Bool = true
const NET_SYNC: Bool = true
const NET_IFACE: Str = ""
const BASE_10_BITRATE: Str = "Auto"
const SHOW_BATTERY: Bool = true
const SELECTED_BATTERY: Str = "Auto"
const SHOW_BATTERY_WATTS: Bool = true

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
