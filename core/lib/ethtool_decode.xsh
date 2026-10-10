##! ethtool's SIOCETHTOOL request layouts, reply decoders and report text.
##! Everything here is pure so the layouts can be tested without a network
##! interface; the applet owns every system call and every exit status.

## Command numbers of the ethtool ioctl (the kernel's `ETHTOOL_*` values).
export const CMD = {
  GSET: 1, SSET: 2, GDRVINFO: 3, GWOL: 5, SWOL: 6, GMSGLVL: 7, SMSGLVL: 8,
  GLINK: 10, GCOALESCE: 14, SCOALESCE: 15, GRINGPARAM: 16, SRINGPARAM: 17,
  GPAUSEPARAM: 18, SPAUSEPARAM: 19, GSTRINGS: 27, GSTATS: 29, GPERMADDR: 32,
  GFEATURES: 58, SFEATURES: 59, GCHANNELS: 60, SCHANNELS: 61, GSSET_INFO: 55,
  GET_TS_INFO: 65, GEEE: 68, SEEE: 69, GLINKSETTINGS: 76, SLINKSETTINGS: 77,
}

## The statistics string set (`ETH_SS_STATS`).
export const SS_STATS = 1
## The device features string set (`ETH_SS_FEATURES`).
export const SS_FEATURES = 4

const POW2 = [
  1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768,
  65536, 131072, 262144, 524288, 1048576, 2097152, 4194304, 8388608, 16777216,
  33554432, 67108864, 134217728, 268435456, 536870912, 1073741824, 2147483648,
]

## Bit `index` of a 32-bit word.
export pure bit(word: Int, index: Int) -> Bool {
  word / POW2[index] % 2 == 1
}

## Hex digits of `value`, zero-padded to `width`.
export pure hex(value: Int, width: Int) -> Str {
  var number = value
  var text = ""
  while number > 0 { text = "0123456789abcdef".byte_slice(number % 16, 1) + text; number /= 16 }
  while text.byte_len() < width { text = "0" + text }
  text
}

## A request made of a command word followed by 32-bit words.
export pure words(command: Int, values: List[Int]) -> Result[Bytes, Error] {
  var chunks: List[Bytes] = [bytes.pack_le(command, 4)?]
  for value in values { chunks += [bytes.pack_le(value, 4)?] }
  Ok(bytes.concat(chunks))
}

## The 32-bit words of a reply, starting after the command word.
export pure unwords(data: Bytes, count: Int) -> Result[List[Int], Error] {
  var found: List[Int] = []
  for index in range(count) { found += [bytes.unpack_le(data, 4, 4 + index * 4)?] }
  Ok(found)
}

## A NUL-terminated string field of `length` bytes at `at`.
export pure text_field(data: Bytes, at: Int, length: Int) -> Result[Str, Error] {
  let raw = data.slice(at, length)
  var end = 0
  while end < length and (raw.byte_at(end) ?? 0) != 0 { end += 1 }
  raw.slice(0, end).utf8()
}

## Kernel strings of a string set: `count` fixed 32-byte entries after the
## three-word `ethtool_gstrings` header.
export pure string_set(data: Bytes, count: Int) -> Result[List[Str], Error] {
  var found: List[Str] = []
  for index in range(count) { found += [text_field(data, 12 + index * 32, 32)?] }
  Ok(found)
}

## `ethtool_sset_info` asking for one string set's length.
export pure sset_request(string_kind: Int) -> Result[Bytes, Error] {
  Ok(bytes.concat([bytes.pack_le(CMD.GSSET_INFO, 4)?, bytes.zero(4)?, bytes.pack_le(POW2[string_kind], 8)?, bytes.zero(4)?]))
}

## The length of the set `sset_request` asked for; 0 when the driver does not
## have it, which is how the kernel reports an unsupported set.
export pure sset_length(reply: Bytes, string_kind: Int) -> Result[Int, Error] {
  let supported = bytes.unpack_le(reply, 8, 8)?
  return Ok(0) when supported / POW2[string_kind] % 2 != 1

  Ok(bytes.unpack_le(reply, 4, 16)?)
}

## `ethtool_gstrings` for `count` strings of one set.
export pure strings_request(string_kind: Int, count: Int) -> Result[Bytes, Error] {
  Ok(bytes.concat([bytes.pack_le(CMD.GSTRINGS, 4)?, bytes.pack_le(string_kind, 4)?, bytes.pack_le(count, 4)?, bytes.zero(count * 32)?]))
}

## `ethtool_stats` with room for `count` counters.
export pure stats_request(count: Int) -> Result[Bytes, Error] {
  Ok(bytes.concat([bytes.pack_le(CMD.GSTATS, 4)?, bytes.pack_le(count, 4)?, bytes.zero(count * 8)?]))
}

## One `NIC statistics` row: the name indented five spaces, then the value.
export pure stats_lines(names: List[Str], reply: Bytes) -> Result[List[Str], Error] {
  var lines: List[Str] = ["NIC statistics:"]
  for index in range(names.len()) {
    lines += [f"     {names[index]}: {bytes.unpack_le(reply, 8, 8 + index * 8)?}"]
  }
  Ok(lines)
}

## One device feature as the kernel names it. Names the kernel leaves blank are
## unused bit positions and never become features.
export type Feature = {index: Int, name: Str, requested: Bool, active: Bool, fixed: Bool}

## `ethtool_gfeatures` with room for `count` features.
export pure features_request(count: Int) -> Result[Bytes, Error] {
  let blocks = (count + 31) / 32
  Ok(bytes.concat([bytes.pack_le(CMD.GFEATURES, 4)?, bytes.pack_le(blocks, 4)?, bytes.zero(blocks * 16)?]))
}

