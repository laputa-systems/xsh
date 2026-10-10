##! Decoding of the ATA and NVMe structures smartctl reports: IDENTIFY DEVICE,
##! SMART data and threshold pages, the error, self-test, selective self-test and
##! directory logs, and the NVMe identify and SMART/health pages.
##!
##! Every function here is a pure decoder over raw device pages. Presentation
##! lives in `smart_ata`, `smart_nvme` and `smart_scsi` (smartctl's text layout
##! and JSON members) over the ordered writer in `smart_json`; device access
##! lives in the `smartctl` applet.

## Largest attribute table a SMART data page carries.
export const ATTRIBUTE_SLOTS = 30

const MAJOR_NAMES = ["", "ATA-1", "ATA-2", "ATA-3", "ATA/ATAPI-4", "ATA/ATAPI-5", "ATA/ATAPI-6", "ATA/ATAPI-7", "ATA8-ACS", "ACS-2", "ACS-3", "ACS-4", "ACS-5", "ACS-6", "ACS >6 (14)"]

const MINOR_TEXT = [
  "0001 ATA-1 X3T9.2/781D prior to revision 4",
  "0002 ATA-1 published, ANSI X3.221-1994",
  "0003 ATA-1 X3T9.2/781D revision 4",
  "0004 ATA-2 published, ANSI X3.279-1996",
  "0005 ATA-2 X3T10/948D prior to revision 2k",
  "0006 ATA-3 X3T10/2008D revision 1",
  "0007 ATA-2 X3T10/948D revision 2k",
  "0008 ATA-3 X3T10/2008D revision 0",
  "0009 ATA-2 X3T10/948D revision 3",
  "000a ATA-3 published, ANSI X3.298-1997",
  "000b ATA-3 X3T10/2008D revision 6",
  "000c ATA-3 X3T13/2008D revision 7 and 7a",
  "000d ATA/ATAPI-4 X3T13/1153D revision 6",
  "000e ATA/ATAPI-4 T13/1153D revision 13",
  "000f ATA/ATAPI-4 X3T13/1153D revision 7",
  "0010 ATA/ATAPI-4 T13/1153D revision 18",
  "0011 ATA/ATAPI-4 T13/1153D revision 15",
  "0012 ATA/ATAPI-4 published, ANSI NCITS 317-1998",
  "0013 ATA/ATAPI-5 T13/1321D revision 3",
  "0014 ATA/ATAPI-4 T13/1153D revision 14",
  "0015 ATA/ATAPI-5 T13/1321D revision 1",
  "0016 ATA/ATAPI-5 published, ANSI NCITS 340-2000",
  "0017 ATA/ATAPI-4 T13/1153D revision 17",
  "0018 ATA/ATAPI-6 T13/1410D revision 0",
  "0019 ATA/ATAPI-6 T13/1410D revision 3a",
  "001a ATA/ATAPI-7 T13/1532D revision 1",
  "001b ATA/ATAPI-6 T13/1410D revision 2",
  "001c ATA/ATAPI-6 T13/1410D revision 1",
  "001d ATA/ATAPI-7 published, ANSI INCITS 397-2005",
  "001e ATA/ATAPI-7 T13/1532D revision 0",
  "001f ACS-3 T13/2161-D revision 3b",
  "0021 ATA/ATAPI-7 T13/1532D revision 4a",
  "0022 ATA/ATAPI-6 published, ANSI INCITS 361-2002",
  "0027 ATA8-ACS T13/1699-D revision 3c",
  "0028 ATA8-ACS T13/1699-D revision 6",
  "0029 ATA8-ACS T13/1699-D revision 4",
  "0031 ACS-2 T13/2015-D revision 2",
  "0033 ATA8-ACS T13/1699-D revision 3e",
  "0039 ATA8-ACS T13/1699-D revision 4c",
  "0042 ATA8-ACS T13/1699-D revision 3f",
  "0052 ATA8-ACS T13/1699-D revision 3b",
  "005e ACS-4 T13/BSR INCITS 529 revision 5",
  "006d ACS-3 T13/2161-D revision 5",
  "0082 ACS-2 published, ANSI INCITS 482-2012",
  "0107 ATA8-ACS T13/1699-D revision 2d",
  "010a ACS-3 published, ANSI INCITS 522-2014",
  "0110 ACS-2 T13/2015-D revision 3",
  "011b ACS-3 T13/2161-D revision 4",
]

# Transport major version word 222, indexed by the highest set bit.
const SATA_VERSIONS = ["ATA8-AST", "SATA 1.0a", "SATA II Ext", "SATA 2.5", "SATA 2.6", "SATA 3.0", "SATA 3.1", "SATA 3.2", "SATA 3.3", "SATA 3.4", "SATA 3.5"]

# Signaling speed bits 1 to 7 of word 76; bits above the third name speeds
# beyond 6.0 Gb/s.
const SATA_SPEEDS = ["", "1.5 Gb/s", "3.0 Gb/s", "6.0 Gb/s", ">6.0 Gb/s (4)", ">6.0 Gb/s (5)", ">6.0 Gb/s (6)", ">6.0 Gb/s (7)"]

