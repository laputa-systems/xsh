##! The nl80211 wireless configuration protocol over generic netlink: request
##! building, attribute parsing, and the text `iw` prints for interfaces,
##! wireless devices, scans, stations, regulatory domains, and events.
##!
##! Requests and replies are bytes moved by the `linux` netlink primitives;
##! every attribute layout, command number, and report format lives here, so a
##! test can drive the decoders with recorded messages and no wireless
##! hardware. The decoders never reach the kernel: formatting functions take
##! messages and return lines.
use gnu

const NETLINK_GENERIC = 16
const GENL_ID_CTRL = 16
const CTRL_CMD_GETFAMILY = 3
const CTRL_ATTR_FAMILY_NAME = 2
const CTRL_ATTR_MCAST_GROUPS = 7
const CTRL_ATTR_MCAST_GRP_NAME = 1
const CTRL_ATTR_MCAST_GRP_ID = 2
const SOL_NETLINK = 270
const NETLINK_ADD_MEMBERSHIP = 1
const NLM_F_REQUEST = 1
const NLM_F_ACK = 4
const NLM_F_DUMP = 768
const NLA_TYPE_MASK = 16383

const CMD_GET_WIPHY = 1
const CMD_SET_WIPHY = 2
const CMD_GET_INTERFACE = 5
const CMD_SET_INTERFACE = 6
const CMD_GET_STATION = 17
const CMD_REQ_SET_REG = 27
const CMD_GET_REG = 31
const CMD_GET_SCAN = 32
const CMD_TRIGGER_SCAN = 33
const CMD_NEW_SCAN_RESULTS = 34
const CMD_SCAN_ABORTED = 35
const CMD_ABORT_SCAN = 114
const CMD_RELOAD_REGDB = 126

const ATTR_WIPHY = 1
const ATTR_WIPHY_NAME = 2
const ATTR_IFINDEX = 3
const ATTR_IFNAME = 4
const ATTR_IFTYPE = 5
const ATTR_MAC = 6
const ATTR_STA_INFO = 21
const ATTR_WIPHY_BANDS = 22
const ATTR_SUPPORTED_IFTYPES = 32
const ATTR_REG_ALPHA2 = 33
const ATTR_REG_RULES = 34
const ATTR_WIPHY_FREQ = 38
const ATTR_WIPHY_CHANNEL_TYPE = 39
const ATTR_IE = 42
const ATTR_MAX_NUM_SCAN_SSIDS = 43
const ATTR_SCAN_FREQUENCIES = 44
const ATTR_SCAN_SSIDS = 45
const ATTR_BSS = 47
const ATTR_REG_INITIATOR = 48
const ATTR_SUPPORTED_COMMANDS = 50
const ATTR_SSID = 52
const ATTR_MESH_ID = 24
const ATTR_REG_TYPE = 49
const ATTR_4ADDR = 83
const ATTR_REASON_CODE = 54
const ATTR_MAX_SCAN_IE_LEN = 56
const ATTR_CIPHER_SUITES = 57
const ATTR_WIPHY_RETRY_SHORT = 61
const ATTR_WIPHY_RETRY_LONG = 62
const ATTR_WIPHY_FRAG_THRESHOLD = 63
const ATTR_WIPHY_RTS_THRESHOLD = 64
const ATTR_STATUS_CODE = 72
const ATTR_DISCONNECTED_BY_AP = 71
const ATTR_WIPHY_COVERAGE_CLASS = 89
const ATTR_WIPHY_TX_POWER_SETTING = 97
const ATTR_WIPHY_TX_POWER_LEVEL = 98
const ATTR_SUPPORT_IBSS_RSN = 104
const ATTR_WIPHY_ANTENNA_AVAIL_TX = 113
const ATTR_WIPHY_ANTENNA_AVAIL_RX = 114
const ATTR_WIPHY_ANTENNA_TX = 105
const ATTR_WIPHY_ANTENNA_RX = 106
const ATTR_MAX_NUM_SCHED_SCAN_SSIDS = 123
const ATTR_SUPPORT_AP_UAPSD = 130
const ATTR_MAX_MATCH_SETS = 133
const ATTR_TDLS_SUPPORT = 139
const ATTR_DFS_REGION = 146
const ATTR_WDEV = 153
const ATTR_SCAN_FLAGS = 158
const ATTR_CHANNEL_WIDTH = 159
const ATTR_CENTER_FREQ1 = 160
const ATTR_CENTER_FREQ2 = 161
const ATTR_SPLIT_WIPHY_DUMP = 174
const ATTR_WIPHY_SELF_MANAGED_REG = 216
const ATTR_MAX_NUM_SCHED_SCAN_PLANS = 222
const ATTR_MAX_SCAN_PLAN_INTERVAL = 223
const ATTR_MAX_SCAN_PLAN_ITERATIONS = 224
const ATTR_MEASUREMENT_DURATION = 235
const ATTR_MEASUREMENT_DURATION_MANDATORY = 236

const BSS_BSSID = 1
const BSS_FREQUENCY = 2
const BSS_TSF = 3
const BSS_BEACON_INTERVAL = 4
const BSS_CAPABILITY = 5
const BSS_INFORMATION_ELEMENTS = 6
const BSS_SIGNAL_MBM = 7
const BSS_SIGNAL_UNSPEC = 8
const BSS_STATUS = 9
const BSS_SEEN_MS_AGO = 10
const BSS_BEACON_IES = 11
const BSS_LAST_SEEN_BOOTTIME = 15
const BSS_FREQUENCY_OFFSET = 20

const STA_INACTIVE_TIME = 1
const STA_RX_BYTES = 2
const STA_TX_BYTES = 3
const STA_SIGNAL = 7
const STA_TX_BITRATE = 8
const STA_RX_PACKETS = 9
const STA_TX_PACKETS = 10
const STA_TX_RETRIES = 11
const STA_TX_FAILED = 12
const STA_SIGNAL_AVG = 13
const STA_RX_BITRATE = 14
const STA_BSS_PARAM = 15
const STA_CONNECTED_TIME = 16
const STA_FLAGS = 17
const STA_BEACON_LOSS = 18
const STA_CHAIN_SIGNAL = 25
const STA_CHAIN_SIGNAL_AVG = 26
const STA_EXPECTED_THROUGHPUT = 27
const STA_RX_DROP_MISC = 28
const STA_RX_BYTES64 = 23
const STA_TX_BYTES64 = 24
const STA_T_OFFSET = 19
const STA_BEACON_RX = 29
const STA_BEACON_SIGNAL_AVG = 30
const STA_RX_DURATION = 32
const STA_ACK_SIGNAL = 34
const STA_ACK_SIGNAL_AVG = 35
const STA_TX_DURATION = 39
const STA_AIRTIME_WEIGHT = 40
const STA_ASSOC_AT_BOOTTIME = 42

const RATE_BITRATE = 1
const RATE_MCS = 2
const RATE_40_MHZ = 3
const RATE_SHORT_GI = 4
const RATE_BITRATE32 = 5
const RATE_VHT_MCS = 6
const RATE_VHT_NSS = 7
const RATE_80_MHZ = 8
const RATE_80P80_MHZ = 9
const RATE_160_MHZ = 10
const RATE_HE_MCS = 13
const RATE_HE_NSS = 14
const RATE_HE_GI = 15
const RATE_HE_DCM = 16
const RATE_HE_RU_ALLOC = 17
const RATE_320_MHZ = 18
const RATE_EHT_MCS = 19
const RATE_EHT_NSS = 20
const RATE_EHT_GI = 21
const RATE_EHT_RU_ALLOC = 22
const RATE_1_MHZ = 25
const RATE_2_MHZ = 26
const RATE_4_MHZ = 27
const RATE_8_MHZ = 28
const RATE_16_MHZ = 29

const BSS_PARAM_CTS_PROT = 1
const BSS_PARAM_SHORT_PREAMBLE = 2
const BSS_PARAM_SHORT_SLOT_TIME = 3
const BSS_PARAM_DTIM_PERIOD = 4
const BSS_PARAM_BEACON_INTERVAL = 5

const BAND_FREQS = 1
const BAND_RATES = 2
const BAND_HT_MCS_SET = 3
const BAND_HT_CAPA = 4
const BAND_HT_AMPDU_FACTOR = 5
const BAND_HT_AMPDU_DENSITY = 6

const FREQ_FREQ = 1
const FREQ_DISABLED = 2
const FREQ_NO_IR = 3
const FREQ_RADAR = 5
const FREQ_OFFSET = 20
const FREQ_MAX_TX_POWER = 6

const RULE_FLAGS = 1
const RULE_FREQ_START = 2
const RULE_FREQ_END = 3
const RULE_MAX_BW = 4
const RULE_MAX_ANT_GAIN = 5
const RULE_MAX_EIRP = 6
const RULE_DFS_CAC_TIME = 7

const IFTYPE_ADHOC = 1
const IFTYPE_STATION = 2
const IFTYPE_AP = 3
const IFTYPE_AP_VLAN = 4
const IFTYPE_WDS = 5
const IFTYPE_MONITOR = 6
const IFTYPE_MESH_POINT = 7
const IFTYPE_P2P_CLIENT = 8
const IFTYPE_P2P_GO = 9
const IFTYPE_OCB = 11

const TX_POWER_AUTOMATIC = 0
const TX_POWER_LIMITED = 1
const TX_POWER_FIXED = 2


const WIDTH_20_NOHT = 0
const WIDTH_20 = 1
const WIDTH_40 = 2

const BSS_STATUS_AUTHENTICATED = 0
const BSS_STATUS_ASSOCIATED = 1
const BSS_STATUS_IBSS_JOINED = 2

const SCAN_FLAG_LOW_PRIORITY = 1
const SCAN_FLAG_FLUSH = 2
const SCAN_FLAG_AP = 4
const SCAN_FLAG_COLOCATED_6GHZ = 64

const REG_RULE_FLAGS = [
  {bit: 1, name: "NO-OFDM"},
  {bit: 2, name: "NO-CCK"},
  {bit: 4, name: "NO-INDOOR"},
  {bit: 8, name: "NO-OUTDOOR"},
  {bit: 16, name: "DFS"},
  {bit: 32, name: "PTP-ONLY"},
  {bit: 64, name: "PTMP-ONLY"},
  {bit: 2048, name: "AUTO-BW"},
  {bit: 128, name: "PASSIVE-SCAN"},
  {bit: 256, name: "NO-IBSS"},
]

const IFTYPE_NAMES = [
  "unspecified", "IBSS", "managed", "AP", "AP/VLAN", "WDS", "monitor", "mesh point", "P2P-client", "P2P-GO",
  "P2P-device", "outside context of a BSS", "NAN",
]

const WIDTH_NAMES = [
  "20 MHz (no HT)", "20 MHz", "40 MHz", "80 MHz", "80+80 MHz", "160 MHz", "5 MHz", "10 MHz", "1 MHz", "2 MHz",
  "4 MHz", "8 MHz", "16 MHz", "320 MHz",
]

const CHANNEL_TYPE_NAMES = ["NO HT", "HT20", "HT40-", "HT40+"]