## The feature table a `GFEATURES` reply and the kernel's names describe. Each
## block holds the available, requested, active, and never-changed words; this
## reads the first three.
export pure features_decode(names: List[Str], reply: Bytes) -> Result[List[Feature], Error] {
  var found: List[Feature] = []
  for index in range(names.len()) {
    continue when names[index] == ""

    let block = 8 + index / 32 * 16
    let bit_index = index % 32
    let available = bytes.unpack_le(reply, 4, block)?
    let requested = bytes.unpack_le(reply, 4, block + 4)?
    let active = bytes.unpack_le(reply, 4, block + 8)?
    # A feature the device does not make available cannot be changed, which is
    # what `[fixed]` reports.
    found += [{index: index, name: names[index], requested: bit(requested, bit_index), active: bit(active, bit_index), fixed: ! bit(available, bit_index)}]
  }
  Ok(found)
}

## `ethtool_sfeatures` that sets the requested value of every feature in
## `wanted` (index to value) and leaves the others alone.
export pure features_set_request(count: Int, wanted: Map[Int, Bool]) -> Result[Bytes, Error] {
  let blocks = (count + 31) / 32
  var chunks: List[Bytes] = [bytes.pack_le(CMD.SFEATURES, 4)?, bytes.pack_le(blocks, 4)?]
  for block in range(blocks) {
    var valid_word = 0
    var requested_word = 0
    for bit_index in range(32) {
      let index = block * 32 + bit_index
      if let Ok(value) = wanted.get(index) {
        valid_word += POW2[bit_index]
        requested_word += if value { POW2[bit_index] } else { 0 }
      }
    }
    chunks += [bytes.pack_le(valid_word, 4)?, bytes.pack_le(requested_word, 4)?]
  }
  Ok(bytes.concat(chunks))
}

# One offload flag as `ethtool -K` names it: the short word, the long name
# `-k` prints, and which kernel features it covers.
type Flag = {short: Str, long: Str, exact: Str, prefix: Str, suffix: Str}

const FLAGS: List[Flag] = [
  {short: "rx", long: "rx-checksumming", exact: "rx-checksum", prefix: "", suffix: ""},
  {short: "tx", long: "tx-checksumming", exact: "", prefix: "tx-checksum-", suffix: ""},
  {short: "sg", long: "scatter-gather", exact: "", prefix: "tx-scatter-gather", suffix: ""},
  {short: "tso", long: "tcp-segmentation-offload", exact: "", prefix: "tx-tcp", suffix: "-segmentation"},
  {short: "ufo", long: "udp-fragmentation-offload", exact: "tx-udp-fragmentation", prefix: "", suffix: ""},
  {short: "gso", long: "generic-segmentation-offload", exact: "tx-generic-segmentation", prefix: "", suffix: ""},
  {short: "gro", long: "generic-receive-offload", exact: "rx-gro", prefix: "", suffix: ""},
  {short: "lro", long: "large-receive-offload", exact: "rx-lro", prefix: "", suffix: ""},
  {short: "rxvlan", long: "rx-vlan-offload", exact: "rx-vlan-hw-parse", prefix: "", suffix: ""},
  {short: "txvlan", long: "tx-vlan-offload", exact: "tx-vlan-hw-insert", prefix: "", suffix: ""},
  {short: "ntuple", long: "ntuple-filters", exact: "rx-ntuple-filter", prefix: "", suffix: ""},
  {short: "rxhash", long: "receive-hashing", exact: "rx-hashing", prefix: "", suffix: ""},
]

pure flag_covers(flag: Flag, name: Str) -> Bool {
  return name == flag.exact when flag.exact != ""

  name.starts_with(flag.prefix) and name.ends_with(flag.suffix) and name.byte_len() >= flag.prefix.byte_len() + flag.suffix.byte_len()
}

# The flag a kernel feature belongs to, or -1.
pure flag_of(name: Str) -> Int {
  for index in range(FLAGS.len()) {
    return index when flag_covers(FLAGS[index], name)
  }

  -1
}

## The kernel feature names a word on a `-K` command line stands for: the
## feature itself, or every feature of an offload flag named by its short or
## long name. A flag the kernel has no feature for expands to nothing.
export pure feature_names(word: Str, known: List[Str]) -> List[Str] {
  if word in known { return [word] }

  for index in range(FLAGS.len()) {
    if FLAGS[index].short == word or FLAGS[index].long == word {
      return [name for name in known if name != "" and flag_covers(FLAGS[index], name)]
    }
  }

  []
}

## Whether `word` names an offload flag, even one this kernel has no features for.
export pure is_flag(word: Str) -> Bool {
  ! [flag for flag in FLAGS if flag.short == word or flag.long == word].is_empty()
}

pure on_off(value: Bool) -> Str { if value { "on" } else { "off" } }

# A boolean field of a set request: the new value when given, else the
# word the kernel reported.
pure flag_word(value: Bool?, current: Int) -> Int {
  if value == null { current } else if value { 1 } else { 0 }
}

pure asked_value(wanted: Map[Int, Bool], index: Int) -> Bool? {
  if let Ok(value) = wanted.get(index) { return value }

  null
}

pure feature_line(indent: Str, name: Str, feature: Feature) -> Str {
  let note = if feature.fixed {
    " [fixed]"
  } else if feature.requested != feature.active {
    if feature.requested { " [requested on]" } else { " [requested off]" }
  } else {
    ""
  }
  f"{indent}{name}: {on_off(feature.active)}{note}"
}