const ATTRIBUTE_NAMES = [
  "1 Raw_Read_Error_Rate", "2 Throughput_Performance", "3 Spin_Up_Time", "4 Start_Stop_Count",
  "5 Reallocated_Sector_Ct", "6 Read_Channel_Margin", "7 Seek_Error_Rate", "8 Seek_Time_Performance",
  "9 Power_On_Hours", "10 Spin_Retry_Count", "11 Calibration_Retry_Count", "12 Power_Cycle_Count",
  "13 Read_Soft_Error_Rate", "22 Helium_Level", "23 Helium_Condition_Lower", "24 Helium_Condition_Upper",
  "175 Program_Fail_Count_Chip", "176 Erase_Fail_Count_Chip", "177 Wear_Leveling_Count",
  "178 Used_Rsvd_Blk_Cnt_Chip", "179 Used_Rsvd_Blk_Cnt_Tot", "180 Unused_Rsvd_Blk_Cnt_Tot",
  "181 Program_Fail_Cnt_Total", "182 Erase_Fail_Count_Total", "183 Runtime_Bad_Block",
  "184 End-to-End_Error", "187 Reported_Uncorrect", "188 Command_Timeout", "189 High_Fly_Writes",
  "190 Airflow_Temperature_Cel", "191 G-Sense_Error_Rate", "192 Power-Off_Retract_Count",
  "193 Load_Cycle_Count", "194 Temperature_Celsius", "195 Hardware_ECC_Recovered",
  "196 Reallocated_Event_Count", "197 Current_Pending_Sector", "198 Offline_Uncorrectable",
  "199 UDMA_CRC_Error_Count", "200 Multi_Zone_Error_Rate", "201 Soft_Read_Error_Rate",
  "202 Data_Address_Mark_Errs", "203 Run_Out_Cancel", "204 Soft_ECC_Correction",
  "205 Thermal_Asperity_Rate", "206 Flying_Height", "207 Spin_High_Current", "208 Spin_Buzz",
  "209 Offline_Seek_Performnce", "220 Disk_Shift", "221 G-Sense_Error_Rate", "222 Loaded_Hours",
  "223 Load_Retry_Count", "224 Load_Friction", "225 Load_Cycle_Count", "226 Load-in_Time",
  "227 Torq-amp_Count", "228 Power-off_Retract_Count", "230 Head_Amplitude", "231 Temperature_Celsius",
  "232 Available_Reservd_Space", "233 Media_Wearout_Indicator", "240 Head_Flying_Hours",
  "241 Total_LBAs_Written", "242 Total_LBAs_Read", "250 Read_Error_Retry_Rate", "254 Free_Fall_Sensor",
]

const COMMAND_NAMES = [
  "00 NOP", "03 CFA REQUEST EXTENDED ERROR CODE", "06 DATA SET MANAGEMENT", "08 DEVICE RESET",
  "0b REQUEST SENSE DATA EXT", "20 READ SECTOR(S)", "21 READ SECTOR(S) WITHOUT RETRY",
  "24 READ SECTOR(S) EXT", "25 READ DMA EXT", "26 READ DMA QUEUED EXT", "27 READ NATIVE MAX ADDRESS EXT",
  "29 READ MULTIPLE EXT", "2a READ STREAM DMA EXT", "2b READ STREAM EXT", "2f READ LOG EXT",
  "30 WRITE SECTOR(S)", "34 WRITE SECTOR(S) EXT", "35 WRITE DMA EXT", "36 WRITE DMA QUEUED EXT",
  "37 SET MAX ADDRESS EXT", "39 WRITE MULTIPLE EXT", "3a WRITE STREAM DMA EXT", "3b WRITE STREAM EXT",
  "3d WRITE DMA FUA EXT", "3e WRITE DMA QUEUED FUA EXT", "3f WRITE LOG EXT", "40 READ VERIFY SECTOR(S)",
  "42 READ VERIFY SECTOR(S) EXT", "45 WRITE UNCORRECTABLE EXT", "51 CONFIGURE STREAM",
  "60 READ FPDMA QUEUED", "61 WRITE FPDMA QUEUED", "63 NCQ QUEUE MANAGEMENT", "64 SEND FPDMA QUEUED",
  "65 RECEIVE FPDMA QUEUED", "70 SEEK", "90 EXECUTE DEVICE DIAGNOSTIC", "91 INITIALIZE DEVICE PARAMETERS",
  "92 DOWNLOAD MICROCODE", "93 DOWNLOAD MICROCODE DMA", "a0 PACKET", "a1 IDENTIFY PACKET DEVICE",
  "a2 SERVICE", "b0 SMART", "b1 DEVICE CONFIGURATION", "b4 SANITIZE DEVICE", "c4 READ MULTIPLE",
  "c5 WRITE MULTIPLE", "c6 SET MULTIPLE MODE", "c7 READ DMA QUEUED", "c8 READ DMA", "ca WRITE DMA",
  "cc WRITE DMA QUEUED", "ce WRITE MULTIPLE FUA EXT", "e0 STANDBY IMMEDIATE", "e1 IDLE IMMEDIATE",
  "e2 STANDBY", "e3 IDLE", "e4 READ BUFFER", "e5 CHECK POWER MODE", "e6 SLEEP", "e7 FLUSH CACHE",
  "e8 WRITE BUFFER", "e9 READ BUFFER DMA", "ea FLUSH CACHE EXT", "eb WRITE BUFFER DMA",
  "ec IDENTIFY DEVICE", "ef SET FEATURES", "f1 SECURITY SET PASSWORD", "f2 SECURITY UNLOCK",
  "f3 SECURITY ERASE PREPARE", "f4 SECURITY ERASE UNIT", "f5 SECURITY FREEZE LOCK",
  "f6 SECURITY DISABLE PASSWORD", "f8 READ NATIVE MAX ADDRESS", "f9 SET MAX ADDRESS",
]

const SMART_FEATURE_NAMES = [
  "d0 READ DATA", "d1 READ ATTRIBUTE THRESHOLDS", "d2 ENABLE/DISABLE ATTRIBUTE AUTOSAVE",
  "d3 SAVE ATTRIBUTE VALUES", "d4 EXECUTE OFF-LINE IMMEDIATE", "d5 READ LOG", "d6 WRITE LOG",
  "d8 ENABLE OPERATIONS", "d9 DISABLE OPERATIONS", "da RETURN STATUS", "db ENABLE/DISABLE AUTO OFFLINE",
]