# Command names in numeric order, as `iw` names them. An empty entry is a
# command `iw` has no name for and prints as `Unknown command (N)`.
const COMMAND_NAMES = [
  "unspec", "get_wiphy", "set_wiphy", "new_wiphy", "del_wiphy", "get_interface", "set_interface", "new_interface",
  "del_interface", "get_key", "set_key", "new_key", "del_key", "get_beacon", "set_beacon", "start_ap", "stop_ap",
  "get_station", "set_station", "new_station", "del_station", "get_mpath", "set_mpath", "new_mpath", "del_mpath",
  "set_bss", "set_reg", "req_set_reg", "get_mesh_config", "set_mesh_config", "", "get_reg", "get_scan",
  "trigger_scan", "new_scan_results", "scan_aborted", "reg_change", "authenticate", "associate", "deauthenticate",
  "disassociate", "michael_mic_failure", "reg_beacon_hint", "join_ibss", "leave_ibss", "testmode", "connect", "roam",
  "disconnect", "set_wiphy_netns", "get_survey", "new_survey_results", "set_pmksa", "del_pmksa", "flush_pmksa",
  "remain_on_channel", "cancel_remain_on_channel", "set_tx_bitrate_mask", "register_frame", "frame",
  "frame_tx_status", "set_power_save", "get_power_save", "set_cqm", "notify_cqm", "set_channel", "set_wds_peer",
  "frame_wait_cancel", "join_mesh", "leave_mesh", "unprot_deauthenticate", "unprot_disassociate",
  "new_peer_candidate", "get_wowlan", "set_wowlan", "start_sched_scan", "stop_sched_scan", "sched_scan_results",
  "sched_scan_stopped", "set_rekey_offload", "pmksa_candidate", "tdls_oper", "tdls_mgmt", "unexpected_frame",
  "probe_client", "register_beacons", "unexpected_4addr_frame", "set_noack_map", "ch_switch_notify",
  "start_p2p_device", "stop_p2p_device", "conn_failed", "set_mcast_rate", "set_mac_acl", "radar_detect",
  "get_protocol_features", "update_ft_ies", "ft_event", "crit_protocol_start", "crit_protocol_stop", "get_coalesce",
  "set_coalesce", "channel_switch", "vendor", "set_qos_map", "add_tx_ts", "del_tx_ts", "get_mpp", "join_ocb",
  "leave_ocb", "ch_switch_started_notify", "tdls_channel_switch", "tdls_cancel_channel_switch", "wiphy_reg_change",
  "abort_scan", "start_nan", "stop_nan", "add_nan_function", "del_nan_function", "change_nan_config", "nan_match",
  "set_multicast_to_unicast", "update_connect_params", "set_pmk", "del_pmk", "port_authorized", "reload_regdb",
  "external_auth", "sta_opmode_changed", "control_port_frame", "get_ftm_responder_stats", "peer_measurement_start",
  "peer_measurement_result", "peer_measurement_complete", "notify_radar", "update_owe_info", "probe_mesh_link",
  "set_tid_config", "unprot_beacon", "control_port_frame_tx_status", "set_sar_specs", "obss_color_collision",
  "color_change_request", "color_change_started", "color_change_aborted", "color_change_completed", "set_fils_aad",
  "assoc_comeback", "add_link", "remove_link", "add_link_sta", "modify_link_sta", "remove_link_sta",
  "set_hw_timestamp", "links_removed", "set_tid_to_link_mapping", "assoc_mlo_reconf", "epcs_cfg",
]

## One netlink attribute with its nesting and byte-order flags removed.
export type Attr = {kind: Int, data: Bytes}

## One generic-netlink message: the command and its attributes.
export type Message = {cmd: Int, attrs: List[Attr]}

## An open generic-netlink socket and the nl80211 family id it addresses.
export type Client = {fd: Int, family: Int}

## A failure that carries a kernel errno and the `strerror` text for it, so
## the applet can report `command failed: TEXT (-ERRNO)` and exit with it.
export error IwError = Errno(code: Int, text: Str) | Missing | NoSocket

# --- attribute codec ---------------------------------------------------

## One attribute: header, payload, and padding to a four-byte boundary.
export proc attr(kind: Int, data: Bytes) [error] -> Result[Bytes, Error] {
  let length = 4 + data.len()
  let padding = (4 - length % 4) % 4
  bytes.concat([bytes.pack_le(length, 2)?, bytes.pack_le(kind, 2)?, data, bytes.zero(padding)?])
}

## A 32-bit little-endian attribute.
export proc attr_u32(kind: Int, value: Int) [error] -> Result[Bytes, Error] {
  attr(kind, bytes.pack_le(value, 4)?)
}

## A 16-bit little-endian attribute.
export proc attr_u16(kind: Int, value: Int) [error] -> Result[Bytes, Error] {
  attr(kind, bytes.pack_le(value, 2)?)
}

## An 8-bit attribute.
export proc attr_u8(kind: Int, value: Int) [error] -> Result[Bytes, Error] {
  attr(kind, bytes.from_ints([value])?)
}

## A 64-bit little-endian attribute.
export proc attr_u64(kind: Int, value: Int) [error] -> Result[Bytes, Error] {
  attr(kind, bytes.pack_le(value, 8)?)
}

## A NUL-terminated string attribute.
export proc attr_str(kind: Int, text: Str) [error] -> Result[Bytes, Error] {
  attr(kind, bytes.concat([bytes.from_text(text), bytes.zero(1)?]))
}

## A flag attribute: present, with no payload.
export proc attr_flag(kind: Int) [error] -> Result[Bytes, Error] {
  attr(kind, b"")
}

## A nested attribute holding already-encoded children.
export proc attr_nested(kind: Int, children: List[Bytes]) [error] -> Result[Bytes, Error] {
  attr(kind, bytes.concat(children))
}

## Splits the attributes that follow `start` fixed bytes of `payload`. The
## nesting and byte-order flag bits are dropped from each kind, and a
## truncated trailing attribute ends the list.
export pure parse_attrs(payload: Bytes, start: Int) -> List[Attr] {
  var found: List[Attr] = []
  var offset = start
  while offset + 4 <= payload.len() {
    let length = uint(payload.slice(offset, 2))
    let kind = uint(payload.slice(offset + 2, 2)).bit_and(NLA_TYPE_MASK)
    if length < 4 or offset + length > payload.len() {
      break
    }
    found += [{kind: kind, data: payload.slice(offset + 4, length - 4)}]
    offset += (length + 3) / 4 * 4
  }
  found
}

## The little-endian unsigned integer held in `data` (at most eight bytes).
export pure uint(data: Bytes) -> Int {
  var value = 0
  var index = data.len()
  while index > 0 {
    index -= 1
    value = value * 256 + (data.byte_at(index) ?? 0)
  }
  value
}

## The first attribute of `kind`, or null.
export pure pick(attrs: List[Attr], kind: Int) -> Bytes? {
  for item in attrs {
    if item.kind == kind {
      return item.data
    }
  }
  null
}

## The unsigned integer attribute `kind`, or null.
export pure pick_uint(attrs: List[Attr], kind: Int) -> Int? {
  let data = pick(attrs, kind)
  if data == null {
    return null
  }
  uint(data)
}

# The first of two optional integers that is present.
pure first_of(first: Int?, second: Int?) -> Int? {
  if first != null { first } else { second }
}

# The first of two optional byte strings that is present.
pure first_bytes(first: Bytes?, second: Bytes?) -> Bytes? {
  if first != null { first } else { second }
}

## The signed byte stored in an eight-bit attribute.
export pure signed8(value: Int) -> Int {
  if value > 127 { value - 256 } else { value }
}

## The text of a NUL-terminated string attribute.
export pure cstring(data: Bytes) -> Str {
  var end = data.len()
  for index in range(data.len()) {
    if data.byte_at(index) == 0 {
      end = index
      break
    }
  }
  data.slice(0, end).utf8() ?? ""
}

# --- text helpers ------------------------------------------------------

## `value` in lowercase hexadecimal, zero-padded to `width` digits.
export pure hex(value: Int, width: Int = 1) -> Str {
  let digits = "0123456789abcdef"
  var number = value
  var text = ""
  while number > 0 {
    text = digits.byte_slice(number % 16, 1) + text
    number /= 16
  }
  while text.byte_len() < width {
    text = "0" + text
  }
  text
}

## `value` as C's `%#x` writes it: `0x` and the digits, or `0` for zero.
export pure c_hex(value: Int) -> Str {
  if value == 0 { "0" } else { "0x" + hex(value) }
}

## A MAC address as six colon-separated lowercase hex pairs.
export pure mac_text(data: Bytes) -> Str {
  var parts: List[Str] = []
  for index in range(data.len()) {
    parts += [hex(data.byte_at(index) ?? 0, 2)]
  }
  parts.join(":")
}

## Parses `aa:bb:cc:dd:ee:ff` into six bytes, or null.
export pure parse_mac(text: Str) -> Bytes? {
  let parts = text.split(":")
  if parts.len() != 6 {
    return null
  }
  var values: List[Int] = []
  for part in parts {
    if part.byte_len() != 2 or ! rx"^[0-9A-Fa-f]{2}$".matches(part) {
      return null
    }
    let value = ("0x" + part).parse_int() ?? -1
    if value < 0 {
      return null
    }
    values += [value]
  }
  if let Ok(packed) = bytes.from_ints(values) {
    return packed
  }
  null
}

## An SSID the way `iw` prints it: printable bytes as they are, a space only
## when interior, everything else as `\xNN`.
export pure ssid_text(data: Bytes) -> Str {
  var out = ""
  let last = data.len() - 1
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte == 92 {
      out += "\\x5c"
    } else if byte == 32 and index != 0 and index != last {
      out += " "
    } else if byte > 32 and byte < 127 {
      out += data.slice(index, 1).utf8() ?? ""
    } else {
      out += "\\x" + hex(byte, 2)
    }
  }
  out
}

## Space-separated lowercase hex of `data`.
export pure hex_bytes(data: Bytes) -> Str {
  var parts: List[Str] = []
  for index in range(data.len()) {
    parts += [hex(data.byte_at(index) ?? 0, 2)]
  }
  parts.join(" ")
}

## The IEEE channel number of a frequency in MHz, 0 when out of range.
export pure frequency_channel(freq: Int) -> Int {
  if freq == 2484 {
    return 14
  }
  if freq == 5935 {
    return 2
  }
  if freq < 2484 {
    return (freq - 2407) / 5
  }
  if freq >= 4910 and freq <= 4980 {
    return (freq - 4000) / 5
  }
  if freq < 5950 {
    return (freq - 5000) / 5
  }
  if freq <= 45000 {
    return (freq - 5950) / 5
  }
  if freq >= 58320 and freq <= 70200 {
    return (freq - 56160) / 2160
  }
  0
}

## The frequency in MHz of a channel on the band `iw set channel` assumes:
## channels 1 to 14 on 2.4 GHz, every other channel on 5 GHz.
export pure channel_frequency(channel: Int) -> Int? {
  if channel < 1 {
    return null
  }
  if channel == 14 {
    return 2484
  }
  if channel < 14 {
    return 2407 + channel * 5
  }
  if channel > 196 {
    return null
  }
  5000 + channel * 5
}

pure list_name(names: List[Str], index: Int, fallback: Str) -> Str {
  if index >= 0 and index < names.len() { names[index] } else { fallback }
}

## The interface type name `iw` prints.
export pure iftype_name(kind: Int) -> Str {
  list_name(IFTYPE_NAMES, kind, f"Unknown mode ({kind})")
}

## The `iw set type` word for an interface type, or null for an unknown word.
export pure parse_iftype(word: Str) -> Int? {
  match word {
    "ibss" | "adhoc" => IFTYPE_ADHOC
    "monitor" => IFTYPE_MONITOR
    "__ap" => IFTYPE_AP
    "__ap_vlan" => IFTYPE_AP_VLAN
    "wds" => IFTYPE_WDS
    "managed" | "mgd" | "station" => IFTYPE_STATION
    "mp" | "mesh" => IFTYPE_MESH_POINT
    "__p2pcl" => IFTYPE_P2P_CLIENT
    "__p2pgo" => IFTYPE_P2P_GO
    "ocb" => IFTYPE_OCB
    else => null
  }
}

## The glibc `strerror` wording for the errnos nl80211 clients commonly meet.
export pure errno_text(code: Int) -> Str {
  match code {
    1 => "Operation not permitted"
    2 => "No such file or directory"
    4 => "Interrupted system call"
    5 => "Input/output error"
    11 => "Resource temporarily unavailable"
    12 => "Cannot allocate memory"
    13 => "Permission denied"
    16 => "Device or resource busy"
    17 => "File exists"
    19 => "No such device"
    22 => "Invalid argument"
    34 => "Numerical result out of range"
    95 => "Operation not supported"
    100 => "Network is down"
    105 => "No buffer space available"
    110 => "Connection timed out"
    else => f"Unknown error {code}"
  }
}

## The failure for errno `code`, with the standard wording.
export pure errno_failure(code: Int) -> IwError {
  IwError.Errno(code: code, text: errno_text(code))
}

# --- transport ---------------------------------------------------------