## The `ethtool -k` report: offload flags first, in ethtool's order, each
## followed by the kernel features it covers; a flag that covers one feature
## is that feature under the flag's name. The features no flag covers follow
## in kernel order.
export pure features_lines(device: Str, features: List[Feature]) -> List[Str] {
  var lines: List[Str] = [f"Features for {device}:"]
  for index in range(FLAGS.len()) {
    let members = [feature for feature in features if flag_of(feature.name) == index]
    continue when members.is_empty()

    if members.len() == 1 {
      lines += [feature_line("", FLAGS[index].long, members[0])]
    } else {
      let any_active = ! [m for m in members if m.active].is_empty()
      lines += [f"{FLAGS[index].long}: {on_off(any_active)}"]
      for member in members { lines += [feature_line("\t", member.name, member)] }
    }
  }
  for feature in features {
    lines += [feature_line("", feature.name, feature)] when flag_of(feature.name) < 0
  }
  lines
}

## The `ethtool --json -k` document: the same order as `features_lines`.
export pure features_json(device: Str, features: List[Feature]) -> Str {
  var entries: List[Str] = [f"        \"ifname\": \"{device}\""]
  for index in range(FLAGS.len()) {
    let members = [feature for feature in features if flag_of(feature.name) == index]
    continue when members.is_empty()

    if members.len() == 1 {
      entries += [json_feature(FLAGS[index].long, members[0].active, members[0].fixed, members[0].requested)]
    } else {
      let any_active = ! [m for m in members if m.active].is_empty()
      entries += [f"        \"{FLAGS[index].long}\": {{\n            \"active\": {any_active},\n            \"fixed\": null,\n            \"requested\": null\n        }}"]
      for member in members { entries += [json_feature(member.name, member.active, member.fixed, member.requested)] }
    }
  }
  for feature in features {
    entries += [json_feature(feature.name, feature.active, feature.fixed, feature.requested)] when flag_of(feature.name) < 0
  }
  "[ {\n" + entries.join(",\n") + "\n    } ]"
}

pure json_feature(name: Str, active: Bool, fixed: Bool, requested: Bool) -> Str {
  f"        \"{name}\": {{\n            \"active\": {active},\n            \"fixed\": {fixed},\n            \"requested\": {requested}\n        }}"
}

## What `ethtool -K` prints after a request: the features whose active state
## changed, plus the requested ones that did not take. A feature the request
## did not name is `[not requested]`; one it did name that is not in the
## requested state shows the state it was asked for. `lines` is empty when
## everything went as asked, `changed` says whether any active state moved, and
## `unsatisfied` whether a request for an unchangeable feature was refused.
export type Outcome = {lines: List[Str], changed: Bool, unsatisfied: Bool}

## Compares feature states before and after a `-K` request; see `Outcome`.
export pure features_outcome(before: List[Feature], after: List[Feature], wanted: Map[Int, Bool]) -> Outcome {
  var lines: List[Str] = []
  var changed = false
  var unsatisfied = false
  var noted = false
  for index in range(after.len()) {
    let now = after[index]
    let was = before[index].active
    let asked = asked_value(wanted, now.index)
    let moved = was != now.active
    let missed = asked != null and asked != now.active
    changed = true when moved
    # Only a refused fixed feature is an error; a feature that is wanted but
    # held off by a dependency is reported and left wanted.
    unsatisfied = true when missed and now.fixed
    continue when ! moved and ! missed

    var note = ""
    if asked == null {
      note = " [not requested]"
    } else if missed {
      note = if asked { " [requested on]" } else { " [requested off]" }
    }
    noted = true when note != ""
    lines += [f"{now.name}: {on_off(now.active)}{note}"]
  }
  {lines: if noted { lines } else { [] }, changed: changed, unsatisfied: unsatisfied}
}

## Driver information, as `ethtool_drvinfo` returns it.
export type Driver = {driver: Str, version: Str, firmware: Str, rom: Str, bus: Str, priv_flags: Int, stats: Int, tests: Int, eeprom: Int, registers: Int}

## `ethtool_drvinfo` request buffer.
export pure drvinfo_request() -> Result[Bytes, Error] {
  Ok(bytes.concat([bytes.pack_le(CMD.GDRVINFO, 4)?, bytes.zero(192)?]))
}

## Decodes an `ethtool_drvinfo` reply.
export pure drvinfo_decode(reply: Bytes) -> Result[Driver, Error] {
  Ok({
    driver: text_field(reply, 4, 32)?, version: text_field(reply, 36, 32)?, firmware: text_field(reply, 68, 32)?,
    bus: text_field(reply, 100, 32)?, rom: text_field(reply, 132, 32)?,
    priv_flags: bytes.unpack_le(reply, 4, 176)?, stats: bytes.unpack_le(reply, 4, 180)?, tests: bytes.unpack_le(reply, 4, 184)?,
    eeprom: bytes.unpack_le(reply, 4, 188)?, registers: bytes.unpack_le(reply, 4, 192)?,
  })
}

pure yes_no(value: Int) -> Str { if value != 0 { "yes" } else { "no" } }

## The `ethtool -i` report.
export pure drvinfo_lines(info: Driver) -> List[Str] {
  [
    f"driver: {info.driver}", f"version: {info.version}", f"firmware-version: {info.firmware}",
    f"expansion-rom-version: {info.rom}", f"bus-info: {info.bus}",
    f"supports-statistics: {yes_no(info.stats)}", f"supports-test: {yes_no(info.tests)}",
    f"supports-eeprom-access: {yes_no(info.eeprom)}", f"supports-register-dump: {yes_no(info.registers)}",
    f"supports-priv-flags: {yes_no(info.priv_flags)}",
  ]
}