const SET_FEATURE_NAMES = [
  "01 Enable 8-bit data transfers", "02 Enable write cache", "03 Set transfer mode",
  "05 Enable advanced power management", "06 Enable Power-Up In Standby", "07 Power-Up In Standby spin-up",
  "10 Enable use of SATA feature", "31 Disable Media Status Notification", "42 Enable Automatic Acoustic Management",
  "55 Disable read look-ahead", "66 Disable reverting to defaults", "82 Disable write cache",
  "85 Disable advanced power management", "86 Disable Power-Up In Standby", "90 Disable use of SATA feature",
  "aa Enable read look-ahead", "c2 Disable Automatic Acoustic Management", "cc Enable reverting to defaults",
]

const LOG_NAMES = [
  "00 R/O Log Directory",
  "01 R/O Summary SMART error log",
  "02 R/O Comprehensive SMART error log",
  "03 R/O Ext. Comprehensive SMART error log",
  "04 R/O Device Statistics log",
  "05 R/O Reserved for CFA",
  "06 R/O SMART self-test log",
  "07 R/O Extended self-test log",
  "08 R/O Power Conditions log",
  "09 R/W Selective self-test log",
  "0a R/W Device Statistics Notification",
  "0c R/O Pending Defects log",
  "10 R/O NCQ Command Error log",
  "11 R/O SATA Phy Event Counters log",
  "12 R/O SATA NCQ Queue Management log",
  "13 R/O SATA NCQ Send and Receive log",
  "14 R/O Hybrid Information log",
  "15 R/W Rebuild Assist log",
  "19 R/O LBA Status log",
  "20 R/O Streaming performance log",
  "21 R/O Write stream error log",
  "22 R/O Read stream error log",
  "24 R/O Current Device Internal Status Data log",
  "25 R/O Saved Device Internal Status Data log",
  "30 R/O IDENTIFY DEVICE data log",
  "e0 R/W SCT Command/Status",
  "e1 R/W SCT Data Transfer",
]

## The 256 16-bit words of an IDENTIFY DEVICE page.
export type Words = List[Int]

## One decoded SMART attribute with its threshold applied.
export type Attribute = {
  id: Int,
  name: Str,
  flags: Int,
  value: Int,
  worst: Int,
  threshold: Int?,
  raw_bytes: List[Int],
  raw_value: Int,
  raw_text: Str,
  prefailure: Bool,
  state: Str,
}

## The SMART data page (command D0h) header fields that precede and follow the
## attribute table.
export type SmartValues = {
  revision: Int,
  offline_status: Int,
  self_test_status: Int,
  offline_seconds: Int,
  offline_capability: Int,
  smart_capability: Int,
  error_log_capability: Int,
  short_minutes: Int,
  extended_minutes: Int,
  conveyance_minutes: Int,
  checksum_ok: Bool,
  attributes: List[Attribute],
}

## One command register image recorded ahead of an error.
export type ErrorCommand = {
  device_control: Int,
  features: Int,
  count: Int,
  lba_low: Int,
  lba_mid: Int,
  lba_high: Int,
  device: Int,
  command: Int,
  timestamp_ms: Int,
}

## One logged ATA error with the commands that led to it, newest first.
export type ErrorEntry = {
  number: Int,
  error: Int,
  count: Int,
  lba_low: Int,
  lba_mid: Int,
  lba_high: Int,
  device: Int,
  status: Int,
  state: Int,
  hours: Int,
  commands: List[ErrorCommand],
}

## The summary SMART error log.
export type ErrorLog = {revision: Int, pointer: Int, count: Int, entries: List[ErrorEntry], checksum_ok: Bool}

## One self-test log entry.
export type SelfTestEntry = {number: Int, kind: Int, status: Int, remaining_percent: Int, hours: Int, checkpoint: Int, lba: Int}

## The SMART self-test log, most recent entry first. A failure older than the
## newest extended test that completed without error is outdated: it still
## counts in `error_count` but `outdated_count` also includes it, and
## `outdated_by` is the number of the test that superseded it (0 when none).
export type SelfTestLog = {
  revision: Int,
  entries: List[SelfTestEntry],
  error_count: Int,
  outdated_count: Int,
  outdated_by: Int,
  checksum_ok: Bool,
}

## One span of the selective self-test log.
export type SelectiveSpan = {start: Int, end: Int}

## The selective self-test log.
export type SelectiveLog = {
  revision: Int,
  spans: List[SelectiveSpan],
  current_lba: Int,
  current_span: Int,
  flags: Int,
  pending_minutes: Int,
  checksum_ok: Bool,
}

## One SMART log directory entry: address and size in sectors.
export type LogDirectoryEntry = {address: Int, sectors: Int}

## The SMART log directory.
export type LogDirectory = {revision: Int, entries: List[LogDirectoryEntry]}

## A World Wide Name split into its NAA, OUI and unique id parts.
export type Wwn = {naa: Int, oui: Int, id: Int}

## A form factor code with its description.
export type FormFactor = {value: Int, name: Str}

## The ATA standard and revision a device reports.
export type AtaVersion = {text: Str, major: Int, minor: Int}

## The SATA generation and link speeds a device reports; speeds are 1 to 3
## for 1.5, 3.0 and 6.0 Gb/s and 0 when unknown.
export type SataVersion = {text: Str, max_speed: Int, current_speed: Int}

## Access mode and description of a log address.
export type LogName = {access: Str, name: Str}

## Lowercase hexadecimal of `value`, at least `width` digits wide.
export pure hex(value: Int, width: Int) -> Str {
  var number = value
  var text = ""
  while number > 0 { text = "0123456789abcdef".byte_slice(number % 16, 1) + text; number /= 16 }
  while text.byte_len() < width { text = "0" + text }
  text
}