## Resolves the nl80211 family and opens a generic-netlink socket. A missing
## family is `IwError.Missing` (the kernel has no wireless support loaded).
export proc connect() [process, error] -> Result[Client, Error] {
  guard let family = linux.genl_family_id("nl80211") else {
    return Err(IwError.Missing())
  }
  guard let fd = linux.netlink_open(NETLINK_GENERIC) else {
    return Err(IwError.NoSocket())
  }
  Ok({fd: fd, family: family})
}

## Closes the socket of `client`.
export proc disconnect(client: Client) [process] {
  unix.close_fd(client.fd)
}

# A failure with an OS error number becomes `IwError.Errno`; any other
# failure (such as a fixture lookup miss) is reported as it is.
proc failure_to_iw(failure: Error) -> Error {
  let code = gnu.errno(failure)
  if code == 0 {
    return failure
  }
  IwError.Errno(code: code, text: gnu.strerror(failure))
}

proc decode_messages(replies: List[LinuxNetlinkMessage]) -> List[Message] {
  var out: List[Message] = []
  for reply in replies {
    if reply.payload.len() >= 4 {
      out += [{cmd: reply.payload.byte_at(0) ?? 0, attrs: parse_attrs(reply.payload, 4)}]
    }
  }
  out
}

## Sends nl80211 command `cmd` with `attrs` and returns the reply messages.
## `dump` asks for every object; a kernel refusal is `IwError.Errno`.
export proc request(client: Client, cmd: Int, attrs: List[Bytes], dump: Bool = false) [process, error] -> Result[List[Message], Error] {
  let header = bytes.from_ints([cmd, 0, 0, 0])?
  let payload = bytes.concat([header] + attrs)
  let flags = if dump { NLM_F_REQUEST + NLM_F_ACK + NLM_F_DUMP } else { NLM_F_REQUEST + NLM_F_ACK }
  match linux.netlink_request(client.fd, client.family, flags, payload) {
    Ok(replies) => Ok(decode_messages(replies))
    Err(failure) => Err(failure_to_iw(failure))
  }
}

## The multicast group ids nl80211 publishes, by name (`scan`, `mlme`,
## `regulatory`, `config`, `vendor`, `nan`, `testmode`).
export proc multicast_groups(client: Client) [process, error] -> Result[Map[Int], Error] {
  let name = bytes.concat([bytes.from_text("nl80211"), bytes.zero(1)?])
  let payload = bytes.concat([
    bytes.from_ints([CTRL_CMD_GETFAMILY, 1, 0, 0])?,
    attr(CTRL_ATTR_FAMILY_NAME, name)?,
  ])
  var groups: Map[Int] = {}
  guard let replies = linux.netlink_request(client.fd, GENL_ID_CTRL, NLM_F_REQUEST, payload) else { |failure|
    return Err(failure_to_iw(failure))
  }
  for reply in replies {
    for entry in parse_attrs(reply.payload, 4) {
      if entry.kind == CTRL_ATTR_MCAST_GROUPS {
        for member in parse_attrs(entry.data, 0) {
          let fields = parse_attrs(member.data, 0)
          let group_name = pick(fields, CTRL_ATTR_MCAST_GRP_NAME)
          let group_id = pick_uint(fields, CTRL_ATTR_MCAST_GRP_ID)
          if group_name != null and group_id != null {
            groups = groups.set(cstring(group_name), group_id)
          }
        }
      }
    }
  }
  groups
}

## Subscribes the socket of `client` to multicast group `id`.
export proc subscribe(client: Client, id: Int) [process, error] -> Result[Unit, Error] {
  guard let _ = linux.setsockopt_int(client.fd, SOL_NETLINK, NETLINK_ADD_MEMBERSHIP, id) else { |failure|
    return Err(failure_to_iw(failure))
  }
}

## Receives the next datagram on `client`'s socket and decodes its first
## message, or null for a message with no generic-netlink header.
export proc receive_event(client: Client) [process, net, error] -> Result[Message?, Error] {
  match linux.recvfrom(client.fd, 65536) {
    Ok(received) => {
      let data = received.data
      if data.len() < 20 {
        return Ok(null)
      }
      Ok({cmd: data.byte_at(16) ?? 0, attrs: parse_attrs(data, 20)})
    }
    Err(failure) => Err(failure_to_iw(failure))
  }
}

# --- device selection --------------------------------------------------

## One interface as GET_INTERFACE reports it.
export type Interface = {name: Str, index: Int, phy: Int}

## Every wireless interface the kernel knows.
export proc interfaces(client: Client) [process, error] -> Result[List[Interface], Error] {
  let messages = request(client, CMD_GET_INTERFACE, [], dump: true)?
  var found: List[Interface] = []
  for message in messages {
    let name = pick(message.attrs, ATTR_IFNAME)
    let index = pick_uint(message.attrs, ATTR_IFINDEX)
    let phy = pick_uint(message.attrs, ATTR_WIPHY)
    if name != null and index != null {
      found += [{name: cstring(name), index: index, phy: phy ?? -1}]
    }
  }
  found
}

## The interface called `name`, or null.
export proc find_interface(client: Client, name: Str) [process, error] -> Result[Interface?, Error] {
  for entry in interfaces(client)? {
    if entry.name == name {
      return Ok(entry)
    }
  }
  Ok(null)
}

## The wiphy index of the device called `name` (`phy0`), or null.
export proc find_phy(client: Client, name: Str) [process, error] -> Result[Int?, Error] {
  let messages = request(client, CMD_GET_WIPHY, [attr_flag(ATTR_SPLIT_WIPHY_DUMP)?], dump: true)?
  for message in messages {
    let wiphy_name = pick(message.attrs, ATTR_WIPHY_NAME)
    if wiphy_name != null and cstring(wiphy_name) == name {
      return Ok(pick_uint(message.attrs, ATTR_WIPHY))
    }
  }
  Ok(null)
}

# --- interfaces --------------------------------------------------------

pure channel_text(attrs: List[Attr]) -> List[Str] {
  let freq = pick_uint(attrs, ATTR_WIPHY_FREQ)
  if freq == null {
    return []
  }
  var text = f"channel {frequency_channel(freq)} ({freq} MHz)"
  let width = pick_uint(attrs, ATTR_CHANNEL_WIDTH)
  let channel_type = pick_uint(attrs, ATTR_WIPHY_CHANNEL_TYPE)
  if width != null {
    text += ", width: " + list_name(WIDTH_NAMES, width, "unknown")
    let center1 = pick_uint(attrs, ATTR_CENTER_FREQ1)
    if center1 != null {
      text += f", center1: {center1} MHz"
    }
    let center2 = pick_uint(attrs, ATTR_CENTER_FREQ2)
    if center2 != null {
      text += f", center2: {center2} MHz"
    }
  } else if channel_type != null {
    text += " " + list_name(CHANNEL_TYPE_NAMES, channel_type, "unknown")
  }
  [text]
}

pure interface_block(attrs: List[Attr], indent: Str, show_wiphy: Bool) -> List[Str] {
  var lines: List[Str] = []
  let name = pick(attrs, ATTR_IFNAME)
  if name != null {
    lines += [f"{indent}Interface {cstring(name)}"]
  } else {
    lines += [f"{indent}Unnamed/non-netdev interface"]
  }
  let index = pick_uint(attrs, ATTR_IFINDEX)
  if index != null {
    lines += [f"{indent}\tifindex {index}"]
  }
  let wdev = pick_uint(attrs, ATTR_WDEV)
  if wdev != null {
    lines += [f"{indent}\twdev 0x{hex(wdev)}"]
  }
  let mac = pick(attrs, ATTR_MAC)
  if mac != null {
    lines += [f"{indent}\taddr {mac_text(mac)}"]
  }
  let ssid = pick(attrs, ATTR_SSID)
  if ssid != null {
    lines += [f"{indent}\tssid {ssid_text(ssid)}"]
  }
  let kind = pick_uint(attrs, ATTR_IFTYPE)
  if kind != null {
    lines += [f"{indent}\ttype {iftype_name(kind)}"]
  }
  let phy = pick_uint(attrs, ATTR_WIPHY)
  if show_wiphy and phy != null {
    lines += [f"{indent}\twiphy {phy}"]
  }
  for text in channel_text(attrs) {
    lines += [f"{indent}\t{text}"]
  }
  let power = pick_uint(attrs, ATTR_WIPHY_TX_POWER_LEVEL)
  if power != null {
    lines += [f"{indent}\ttxpower {power / 100}.{power % 100:02} dBm"]
  }
  let wds = pick_uint(attrs, ATTR_4ADDR)
  if wds != null and wds != 0 {
    lines += [f"{indent}\t4addr: on"]
  }
  lines
}

## The `iw dev` listing: each interface under a `phy#N` header that appears
## whenever the wiphy changes.
export pure dev_lines(messages: List[Message]) -> List[Str] {
  var lines: List[Str] = []
  var current = -1
  for message in messages {
    let phy = pick_uint(message.attrs, ATTR_WIPHY)
    if phy != null and phy != current {
      lines += [f"phy#{phy}"]
      current = phy
    }
    lines += interface_block(message.attrs, "\t", false)
  }
  lines
}

## The `iw dev DEV info` report for one interface message.
export pure info_lines(message: Message) -> List[Str] {
  interface_block(message.attrs, "", true)
}

# --- regulatory domains ------------------------------------------------

pure dfs_region_name(region: Int) -> Str {
  match region {
    0 => "UNSET"
    1 => "FCC"
    2 => "ETSI"
    3 => "JP"
    else => "invalid"
  }
}

## `iw reg get`: one block per regulatory domain message (the global domain
## and each self-managed wiphy), each followed by a blank line.
export pure reg_lines(messages: List[Message]) -> List[Str] {
  var lines: List[Str] = []
  for message in messages {
    let alpha2 = pick(message.attrs, ATTR_REG_ALPHA2)
    let rules = pick(message.attrs, ATTR_REG_RULES)
    if alpha2 == null {
      lines += ["No alpha2"]
      continue
    }
    if rules == null {
      lines += ["No reg rules"]
      continue
    }
    let phy = pick_uint(message.attrs, ATTR_WIPHY)
    if phy != null {
      let managed = pick(message.attrs, ATTR_WIPHY_SELF_MANAGED_REG)
      lines += [if managed != null { f"phy#{phy} (self-managed)" } else { f"phy#{phy}" }]
    } else {
      lines += ["global"]
    }
    # A domain with no DFS region attribute is the unset region.
    let region = pick_uint(message.attrs, ATTR_DFS_REGION) ?? 0
    lines += ["country " + cstring(alpha2) + f": DFS-{dfs_region_name(region)}"]
    for rule in parse_attrs(rules, 0) {
      let fields = parse_attrs(rule.data, 0)
      let flags = pick_uint(fields, RULE_FLAGS) ?? 0
      let start = pick_uint(fields, RULE_FREQ_START) ?? 0
      let end = pick_uint(fields, RULE_FREQ_END) ?? 0
      let bandwidth = pick_uint(fields, RULE_MAX_BW) ?? 0
      let gain = pick_uint(fields, RULE_MAX_ANT_GAIN) ?? 0
      let eirp = pick_uint(fields, RULE_MAX_EIRP) ?? 0
      let cac = pick_uint(fields, RULE_DFS_CAC_TIME) ?? 0
      # Frequencies are whole MHz and the powers whole dBm, as `iw` prints them.
      var text = f"\t({start / 1000} - {end / 1000} @ {bandwidth / 1000}), ("
      text += if gain == 0 { "N/A" } else { f"{gain / 100}" }
      text += f", {eirp / 100}), "
      text += if flags.bit_and(16) != 0 { f"({cac} ms)" } else { "(N/A)" }
      for item in REG_RULE_FLAGS {
        if flags.bit_and(item.bit) != 0 {
          text += ", " + item.name
        }
      }
      lines += [text]
    }
    lines += [""]
  }
  lines
}

# --- scan results ------------------------------------------------------