## `Permanent address:` for an `ethtool_perm_addr` reply; an all-zero address
## is reported as not set.
export pure permaddr_line(reply: Bytes) -> Result[Str, Error] {
  let size = bytes.unpack_le(reply, 4, 4)?
  var parts: List[Str] = []
  var any = false
  for index in range(size) {
    let value = reply.byte_at(8 + index) ?? 0
    any = true when value != 0
    parts += [hex(value, 2)]
  }
  return Ok("Permanent address: not set") when ! any

  Ok(f"Permanent address: {parts.join(":")}")
}

# A reply field that is zero when the driver has nothing to report prints
# `n/a`, as the netlink-served commands do.
pure or_na(value: Int) -> Str { if value == 0 { "n/a" } else { f"{value}" } }

## The kernel's reason a channel request was refused with EINVAL, with the byte
## offset of the offending attribute in the request message, rebuilt from the
## attribute layout (counts are 8 bytes each, in rx, tx, other, combined order,
## after the header that carries the device name). Null when none of the
## limits explains it.
export pure channels_refusal(current: Bytes, requested: List[Int?], name_length: Int) -> Result[Str?, Error] {
  let v = unwords(current, 8)?
  var counts: List[Int] = []
  var offsets: List[Int] = []
  var offset = 24 + (name_length + 4) / 4 * 4 + 4
  for index in range(4) {
    counts += [requested[index] ?? v[4 + index]]
    offsets += [offset]
    offset += 8 when requested[index] != null
  }
  for index in range(4) {
    if requested[index] != null and counts[index] > v[index] {
      return Ok(f"requested channel count exceeds maximum (offset {offsets[index]})")
    }
  }
  if requested[0] != null and counts[3] == 0 and counts[0] == 0 {
    return Ok(f"requested channel counts would result in no RX or TX channel being configured (offset {offsets[0]})")
  }
  if requested[1] != null and counts[3] == 0 and counts[1] == 0 {
    return Ok(f"requested channel counts would result in no RX or TX channel being configured (offset {offsets[1]})")
  }
  Ok(null)
}

## The `ethtool -l` report from an `ethtool_channels` reply.
export pure channels_lines(device: Str, reply: Bytes) -> Result[List[Str], Error] {
  let v = unwords(reply, 8)?
  Ok([
    f"Channel parameters for {device}:", "Pre-set maximums:",
    f"RX:\t\t{or_na(v[0])}", f"TX:\t\t{or_na(v[1])}", f"Other:\t\t{or_na(v[2])}", f"Combined:\t{or_na(v[3])}",
    "Current hardware settings:",
    f"RX:\t\t{or_na(v[4])}", f"TX:\t\t{or_na(v[5])}", f"Other:\t\t{or_na(v[6])}", f"Combined:\t{or_na(v[7])}",
  ])
}

## `ethtool_channels` with the counts changed to `requested` (rx, tx, other,
## combined; null keeps the current value).
export pure channels_set_request(reply: Bytes, requested: List[Int?]) -> Result[Bytes, Error] {
  let v = unwords(reply, 8)?
  var counts: List[Int] = v[0..4]
  for index in range(4) { counts += [requested[index] ?? v[4 + index]] }
  words(CMD.SCHANNELS, counts)
}

## The `ethtool -g` report from an `ethtool_ringparam` reply.
export pure ring_lines(device: Str, reply: Bytes) -> Result[List[Str], Error] {
  let v = unwords(reply, 8)?
  Ok([
    f"Ring parameters for {device}:", "Pre-set maximums:",
    f"RX:\t\t\t{or_na(v[0])}", f"RX Mini:\t\t{or_na(v[1])}", f"RX Jumbo:\t\t{or_na(v[2])}", f"TX:\t\t\t{or_na(v[3])}",
    "Current hardware settings:",
    f"RX:\t\t\t{or_na(v[4])}", f"RX Mini:\t\t{or_na(v[5])}", f"RX Jumbo:\t\t{or_na(v[6])}", f"TX:\t\t\t{or_na(v[7])}",
    "",
  ])
}

## `ethtool_ringparam` with the pending counts changed (rx, rx-mini, rx-jumbo, tx).
export pure ring_set_request(reply: Bytes, requested: List[Int?]) -> Result[Bytes, Error] {
  let v = unwords(reply, 8)?
  var counts: List[Int] = v[0..4]
  for index in range(4) { counts += [requested[index] ?? v[4 + index]] }
  words(CMD.SRINGPARAM, counts)
}

## Coalescing fields in `ethtool_coalesce` order.
export const COALESCE_FIELDS = [
  "rx-usecs", "rx-frames", "rx-usecs-irq", "rx-frames-irq", "tx-usecs", "tx-frames", "tx-usecs-irq", "tx-frames-irq",
  "stats-block-usecs", "adaptive-rx", "adaptive-tx", "pkt-rate-low", "rx-usecs-low", "rx-frames-low", "tx-usecs-low",
  "tx-frames-low", "pkt-rate-high", "rx-usecs-high", "rx-frames-high", "tx-usecs-high", "tx-frames-high", "sample-interval",
]