## Pads `text` with trailing spaces to `width` bytes.
export pure pad_end(text: Str, width: Int) -> Str {
  var padded = text
  while padded.byte_len() < width { padded += " " }
  padded
}

## Pads `text` with leading spaces to `width` bytes.
export pure pad_start(text: Str, width: Int) -> Str {
  var padded = text
  while padded.byte_len() < width { padded = " " + padded }
  padded
}

## Pads a number with leading zeros to `width` digits.
export pure zero_pad(value: Int, width: Int) -> Str {
  var text = f"{value}"
  while text.byte_len() < width { text = "0" + text }
  text
}

## Decimal with comma thousands separators, as smartctl prints large counts.
export pure thousands(value: Int) -> Str {
  let digits = f"{value}"
  var out = ""
  var count = 0
  var cursor = digits.byte_len()
  while cursor > 0 {
    if count > 0 and count % 3 == 0 { out = "," + out }
    out = digits.byte_slice(cursor - 1, 1) + out
    count += 1
    cursor -= 1
  }
  out
}

## Capacity with three significant digits in decimal units, truncated rather
## than rounded: `1.00 TB`, `15.3 TB`, `500 GB`.
export pure capacity_text(size: Int) -> Str {
  let prefixes = ["", "K", "M", "G", "T", "P"]
  var unit = 1
  var index = 0
  while index < 5 and size / unit >= 1000 { unit *= 1000; index += 1 }
  if index == 0 { return f"{size} bytes" }
  let whole = size / unit
  let rest = size % unit
  let suffix = prefixes[index] + "B"
  if whole < 10 { return f"{whole}.{zero_pad(rest * 100 / unit, 2)} {suffix}" }
  if whole < 100 { return f"{whole}.{rest * 10 / unit} {suffix}" }
  f"{whole} {suffix}"
}

## A little-endian unsigned field; a field past the end of the page reads as 0.
export pure le(data: Bytes, offset: Int, width: Int) -> Int {
  bytes.unpack_le(data, width, offset) ?? 0
}

## One byte of a page; 0 past the end.
export pure at(data: Bytes, offset: Int) -> Int {
  data.byte_at(offset) ?? 0
}

## True when every byte of the range is zero.
export pure all_zero(data: Bytes, offset: Int, length: Int) -> Bool {
  for index in range(length) {
    if at(data, offset + index) != 0 { return false }
  }
  true
}

## Whether the 512 bytes sum to zero modulo 256, the checksum every SMART page
## carries in its last byte.
export pure checksum_ok(page: Bytes) -> Bool {
  var sum = 0
  for index in range(page.len()) { sum += at(page, index) }
  sum % 256 == 0
}

## The 256 words of an IDENTIFY DEVICE page.
export pure words(page: Bytes) -> Words {
  [le(page, index * 2, 2) for index in range(256)]
}

## The most significant set bit of `value` in the inclusive bit range, or -1.
export pure highest_bit(value: Int, low: Int, high: Int) -> Int {
  var bit = high
  while bit >= low {
    if value / pow2(bit) % 2 == 1 { return bit }
    bit -= 1
  }
  -1
}

## 2 raised to a small non-negative power.
export pure pow2(power: Int) -> Int {
  var result = 1
  for _ in range(power) { result *= 2 }
  result
}

## Whether bit `bit` of `value` is set.
export pure bit_set(value: Int, bit: Int) -> Bool {
  value / pow2(bit) % 2 == 1
}

## An ATA string: each 16-bit word holds two characters, high byte first;
## leading and trailing blanks are dropped.
export pure ata_string(page: Bytes, first_word: Int, word_count: Int) -> Str {
  var chars: List[Int] = []
  for index in range(word_count) {
    chars += [at(page, (first_word + index) * 2 + 1), at(page, (first_word + index) * 2)]
  }
  let raw = bytes.from_ints(chars) ?? b""
  let text = raw.utf8() ?? ""
  text.trim()
}

## Logical sector size in bytes from IDENTIFY words 106 and 117 to 118.
export pure logical_sector_bytes(w: Words) -> Int {
  let layout = w[106]
  if layout / 16384 == 1 and bit_set(layout, 12) {
    let size = w[117] + w[118] * 65536
    if size >= 256 { return size * 2 }
  }
  512
}

## Physical sector size in bytes: the logical size times the power of two
## IDENTIFY word 106 reports when several logical sectors share one.
export pure physical_sector_bytes(w: Words) -> Int {
  let logical = logical_sector_bytes(w)
  let layout = w[106]
  if layout / 16384 == 1 and bit_set(layout, 13) {
    return logical * pow2(layout % 16)
  }
  logical
}

## User-addressable sectors: the 48-bit count when supported, else 28-bit.
export pure sector_count(w: Words) -> Int {
  if bit_set(w[83], 10) and w[83] / 16384 == 1 {
    let big = w[100] + w[101] * 65536 + w[102] * 4294967296 + w[103] * 281474976710656
    if big > 0 { return big }
  }
  w[60] + w[61] * 65536
}

## The World Wide Name from words 108 to 111 when word 87 says it is valid.
export pure wwn(w: Words) -> Wwn? {
  guard w[87] / 16384 == 1 and bit_set(w[87], 8) else { return null }
  guard w[108] != 0 or w[109] != 0 or w[110] != 0 or w[111] != 0 else { return null }
  {
    naa: w[108] / 4096,
    oui: w[108] % 4096 * 4096 + w[109] / 16,
    id: w[109] % 16 * 4294967296 + w[110] * 65536 + w[111],
  }
}

## Nominal media rotation rate: 0 not reported, 1 solid state, N rpm, -1 reserved.
export pure rotation_rate(w: Words) -> Int {
  let word = w[217]
  if word == 0 or word == 65535 { return 0 }
  if word == 1 { return 1 }
  if word > 1024 and word < 65535 { return word }
  -1
}