const CAPABILITY_NAMES = [
  {bit: 1, name: "ESS"}, {bit: 2, name: "IBSS"}, {bit: 4, name: "CfPollable"}, {bit: 8, name: "CfPollReq"},
  {bit: 16, name: "Privacy"}, {bit: 32, name: "ShortPreamble"},
  {bit: 64, name: "PBCC"}, {bit: 128, name: "ChannelAgility"}, {bit: 256, name: "SpectrumMgmt"},
  {bit: 512, name: "QoS"}, {bit: 1024, name: "ShortSlotTime"}, {bit: 2048, name: "APSD"},
  {bit: 4096, name: "RadioMeasure"}, {bit: 8192, name: "DSSS-OFDM"}, {bit: 16384, name: "DelayedBACK"},
  {bit: 32768, name: "ImmediateBACK"},
]

type InformationElement = {id: Int, data: Bytes}

pure split_elements(data: Bytes) -> List[InformationElement] {
  var found: List[InformationElement] = []
  var offset = 0
  while offset + 2 <= data.len() {
    let id = data.byte_at(offset) ?? 0
    let length = data.byte_at(offset + 1) ?? 0
    if offset + 2 + length > data.len() {
      break
    }
    found += [{id: id, data: data.slice(offset + 2, length)}]
    offset += 2 + length
  }
  found
}

pure rate_list(data: Bytes) -> Str {
  var text = ""
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    let rate = byte.bit_and(127)
    let basic = byte.bit_and(128) != 0
    if rate == 127 and basic {
      text += "VHT"
    } else if rate == 126 and basic {
      text += "HT"
    } else {
      text += f"{rate / 2}.{5 * (rate % 2)}"
    }
    text += if basic { "* " } else { " " }
  }
  text
}

# The name `iw` gives a cipher or key-management suite selector, or the
# selector as `00-0f-ac:N` when it has none.
pure suite_text(data: Bytes, akm: Bool) -> Str {
  let oui = hex_bytes(data.slice(0, 3))
  let kind = data.byte_at(3) ?? 0
  if oui == "00 0f ac" {
    for item in if akm { AKM_NAMES } else { CIPHER_SUITE_NAMES } {
      if item.kind == kind {
        return item.name
      }
    }
  }
  if oui == "00 50 f2" {
    for item in if akm { MS_AKM_NAMES } else { MS_CIPHER_NAMES } {
      if item.kind == kind {
        return item.name
      }
    }
  }
  suite_dashed(data)
}

# Cipher suite types of the 00-0f-ac OUI as the RSN element lists them.
const CIPHER_SUITE_NAMES = [
  {kind: 0, name: "Use group cipher suite"}, {kind: 1, name: "WEP-40"}, {kind: 2, name: "TKIP"},
  {kind: 4, name: "CCMP"}, {kind: 5, name: "WEP-104"}, {kind: 6, name: "AES-128-CMAC"},
  {kind: 7, name: "NO-GROUP"}, {kind: 8, name: "GCMP"}, {kind: 9, name: "GCMP-256"}, {kind: 10, name: "CCMP-256"},
  {kind: 11, name: "BIP-GMAC-128"}, {kind: 12, name: "BIP-GMAC-256"}, {kind: 13, name: "BIP-CMAC-256"},
]

# Cipher suite types of the 00-50-f2 (WPA) OUI.
const MS_CIPHER_NAMES = [
  {kind: 0, name: "Use group cipher suite"}, {kind: 1, name: "WEP-40"}, {kind: 2, name: "TKIP"},
  {kind: 4, name: "CCMP"}, {kind: 5, name: "WEP-104"},
]

# Authentication suite types of the 00-50-f2 (WPA) OUI.
const MS_AKM_NAMES = [{kind: 1, name: "IEEE 802.1X"}, {kind: 2, name: "PSK"}]

# Authentication and key management suite types of the 00-0f-ac OUI.
const AKM_NAMES = [
  {kind: 1, name: "IEEE 802.1X"}, {kind: 2, name: "PSK"}, {kind: 3, name: "FT/IEEE 802.1X"},
  {kind: 4, name: "FT/PSK"}, {kind: 5, name: "IEEE 802.1X/SHA-256"}, {kind: 6, name: "PSK/SHA-256"},
  {kind: 7, name: "TDLS/TPK"}, {kind: 8, name: "SAE"}, {kind: 9, name: "FT/SAE"},
  {kind: 11, name: "IEEE 802.1X/SUITE-B"}, {kind: 12, name: "IEEE 802.1X/SUITE-B-192"},
  {kind: 13, name: "FT/IEEE 802.1X/SHA-384"}, {kind: 14, name: "FILS/SHA-256"}, {kind: 15, name: "FILS/SHA-384"},
  {kind: 16, name: "FT/FILS/SHA-256"}, {kind: 17, name: "FT/FILS/SHA-384"}, {kind: 18, name: "OWE"},
  {kind: 19, name: "FT/PSK/SHA-384"}, {kind: 20, name: "PSK/SHA-384"},
]

pure suites(data: Bytes, offset: Int, akm: Bool) -> List[Str] {
  var names: List[Str] = []
  if offset + 2 > data.len() {
    return names
  }
  let count = uint(data.slice(offset, 2))
  var at = offset + 2
  var remaining = count
  while remaining > 0 and at + 4 <= data.len() {
    names += [suite_text(data.slice(at, 4), akm)]
    at += 4
    remaining -= 1
  }
  names
}

# The lines of an RSN or WPA element after its name: version, ciphers,
# authentication suites, and for RSN the capabilities, PMKIDs, and group
# management cipher. `offset` is where the group cipher suite starts. Lines
# after the first carry the leading tab `iw` writes on every line but the
# first.
pure cipher_lines(data: Bytes, offset: Int, version: Int, rsn: Bool) -> List[Str] {
  var lines: List[Str] = [f"\t * Version: {version}"]
  if offset + 4 > data.len() {
    return lines
  }
  lines += [f"\t\t * Group cipher: {suite_text(data.slice(offset, 4), false)}"]
  let pairwise_at = offset + 4
  if pairwise_at + 2 > data.len() {
    return lines
  }
  let pairwise = suites(data, pairwise_at, false)
  lines += ["\t\t * Pairwise ciphers: " + pairwise.join(" ")]
  let akm_at = pairwise_at + 2 + 4 * pairwise.len()
  if akm_at + 2 > data.len() {
    return lines
  }
  let key_management = suites(data, akm_at, true)
  lines += ["\t\t * Authentication suites: " + key_management.join(" ")]
  var at = akm_at + 2 + 4 * key_management.len()
  if ! rsn or at + 2 > data.len() {
    return lines
  }
  let value = uint(data.slice(at, 2))
  at += 2
  var text = "\t\t * Capabilities:"
  if value.bit_and(1) != 0 {
    text += " PreAuth"
  }
  if value.bit_and(2) != 0 {
    text += " NoPairwise"
  }
  text += f" {[1, 2, 4, 16][value / 4 % 4]}-PTKSA-RC {[1, 2, 4, 16][value / 16 % 4]}-GTKSA-RC"
  for item in [
    {bit: 64, name: "MFP-required"},
    {bit: 128, name: "MFP-capable"},
    {bit: 512, name: "Peerkey-enabled"},
    {bit: 1024, name: "SPP-AMSDU-capable"},
    {bit: 2048, name: "SPP-AMSDU-required"},
    {bit: 8192, name: "Extended-Key-ID"},
  ] {
    if value.bit_and(item.bit) != 0 {
      text += " " + item.name
    }
  }
  lines += [text + f" (0x{hex(value, 4)})"]
  if at + 2 <= data.len() {
    let count = uint(data.slice(at, 2))
    at += 2
    lines += [f"\t\t * {count} PMKIDs"]
    at += 16 * count
  }
  if at + 4 <= data.len() {
    lines += ["\t\t * Group mgmt cipher suite: " + suite_text(data.slice(at, 4), false)]
  }
  lines
}

# The report lines for one information element; an element this decoder has
# no printer for is listed only when `unknown` is set.
pure element_lines(element: InformationElement, unknown: Bool) -> List[Str] {
  let data = element.data
  match element.id {
    0 => [f"\tSSID: {ssid_text(data)}"]
    1 => ["\tSupported rates: " + rate_list(data)]
    3 => if data.len() >= 1 { [f"\tDS Parameter set: channel {data.byte_at(0) ?? 0}"] } else { [] }
    5 => tim_lines(data)
    7 => country_lines(data)
    11 => load_lines(data)
    32 => if data.len() >= 1 { [f"\tPower constraint: {data.byte_at(0) ?? 0} dB"] } else { [] }
    42 => erp_lines(data)
    48 => rsn_lines(data)
    50 => ["\tExtended supported rates: " + rate_list(data)]
    221 => vendor_lines(data, unknown)
    else => unknown_lines(element, unknown)
  }
}

# `iw` prints each data byte after a space, so an empty element leaves no
# trailing space.
pure spaced_bytes(data: Bytes) -> Str {
  if data.is_empty() { "" } else { " " + hex_bytes(data) }
}

pure unknown_lines(element: InformationElement, unknown: Bool) -> List[Str] {
  if unknown {
    [f"\tUnknown IE ({element.id}):" + spaced_bytes(element.data)]
  } else {
    []
  }
}

pure tim_lines(data: Bytes) -> List[Str] {
  if data.len() < 4 {
    return ["\tTIM: <invalid: " + f"{data.len()}" + " bytes>"]
  }
  let count = data.byte_at(0) ?? 0
  let period = data.byte_at(1) ?? 0
  let control = data.byte_at(2) ?? 0
  let bitmap = data.byte_at(3) ?? 0
  var text = f"\tTIM: DTIM Count {count} DTIM Period {period} Bitmap Control 0x{hex(control)} Bitmap[0] 0x{hex(bitmap)}"
  if data.len() > 4 {
    text += f" (+ {data.len() - 4} octets)"
  }
  [text]
}

pure country_lines(data: Bytes) -> List[Str] {
  if data.len() < 3 {
    return ["\tCountry: <invalid>"]
  }
  let code = data.slice(0, 2).utf8() ?? "??"
  let environment = data.byte_at(2) ?? 32
  let place = match environment {
    32 => "Indoor/Outdoor"
    79 => "Outdoor"
    73 => "Indoor"
    else => "bogus"
  }
  var lines: List[Str] = [f"\tCountry: {code}\tEnvironment: {place}"]
  var at = 3
  while at + 3 <= data.len() {
    let first = data.byte_at(at) ?? 0
    let second = data.byte_at(at + 1) ?? 0
    let third = data.byte_at(at + 2) ?? 0
    if first >= 201 {
      lines += [f"\t\tExtension ID: {first} Regulatory Class: {second} Coverage class: {third} (up to {450 * third}m)"]
    } else {
      let step = if first > 14 { 4 } else { 1 }
      lines += [f"\t\tChannels [{first} - {first + (second - 1) * step}] @ {third} dBm"]
    }
    at += 3
  }
  lines
}

pure load_lines(data: Bytes) -> List[Str] {
  if data.len() < 5 {
    return ["\tBSS Load: <invalid>"]
  }
  [
    "\tBSS Load:",
    f"\t\t * station count: {uint(data.slice(0, 2))}",
    f"\t\t * channel utilisation: {data.byte_at(2) ?? 0}/255",
    f"\t\t * available admission capacity: {uint(data.slice(3, 2))} [*32us]",
  ]
}

pure erp_lines(data: Bytes) -> List[Str] {
  let flags = data.byte_at(0) ?? 0
  var text = "\tERP:"
  if flags.bit_and(1) != 0 {
    text += " <Non-ERP present>"
  }
  if flags.bit_and(2) != 0 {
    text += " <use protection>"
  }
  if flags.bit_and(4) != 0 {
    text += " <barker preamble mode>"
  }
  if flags.bit_and(7) == 0 {
    text += " <no flags>"
  }
  [text]
}

pure rsn_lines(data: Bytes) -> List[Str] {
  if data.len() < 2 {
    return ["\tRSN: <invalid>"]
  }
  let version = uint(data.slice(0, 2))
  let body = cipher_lines(data, 2, version, true)
  ["\tRSN:" + body[0]] + body[1..]
}