## The values are the driver's, with no way to tell an unsupported field from
## a zero one through this interface, so none is reported as `n/a`.
export pure coalesce_lines(device: Str, reply: Bytes) -> Result[List[Str], Error] {
  let v = unwords(reply, 22)?
  Ok([
    f"Coalesce parameters for {device}:",
    f"Adaptive RX: {on_off(v[9] != 0)}  TX: {on_off(v[10] != 0)}",
    f"stats-block-usecs:\t{v[8]}", f"sample-interval:\t{v[21]}", f"pkt-rate-low:\t\t{v[11]}", f"pkt-rate-high:\t\t{v[16]}", "",
    f"rx-usecs:\t{v[0]}", f"rx-frames:\t{v[1]}", f"rx-usecs-irq:\t{v[2]}", f"rx-frames-irq:\t{v[3]}", "",
    f"tx-usecs:\t{v[4]}", f"tx-frames:\t{v[5]}", f"tx-usecs-irq:\t{v[6]}", f"tx-frames-irq:\t{v[7]}", "",
    f"rx-usecs-low:\t{v[12]}", f"rx-frame-low:\t{v[13]}", f"tx-usecs-low:\t{v[14]}", f"tx-frame-low:\t{v[15]}", "",
    f"rx-usecs-high:\t{v[17]}", f"rx-frame-high:\t{v[18]}", f"tx-usecs-high:\t{v[19]}", f"tx-frame-high:\t{v[20]}", "",
  ])
}

## `ethtool_coalesce` with the fields in `changes` (field position to value) replaced.
export pure coalesce_set_request(reply: Bytes, changes: Map[Int, Int]) -> Result[Bytes, Error] {
  var values = unwords(reply, 22)?
  var next: List[Int] = []
  for index in range(22) { next += [changes.get(index) ?? values[index]] }
  words(CMD.SCOALESCE, next)
}

## The `ethtool -a` report from an `ethtool_pauseparam` reply.
export pure pause_lines(device: Str, reply: Bytes) -> Result[List[Str], Error] {
  let v = unwords(reply, 3)?
  Ok([
    f"Pause parameters for {device}:",
    f"Autonegotiate:\t{on_off(v[0] != 0)}", f"RX:\t\t{on_off(v[1] != 0)}", f"TX:\t\t{on_off(v[2] != 0)}", "",
  ])
}

## `ethtool_pauseparam` with autoneg, rx and tx replaced where given.
export pure pause_set_request(reply: Bytes, autoneg: Bool?, rx: Bool?, tx: Bool?) -> Result[Bytes, Error] {
  let v = unwords(reply, 3)?
  words(CMD.SPAUSEPARAM, [flag_word(autoneg, v[0]), flag_word(rx, v[1]), flag_word(tx, v[2])])
}

const TS_CAPABILITIES = [
  "hardware-transmit", "software-transmit", "hardware-receive", "software-receive", "software-system-clock",
  "hardware-legacy-clock", "hardware-raw-clock",
]

const TS_TX_TYPES = ["off", "on", "onestep-sync", "onestep-p2p"]
const TS_TX_CONSTANTS = ["HWTSTAMP_TX_OFF", "HWTSTAMP_TX_ON", "HWTSTAMP_TX_ONESTEP_SYNC", "HWTSTAMP_TX_ONESTEP_P2P"]

const TS_FILTERS = [
  "none", "all", "some", "ptpv1-l4-event", "ptpv1-l4-sync", "ptpv1-l4-delay-req", "ptpv2-l4-event", "ptpv2-l4-sync",
  "ptpv2-l4-delay-req", "ptpv2-l2-event", "ptpv2-l2-sync", "ptpv2-l2-delay-req", "ptpv2-event", "ptpv2-sync",
  "ptpv2-delay-req", "ntp-all",
]
const TS_FILTER_CONSTANTS = [
  "HWTSTAMP_FILTER_NONE", "HWTSTAMP_FILTER_ALL", "HWTSTAMP_FILTER_SOME", "HWTSTAMP_FILTER_PTP_V1_L4_EVENT",
  "HWTSTAMP_FILTER_PTP_V1_L4_SYNC", "HWTSTAMP_FILTER_PTP_V1_L4_DELAY_REQ", "HWTSTAMP_FILTER_PTP_V2_L4_EVENT",
  "HWTSTAMP_FILTER_PTP_V2_L4_SYNC", "HWTSTAMP_FILTER_PTP_V2_L4_DELAY_REQ", "HWTSTAMP_FILTER_PTP_V2_L2_EVENT",
  "HWTSTAMP_FILTER_PTP_V2_L2_SYNC", "HWTSTAMP_FILTER_PTP_V2_L2_DELAY_REQ", "HWTSTAMP_FILTER_PTP_V2_EVENT",
  "HWTSTAMP_FILTER_PTP_V2_SYNC", "HWTSTAMP_FILTER_PTP_V2_DELAY_REQ", "HWTSTAMP_FILTER_NTP_ALL",
]

## `ethtool_ts_info` request buffer.
export pure tsinfo_request() -> Result[Bytes, Error] {
  Ok(bytes.concat([bytes.pack_le(CMD.GET_TS_INFO, 4)?, bytes.zero(40)?]))
}