## Form factor name from word 168, or null when it is not reported.
export pure form_factor(w: Words) -> FormFactor? {
  let value = w[168] % 16
  let names = ["", "5.25 inches", "3.5 inches", "2.5 inches", "1.8 inches", "< 1.8 inches"]
  guard w[168] < 16 and value >= 1 and value <= 5 else { return null }
  {value: value, name: names[value]}
}

## The TRIM description from words 169 and 69, or null when unsupported.
export pure trim_text(w: Words) -> Str? {
  guard bit_set(w[169], 0) else { return null }
  var text = "Available"
  if bit_set(w[69], 14) { text += ", deterministic" }
  if bit_set(w[69], 5) { text += ", zeroed" }
  text
}

## The ATA version line: the standard named by the highest bit of IDENTIFY word
## 80 and the draft or publication named by word 81.
export pure ata_version(w: Words) -> AtaVersion {
  let major = w[80]
  let minor = w[81]
  let bit = if major == 0 or major == 65535 { -1 } else { highest_bit(major, 1, 14) }
  guard bit >= 1 else { return {text: "Unknown", major: 0, minor: minor} }
  let name = MAJOR_NAMES[bit]
  if minor == 0 or minor == 65535 {
    return {text: f"{name} (minor revision not indicated)", major: bit, minor: minor}
  }
  let code = hex(minor, 4)
  for entry in MINOR_TEXT {
    if entry.starts_with(code + " ") {
      let text = entry.byte_slice(5)
      if text.starts_with(name) { return {text: text, major: bit, minor: minor} }
      return {text: f"{name}, {text}", major: bit, minor: minor}
    }
  }
  {text: f"{name} (unknown minor revision code: 0x{code})", major: bit, minor: minor}
}

## The SATA version, maximum and current link speed, or null for a device that
## does not describe itself as SATA.
export pure sata_version(w: Words) -> SataVersion? {
  let caps = w[76]
  let transport = w[222]
  let speed = if caps == 0 or caps == 65535 { 0 } else { highest_bit(caps, 1, 7) }
  let current = if caps == 0 or caps == 65535 { 0 } else { w[77] / 2 % 8 }
  var version = ""
  if transport != 0 and transport != 65535 and transport / 4096 == 1 {
    let bit = highest_bit(transport, 0, 11)
    if bit >= 0 and bit <= 10 { version = SATA_VERSIONS[bit] }
    if bit == 11 { version = "SATA >3.5 (11)" }
  }
  if version == "" {
    guard speed >= 1 else { return null }
    version = if speed >= 3 { "SATA 3.0" } else if speed == 2 { "SATA 2.6" } else { "SATA 1.0a" }
  }
  {text: version, max_speed: if speed > 0 { speed } else { 0 }, current_speed: if current >= 1 and current <= 7 { current } else { 0 }}
}

## The `SATA Version is:` value including link speeds.
export pure sata_version_text(info: SataVersion) -> Str {
  var text = info.text
  if info.max_speed >= 1 and info.max_speed <= 7 { text += ", " + SATA_SPEEDS[info.max_speed] }
  if info.current_speed >= 1 and info.current_speed <= 7 { text += f" (current: {SATA_SPEEDS[info.current_speed]})" }
  text
}

## SMART availability: 1 available, 0 absent, -1 words 82 to 83 are ambiguous.
export pure smart_supported(w: Words) -> Int {
  if w[83] / 16384 != 1 { return -1 }
  if bit_set(w[82], 0) { return 1 }
  0
}

## SMART enablement: 1 enabled, 0 disabled, -1 words 85 to 87 are ambiguous.
export pure smart_enabled(w: Words) -> Int {
  if w[87] / 16384 != 1 { return -1 }
  if bit_set(w[85], 0) { return 1 }
  0
}

## Whether the device supports the general purpose logging feature set.
export pure gp_logging(w: Words) -> Bool {
  w[84] / 16384 == 1 and bit_set(w[84], 5)
}

## The APM line.
export pure apm_text(w: Words) -> Str {
  guard w[83] / 16384 == 1 and bit_set(w[83], 3) else { return "APM feature is:   Unavailable" }
  guard bit_set(w[86], 3) else { return "APM feature is:   Disabled" }
  let level = w[91] % 256
  let name = if level == 254 { "maximum performance" } else if level == 128 { "minimum power consumption without standby" } else if level > 128 { "intermediate level without standby" } else if level == 1 { "minimum power consumption with standby" } else { "intermediate level with standby" }
  f"APM level is:     {level} ({name})"
}

## The AAM line.
export pure aam_text(w: Words) -> Str {
  guard w[83] / 16384 == 1 and bit_set(w[83], 9) else { return "AAM feature is:   Unavailable" }
  guard bit_set(w[86], 9) else { return "AAM feature is:   Disabled" }
  let level = w[94] % 256
  let recommended = w[94] / 256
  let name = if level == 0 { "vendor specific" } else if level < 128 { "unknown/retired" } else if level == 128 { "quiet" } else if level < 254 { "intermediate" } else if level == 254 { "maximum performance" } else { "reserved" }
  if recommended != 0 { return f"AAM level is:     {level} ({name}), recommended: {recommended}" }
  f"AAM level is:     {level} ({name})"
}

## The read look-ahead line.
export pure lookahead_text(w: Words) -> Str {
  guard bit_set(w[82], 6) else { return "Rd look-ahead is: Unavailable" }
  if bit_set(w[85], 6) { return "Rd look-ahead is: Enabled" }
  "Rd look-ahead is: Disabled"
}