pure vendor_lines(data: Bytes, unknown: Bool) -> List[Str] {
  if data.len() >= 8 and hex_bytes(data.slice(0, 4)) == "00 50 f2 01" {
    let version = uint(data.slice(4, 2))
    let body = cipher_lines(data, 6, version, false)
    return ["\tWPA:" + body[0]] + body[1..]
  }
  if unknown {
    let oui = hex_bytes(data[0..3]).replace(" ", with: ":")
    [f"\tVendor specific: OUI {oui}, data:" + spaced_bytes(data[3..])]
  } else {
    []
  }
}

## The element lines of a raw information-element block.
export pure element_block(data: Bytes, unknown: Bool) -> List[Str] {
  var lines: List[Str] = []
  for element in split_elements(data) {
    lines += element_lines(element, unknown)
  }
  lines
}

pure tsf_text(tsf: Int) -> Str {
  let seconds = tsf / 1000000
  let days = seconds / 86400
  let hours = seconds / 3600 % 24
  let minutes = seconds / 60 % 60
  f"{tsf} usec ({days}d, {hours:02}:{minutes:02}:{seconds % 60:02})"
}

## `iw scan` / `iw scan dump` lines for one BSS message. `interface` names
## the device the scan belongs to; `unknown` is `-u`.
export pure bss_lines(message: Message, interface: Str, unknown: Bool) -> List[Str] {
  let container = pick(message.attrs, ATTR_BSS)
  if container == null {
    return []
  }
  let bss = parse_attrs(container, 0)
  let bssid = pick(bss, BSS_BSSID)
  if bssid == null {
    return []
  }
  var head = f"BSS {mac_text(bssid)}(on {interface})"
  let status = pick_uint(bss, BSS_STATUS)
  if status != null {
    head += match status {
      0 => " -- authenticated"
      1 => " -- associated"
      2 => " -- joined"
      else => f" -- unknown status: {status}"
    }
  }
  var lines: List[Str] = [head]
  let boottime = pick_uint(bss, BSS_LAST_SEEN_BOOTTIME)
  if boottime != null {
    let milliseconds = boottime / 1000000
    lines += [f"\tlast seen: {milliseconds / 1000}.{milliseconds % 1000:03}s [boottime]"]
  }
  let tsf = pick_uint(bss, BSS_TSF)
  if tsf != null {
    lines += ["\tTSF: " + tsf_text(tsf)]
  }
  let freq = pick_uint(bss, BSS_FREQUENCY)
  if freq != null {
    let offset = pick_uint(bss, BSS_FREQUENCY_OFFSET)
    lines += [if offset != null { f"\tfreq: {freq}.{offset}" } else { f"\tfreq: {freq}" }]
  }
  let interval = pick_uint(bss, BSS_BEACON_INTERVAL)
  if interval != null {
    lines += [f"\tbeacon interval: {interval} TUs"]
  }
  let capability = pick_uint(bss, BSS_CAPABILITY)
  if capability != null {
    var text = "\tcapability:"
    for item in CAPABILITY_NAMES {
      if capability.bit_and(item.bit) != 0 {
        text += " " + item.name
      }
    }
    lines += [text + f" (0x{hex(capability, 4)})"]
  }
  let signal = pick(bss, BSS_SIGNAL_MBM)
  if signal != null {
    var mbm = uint(signal)
    if mbm >= 2147483648 {
      mbm -= 4294967296
    }
    let magnitude = if mbm < 0 { -mbm } else { mbm }
    let sign = if mbm < 0 and mbm > -100 { "-" } else { "" }
    lines += [f"\tsignal: {sign}{mbm / 100}.{magnitude % 100:02} dBm"]
  }
  let unspecified = pick_uint(bss, BSS_SIGNAL_UNSPEC)
  if unspecified != null {
    lines += [f"\tsignal: {unspecified}/100"]
  }
  let age = pick_uint(bss, BSS_SEEN_MS_AGO)
  if age != null {
    lines += [f"\tlast seen: {age} ms ago"]
  }
  let response = pick(bss, BSS_INFORMATION_ELEMENTS)
  let beacon = pick(bss, BSS_BEACON_IES)
  let both = response != null and beacon != null and response != beacon
  if response != null {
    if both {
      lines += ["\tInformation elements from Probe Response frame:"]
    }
    lines += element_block(response, unknown)
  }
  if beacon != null {
    if both {
      lines += ["\tInformation elements from Beacon frame:"]
    }
    if both or response == null {
      lines += element_block(beacon, unknown)
    }
  }
  lines
}

# --- stations and the current link --------------------------------------

# A station's bit rate as `iw` writes it: the rate, then each flag and index
# the kernel reported, in the order `iw` checks them.
pure bitrate_text(rate: Bytes) -> Str {
  let fields = parse_attrs(rate, 0)
  let tenths = first_of(pick_uint(fields, RATE_BITRATE32), pick_uint(fields, RATE_BITRATE)) ?? 0
  var text = if tenths > 0 { f"{tenths / 10}.{tenths % 10} MBit/s" } else { "(unknown)" }
  let mcs = pick_uint(fields, RATE_MCS)
  if mcs != null {
    text += f" MCS {mcs}"
  }
  let vht_mcs = pick_uint(fields, RATE_VHT_MCS)
  if vht_mcs != null {
    text += f" VHT-MCS {vht_mcs}"
  }
  for width in [
    {kind: RATE_40_MHZ, name: " 40MHz"},
    {kind: RATE_80_MHZ, name: " 80MHz"},
    {kind: RATE_80P80_MHZ, name: " 80P80MHz"},
    {kind: RATE_160_MHZ, name: " 160MHz"},
    {kind: RATE_320_MHZ, name: " 320MHz"},
    {kind: RATE_1_MHZ, name: " 1MHz"},
    {kind: RATE_2_MHZ, name: " 2MHz"},
    {kind: RATE_4_MHZ, name: " 4MHz"},
    {kind: RATE_8_MHZ, name: " 8MHz"},
    {kind: RATE_16_MHZ, name: " 16MHz"},
  ] {
    if pick(fields, width.kind) != null {
      text += width.name
    }
  }
  if pick(fields, RATE_SHORT_GI) != null {
    text += " short GI"
  }
  for number in [
    {kind: RATE_VHT_NSS, name: "VHT-NSS"},
    {kind: RATE_HE_MCS, name: "HE-MCS"},
    {kind: RATE_HE_NSS, name: "HE-NSS"},
    {kind: RATE_HE_GI, name: "HE-GI"},
    {kind: RATE_HE_DCM, name: "HE-DCM"},
    {kind: RATE_HE_RU_ALLOC, name: "HE-RU-ALLOC"},
    {kind: RATE_EHT_MCS, name: "EHT-MCS"},
    {kind: RATE_EHT_NSS, name: "EHT-NSS"},
    {kind: RATE_EHT_GI, name: "EHT-GI"},
    {kind: RATE_EHT_RU_ALLOC, name: "EHT-RU-ALLOC"},
  ] {
    let value = pick_uint(fields, number.kind)
    if value != null {
      text += f" {number.name} {value}"
    }
  }
  text
}

pure yes_no(value: Bool) -> Str {
  if value { "yes" } else { "no" }
}

# The `[a, b] ` list of per-chain signals that follows a signal value, or an
# empty string when the kernel sent no chain data.
pure chain_signals(chains: Bytes?) -> Str {
  if chains == null {
    return ""
  }
  var values: List[Str] = []
  for chain in parse_attrs(chains, 0) {
    values += [f"{signed8(uint(chain.data))}"]
  }
  "[" + values.join(", ") + "] "
}

## `iw dev DEV station dump` lines for one station message. Each field is
## printed on its own tab-indented line, as `iw` does. `boot_ms` is the
## milliseconds since boot and `wall_ms` the wall-clock milliseconds, which
## `iw` uses to date the association.
export pure station_lines(message: Message, interface: Str, boot_ms: Int, wall_ms: Int) -> List[Str] {
  let mac = pick(message.attrs, ATTR_MAC)
  let container = pick(message.attrs, ATTR_STA_INFO)
  if mac == null or container == null {
    return []
  }
  let info = parse_attrs(container, 0)
  var lines: List[Str] = [f"Station {mac_text(mac)} (on {interface})"]
  let inactive = pick_uint(info, STA_INACTIVE_TIME)
  if inactive != null {
    lines += [f"\tinactive time:\t{inactive} ms"]
  }
  let rx_bytes = first_of(pick_uint(info, STA_RX_BYTES64), pick_uint(info, STA_RX_BYTES))
  if rx_bytes != null {
    lines += [f"\trx bytes:\t{rx_bytes}"]
  }
  let rx_packets = pick_uint(info, STA_RX_PACKETS)
  if rx_packets != null {
    lines += [f"\trx packets:\t{rx_packets}"]
  }
  let tx_bytes = first_of(pick_uint(info, STA_TX_BYTES64), pick_uint(info, STA_TX_BYTES))
  if tx_bytes != null {
    lines += [f"\ttx bytes:\t{tx_bytes}"]
  }
  let tx_packets = pick_uint(info, STA_TX_PACKETS)
  if tx_packets != null {
    lines += [f"\ttx packets:\t{tx_packets}"]
  }
  let retries = pick_uint(info, STA_TX_RETRIES)
  if retries != null {
    lines += [f"\ttx retries:\t{retries}"]
  }
  let failed = pick_uint(info, STA_TX_FAILED)
  if failed != null {
    lines += [f"\ttx failed:\t{failed}"]
  }
  let beacon_loss = pick_uint(info, STA_BEACON_LOSS)
  if beacon_loss != null {
    lines += [f"\tbeacon loss:\t{beacon_loss}"]
  }
  let beacon_rx = pick_uint(info, STA_BEACON_RX)
  if beacon_rx != null {
    lines += [f"\t\tbeacon rx:\t{beacon_rx}"]
  }
  let dropped = pick_uint(info, STA_RX_DROP_MISC)
  if dropped != null {
    lines += [f"\trx drop misc:\t{dropped}"]
  }
  let signal = pick_uint(info, STA_SIGNAL)
  if signal != null {
    lines += [f"\tsignal:  \t{signed8(signal)} " + chain_signals(pick(info, STA_CHAIN_SIGNAL)) + "dBm"]
  }
  let average = pick_uint(info, STA_SIGNAL_AVG)
  if average != null {
    lines += [f"\tsignal avg:\t{signed8(average)} " + chain_signals(pick(info, STA_CHAIN_SIGNAL_AVG)) + "dBm"]
  }
  let beacon_average = pick_uint(info, STA_BEACON_SIGNAL_AVG)
  if beacon_average != null {
    lines += [f"\tbeacon signal avg:\t{signed8(beacon_average)} dBm"]
  }
  let offset = pick_uint(info, STA_T_OFFSET)
  if offset != null {
    lines += [f"\tToffset:\t{offset} us"]
  }
  let tx_rate = pick(info, STA_TX_BITRATE)
  if tx_rate != null {
    lines += ["\ttx bitrate:\t" + bitrate_text(tx_rate)]
  }
  let tx_duration = pick_uint(info, STA_TX_DURATION)
  if tx_duration != null {
    lines += [f"\ttx duration:\t{tx_duration} us"]
  }
  let rx_rate = pick(info, STA_RX_BITRATE)
  if rx_rate != null {
    lines += ["\trx bitrate:\t" + bitrate_text(rx_rate)]
  }
  let rx_duration = pick_uint(info, STA_RX_DURATION)
  if rx_duration != null {
    lines += [f"\trx duration:\t{rx_duration} us"]
  }
  let ack_signal = pick_uint(info, STA_ACK_SIGNAL)
  if ack_signal != null {
    lines += [f"\tlast ack signal:{signed8(ack_signal)} dBm"]
  }
  let ack_average = pick_uint(info, STA_ACK_SIGNAL_AVG)
  if ack_average != null {
    lines += [f"\tavg ack signal:\t{signed8(ack_average)} dBm"]
  }
  let weight = pick_uint(info, STA_AIRTIME_WEIGHT)
  if weight != null {
    lines += [f"\tairtime weight: {weight}"]
  }
  let throughput = pick_uint(info, STA_EXPECTED_THROUGHPUT)
  if throughput != null {
    lines += [f"\texpected throughput:\t{throughput / 1000}.{throughput % 1000}Mbps"]
  }
  let parameters = pick(info, STA_BSS_PARAM)
  if parameters != null {
    let fields = parse_attrs(parameters, 0)
    let dtim = pick_uint(fields, BSS_PARAM_DTIM_PERIOD)
    if dtim != null {
      lines += [f"\tDTIM period:\t{dtim}"]
    }
    let interval = pick_uint(fields, BSS_PARAM_BEACON_INTERVAL)
    if interval != null {
      lines += [f"\tbeacon interval:{interval}"]
    }
    if pick(fields, BSS_PARAM_CTS_PROT) != null {
      lines += ["\tCTS protection:\tyes"]
    }
    if pick(fields, BSS_PARAM_SHORT_PREAMBLE) != null {
      lines += ["\tshort preamble:\tyes"]
    }
    if pick(fields, BSS_PARAM_SHORT_SLOT_TIME) != null {
      lines += ["\tshort slot time:\tyes"]
    }
  }
  let connected = pick_uint(info, STA_CONNECTED_TIME)
  if connected != null {
    lines += [f"\tconnected time:\t{connected} seconds"]
  }
  let associated = pick_uint(info, STA_ASSOC_AT_BOOTTIME)
  if associated != null {
    let boot_seconds = associated / 1000000000
    let boot_millis = associated / 1000000 % 1000
    lines += [f"\tassociated at [boottime]:\t{boot_seconds}.{boot_millis:03}s"]
    lines += [f"\tassociated at:\t{wall_ms - (boot_ms * 1000000 - associated) / 1000000} ms"]
  }
  let flags = pick(info, STA_FLAGS)
  if flags != null and flags.len() >= 8 {
    let mask = uint(flags.slice(0, 4))
    let value_bits = uint(flags.slice(4, 4))
    # Station flag bits: authorized 1, short preamble 2, WME 3, MFP 4,
    # authenticated 5, TDLS peer 6, associated 7.
    if mask.bit_and(2) != 0 {
      lines += [f"\tauthorized:\t{yes_no(value_bits.bit_and(2) != 0)}"]
    }
    if mask.bit_and(32) != 0 {
      lines += [f"\tauthenticated:\t{yes_no(value_bits.bit_and(32) != 0)}"]
    }
    if mask.bit_and(128) != 0 {
      lines += [f"\tassociated:\t{yes_no(value_bits.bit_and(128) != 0)}"]
    }
    if mask.bit_and(4) != 0 {
      lines += ["\tpreamble:\t" + (if value_bits.bit_and(4) != 0 { "short" } else { "long" })]
    }
    if mask.bit_and(8) != 0 {
      lines += [f"\tWMM/WME:\t{yes_no(value_bits.bit_and(8) != 0)}"]
    }
    if mask.bit_and(16) != 0 {
      lines += [f"\tMFP:\t\t{yes_no(value_bits.bit_and(16) != 0)}"]
    }
    if mask.bit_and(64) != 0 {
      lines += [f"\tTDLS peer:\t{yes_no(value_bits.bit_and(64) != 0)}"]
    }
  }
  lines += [f"\tcurrent time:\t{wall_ms} ms"]
  lines
}