## `ethtool_ts_info`: capability bits, clock index, transmit types, filters.
export pure tsinfo_lines(device: Str, reply: Bytes) -> Result[List[Str], Error] {
  let flags = bytes.unpack_le(reply, 4, 4)?
  let clock = bytes.unpack_le(reply, 4, 8)?
  let tx_types = bytes.unpack_le(reply, 4, 12)?
  let rx_filters = bytes.unpack_le(reply, 4, 28)?
  var lines: List[Str] = [f"Time stamping parameters for {device}:", "Capabilities:"]
  for index in range(TS_CAPABILITIES.len()) {
    lines += [f"\t{TS_CAPABILITIES[index]}"] when bit(flags, index)
  }
  lines += [if clock >= 0 and clock < 2147483648 { f"PTP Hardware Clock: {clock}" } else { "PTP Hardware Clock: none" }]
  lines += ["Hardware Transmit Timestamp Modes:" + (if tx_types == 0 { " none" } else { "" })]
  for index in range(TS_TX_TYPES.len()) {
    lines += [f"\t{TS_TX_TYPES[index]:<22}({TS_TX_CONSTANTS[index]})"] when bit(tx_types, index)
  }
  lines += ["Hardware Receive Filter Modes:" + (if rx_filters == 0 { " none" } else { "" })]
  for index in range(TS_FILTERS.len()) {
    lines += [f"\t{TS_FILTERS[index]:<22}({TS_FILTER_CONSTANTS[index]})"] when bit(rx_filters, index)
  }
  Ok(lines)
}

# Link modes by bit number; a blank entry is a bit that is not a speed mode
# (autonegotiation, port, pause, and FEC bits are decoded separately).
const LINK_MODES = [
  "10baseT/Half", "10baseT/Full", "100baseT/Half", "100baseT/Full", "1000baseT/Half", "1000baseT/Full", "", "", "", "",
  "", "", "10000baseT/Full", "", "", "2500baseX/Full", "", "1000baseKX/Full", "10000baseKX4/Full", "10000baseKR/Full",
  "10000baseR_FEC", "20000baseMLD2/Full", "20000baseKR2/Full", "40000baseKR4/Full", "40000baseCR4/Full",
  "40000baseSR4/Full", "40000baseLR4/Full", "56000baseKR4/Full", "56000baseCR4/Full", "56000baseSR4/Full",
  "56000baseLR4/Full", "25000baseCR/Full", "25000baseKR/Full", "25000baseSR/Full", "50000baseCR2/Full",
  "50000baseKR2/Full", "100000baseKR4/Full", "100000baseSR4/Full", "100000baseCR4/Full", "100000baseLR4_ER4/Full",
  "50000baseSR2/Full", "1000baseX/Full", "10000baseCR/Full", "10000baseSR/Full", "10000baseLR/Full",
  "10000baseLRM/Full", "10000baseER/Full", "2500baseT/Full", "5000baseT/Full", "", "", "", "50000baseKR/Full",
  "50000baseSR/Full", "50000baseCR/Full", "50000baseLR_ER_FR/Full", "50000baseDR/Full", "100000baseKR2/Full",
  "100000baseSR2/Full", "100000baseCR2/Full", "100000baseLR2_ER2_FR2/Full", "100000baseDR2/Full", "200000baseKR4/Full",
  "200000baseSR4/Full", "200000baseLR4_ER4_FR4/Full", "200000baseDR4/Full", "200000baseCR4/Full", "100baseT1/Full",
  "1000baseT1/Full", "400000baseKR8/Full", "400000baseSR8/Full", "400000baseLR8_ER8_FR8/Full", "400000baseDR8/Full",
  "400000baseCR8/Full", "", "100000baseKR/Full", "100000baseSR/Full", "100000baseLR_ER_FR/Full", "100000baseCR/Full",
  "100000baseDR/Full", "200000baseKR2/Full", "200000baseSR2/Full", "200000baseLR2_ER2_FR2/Full", "200000baseDR2/Full",
  "200000baseCR2/Full", "400000baseKR4/Full", "400000baseSR4/Full", "400000baseLR4_ER4_FR4/Full", "400000baseDR4/Full",
  "400000baseCR4/Full", "100baseFX/Half", "100baseFX/Full",
]

pure spaces(count: Int) -> Str {
  var text = ""
  while text.byte_len() < count { text += " " }
  text
}

# The autonegotiation, port, and pause link-mode bits.
const LM_AUTONEG = 6
const LM_PORTS = [{bit: 7, name: "TP"}, {bit: 8, name: "AUI"}, {bit: 9, name: "MII"}, {bit: 10, name: "FIBRE"}, {bit: 11, name: "BNC"}, {bit: 16, name: "Backplane"}]
const LM_PAUSE = 13
const LM_ASYM_PAUSE = 14
const LM_FECS = [{bit: 49, name: "None"}, {bit: 51, name: "BaseR"}, {bit: 50, name: "RS"}, {bit: 74, name: "LLRS"}]

pure mode_set(mask: List[Int], index: Int) -> Bool {
  index / 32 < mask.len() and bit(mask[index / 32], index % 32)
}

pure mode_any(mask: List[Int]) -> Bool {
  ! [word for word in mask if word != 0].is_empty()
}

# The modes of a bitmap in table order, a Half/Full pair on one line, as the
# continuation lines of a labelled block: `label` names the first line and
# `pad` aligns the rest under it.
pure mode_block(label: Str, mask: List[Int]) -> List[Str] {
  var rows: List[Str] = []
  var row = ""
  for index in range(LINK_MODES.len()) {
    let name = LINK_MODES[index]
    continue when name == "" or ! mode_set(mask, index)

    let pairs_with_previous = index > 0 and name.ends_with("/Full") and LINK_MODES[index - 1] == name.byte_slice(0, name.byte_len() - 5) + "/Half" and mode_set(mask, index - 1)
    if pairs_with_previous {
      row += f"{name} "
    } else {
      rows += [row] when row != ""
      row = f"{name} "
    }
  }
  rows += [row] when row != ""
  return [label + "Not reported"] when rows.is_empty()

  var lines: List[Str] = []
  let blank = spaces(label.byte_len())
  for index in range(rows.len()) { lines += [(if index == 0 { label } else { blank }) + rows[index]] }
  lines
}