## The volatile write cache line.
export pure write_cache_text(w: Words) -> Str {
  guard bit_set(w[82], 5) else { return "Write cache is:   Unavailable" }
  if bit_set(w[85], 5) { return "Write cache is:   Enabled" }
  "Write cache is:   Disabled"
}

## The Device Statistics Notification line.
export pure dsn_text(w: Words) -> Str {
  guard w[119] / 16384 == 1 and bit_set(w[119], 9) else { return "DSN feature is:   Unavailable" }
  if bit_set(w[120], 9) { return "DSN feature is:   Enabled" }
  "DSN feature is:   Disabled"
}

## The ATA security line with its security-state number.
export pure security_text(w: Words) -> Str {
  guard bit_set(w[82], 1) and bit_set(w[128], 0) else { return "ATA Security is:  Unavailable" }
  let enabled = bit_set(w[128], 1)
  let locked = bit_set(w[128], 2)
  let frozen = bit_set(w[128], 3)
  if !enabled {
    if frozen { return "ATA Security is:  Disabled, frozen [SEC2]" }
    return "ATA Security is:  Disabled, NOT FROZEN [SEC1]"
  }
  let level = if bit_set(w[128], 8) { "MAX" } else { "HIGH" }
  let exceeded = if bit_set(w[128], 4) { ", PW ATTEMPTS EXCEEDED" } else { "" }
  if locked { return f"ATA Security is:  ENABLED, PW level {level}, **LOCKED** [SEC4]{exceeded}" }
  if frozen { return f"ATA Security is:  ENABLED, PW level {level}, not locked, frozen [SEC6]{exceeded}" }
  f"ATA Security is:  ENABLED, PW level {level}, not locked, not frozen [SEC5]{exceeded}"
}

## The standard name smartctl gives attribute `id`; unnamed ids are
## `Unknown_Attribute`.
export pure attribute_name(id: Int) -> Str {
  let prefix = f"{id} "
  for entry in ATTRIBUTE_NAMES {
    if entry.starts_with(prefix) { return entry.byte_slice(prefix.byte_len()) }
  }
  "Unknown_Attribute"
}

## The 48-bit little-endian integer of six raw attribute bytes.
export pure raw_number(raw: List[Int]) -> Int {
  var value = 0
  for index in range(6) { value += raw[5 - index] * pow2((5 - index) * 8) }
  value
}

## The raw value rendering smartctl uses by default: the 48-bit integer, and for
## the temperature attributes the current value followed by the recorded
## minimum and maximum when the bytes are consistent with that layout.
export pure raw_text(id: Int, raw: List[Int]) -> Str {
  if id not in [190, 194, 231] { return f"{raw_number(raw)}" }
  let word1 = raw[2] + raw[3] * 256
  let word2 = raw[4] + raw[5] * 256
  let temperature = raw[0]
  if word1 == 0 and word2 == 0 and raw[1] == 0 { return f"{temperature}" }
  let low = raw[2]
  let high = raw[3]
  if raw[1] == 0 and low <= high and high != 0 {
    if word2 == 0 { return f"{temperature} (Min/Max {low}/{high})" }
    return f"{temperature} (Min/Max {low}/{high} #{word2})"
  }
  f"{temperature} ({raw[1]} {raw[2]} {raw[3]} {raw[4]} {raw[5]})"
}

## Decodes the SMART data page and, when given, applies the threshold page.
export pure smart_values(data: Bytes, thresholds: Bytes?) -> SmartValues {
  var attributes: List[Attribute] = []
  for slot in range(ATTRIBUTE_SLOTS) {
    let base = 2 + slot * 12
    let id = at(data, base)
    continue when id == 0
    let flags = le(data, base + 1, 2)
    let raw = [at(data, base + 5 + index) for index in range(6)]
    var threshold: Int? = null
    if let table = thresholds {
      # Thresholds normally sit at the same slot, but a device may order the
      # threshold table differently, so look the id up when the slot differs.
      for candidate in range(ATTRIBUTE_SLOTS) {
        let tbase = 2 + candidate * 12
        let slot_id = at(table, tbase)
        let preferred = slot_id == id and candidate == slot
        if slot_id == id and (preferred or threshold == null) { threshold = at(table, tbase + 1) }
      }
    }
    let value = at(data, base + 3)
    let worst = at(data, base + 4)
    var state = "ok"
    if threshold == null { state = "no_threshold" } else if threshold != 0 {
      if value <= threshold { state = "failed_now" } else if worst <= threshold { state = "failed_past" }
    }
    attributes += [{
      id: id,
      name: attribute_name(id),
      flags: flags,
      value: value,
      worst: worst,
      threshold: threshold,
      raw_bytes: raw,
      raw_value: raw_number(raw),
      raw_text: raw_text(id, raw),
      prefailure: bit_set(flags, 0),
      state: state,
    }]
  }
  let extended = if at(data, 373) == 255 { le(data, 375, 2) } else { at(data, 373) }
  {
    revision: le(data, 0, 2),
    offline_status: at(data, 362),
    self_test_status: at(data, 363),
    offline_seconds: le(data, 364, 2),
    offline_capability: at(data, 367),
    smart_capability: le(data, 368, 2),
    error_log_capability: at(data, 370),
    short_minutes: at(data, 372),
    extended_minutes: extended,
    conveyance_minutes: at(data, 374),
    checksum_ok: checksum_ok(data),
    attributes: attributes,
  }
}

pure table_name(table: List[Str], code: Int) -> Str? {
  let prefix = hex(code, 2) + " "
  for entry in table {
    if entry.starts_with(prefix) { return entry.byte_slice(3) }
  }
  null
}