## The BSS the interface is connected to, as the scan dump reports one with
## a status: its address and report lines, or null when not connected.
export type Connection = {bssid: Bytes, lines: List[Str]}

## Finds the associated (or joined) BSS among scan messages and renders the
## head of `iw dev DEV link`.
export pure connection_of(messages: List[Message], interface: Str) -> Connection? {
  for message in messages {
    let container = pick(message.attrs, ATTR_BSS)
    if container == null {
      continue
    }
    let bss = parse_attrs(container, 0)
    let bssid = pick(bss, BSS_BSSID)
    let status = pick_uint(bss, BSS_STATUS)
    if bssid == null or status == null {
      continue
    }
    if status == BSS_STATUS_AUTHENTICATED {
      return {bssid: bssid, lines: [f"Authenticated with {mac_text(bssid)} (on {interface})"]}
    }
    var lines: List[Str] = []
    if status == BSS_STATUS_ASSOCIATED {
      lines += [f"Connected to {mac_text(bssid)} (on {interface})"]
    } else if status == BSS_STATUS_IBSS_JOINED {
      lines += [f"Joined IBSS {mac_text(bssid)} (on {interface})"]
    } else {
      continue
    }
    let elements = first_bytes(pick(bss, BSS_INFORMATION_ELEMENTS), pick(bss, BSS_BEACON_IES))
    if elements != null {
      for element in split_elements(elements) {
        if element.id == 0 {
          lines += [f"\tSSID: {ssid_text(element.data)}"]
          break
        }
      }
    }
    let freq = pick_uint(bss, BSS_FREQUENCY)
    if freq != null {
      let offset = pick_uint(bss, BSS_FREQUENCY_OFFSET) ?? 0
      lines += [f"\tfreq: {freq}.{offset}"]
    }
    return {bssid: bssid, lines: lines}
  }
  null
}

## The statistics of `iw dev DEV link` for the connected station message.
export pure link_lines(message: Message) -> List[Str] {
  let container = pick(message.attrs, ATTR_STA_INFO)
  if container == null {
    return []
  }
  let info = parse_attrs(container, 0)
  var lines: List[Str] = []
  let rx_bytes = pick_uint(info, STA_RX_BYTES)
  let rx_packets = pick_uint(info, STA_RX_PACKETS)
  if rx_bytes != null and rx_packets != null {
    lines += [f"\tRX: {rx_bytes} bytes ({rx_packets} packets)"]
  }
  let tx_bytes = pick_uint(info, STA_TX_BYTES)
  let tx_packets = pick_uint(info, STA_TX_PACKETS)
  if tx_bytes != null and tx_packets != null {
    lines += [f"\tTX: {tx_bytes} bytes ({tx_packets} packets)"]
  }
  let signal = pick_uint(info, STA_SIGNAL)
  if signal != null {
    lines += [f"\tsignal: {signed8(signal)} dBm"]
  }
  let rx_rate = pick(info, STA_RX_BITRATE)
  if rx_rate != null {
    lines += ["\trx bitrate: " + bitrate_text(rx_rate)]
  }
  let tx_rate = pick(info, STA_TX_BITRATE)
  if tx_rate != null {
    lines += ["\ttx bitrate: " + bitrate_text(tx_rate)]
  }
  let parameters = pick(info, STA_BSS_PARAM)
  if parameters != null {
    let fields = parse_attrs(parameters, 0)
    var flags: List[Str] = []
    if pick(fields, BSS_PARAM_CTS_PROT) != null {
      flags += ["CTS-protection"]
    }
    if pick(fields, BSS_PARAM_SHORT_PREAMBLE) != null {
      flags += ["short-preamble"]
    }
    if pick(fields, BSS_PARAM_SHORT_SLOT_TIME) != null {
      flags += ["short-slot-time"]
    }
    lines += ["\tbss flags: " + flags.join(" ")]
    let dtim = pick_uint(fields, BSS_PARAM_DTIM_PERIOD)
    if dtim != null {
      lines += [f"\tdtim period: {dtim}"]
    }
    let interval = pick_uint(fields, BSS_PARAM_BEACON_INTERVAL)
    if interval != null {
      lines += [f"\tbeacon int: {interval}"]
    }
  }
  lines
}

# --- wireless devices --------------------------------------------------

# Cipher suite selectors of the 00-0f-ac family, by suite type.
const CIPHER_NAMES = [
  {kind: 1, name: "WEP40"}, {kind: 5, name: "WEP104"}, {kind: 2, name: "TKIP"}, {kind: 4, name: "CCMP-128"},
  {kind: 10, name: "CCMP-256"}, {kind: 8, name: "GCMP-128"}, {kind: 9, name: "GCMP-256"}, {kind: 6, name: "CMAC"},
  {kind: 13, name: "CMAC-256"}, {kind: 11, name: "GMAC-128"}, {kind: 12, name: "GMAC-256"},
]

# The name `iw` gives a cipher suite selector (three OUI bytes and a type).
pure cipher_name(selector: Bytes) -> Str {
  let kind = selector.byte_at(3) ?? 0
  if hex_bytes(selector.slice(0, 3)) == "00 0f ac" {
    for item in CIPHER_NAMES {
      if item.kind == kind {
        return f"{item.name} ({suite_dashed(selector)})"
      }
    }
  }
  if hex_bytes(selector) == "00 14 72 01" {
    return f"WPI-SMS4 ({suite_dashed(selector)})"
  }
  suite_dashed(selector)
}

# The selector as `00-0f-ac:4`.
pure suite_dashed(selector: Bytes) -> Str {
  let oui = hex_bytes(selector.slice(0, 3)).replace(" ", with: "-")
  f"{oui}:{selector.byte_at(3) ?? 0}"
}

pure frequency_lines(freqs: Bytes) -> List[Str] {
  var lines: List[Str] = []
  for item in parse_attrs(freqs, 0) {
    let fields = parse_attrs(item.data, 0)
    let freq = pick_uint(fields, FREQ_FREQ)
    if freq == null {
      continue
    }
    let offset = pick_uint(fields, FREQ_OFFSET)
    var text = if offset != null { f"\t\t\t* {freq}.{offset} MHz" } else { f"\t\t\t* {freq} MHz" }
    text += f" [{frequency_channel(freq)}]"
    if pick(fields, FREQ_DISABLED) != null {
      lines += [text + " (disabled)"]
      continue
    }
    let power = pick_uint(fields, FREQ_MAX_TX_POWER)
    if power != null {
      let tenths = (power + 5) / 10
      text += f" ({tenths / 10}.{tenths % 10} dBm)"
    }
    var restrictions: List[Str] = []
    if pick(fields, FREQ_NO_IR) != null {
      restrictions += ["no IR"]
    }
    if pick(fields, FREQ_RADAR) != null {
      restrictions += ["radar detection"]
    }
    if ! restrictions.is_empty() {
      text += " (" + restrictions.join(", ") + ")"
    }
    lines += [text]
  }
  lines
}

pure bitrate_lines(rates: Bytes) -> List[Str] {
  var lines: List[Str] = []
  for item in parse_attrs(rates, 0) {
    let fields = parse_attrs(item.data, 0)
    let rate = pick_uint(fields, 1)
    if rate == null {
      continue
    }
    var text = f"\t\t\t* {rate / 10}.{rate % 10} Mbps"
    if pick(fields, 2) != null {
      text += " (short preamble supported)"
    }
    lines += [text]
  }
  lines
}

const AMPDU_SPACING_NAMES = ["No restriction", "1/4 usec", "1/2 usec", "1 usec", "2 usec", "4 usec", "8 usec", "16 usec"]

# The HT capability bits of a 2.4 or 5 GHz band, one indented line each.
pure ht_capability_lines(cap: Int) -> List[Str] {
  var lines: List[Str] = [f"\t\tCapabilities: 0x{hex(cap, 2)}"]
  let power_save = cap / 4 % 4
  let rx_stbc = cap / 256 % 4
  if cap.bit_and(1) != 0 {
    lines += ["\t\t\tRX LDPC"]
  }
  lines += [if cap.bit_and(2) != 0 { "\t\t\tHT20/HT40" } else { "\t\t\tHT20" }]
  match power_save {
    0 => lines += ["\t\t\tStatic SM Power Save"]
    1 => lines += ["\t\t\tDynamic SM Power Save"]
    3 => lines += ["\t\t\tSM Power Save disabled"]
    else => {}
  }
  if cap.bit_and(16) != 0 {
    lines += ["\t\t\tRX Greenfield"]
  }
  if cap.bit_and(32) != 0 {
    lines += ["\t\t\tRX HT20 SGI"]
  }
  if cap.bit_and(64) != 0 {
    lines += ["\t\t\tRX HT40 SGI"]
  }
  if cap.bit_and(128) != 0 {
    lines += ["\t\t\tTX STBC"]
  }
  lines += [["\t\t\tNo RX STBC", "\t\t\tRX STBC 1-stream", "\t\t\tRX STBC 2-streams", "\t\t\tRX STBC 3-streams"][rx_stbc]]
  if cap.bit_and(1024) != 0 {
    lines += ["\t\t\tHT Delayed Block Ack"]
  }
  lines += [if cap.bit_and(2048) != 0 { "\t\t\tMax AMSDU length: 7935 bytes" } else { "\t\t\tMax AMSDU length: 3839 bytes" }]
  lines += [if cap.bit_and(4096) != 0 { "\t\t\tDSSS/CCK HT40" } else { "\t\t\tNo DSSS/CCK HT40" }]
  if cap.bit_and(16384) != 0 {
    lines += ["\t\t\t40 MHz Intolerant"]
  }
  if cap.bit_and(32768) != 0 {
    lines += ["\t\t\tL-SIG TXOP protection"]
  }
  lines
}