pure fec_text(mask: List[Int]) -> Str {
  let names = [mode.name for mode in LM_FECS if mode_set(mask, mode.bit)]
  if names.is_empty() { "Not reported" } else { names.join(" ") + " " }
}

pure pause_text(mask: List[Int]) -> Str {
  let pause = mode_set(mask, LM_PAUSE)
  let asym = mode_set(mask, LM_ASYM_PAUSE)
  if pause {
    if asym { "Symmetric Receive-only" } else { "Symmetric" }
  } else if asym {
    "Transmit-only"
  } else {
    "No"
  }
}

## Link settings as `ethtool_link_settings` returns them.
export type LinkSettings = {speed: Int, duplex: Int, port: Int, phy: Int, autoneg: Int, mdix: Int, mdix_ctrl: Int, transceiver: Int, supported: List[Int], advertising: List[Int], partner: List[Int], nwords: Int, raw: Bytes}

## The request that asks how many mask words the kernel will return (the
## reply carries the negative count) or, with `nwords` set, the settings.
export pure link_settings_request(nwords: Int) -> Result[Bytes, Error] {
  let head = bytes.concat([bytes.pack_le(CMD.GLINKSETTINGS, 4)?, bytes.zero(11)?, bytes.from_ints([nwords])?])
  Ok(bytes.concat([head, bytes.zero(32 + nwords * 12)?]))
}

## The signed mask word count a settings reply reports.
export pure link_settings_nwords(reply: Bytes) -> Int {
  let raw = reply.byte_at(15) ?? 0
  if raw >= 128 { raw - 256 } else { raw }
}

## Decodes a complete `ethtool_link_settings` reply.
export pure link_settings_decode(reply: Bytes) -> Result[LinkSettings, Error] {
  let nwords = link_settings_nwords(reply)
  var supported: List[Int] = []
  var advertising: List[Int] = []
  var partner: List[Int] = []
  for index in range(nwords) {
    supported += [bytes.unpack_le(reply, 4, 48 + index * 4)?]
    advertising += [bytes.unpack_le(reply, 4, 48 + (nwords + index) * 4)?]
    partner += [bytes.unpack_le(reply, 4, 48 + (2 * nwords + index) * 4)?]
  }
  Ok({
    speed: bytes.unpack_le(reply, 4, 4)?, duplex: reply.byte_at(8) ?? 0, port: reply.byte_at(9) ?? 0, phy: reply.byte_at(10) ?? 0,
    autoneg: reply.byte_at(11) ?? 0, mdix: reply.byte_at(13) ?? 0, mdix_ctrl: reply.byte_at(14) ?? 0,
    transceiver: reply.byte_at(16) ?? 0, supported: supported, advertising: advertising, partner: partner, nwords: nwords, raw: reply,
  })
}

## A `SLINKSETTINGS` request: the reply it was read from with the cmd word
## switched and the changed scalar fields and advertised mask written back.
export pure link_settings_set_request(settings: LinkSettings, speed: Int?, duplex: Int?, port: Int?, phy: Int?, autoneg: Int?, mdix_ctrl: Int?, advertising: List[Int]) -> Result[Bytes, Error] {
  let raw = settings.raw
  var chunks: List[Bytes] = [
    bytes.pack_le(CMD.SLINKSETTINGS, 4)?, bytes.pack_le(speed ?? settings.speed, 4)?,
    bytes.from_ints([duplex ?? settings.duplex, port ?? settings.port, phy ?? settings.phy, autoneg ?? settings.autoneg])?,
    raw.slice(12, 1), bytes.from_ints([settings.mdix, mdix_ctrl ?? settings.mdix_ctrl])?,
    raw.slice(15, 33),
  ]
  for word in settings.supported { chunks += [bytes.pack_le(word, 4)?] }
  for word in advertising { chunks += [bytes.pack_le(word, 4)?] }
  for word in settings.partner { chunks += [bytes.pack_le(word, 4)?] }
  Ok(bytes.concat(chunks))
}

pure port_name(port: Int) -> Str {
  if port == 0 { "Twisted Pair" } else if port == 1 { "AUI" } else if port == 2 { "MII" } else if port == 3 { "FIBRE" } else if port == 4 { "BNC" } else if port == 5 { "Direct Attach Copper" } else if port == 239 { "None" } else if port == 255 { "Other" } else { "Unknown" }
}