## The command mnemonic smartctl prints for an error log command register image.
export pure command_name(command: Int, features: Int) -> Str {
  if command == 176 {
    guard let sub = table_name(SMART_FEATURE_NAMES, features) else { return "SMART [vendor-specific feature]" }
    return f"SMART {sub}"
  }
  if command == 239 {
    guard let sub = table_name(SET_FEATURE_NAMES, features) else { return "SET FEATURES [unknown feature]" }
    return f"SET FEATURES [{sub}]"
  }
  table_name(COMMAND_NAMES, command) ?? "[unknown command]"
}

## `DDd+HH:MM:SS.mmm` for a millisecond count, as the error log prints power-on time.
export pure power_up_time(ms: Int) -> Str {
  let millis = ms % 1000
  let seconds = ms / 1000 % 60
  let minutes = ms / 60000 % 60
  let hours = ms / 3600000 % 24
  let days = ms / 86400000
  let clock = f"{zero_pad(hours, 2)}:{zero_pad(minutes, 2)}:{zero_pad(seconds, 2)}.{zero_pad(millis, 3)}"
  if days > 0 { return f"{days}d+{clock}" }
  clock
}

## Error-register mnemonic list for an ATA error.
export pure error_flags(error: Int) -> List[Str] {
  var names: List[Str] = []
  if bit_set(error, 7) { names += ["ICRC"] }
  if bit_set(error, 6) { names += ["UNC"] }
  if bit_set(error, 5) { names += ["MC"] }
  if bit_set(error, 4) { names += ["IDNF"] }
  if bit_set(error, 3) { names += ["MCR"] }
  if bit_set(error, 2) { names += ["ABRT"] }
  if bit_set(error, 1) { names += ["TK0NF"] }
  if bit_set(error, 0) { names += ["AMNF"] }
  names
}

## The device state description of an error log entry.
export pure error_state_text(state: Int) -> Str {
  let code = state % 16
  match code {
    0 => "in an unknown state"
    1 => "sleeping"
    2 => "in standby mode"
    3 => "active or idle"
    4 => "doing SMART Offline or Self-test"
    _ => if code < 11 { "in a reserved state" } else { "in a vendor specific state" }
  }
}

## The `Error: ...` suffix of the register line of one error entry.
export pure error_description(entry: ErrorEntry) -> Str {
  var prefix = ""
  if bit_set(entry.status, 7) { prefix += "Busy; " }
  if bit_set(entry.status, 5) { prefix += "Device Fault; " }
  guard bit_set(entry.status, 0) or entry.error != 0 else { return prefix.trim() }
  var text = prefix + "Error: " + error_flags(entry.error).join(", ")
  if entry.count != 0 { text += f" {entry.count} sectors" }
  if bit_set(entry.device, 6) {
    let lba = entry.device % 16 * 16777216 + entry.lba_high * 65536 + entry.lba_mid * 256 + entry.lba_low
    text += f" at LBA = 0x{hex(lba, 8)} = {lba}"
  } else {
    let cylinder = entry.lba_high * 256 + entry.lba_mid
    text += f" at CHS = 0x{hex(cylinder, 4)}/0x{hex(entry.device % 16, 2)}/0x{hex(entry.lba_low, 2)}"
  }
  text
}

## Decodes the summary SMART error log (log address 1).
export pure error_log(page: Bytes) -> ErrorLog {
  let pointer = at(page, 1)
  let count = le(page, 452, 2)
  var entries: List[ErrorEntry] = []
  if pointer >= 1 and pointer <= 5 {
    for step in range(5) {
      let k = 4 - step
      let slot = (pointer + k) % 5
      let base = 2 + slot * 90
      continue when all_zero(page, base, 90)
      var commands: List[ErrorCommand] = []
      for back in range(5) {
        let c = base + (4 - back) * 12
        continue when all_zero(page, c, 12)
        commands += [{
          device_control: at(page, c),
          features: at(page, c + 1),
          count: at(page, c + 2),
          lba_low: at(page, c + 3),
          lba_mid: at(page, c + 4),
          lba_high: at(page, c + 5),
          device: at(page, c + 6),
          command: at(page, c + 7),
          timestamp_ms: le(page, c + 8, 4),
        }]
      }
      let e = base + 60
      entries += [{
        number: count + k - 4,
        error: at(page, e + 1),
        count: at(page, e + 2),
        lba_low: at(page, e + 3),
        lba_mid: at(page, e + 4),
        lba_high: at(page, e + 5),
        device: at(page, e + 6),
        status: at(page, e + 7),
        state: at(page, e + 27),
        hours: le(page, e + 28, 2),
        commands: commands,
      }]
    }
  }
  {revision: at(page, 0), pointer: pointer, count: count, entries: entries, checksum_ok: checksum_ok(page)}
}

## Decodes the SMART self-test log (log address 6).
export pure self_test_log(page: Bytes) -> SelfTestLog {
  let latest = at(page, 508)
  var entries: List[SelfTestEntry] = []
  var errors = 0
  var outdated = 0
  var superseded_by = 0
  var number = 0
  if latest >= 1 and latest <= 21 {
    for step in range(21) {
      let slot = (20 - step + latest) % 21
      let base = 2 + slot * 24
      continue when all_zero(page, base, 24)
      number += 1
      let status = at(page, base + 1)
      let nibble = status / 16
      let kind = at(page, base)
      if nibble >= 3 and nibble <= 8 {
        errors += 1
        if superseded_by > 0 { outdated += 1 }
      }
      if nibble == 0 and (kind == 2 or kind == 130) and superseded_by == 0 { superseded_by = number }
      entries += [{
        number: number,
        kind: at(page, base),
        status: nibble,
        remaining_percent: status % 16 * 10,
        hours: le(page, base + 2, 2),
        checkpoint: at(page, base + 4),
        lba: le(page, base + 5, 4),
      }]
    }
  }
  {
    revision: le(page, 0, 2),
    entries: entries,
    error_count: errors,
    outdated_count: outdated,
    outdated_by: superseded_by,
    checksum_ok: checksum_ok(page),
  }
}