# The set bits of a 77-bit MCS bitmap as ` 0-7, 32`: runs of consecutive
# indexes are written as ranges.
pure mcs_index_text(mcs: Bytes) -> Str {
  var runs: List[Str] = []
  var start = -1
  var previous = -1
  for index in range(78) {
    let present = index < 77 and (mcs.byte_at(index / 8) ?? 0).bit_and([1, 2, 4, 8, 16, 32, 64, 128][index % 8]) != 0
    if present {
      if start < 0 {
        start = index
      }
      previous = index
    } else if start >= 0 {
      runs += [if previous == start { f"{start}" } else { f"{start}-{previous}" }]
      start = -1
    }
  }
  " " + runs.join(", ")
}

# The HT MCS set: the receive data rate and the supported rate indexes.
pure ht_mcs_lines(mcs: Bytes) -> List[Str] {
  var lines: List[Str] = []
  let max_rate = (mcs.byte_at(10) ?? 0) + (mcs.byte_at(11) ?? 0).bit_and(3) * 256
  let tx_byte = mcs.byte_at(12) ?? 0
  let tx_defined = tx_byte.bit_and(1) != 0
  let tx_equal = tx_byte.bit_and(2) == 0
  let streams = tx_byte / 4 % 4 + 1
  let unequal = tx_byte.bit_and(16) != 0
  if max_rate != 0 {
    lines += [f"\t\tHT Max RX data rate: {max_rate} Mbps"]
  }
  if tx_defined {
    if tx_equal {
      lines += ["\t\tHT TX/RX MCS rate indexes supported:" + mcs_index_text(mcs)]
    } else {
      lines += ["\t\tHT RX MCS rate indexes supported:" + mcs_index_text(mcs)]
      lines += [if unequal { "\t\tTX unequal modulation supported" } else { "\t\tTX unequal modulation not supported" }]
      lines += [f"\t\tHT TX Max spatial streams: {streams}"]
      lines += ["\t\tHT TX MCS rate indexes supported may differ"]
    }
  } else {
    lines += ["\t\tHT RX MCS rate indexes supported:" + mcs_index_text(mcs)]
    lines += ["\t\tHT TX MCS rate indexes are undefined"]
  }
  lines
}

# One band: the HT block, then frequencies, then non-HT bitrates, which is
# the order `iw` prints them in.
pure band_lines(index: Int, band: Bytes) -> List[Str] {
  let fields = parse_attrs(band, 0)
  var lines: List[Str] = [f"\tBand {index + 1}:"]
  let capability = pick_uint(fields, BAND_HT_CAPA)
  if capability != null {
    lines += ht_capability_lines(capability)
  }
  let factor = pick_uint(fields, BAND_HT_AMPDU_FACTOR)
  if factor != null {
    var length = 8192
    for _ in range(factor) {
      length *= 2
    }
    lines += [f"\t\tMaximum RX AMPDU length {length - 1} bytes (exponent: 0x0{hex(factor, 2)})"]
  }
  let density = pick_uint(fields, BAND_HT_AMPDU_DENSITY)
  if density != null {
    lines += [f"\t\tMinimum RX AMPDU time spacing: {list_name(AMPDU_SPACING_NAMES, density, "unknown")} (0x{hex(density, 2)})"]
  }
  let mcs = pick(fields, BAND_HT_MCS_SET)
  if mcs != null and mcs.len() == 16 {
    lines += ht_mcs_lines(mcs)
  }
  let freqs = pick(fields, BAND_FREQS)
  if freqs != null {
    lines += ["\t\tFrequencies:"]
    lines += frequency_lines(freqs)
  }
  let rates = pick(fields, BAND_RATES)
  if rates != null {
    lines += ["\t\tBitrates (non-HT):"]
    lines += bitrate_lines(rates)
  }
  lines
}

## The `iw phy` / `iw list` / `iw phy PHY info` report for the messages of a
## split wiphy dump. Each message prints the sections it carries, in the
## order `iw` does; a `Wiphy NAME` header starts each device.
export pure phy_lines(messages: List[Message]) -> List[Str] {
  var lines: List[Str] = []
  var band_index = 0
  for message in messages {
    let attrs = message.attrs
    let name = pick(attrs, ATTR_WIPHY_NAME)
    if name != null {
      lines += [f"Wiphy {cstring(name)}"]
      band_index = 0
    }
    let index = pick_uint(attrs, ATTR_WIPHY)
    if index != null {
      lines += [f"\twiphy index: {index}"]
    }
    let scan_ssids = pick_uint(attrs, ATTR_MAX_NUM_SCAN_SSIDS)
    if scan_ssids != null {
      lines += [f"\tmax # scan SSIDs: {scan_ssids}"]
    }
    let scan_ie = pick_uint(attrs, ATTR_MAX_SCAN_IE_LEN)
    if scan_ie != null {
      lines += [f"\tmax scan IEs length: {scan_ie} bytes"]
    }
    let sched_ssids = pick_uint(attrs, ATTR_MAX_NUM_SCHED_SCAN_SSIDS)
    if sched_ssids != null {
      lines += [f"\tmax # sched scan SSIDs: {sched_ssids}"]
    }
    let match_sets = pick_uint(attrs, ATTR_MAX_MATCH_SETS)
    if match_sets != null {
      lines += [f"\tmax # match sets: {match_sets}"]
    }
    let plans = pick_uint(attrs, ATTR_MAX_NUM_SCHED_SCAN_PLANS)
    if plans != null {
      lines += [f"\tmax # scan plans: {plans}"]
    }
    let plan_interval = pick_uint(attrs, ATTR_MAX_SCAN_PLAN_INTERVAL)
    if plan_interval != null {
      lines += [f"\tmax scan plan interval: {plan_interval}"]
    }
    let plan_iterations = pick_uint(attrs, ATTR_MAX_SCAN_PLAN_ITERATIONS)
    if plan_iterations != null {
      lines += [f"\tmax scan plan iterations: {plan_iterations}"]
    }
    let retry_short = pick_uint(attrs, ATTR_WIPHY_RETRY_SHORT)
    let retry_long = pick_uint(attrs, ATTR_WIPHY_RETRY_LONG)
    if retry_short != null and retry_short == retry_long {
      lines += [f"\tRetry short long limit: {retry_short}"]
    } else {
      if retry_short != null {
        lines += [f"\tRetry short limit: {retry_short}"]
      }
      if retry_long != null {
        lines += [f"\tRetry long limit: {retry_long}"]
      }
    }
    let frag = pick_uint(attrs, ATTR_WIPHY_FRAG_THRESHOLD)
    if frag != null {
      lines += [if frag >= 2147483648 { "\tFragmentation threshold: disabled" } else { f"\tFragmentation threshold: {frag}" }]
    }
    let rts = pick_uint(attrs, ATTR_WIPHY_RTS_THRESHOLD)
    if rts != null {
      lines += [if rts >= 2147483648 { "\tRTS threshold: disabled" } else { f"\tRTS threshold: {rts}" }]
    }
    let coverage = pick_uint(attrs, ATTR_WIPHY_COVERAGE_CLASS)
    if coverage != null {
      lines += [f"\tCoverage class: {coverage} (up to {450 * coverage}m)"]
    }
    if pick(attrs, ATTR_SUPPORT_IBSS_RSN) != null {
      lines += ["\tDevice supports RSN-IBSS."]
    }
    if pick(attrs, ATTR_SUPPORT_AP_UAPSD) != null {
      lines += ["\tDevice supports AP-side u-APSD."]
    }
    if pick(attrs, ATTR_TDLS_SUPPORT) != null {
      lines += ["\tDevice supports T-DLS."]
    }
    let ciphers = pick(attrs, ATTR_CIPHER_SUITES)
    if ciphers != null {
      lines += ["\tSupported Ciphers:"]
      var at = 0
      while at + 4 <= ciphers.len() {
        let selector = ciphers.slice(at, 4)
        lines += [f"\t\t* {cipher_name(selector)}"]
        at += 4
      }
    }
    let avail_tx = pick_uint(attrs, ATTR_WIPHY_ANTENNA_AVAIL_TX)
    let avail_rx = pick_uint(attrs, ATTR_WIPHY_ANTENNA_AVAIL_RX)
    if avail_tx != null or avail_rx != null {
      lines += [f"\tAvailable Antennas: TX {c_hex(avail_tx ?? 0)} RX {c_hex(avail_rx ?? 0)}"]
    }
    let config_tx = pick_uint(attrs, ATTR_WIPHY_ANTENNA_TX)
    let config_rx = pick_uint(attrs, ATTR_WIPHY_ANTENNA_RX)
    if config_tx != null or config_rx != null {
      lines += [f"\tConfigured Antennas: TX {c_hex(config_tx ?? 0)} RX {c_hex(config_rx ?? 0)}"]
    }
    let modes = pick(attrs, ATTR_SUPPORTED_IFTYPES)
    if modes != null {
      lines += ["\tSupported interface modes:"]
      for mode in parse_attrs(modes, 0) {
        lines += [f"\t\t * {iftype_name(mode.kind)}"]
      }
    }
    let bands = pick(attrs, ATTR_WIPHY_BANDS)
    if bands != null {
      for band in parse_attrs(bands, 0) {
        lines += band_lines(band.kind, band.data)
        band_index += 1
      }
    }
    let commands = pick(attrs, ATTR_SUPPORTED_COMMANDS)
    if commands != null {
      lines += ["\tSupported commands:"]
      for command in parse_attrs(commands, 0) {
        let number = uint(command.data)
        let command_name = list_name(COMMAND_NAMES, number, "")
        lines += [if command_name == "" { f"\t\t * Unknown command ({number})" } else { f"\t\t * {command_name}" }]
      }
    }
  }
  lines
}

# --- events ------------------------------------------------------------

# The frequencies of a scan event, each after a space.
pure frequency_list(attrs: List[Attr]) -> Str {
  let freqs = pick(attrs, ATTR_SCAN_FREQUENCIES)
  if freqs == null {
    return ""
  }
  var text = ""
  for item in parse_attrs(freqs, 0) {
    text += f" {uint(item.data)}"
  }
  text
}

# The SSIDs of a scan event, each quoted after a space.
pure ssid_list(attrs: List[Attr]) -> Str {
  let ssids = pick(attrs, ATTR_SCAN_SSIDS)
  if ssids == null {
    return ""
  }
  var text = ""
  for item in parse_attrs(ssids, 0) {
    text += " \"" + ssid_text(item.data) + "\""
  }
  text
}

pure reg_initiator_text(kind: Int) -> Str {
  match kind {
    0 => "the wireless core upon initialization"
    1 => "a user"
    2 => "a driver"
    3 => "a country IE"
    else => "unknown source (upgrade this utility)"
  }
}

const DISCONNECT_REASONS = [
  "<unknown>", "Unspecified", "Previous authentication no longer valid",
  "Deauthenticated because sending station is leaving (or has left) the IBSS or ESS",
  "Disassociated due to inactivity", "Disassociated because AP is unable to handle all currently associated STA",
  "Class 2 frame received from non-authenticated station", "Class 3 frame received from non-authenticated station",
  "Disassociated because sending station is leaving (or has left) the BSS",
  "Station requesting (re)association is not authenticated with responding station",
  "Disassociated because the information in the Power Capability element is unacceptable",
  "Disassociated because the information in the Supported Channels element is unacceptable", "<unknown>",
  "Invalid information element", "MIC failure", "4-way handshake timeout", "Group key update timeout",
  "Information element in 4-way handshake different from (Re-)associate request/Probe response/Beacon",
  "Multicast cipher is not valid", "Unicast cipher is not valid", "AKMP is not valid", "Unsupported RSNE version",
  "Invalid RSNE capabilities", "IEEE 802.1X authentication failed", "Cipher Suite rejected per security policy",
]