## The link-settings block of `ethtool DEV`.
export pure settings_lines(settings: LinkSettings) -> List[Str] {
  let supported = settings.supported
  let advertising = settings.advertising
  let ports = [mode.name for mode in LM_PORTS if mode_set(supported, mode.bit)]
  var lines: List[Str] = [f"\tSupported ports: [ {ports.join(" ")} ]"]
  for line in mode_block("Supported link modes:   ", supported) { lines += ["\t" + line] }
  lines += [f"\tSupported pause frame use: {pause_text(supported)}"]
  lines += [f"\tSupports auto-negotiation: {if mode_set(supported, LM_AUTONEG) { "Yes" } else { "No" }}"]
  lines += [f"\tSupported FEC modes: {fec_text(supported)}"]
  for line in mode_block("Advertised link modes:  ", advertising) { lines += ["\t" + line] }
  lines += [f"\tAdvertised pause frame use: {pause_text(advertising)}"]
  lines += [f"\tAdvertised auto-negotiation: {if mode_set(advertising, LM_AUTONEG) { "Yes" } else { "No" }}"]
  lines += [f"\tAdvertised FEC modes: {fec_text(advertising)}"]
  if mode_any(settings.partner) {
    for line in mode_block("Link partner advertised link modes:  ", settings.partner) { lines += ["\t" + line] }
    lines += [f"\tLink partner advertised pause frame use: {pause_text(settings.partner)}"]
    lines += [f"\tLink partner advertised auto-negotiation: {if mode_set(settings.partner, LM_AUTONEG) { "Yes" } else { "No" }}"]
    lines += [f"\tLink partner advertised FEC modes: {fec_text(settings.partner)}"]
  }
  let known = settings.speed != 0 and settings.speed != 65535 and settings.speed != 4294967295
  lines += [if known { f"\tSpeed: {settings.speed}Mb/s" } else { "\tSpeed: Unknown!" }]
  lines += [if settings.duplex == 0 { "\tDuplex: Half" } else if settings.duplex == 1 { "\tDuplex: Full" } else { f"\tDuplex: Unknown! ({settings.duplex})" }]
  lines += [f"\tAuto-negotiation: {if settings.autoneg == 1 { "on" } else { "off" }}"]
  lines += [f"\tPort: {port_name(settings.port)}"]
  lines += [f"\tPHYAD: {settings.phy}"]
  lines += [f"\tTransceiver: {if settings.transceiver == 0 { "internal" } else if settings.transceiver == 1 { "external" } else { "unknown" }}"]
  if settings.port == 0 {
    let state = if settings.mdix == 1 { "off" } else if settings.mdix == 2 { "on" } else { "Unknown" }
    lines += [f"\tMDI-X: {state}{if settings.mdix_ctrl == 3 { " (auto)" } else { "" }}"]
  }
  lines
}

const WOL_LETTERS = ["p", "u", "m", "b", "a", "g", "s", "f"]

## Wake-on-LAN letters for a bitmap, `d` when none is set.
export pure wol_letters(mask: Int) -> Str {
  let letters = [WOL_LETTERS[index] for index in range(8) if bit(mask, index)]
  if letters.is_empty() { "d" } else { letters.join("") }
}

## The wake-on bitmap a letter string spells, or null for a letter that is not
## one of `pumbagsfd`.
export pure wol_parse(text: Str) -> Int? {
  var mask = 0
  for index in range(text.byte_len()) {
    let letter = text.byte_slice(index, 1)
    continue when letter == "d"

    var found = -1
    for position in range(8) { found = position when WOL_LETTERS[position] == letter }
    return null when found < 0

    mask = mask.bit_or(POW2[found])
  }
  mask
}

## `ethtool_wolinfo`: cmd, supported, wolopts, then the six-byte password.
export pure wol_lines(reply: Bytes) -> Result[List[Str], Error] {
  let supported = bytes.unpack_le(reply, 4, 4)?
  let options = bytes.unpack_le(reply, 4, 8)?
  var lines: List[Str] = [f"\tSupports Wake-on: {wol_letters(supported)}", f"\tWake-on: {wol_letters(options)}"]
  if bit(supported, 6) and bit(options, 6) {
    var parts: List[Str] = []
    for index in range(6) { parts += [hex(reply.byte_at(12 + index) ?? 0, 2)] }
    lines += [f"\tSecureOn password: {parts.join(":")}"]
  }
  Ok(lines)
}

const MSGLVL_NAMES = ["drv", "probe", "link", "timer", "ifdown", "ifup", "rx_err", "tx_err", "tx_queued", "intr", "tx_done", "rx_status", "pktdata", "hw", "wol"]

## The message level word and the names of its set bits.
export pure msglvl_lines(level: Int) -> List[Str] {
  let names = [MSGLVL_NAMES[index] for index in range(MSGLVL_NAMES.len()) if bit(level, index)]
  [f"\tCurrent message level: 0x{hex(level, 8)} ({level})", "\t\t\t       " + names.join(" ")]
}

## `ethtool_eee`: cmd, supported, advertised, link partner, active, enabled,
## tx-lpi enabled, tx-lpi timer.
export pure eee_lines(device: Str, reply: Bytes) -> Result[List[Str], Error] {
  let v = unwords(reply, 7)?
  var lines: List[Str] = [f"EEE settings for {device}:"]
  if v[0] == 0 {
    return Ok(lines + ["\tEEE status: not supported"])
  }
  let status = if v[4] == 0 { "disabled" } else if v[3] != 0 { "enabled - active" } else { "enabled - inactive" }
  lines += [f"\tEEE status: {status}"]
  lines += [if v[5] != 0 { f"\tTx LPI: {v[6]} (us)" } else { "\tTx LPI: disabled" }]
  for line in mode_block("Supported EEE link modes:  ", [v[0]]) { lines += ["\t" + line] }
  for line in mode_block("Advertised EEE link modes:  ", [v[1]]) { lines += ["\t" + line] }
  for line in mode_block("Link partner advertised EEE link modes:  ", [v[2]]) { lines += ["\t" + line] }
  Ok(lines)
}

## `ethtool_eee` with the enable flags and timer replaced where given.
export pure eee_set_request(reply: Bytes, advertise: Int?, enabled: Bool?, tx_lpi: Bool?, timer: Int?) -> Result[Bytes, Error] {
  let v = unwords(reply, 7)?
  words(CMD.SEEE, [v[0], advertise ?? v[1], v[2], v[3], flag_word(enabled, v[4]), flag_word(tx_lpi, v[5]), timer ?? v[6], 0, 0])
}