## The test name of a self-test log entry.
export pure self_test_name(kind: Int) -> Str {
  match kind {
    0 => "Offline"
    1 => "Short offline"
    2 => "Extended offline"
    3 => "Conveyance offline"
    4 => "Selective offline"
    127 => "Abort offline test"
    129 => "Short captive"
    130 => "Extended captive"
    131 => "Conveyance captive"
    132 => "Selective captive"
    _ => if (kind >= 64 and kind <= 126) or kind >= 144 { f"Vendor (0x{hex(kind, 2)})" } else { f"Reserved (0x{hex(kind, 2)})" }
  }
}

## The status text of a self-test log entry (high nibble of the status byte).
export pure self_test_status_text(nibble: Int) -> Str {
  match nibble {
    0 => "Completed without error"
    1 => "Aborted by host"
    2 => "Interrupted (host reset)"
    3 => "Fatal or unknown error"
    4 => "Completed: unknown failure"
    5 => "Completed: electrical failure"
    6 => "Completed: servo/seek failure"
    7 => "Completed: read failure"
    8 => "Completed: handling damage??"
    15 => "Self-test routine in progress"
    _ => f"Unknown status (0x{hex(nibble, 1)})"
  }
}

## Decodes the selective self-test log (log address 9).
export pure selective_log(page: Bytes) -> SelectiveLog {
  var spans: List[SelectiveSpan] = []
  for index in range(5) {
    spans += [{start: le(page, 2 + index * 16, 8), end: le(page, 10 + index * 16, 8)}]
  }
  {
    revision: le(page, 0, 2),
    spans: spans,
    current_lba: le(page, 82, 8),
    current_span: le(page, 90, 2),
    flags: le(page, 92, 2),
    pending_minutes: le(page, 508, 2),
    checksum_ok: checksum_ok(page),
  }
}

## Decodes the SMART log directory (log address 0).
export pure log_directory(page: Bytes) -> LogDirectory {
  var entries: List[LogDirectoryEntry] = [{address: 0, sectors: 1}]
  for address in range(1, 256) {
    let sectors = le(page, address * 2, 2)
    if sectors > 0 { entries += [{address: address, sectors: sectors}] }
  }
  {revision: le(page, 0, 2), entries: entries}
}

## Access mode and description of a log address.
export pure log_name(address: Int) -> LogName {
  let prefix = hex(address, 2) + " "
  for entry in LOG_NAMES {
    if entry.starts_with(prefix) { return {access: entry.byte_slice(3, 3), name: entry.byte_slice(7)} }
  }
  if address >= 128 and address <= 159 { return {access: "R/W", name: "Host vendor specific log"} }
  if address >= 160 and address <= 223 { return {access: "VS", name: "Device vendor specific log"} }
  {access: "-", name: "Reserved log"}
}

## What the offline data collection status byte says the collection did.
export pure offline_activity(status: Int) -> Str {
  match status % 128 {
    0 => "was never started"
    2 => "was completed without error"
    3 => "is in progress"
    4 => "was suspended by an interrupting command from host"
    5 => "was aborted by an interrupting command from host"
    6 => "was aborted by the device with a fatal error"
    _ => if status % 128 >= 64 { "is in a Vendor Specific state" } else { "is in a Reserved state" }
  }
}

## The offline data collection status text of the `General SMART Values` block,
## including the continuation line that reports automatic collection.
export pure offline_status_text(status: Int) -> Str {
  let auto = if status >= 128 { "Enabled" } else { "Disabled" }
  f"Offline data collection activity\n\t\t\t\t\t{offline_activity(status)}.\n\t\t\t\t\tAuto Offline Data Collection: {auto}."
}

## The self-test execution status message.
export pure self_test_exec_text(status: Int) -> Str {
  let indent = "\n\t\t\t\t\t"
  match status / 16 {
    0 => f"The previous self-test routine completed{indent}without error or no self-test has ever {indent}been run."
    1 => f"The self-test routine was aborted by{indent}the host."
    2 => f"The self-test routine was interrupted{indent}by the host with a hard or soft reset."
    3 => f"A fatal error or unknown test error{indent}occurred while the device was executing{indent}its self-test routine and the device {indent}was unable to complete the self-test {indent}routine."
    4 => f"The previous self-test completed having{indent}a test element that failed and the test{indent}element that failed is not known."
    5 => f"The previous self-test completed having{indent}the electrical element of the test{indent}failed."
    6 => f"The previous self-test completed having{indent}the servo (and/or seek) element of the {indent}test failed."
    7 => f"The previous self-test completed having{indent}the read element of the test failed."
    8 => f"The previous self-test completed having{indent}a test element that failed and the{indent}device is suspected of having handling{indent}damage."
    15 => if status % 16 == 0 { f"The previous self-test completed having{indent}with unknown result or self-test in{indent}progress with less than 10% remaining." } else { f"Self-test routine in progress...{indent}{status % 16}0% of test remaining." }
    _ => "Reserved."
  }
}

## A short description of the self-test execution status for machine output.
export pure self_test_exec_name(status: Int) -> Str {
  match status / 16 {
    0 => "completed without error"
    1 => "was aborted by the host"
    2 => "was interrupted by the host with a hard or soft reset"
    3 => "a fatal error or unknown test error occurred while the device was executing its self-test routine and the device was unable to complete the self-test routine"
    4 => "completed having a test element that failed and the test element that failed is not known"
    5 => "completed having the electrical element of the test failed"
    6 => "completed having the servo (and/or seek) test element of the test failed"
    7 => "completed having the read element of the test failed"
    8 => "completed having the handling element of the test failed"
    15 => f"in progress, {status % 16}0% remaining"
    _ => "reserved"
  }
}