# The `iw` wording of a deauthentication or disassociation reason code, or
# `<unknown>` for a code its table does not name.
pure reason_text(code: Int) -> Str {
  if code >= 0 and code < DISCONNECT_REASONS.len() { DISCONNECT_REASONS[code] } else { "<unknown>" }
}

const STATUS_TEXTS = [
  {code: 0, text: "Successful"},
  {code: 1, text: "Unspecified failure"},
  {code: 10, text: "Cannot support all requested capabilities in the capability information field"},
  {code: 11, text: "Reassociation denied due to inability to confirm that association exists"},
  {code: 12, text: "Association denied due to reason outside the scope of this standard"},
  {code: 13, text: "Responding station does not support the specified authentication algorithm"},
  {code: 14, text: "Received an authentication frame with authentication transaction sequence number out of expected sequence"},
  {code: 15, text: "Authentication rejected because of challenge failure"},
  {code: 16, text: "Authentication rejected due to timeout waiting for next frame in sequence"},
  {code: 17, text: "Association denied because AP is unable to handle additional associated STA"},
  {code: 18, text: "Association denied due to requesting station not supporting all of the data rates in the BSSBasicRateSet parameter"},
  {code: 19, text: "Association denied due to requesting station not supporting the short preamble option"},
  {code: 20, text: "Association denied due to requesting station not supporting the PBCC modulation option"},
  {code: 21, text: "Association denied due to requesting station not supporting the channel agility option"},
  {code: 22, text: "Association request rejected because Spectrum Management capability is required"},
  {code: 23, text: "Association request rejected because the information in the Power Capability element is unacceptable"},
  {code: 24, text: "Association request rejected because the information in the Supported Channels element is unacceptable"},
  {code: 25, text: "Association request rejected due to requesting station not supporting the short slot time option"},
  {code: 26, text: "Association request rejected due to requesting station not supporting the ER-PBCC modulation option"},
  {code: 27, text: "Association denied due to requesting STA not supporting HT features"},
  {code: 28, text: "R0KH Unreachable"},
  {code: 29, text: "Association denied because the requesting STA does not support the PCO transition required by the AP"},
  {code: 30, text: "Association request rejected temporarily; try again later"},
  {code: 31, text: "Robust Management frame policy violation"},
  {code: 32, text: "Unspecified, QoS related failure"},
  {code: 33, text: "Association denied due to QAP having insufficient bandwidth to handle another QSTA"},
  {code: 34, text: "Association denied due to poor channel conditions"},
  {code: 35, text: "Association (with QBSS) denied due to requesting station not supporting the QoS facility"},
]

# The `iw` wording of an association status code, or `<unknown>`.
pure status_text(code: Int) -> Str {
  for item in STATUS_TEXTS {
    if item.code == code {
      return item.text
    }
  }
  "<unknown>"
}

# The `type NAME` words of an interface event.
pure event_iftype(kind: Int) -> Str {
  match kind {
    1 => "adhoc"
    2 => "station"
    3 => "access point"
    4 => "AP-VLAN"
    5 => "WDS"
    6 => "monitor"
    7 => "mesh point"
    8 => "P2P-client"
    9 => "P2P-GO"
    10 => "P2P-Device"
    else => f"unknown ({kind})"
  }
}

## One line for a multicast event. `names` maps interface indexes to names.
## A command this decoder does not describe is reported by number and name,
## which is how `iw` reports an event it has no decoder for.
export pure event_line(message: Message, names: Map[Str]) -> Str {
  let attrs = message.attrs
  var prefix = ""
  let index = pick_uint(attrs, ATTR_IFINDEX)
  let phy = pick_uint(attrs, ATTR_WIPHY)
  let wdev = pick_uint(attrs, ATTR_WDEV)
  let name = if index != null { names.get(f"{index}") ?? f"if{index}" } else { "" }
  if index != null and phy != null {
    prefix = f"{name} (phy #{phy}): "
  } else if wdev != null and phy != null {
    prefix = f"wdev 0x{hex(wdev)} (phy #{phy}): "
  } else if index != null {
    prefix = f"{name}: "
  } else if wdev != null {
    prefix = f"wdev 0x{hex(wdev)}: "
  } else if phy != null {
    prefix = f"phy #{phy}: "
  }
  let mac = pick(attrs, ATTR_MAC)
  let target = if mac != null { mac_text(mac) } else { "" }
  let reason = pick_uint(attrs, ATTR_REASON_CODE)
  # Command numbers: 6 set_interface, 7 new_interface, 8 del_interface,
  # 19 new_station, 20 del_station, 33 trigger_scan, 34 new_scan_results,
  # 35 scan_aborted, 36 reg_change, 43 join_ibss, 46 connect, 47 roam,
  # 48 disconnect, 113 wiphy_reg_change.
  match message.cmd {
    33 => prefix + "scan started"
    34 => prefix + "scan finished:" + frequency_list(attrs) + "," + ssid_list(attrs)
    35 => prefix + "scan aborted:" + frequency_list(attrs) + "," + ssid_list(attrs)
    36 | 113 => {
      var text = prefix + (if message.cmd == 36 { "regulatory domain change: " } else { "regulatory domain change (phy): " })
      let initiator = reg_initiator_text(pick_uint(attrs, ATTR_REG_INITIATOR) ?? 0)
      let alpha2 = pick(attrs, ATTR_REG_ALPHA2)
      let code = if alpha2 != null { cstring(alpha2) } else { "??" }
      match pick_uint(attrs, ATTR_REG_TYPE) ?? 0 {
        0 => {
          text += f"set to {code} by {initiator} request"
          if phy != null {
            text += f" on phy{phy}"
          }
        }
        1 => text += f"set to world roaming by {initiator} request"
        2 => text += f"custom world roaming rules in place on phy{phy ?? 0} by {initiator} request"
        3 => text += f"intersection used due to a request made by {initiator}"
        else => text += "unknown regulatory domain change"
      }
      text
    }
    6 | 7 | 8 => {
      var text = prefix + (if message.cmd == 7 { "new" } else if message.cmd == 8 { "del" } else { "set" }) + " interface"
      let kind = pick_uint(attrs, ATTR_IFTYPE)
      if kind != null {
        text += " type " + event_iftype(kind)
      }
      let mesh_id = pick(attrs, ATTR_MESH_ID)
      if mesh_id != null {
        text += " meshid " + ssid_text(mesh_id)
      }
      let wds = pick_uint(attrs, ATTR_4ADDR)
      if wds != null {
        text += f" use 4addr {wds}"
      }
      text
    }
    19 => prefix + f"new station {target}"
    20 => prefix + f"del station {target}"
    43 => prefix + f"IBSS {target} joined"
    46 => {
      let status = pick_uint(attrs, ATTR_STATUS_CODE)
      var text = prefix + (if status == null { "unknown connect status" } else if status == 0 { "connected" } else { "failed to connect" })
      if mac != null {
        text += f" to {target}"
      }
      if status != null and status > 0 {
        text += f", status: {status}: {status_text(status)}"
      }
      text
    }
    47 => prefix + "roamed" + (if mac != null { f" to {target}" } else { "" })
    48 => {
      var text = prefix + "disconnected"
      text += if pick(attrs, ATTR_DISCONNECTED_BY_AP) != null { " (by AP)" } else { " (local request)" }
      if reason != null {
        text += f" reason: {reason}: {reason_text(reason)}"
      }
      text
    }
    else => {
      let known = list_name(COMMAND_NAMES, message.cmd, "")
      let label = if known == "" { f"Unknown command ({message.cmd})" } else { known }
      prefix + f"unknown event {message.cmd} ({label})"
    }
  }
}

# --- requests ----------------------------------------------------------

## The attributes that start a scan: SSIDs (a wildcard when none), frequencies,
## extra elements, the duration, and the scan flags.
export type ScanRequest = {
  ssids: List[Str],
  freqs: List[Int],
  elements: Bytes,
  duration: Int?,
  mandatory: Bool,
  flags: Int,
  passive: Bool,
}

## The attributes of a TRIGGER_SCAN request for interface `index`.
export proc scan_attrs(index: Int, scan: ScanRequest) [error] -> Result[List[Bytes], Error] {
  var attrs: List[Bytes] = [attr_u32(ATTR_IFINDEX, index)?]
  if ! scan.passive {
    var entries: List[Bytes] = []
    var number = 0
    if scan.ssids.is_empty() {
      entries += [attr(number, b"")?]
    } else {
      for ssid in scan.ssids {
        entries += [attr(number, bytes.from_text(ssid))?]
        number += 1
      }
    }
    attrs += [attr_nested(ATTR_SCAN_SSIDS, entries)?]
  }
  if ! scan.freqs.is_empty() {
    var entries: List[Bytes] = []
    var number = 0
    for freq in scan.freqs {
      entries += [attr_u32(number, freq)?]
      number += 1
    }
    attrs += [attr_nested(ATTR_SCAN_FREQUENCIES, entries)?]
  }
  if ! scan.elements.is_empty() {
    attrs += [attr(ATTR_IE, scan.elements)?]
  }
  if scan.duration != null {
    attrs += [attr_u16(ATTR_MEASUREMENT_DURATION, scan.duration)?]
  }
  if scan.mandatory {
    attrs += [attr_flag(ATTR_MEASUREMENT_DURATION_MANDATORY)?]
  }
  if scan.flags != 0 {
    attrs += [attr_u32(ATTR_SCAN_FLAGS, scan.flags)?]
  }
  attrs
}

## The nl80211 command numbers the applet sends and recognises.
export const CMD = {
  get_wiphy: CMD_GET_WIPHY,
  set_wiphy: CMD_SET_WIPHY,
  get_interface: CMD_GET_INTERFACE,
  set_interface: CMD_SET_INTERFACE,
  get_station: CMD_GET_STATION,
  req_set_reg: CMD_REQ_SET_REG,
  get_reg: CMD_GET_REG,
  get_scan: CMD_GET_SCAN,
  trigger_scan: CMD_TRIGGER_SCAN,
  new_scan_results: CMD_NEW_SCAN_RESULTS,
  scan_aborted: CMD_SCAN_ABORTED,
  abort_scan: CMD_ABORT_SCAN,
  reload_regdb: CMD_RELOAD_REGDB,
}

## Attribute numbers the applet needs to build requests.
export const ATTRS = {
  ifindex: ATTR_IFINDEX,
  wiphy: ATTR_WIPHY,
  iftype: ATTR_IFTYPE,
  mac: ATTR_MAC,
  reg_alpha2: ATTR_REG_ALPHA2,
  freq: ATTR_WIPHY_FREQ,
  channel_type: ATTR_WIPHY_CHANNEL_TYPE,
  width: ATTR_CHANNEL_WIDTH,
  center1: ATTR_CENTER_FREQ1,
  tx_power_setting: ATTR_WIPHY_TX_POWER_SETTING,
  tx_power_level: ATTR_WIPHY_TX_POWER_LEVEL,
  split: ATTR_SPLIT_WIPHY_DUMP,
}

## Scan flag bits for the `lowpri`, `flush`, `ap-force`, and `coloc` words.
export const SCAN_FLAGS = {
  lowpri: SCAN_FLAG_LOW_PRIORITY,
  flush: SCAN_FLAG_FLUSH,
  ap_force: SCAN_FLAG_AP,
  coloc: SCAN_FLAG_COLOCATED_6GHZ,
}

## The transmit-power setting numbers of SET_WIPHY.
export const TX_POWER = {auto: TX_POWER_AUTOMATIC, limit: TX_POWER_LIMITED, fixed: TX_POWER_FIXED}

## The channel-width numbers of SET_WIPHY.
export const WIDTHS = {noht: WIDTH_20_NOHT, w20: WIDTH_20, w40: WIDTH_40}
