use core.lib.smart
use core.lib.smart_ata as ata
use core.lib.smart_nvme as nvme
use core.lib.smart_scsi as scsi
use core.lib.smart_json as j

type Run = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

# Hexadecimal text to an integer, so register values read like the
# specifications.
pure h(text: Str) -> Int {
  var value = 0
  for index in range(text.byte_len()) {
    let digit = "0123456789abcdef".find(text.byte_slice(index, 1)) ?? 0
    value = value * 16 + digit
  }
  value
}

# A zero page as a list of byte values.
pure cells(length: Int) -> List[Int] {
  [0 for _ in range(length)]
}

# Writes a little-endian field into a list of byte values.
pure with_field(page: List[Int], offset: Int, width: Int, value: Int) -> List[Int] {
  var out = page
  var rest = value
  for index in range(width) {
    out[offset + index] = rest % 256
    rest /= 256
  }
  out
}

# Writes text bytes into a list of byte values.
pure with_text(page: List[Int], offset: Int, text: Str, width: Int) -> List[Int] {
  var out = page
  let raw = bytes.from_text(text)
  for index in range(width) {
    out[offset + index] = if index < raw.len() { raw.byte_at(index) ?? 32 } else { 32 }
  }
  out
}

# Writes an ATA identify string: two characters per word, first one high.
pure with_ata_string(page: List[Int], word: Int, text: Str, words: Int) -> List[Int] {
  var out = page
  let raw = bytes.from_text(text)
  for index in range(words) {
    let first = if index * 2 < raw.len() { raw.byte_at(index * 2) ?? 32 } else { 32 }
    let second = if index * 2 + 1 < raw.len() { raw.byte_at(index * 2 + 1) ?? 32 } else { 32 }
    out[(word + index) * 2] = second
    out[(word + index) * 2 + 1] = first
  }
  out
}

# Makes the byte sum of a 512-byte page zero, as device pages carry.
pure sealed(page: List[Int]) -> Bytes {
  var out = page
  var sum = 0
  for index in range(511) { sum += out[index] }
  out[511] = (256 - sum % 256) % 256
  bytes.from_ints(out) ?? b""
}

pure raw_page(page: List[Int]) -> Bytes {
  bytes.from_ints(page) ?? b""
}

# An IDENTIFY DEVICE page for a 1 TB-class 3.5 inch SATA disk with 4 KiB
# physical sectors, SMART enabled and general purpose logging.
pure identify_page() -> Bytes {
  var page = cells(512)
  page = with_ata_string(page, 10, "WD-WCC4E1234567", 10)
  page = with_ata_string(page, 23, "82.00A82", 4)
  page = with_ata_string(page, 27, "WDC WD10EZEX-00BN5A0", 20)
  page = with_field(page, 120, 4, 268435455)
  page = with_field(page, 152, 2, h("000e"))
  page = with_field(page, 154, 2, h("0006"))
  page = with_field(page, 160, 2, h("07fe"))
  page = with_field(page, 162, 2, h("006d"))
  page = with_field(page, 164, 2, h("0063"))
  page = with_field(page, 166, 2, h("4400"))
  page = with_field(page, 168, 2, h("4020"))
  page = with_field(page, 170, 2, h("0061"))
  page = with_field(page, 172, 2, h("0400"))
  page = with_field(page, 174, 2, h("4100"))
  page = with_field(page, 200, 8, 1953525168)
  page = with_field(page, 212, 2, h("6003"))
  page = with_field(page, 216, 2, h("5001"))
  page = with_field(page, 218, 2, h("4ee2"))
  page = with_field(page, 220, 2, h("abcd"))
  page = with_field(page, 222, 2, h("ef01"))
  page = with_field(page, 336, 2, 2)
  page = with_field(page, 412, 2, h("3037"))
  page = with_field(page, 434, 2, 7200)
  page = with_field(page, 444, 2, h("107e"))
  sealed(page)
}

# One SMART attribute: id, flags, value, worst and six raw bytes.
pure attribute(page: List[Int], slot: Int, id: Int, flags: Int, value: Int, worst: Int, raw: List[Int]) -> List[Int] {
  var out = page
  let base = 2 + slot * 12
  out[base] = id
  out = with_field(out, base + 1, 2, flags)
  out[base + 3] = value
  out[base + 4] = worst
  for index in range(6) { out[base + 5 + index] = raw[index] }
  out
}

# A SMART data page. `pending` is the raw count of attribute 197 and
# `reallocated` the normalized value of attribute 5.
pure smart_data_page(reallocated: Int) -> Bytes {
  var page = cells(512)
  page = with_field(page, 0, 2, 16)
  page = attribute(page, 0, 1, h("000f"), 100, 253, [0, 0, 0, 0, 0, 0])
  page = attribute(page, 1, 3, h("0003"), 150, 148, [h("a7"), h("1a"), 0, 0, 0, 0])
  page = attribute(page, 2, 5, h("0033"), reallocated, 200, [0, 0, 0, 0, 0, 0])
  page = attribute(page, 3, 9, h("0032"), 90, 90, [h("14"), h("1e"), 0, 0, 0, 0])
  page = attribute(page, 4, 194, h("0022"), 110, 100, [40, 0, 20, 50, 0, 0])
  page = attribute(page, 5, 197, h("0032"), 200, 200, [0, 0, 0, 0, 0, 0])
  page[362] = h("82")
  page[363] = 0
  page = with_field(page, 364, 2, 432)
  page[367] = h("7b")
  page = with_field(page, 368, 2, 3)
  page[370] = 1
  page[372] = 2
  page[373] = 60
  page[374] = 5
  sealed(page)
}

pure threshold_page() -> Bytes {
  var page = cells(512)
  page = with_field(page, 0, 2, 16)
  page[2] = 1
  page[3] = 51
  page[14] = 3
  page[15] = 21
  page[26] = 5
  page[27] = 140
  page[38] = 9
  page[50] = 194
  page[62] = 197
  sealed(page)
}

# A summary error log with one error: UNC while reading 8 sectors at LBA
# 0x5e0, 1234 hours into the drive's life.
pure error_log_page(count: Int) -> Bytes {
  var page = cells(512)
  page[0] = 1
  page[1] = if count == 0 { 0 } else { 1 }
  if count > 0 {
    # The newest command is the fifth structure of the entry in slot 0.
    let command = 2 + 4 * 12
    page[command + 2] = 8
    page[command + 3] = h("e0")
    page[command + 4] = 5
    page[command + 6] = h("40")
    page[command + 7] = h("25")
    page = with_field(page, command + 8, 4, 3455)
    let error = 2 + 60
    page[error + 1] = h("40")
    page[error + 2] = 8
    page[error + 3] = h("e0")
    page[error + 4] = 5
    page[error + 6] = h("40")
    page[error + 7] = h("51")
    page[error + 27] = 3
    page = with_field(page, error + 28, 2, 1234)
  }
  page = with_field(page, 452, 2, count)
  sealed(page)
}

# A self-test log: a short test passed at 1000 hours, then an extended test
# that failed with a read failure at 1500 hours with LBA 4096.
pure self_test_page() -> Bytes {
  var page = cells(512)
  page[0] = 1
  page[2] = 1
  page = with_field(page, 4, 2, 1000)
  let second = 2 + 24
  page[second] = 2
  page[second + 1] = h("71")
  page = with_field(page, second + 2, 2, 1500)
  page = with_field(page, second + 5, 4, 4096)
  page[508] = 2
  sealed(page)
}

pure selective_page() -> Bytes {
  var page = cells(512)
  page[0] = 1
  sealed(page)
}

# The SMART log directory carries no checksum, so it is not sealed.
pure directory_page() -> Bytes {
  var page = cells(512)
  page[0] = 1
  page[2] = 1
  page[4] = 5
  page[12] = 1
  page[18] = 1
  page[h("e0") * 2] = 1
  raw_page(page)
}

# The SAT ATA PASS-THROUGH(16) CDB smartmontools sends for a command.
pure ata16(features: Int, count: Int, low: Int, mid: Int, high: Int, command: Int, data: Bool) -> Bytes {
  let protocol = if data { 8 } else { 6 }
  let flags = if data { h("2e") } else { h("20") }
  bytes.from_ints([h("85"), protocol, flags, 0, features, 0, count, 0, low, 0, mid, 0, high, 0, command, 0]) ?? b""
}

pure smart_cdb(features: Int, count: Int, low: Int, data: Bool) -> Bytes {
  ata16(features, count, low, h("4f"), h("c2"), h("b0"), data)
}

# Descriptor-format sense data carrying one ATA Status Return descriptor.
pure ata_sense(mid: Int, high: Int) -> Bytes {
  bytes.from_ints([114, 0, 0, 29, 0, 0, 0, 14, 9, 12, 0, 0, 0, 1, 0, 0, 0, mid, 0, high, 64, 80]) ?? b""
}

pure sg_line(cdb: Bytes, data: Bytes, sense: Bytes, direction: Str) -> Record {
  {
    op: "sg_io",
    cdb: cdb.base64(),
    direction: direction,
    data_len: data.len(),
    status: if sense.len() > 0 { 2 } else { 0 },
    driver_status: if sense.len() > 0 { 8 } else { 0 },
    sense: sense.base64(),
    data: data.base64(),
  }
}

# One fixture exchange for a data-in ATA command.
pure ata_data(cdb: Bytes, data: Bytes) -> Record {
  sg_line(cdb, data, ata_sense(h("4f"), h("c2")), "from_device")
}

pure ata_quiet(cdb: Bytes, mid: Int, high: Int) -> Record {
  {op: "sg_io", cdb: cdb.base64(), status: 2, driver_status: 8, sense: ata_sense(mid, high).base64()}
}

pure identify_line(page: Bytes) -> Record {
  sg_line(ata16(0, 1, 0, 0, 0, h("ec"), true), page, ata_sense(0, 0), "from_device")
}

pure ata_fixture(values: Bytes) -> List[Any] {
  [
    identify_line(identify_page()),
    ata_data(smart_cdb(h("d0"), 1, 0, true), values),
    ata_data(smart_cdb(h("d1"), 1, 0, true), threshold_page()),
    ata_data(smart_cdb(h("d5"), 1, 0, true), directory_page()),
    ata_data(smart_cdb(h("d5"), 1, 1, true), error_log_page(0)),
    ata_data(smart_cdb(h("d5"), 1, 6, true), self_test_page()),
    ata_data(smart_cdb(h("d5"), 1, 9, true), selective_page()),
    ata_quiet(smart_cdb(h("da"), 0, 0, false), h("4f"), h("c2")),
  ]
}

# Installs the linux fake over `lines` and runs the applet.
proc smartctl(ctx: TestContext, lines: List[Any], args: List[Str]) [fs, process, error] -> Run {
  let root = test.temp_dir(ctx)?
  let fixture = fp"{root}/fixture.jsonl"
  fixture.write(json.encode_lines(lines)?)
  fp"{root}/device".write(b"")
  test.linux_fake(ctx, {storage_fixture: fixture, log: fp"{root}/calls.log"})?
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  var argv = args
  argv += [fp"{root}/device".display()]
  test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "smartctl")?
}

proc smartctl_plain(ctx: TestContext, args: List[Str]) [fs, process, error] -> Run {
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  test.run_script(ctx, source, args, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "smartctl")?
}

# An NVMe identify controller page for a 500 GB drive with five power states.
pure nvme_controller_page() -> Bytes {
  var page = cells(4096)
  page = with_field(page, 0, 2, h("144d"))
  page = with_field(page, 2, 2, h("144d"))
  page = with_text(page, 4, "S466NX0K123456", 20)
  page = with_text(page, 24, "Samsung SSD 970 EVO 500GB", 40)
  page = with_text(page, 64, "2B2QEXE7", 8)
  page[73] = h("38")
  page[74] = h("25")
  page[75] = 0
  page[77] = 5
  page = with_field(page, 78, 2, 4)
  page = with_field(page, 80, 4, h("00010300"))
  page = with_field(page, 256, 2, h("0017"))
  page[260] = h("16")
  page[261] = 3
  page[262] = 63
  page[263] = 4
  page = with_field(page, 266, 2, 357)
  page = with_field(page, 268, 2, 358)
  page = with_field(page, 280, 8, 500107862016)
  page = with_field(page, 516, 4, 1)
  page = with_field(page, 520, 2, h("005f"))
  # State 0: 6.20 W; state 3: 0.0400 W (units of 0.0001 W), non-operational.
  page = with_field(page, 2048, 2, 620)
  page = with_field(page, 2048 + 3 * 32, 2, 400)
  page[2048 + 3 * 32 + 3] = 3
  page = with_field(page, 2048 + 3 * 32 + 4, 4, 210)
  page = with_field(page, 2048 + 3 * 32 + 8, 4, 1500)
  raw_page(page)
}

pure nvme_namespace_page() -> Bytes {
  var page = cells(4096)
  page = with_field(page, 0, 8, 976773168)
  page = with_field(page, 8, 8, 976773168)
  page = with_field(page, 16, 8, 100000000)
  page[25] = 1
  page[26] = 0
  let eui = [0, h("25"), h("38"), h("5b"), h("21"), h("b2"), h("c3"), h("d4")]
  for index in range(8) { page[120 + index] = eui[index] }
  page[130] = 9
  page[134] = 12
  page[135] = 1
  raw_page(page)
}

pure nvme_health_page(warning: Int) -> Bytes {
  var page = cells(512)
  page[0] = warning
  page = with_field(page, 1, 2, 308)
  page[3] = 100
  page[4] = 10
  page[5] = 1
  page = with_field(page, 32, 8, 4567890)
  page = with_field(page, 48, 8, 3456789)
  page = with_field(page, 64, 8, 56789012)
  page = with_field(page, 80, 8, 45678901)
  page = with_field(page, 96, 8, 1234)
  page = with_field(page, 112, 8, 234)
  page = with_field(page, 128, 8, 5678)
  page = with_field(page, 144, 8, 45)
  page = with_field(page, 176, 8, 2)
  page = with_field(page, 200, 2, 308)
  raw_page(page)
}

pure nvme_error_page() -> Bytes {
  var page = cells(1024)
  # The status field carries the phase tag in bit 0; 0x4002 is More with
  # Invalid Field in Command.
  page[0] = 2
  page[10] = h("15")
  page = with_field(page, 12, 2, h("4002") * 2 + 1)
  page = with_field(page, 14, 2, h("28"))
  page = with_field(page, 24, 4, 4294967295)
  page[64] = 1
  page = with_field(page, 64 + 12, 2, h("4002") * 2 + 1)
  raw_page(page)
}

pure nvme_self_test_page() -> Bytes {
  var page = cells(564)
  for index in range(20) { page[4 + index * 28] = h("f0") }
  page[4] = 1
  page = with_field(page, 4 + 4, 8, 5000)
  raw_page(page)
}

pure nvme_line(opcode: Int, nsid: Int, cdw10: Int, data: Bytes) -> Record {
  {op: "nvme_admin", opcode: opcode, nsid: nsid, cdw10: cdw10, direction: if data.len() > 0 { "from_device" } else { "none" }, data_len: data.len(), data: data.base64()}
}

pure nvme_fixture() -> List[Any] {
  [
    nvme_line(6, 0, 1, nvme_controller_page()),
    {op: "nvme_namespace_id", nsid: 1},
    nvme_line(6, 1, 0, nvme_namespace_page()),
    nvme_line(2, 4294967295, 127 * 65536 + 2, nvme_health_page(0)),
    nvme_line(2, 4294967295, 255 * 65536 + 1, nvme_error_page()),
    nvme_line(2, 4294967295, 140 * 65536 + 6, nvme_self_test_page()),
    nvme_line(20, 4294967295, 1, b""),
    nvme_line(20, 4294967295, 2, b""),
    nvme_line(20, 4294967295, 15, b""),
  ]
}

pure be32(value: Int) -> List[Int] {
  [value / 16777216 % 256, value / 65536 % 256, value / 256 % 256, value % 256]
}

pure scsi_inquiry_page(vendor: Str) -> Bytes {
  var page = cells(36)
  page[2] = 6
  page = with_text(page, 8, vendor, 8)
  page = with_text(page, 16, "ST300MM0006", 16)
  page = with_text(page, 32, "LS0A", 4)
  raw_page(page)
}

# A data-in SCSI exchange: `length` is the allocation length the command
# asks for; the device returns `data`, which may be shorter.
pure scsi_line(cdb: Bytes, length: Int, data: Bytes) -> Record {
  {op: "sg_io", cdb: cdb.base64(), direction: "from_device", data_len: length, data: data.base64()}
}

# Fixture lines for a SAS disk; `asc` is the informational exceptions code.
pure scsi_fixture(vendor: Str, asc: Int) -> List[Any] {
  let serial = bytes.concat([b"\x00\x80\x00\x08", bytes.from_text("S0K1ABCD")])
  var exceptions = cells(12)
  exceptions[0] = h("2f")
  exceptions[3] = 8
  exceptions[7] = 4
  exceptions[8] = asc
  exceptions[10] = 31
  var temperature = cells(16)
  temperature[0] = h("0d")
  temperature[3] = 12
  temperature[7] = 2
  temperature[9] = 31
  temperature[11] = 1
  temperature[13] = 2
  temperature[15] = 68
  [
    scsi_line(b"\x12\x00\x00\x00\x24\x00", 36, scsi_inquiry_page(vendor)),
    scsi_line(b"\x12\x01\x80\x00\xff\x00", 255, serial),
    scsi_line(b"\x25\x00\x00\x00\x00\x00\x00\x00\x00\x00", 8, bytes.from_ints(be32(585937499) + be32(512)) ?? b""),
    scsi_line(b"\x4d\x00\x6f\x00\x00\x00\x00\x00\xfc\x00", 252, raw_page(exceptions)),
    scsi_line(b"\x4d\x00\x4d\x00\x00\x00\x00\x00\xfc\x00", 252, raw_page(temperature)),
  ]
}

# Runs the applet with a call log of its own and returns the run and the log.
type Logged = {result: Run, log: Str}

proc smartctl_logged(ctx: TestContext, lines: List[Any], args: List[Str]) [fs, process, error] -> Logged {
  let root = test.temp_dir(ctx)?
  let fixture = fp"{root}/fixture.jsonl"
  let log = fp"{root}/calls.log"
  fixture.write(json.encode_lines(lines)?)
  fp"{root}/device".write(b"")
  log.write("")
  test.linux_fake(ctx, {storage_fixture: fixture, log: log})?
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  var argv = args
  argv += [fp"{root}/device".display()]
  let result = test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "smartctl")?
  {result: result, log: log.read_text()?}
}

pure has_line(text: Str, wanted: Str) -> Bool {
  wanted in text.lines()
}

test test_ata_identity_is_decoded_from_identify_device { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(100)), ["-d", "sat", "-i"])
  assert out.status == 0, out.stderr
  assert out.stdout.starts_with("smartctl 7.5 2025-04-30 r5714 [")
  assert has_line(out.stdout, "=== START OF INFORMATION SECTION ===")
  assert has_line(out.stdout, "Device Model:     WDC WD10EZEX-00BN5A0")
  assert has_line(out.stdout, "Serial Number:    WD-WCC4E1234567")
  assert has_line(out.stdout, "LU WWN Device Id: 5 0014ee 2abcdef01")
  assert has_line(out.stdout, "Firmware Version: 82.00A82")
  assert has_line(out.stdout, "User Capacity:    1,000,204,886,016 bytes [1.00 TB]")
  assert has_line(out.stdout, "Sector Sizes:     512 bytes logical, 4096 bytes physical")
  assert has_line(out.stdout, "Rotation Rate:    7200 rpm")
  assert has_line(out.stdout, "Form Factor:      3.5 inches")
  assert has_line(out.stdout, "ATA Version is:   ACS-3 T13/2161-D revision 5")
  assert has_line(out.stdout, "SATA Version is:  SATA 3.1, 6.0 Gb/s (current: 6.0 Gb/s)")
  assert has_line(out.stdout, "SMART support is: Available - device has SMART capability.")
  assert has_line(out.stdout, "SMART support is: Enabled")
  assert has_line(out.stdout, "Write cache is:   Enabled")
  # Only the identify command was issued: -i reads nothing else.
  assert "SMART overall-health" not in out.stdout
}

test test_ata_health_reports_the_return_status_verdict { |ctx|
  let passed = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-H"])
  assert passed.status == 0, passed.stderr
  assert passed.stdout.ends_with("=== START OF READ SMART DATA SECTION ===\nSMART overall-health self-assessment test result: PASSED\n\n")

  # A failing verdict lists the attributes at or below their thresholds; the
  # failing pre-fail attribute also sets the attribute bit.
  var lines: List[Any] = ata_fixture(smart_data_page(100))
  lines[7] = ata_quiet(smart_cdb(h("da"), 0, 0, false), h("f4"), h("2c"))
  let failed = smartctl(ctx, lines, ["-d", "sat", "-H"])
  assert failed.status == 8 + 16, failed.stderr
  assert "SMART overall-health self-assessment test result: FAILED!" in failed.stdout
  assert "Drive failure expected in less than 24 hours. SAVE ALL DATA." in failed.stdout
  assert has_line(failed.stdout, "Failed Attributes:")
  assert has_line(failed.stdout, "  5 Reallocated_Sector_Ct   0x0033   100   200   140    Pre-fail  Always   FAILING_NOW 0")

  # With no readable attribute table the verdict points at the attribute list.
  var unreadable: List[Any] = [identify_line(identify_page()), lines[7]]
  let blind = smartctl(ctx, unreadable, ["-d", "sat", "-H"])
  assert blind.status == 8, blind.stderr
  assert has_line(blind.stdout, "See vendor-specific Attribute list for failed Attributes.")

  # A pass with an attribute that was below its threshold in the past is
  # marked marginal.
  var marginal_page: List[Int] = []
  let healthy = smart_data_page(200)
  for index in range(healthy.len()) { marginal_page += [healthy.byte_at(index) ?? 0] }
  # Attribute 5 sits in slot 2: raise its value above and keep worst below.
  marginal_page[2 + 2 * 12 + 4] = 100
  var marginal_lines: List[Any] = ata_fixture(smart_data_page(200))
  marginal_lines[1] = ata_data(smart_cdb(h("d0"), 1, 0, true), sealed(marginal_page))
  let marginal = smartctl(ctx, marginal_lines, ["-d", "sat", "-H"])
  assert marginal.status == 32, marginal.stderr
  assert has_line(marginal.stdout, "Please note the following marginal Attributes:")
  assert has_line(marginal.stdout, "  5 Reallocated_Sector_Ct   0x0033   200   100   140    Pre-fail  Always   In_the_past 0")
}

test test_ata_attribute_table_layout_thresholds_and_exit_bits { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(100)), ["-d", "sat", "-A"])
  # Attribute 5 is a pre-fail attribute at 100 with threshold 140.
  assert out.status == 16, out.stderr
  assert has_line(out.stdout, "ID# ATTRIBUTE_NAME          FLAG     VALUE WORST THRESH TYPE      UPDATED  WHEN_FAILED RAW_VALUE")
  assert has_line(out.stdout, "  1 Raw_Read_Error_Rate     0x000f   100   253   051    Pre-fail  Always       -       0")
  assert has_line(out.stdout, "  3 Spin_Up_Time            0x0003   150   148   021    Pre-fail  Always       -       6823")
  assert has_line(out.stdout, "  5 Reallocated_Sector_Ct   0x0033   100   200   140    Pre-fail  Always   FAILING_NOW 0")
  assert has_line(out.stdout, "  9 Power_On_Hours          0x0032   090   090   000    Old_age   Always       -       7700")
  assert has_line(out.stdout, "194 Temperature_Celsius     0x0022   110   100   000    Old_age   Always       -       40 (Min/Max 20/50)")
  assert has_line(out.stdout, "SMART Attributes Data Structure revision number: 16")
  assert has_line(out.stdout, "Vendor Specific SMART Attributes with Thresholds:")

  # A healthy table exits 0 and an attribute whose worst value crossed its
  # threshold is reported as failed in the past.
  let healthy = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-A"])
  assert healthy.status == 0, healthy.stderr
  assert "FAILING_NOW" not in healthy.stdout
}

test test_ata_capabilities_block { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(100)), ["-d", "sat", "-c"])
  assert out.status == 0, out.stderr
  assert has_line(out.stdout, "General SMART Values:")
  assert has_line(out.stdout, "Offline data collection status:  (0x82)\tOffline data collection activity")
  assert has_line(out.stdout, "\t\t\t\t\twas completed without error.")
  assert has_line(out.stdout, "\t\t\t\t\tAuto Offline Data Collection: Enabled.")
  assert has_line(out.stdout, "Self-test execution status:      (   0)\tThe previous self-test routine completed")
  assert has_line(out.stdout, "data collection: \t\t(  432) seconds.")
  assert has_line(out.stdout, "capabilities: \t\t\t (0x7b) SMART execute Offline immediate.")
  assert has_line(out.stdout, "\t\t\t\t\tSuspend Offline collection upon new")
  assert has_line(out.stdout, "SMART capabilities:            (0x0003)\tSaves SMART data before entering")
  assert has_line(out.stdout, "Error logging capability:        (0x01)\tError logging supported.")
  assert has_line(out.stdout, "recommended polling time: \t (   2) minutes.")
  assert has_line(out.stdout, "recommended polling time: \t (  60) minutes.")
  assert has_line(out.stdout, "SCT capabilities: \t       (0x3037)\tSCT Status supported.")
}

test test_ata_error_log_entries_and_exit_bit { |ctx|
  var lines: List[Any] = ata_fixture(smart_data_page(200))
  lines[4] = ata_data(smart_cdb(h("d5"), 1, 1, true), error_log_page(1))
  let out = smartctl(ctx, lines, ["-d", "sat", "-l", "error"])
  assert out.status == 64, out.stderr
  assert has_line(out.stdout, "SMART Error Log Version: 1")
  assert has_line(out.stdout, "ATA Error Count: 1")
  assert has_line(out.stdout, "Error 1 occurred at disk power-on lifetime: 1234 hours (51 days + 10 hours)")
  assert has_line(out.stdout, "  When the command that caused the error occurred, the device was active or idle.")
  assert has_line(out.stdout, "  40 51 08 e0 05 00 40  Error: UNC 8 sectors at LBA = 0x000005e0 = 1504")
  assert has_line(out.stdout, "  25 00 08 e0 05 00 40 00      00:00:03.455  READ DMA EXT")
  assert has_line(out.stdout, "\tER = Error register [HEX]")
}

test test_ata_self_test_log_and_exit_bit { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-l", "selftest"])
  assert out.status == 128, out.stderr
  assert has_line(out.stdout, "SMART Self-test log structure revision number 1")
  assert has_line(out.stdout, "Num  Test_Description    Status                  Remaining  LifeTime(hours)  LBA_of_first_error")
  assert has_line(out.stdout, "# 1  Extended offline    Completed: read failure       10%      1500         4096")
  assert has_line(out.stdout, "# 2  Short offline       Completed without error       00%      1000         -")
}

test test_ata_selective_and_directory_logs { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-l", "selective", "-l", "directory"])
  assert out.status == 0, out.stderr
  assert has_line(out.stdout, "SMART Selective self-test log data structure revision number 1")
  assert has_line(out.stdout, " SPAN  MIN_LBA  MAX_LBA  CURRENT_TEST_STATUS")
  assert has_line(out.stdout, "    1        0        0  Not_testing")
  assert has_line(out.stdout, "  After scanning selected spans, do NOT read-scan remainder of disk.")
  assert has_line(out.stdout, "If Selective self-test is pending on power-up, resume after 0 minute delay.")
  assert has_line(out.stdout, "SMART Log Directory Version 1 [multi-sector log support]")
  assert has_line(out.stdout, "Address    Access  R/W   Size  Description")
  assert has_line(out.stdout, "0x00           SL  R/O      1  Log Directory")
  assert has_line(out.stdout, "0x02           SL  R/O      5  Comprehensive SMART error log")
  assert has_line(out.stdout, "0x09           SL  R/W      1  Selective self-test log")
}

test test_all_option_reads_every_section_without_mutating { |ctx|
  let out = smartctl_logged(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-a"])
  assert out.result.status == 128, out.result.stderr
  let order = ["=== START OF INFORMATION SECTION ===", "=== START OF READ SMART DATA SECTION ===", "General SMART Values:", "SMART Attributes Data Structure revision number: 16", "SMART Error Log Version: 1", "SMART Self-test log structure revision number 1", "SMART Selective self-test log data structure revision number 1"]
  var last = -1
  for marker in order {
    let found = out.result.stdout.find(marker) ?? -1
    assert found > last, f"{marker} out of order"
    last = found
  }
  assert "ata_smart_enable" not in out.log
  assert "ata_smart_disable" not in out.log
  assert "ata_smart_start_self_test" not in out.log
}

test test_xall_uses_brief_attribute_columns_and_adds_directory { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-x"])
  assert out.status == 128, out.stderr
  assert has_line(out.stdout, "ID# ATTRIBUTE_NAME          FLAGS    VALUE WORST THRESH FAIL RAW_VALUE")
  assert has_line(out.stdout, "  1 Raw_Read_Error_Rate     POSR--   100   253   051    -    0")
  assert has_line(out.stdout, "                            |______ P prefailure warning")
  assert "Log Directory Version" in out.stdout
}

test test_brief_format_option_changes_columns { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-A", "-f", "brief"])
  assert has_line(out.stdout, "ID# ATTRIBUTE_NAME          FLAGS    VALUE WORST THRESH FAIL RAW_VALUE")
  let old = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-A", "-f", "old"])
  assert has_line(old.stdout, "ID# ATTRIBUTE_NAME          FLAG     VALUE WORST THRESH TYPE      UPDATED  WHEN_FAILED RAW_VALUE")
}

test test_start_self_test_sends_execute_offline_immediate { |ctx|
  var lines = ata_fixture(smart_data_page(200))
  # EXECUTE OFF-LINE IMMEDIATE, subcommand 1 (short) in the LBA low register.
  lines += [ata_quiet(smart_cdb(h("d4"), 0, 1, false), h("4f"), h("c2"))]
  let out = smartctl_logged(ctx, lines, ["-d", "sat", "-t", "short"])
  assert out.result.status == 0, out.result.stderr
  assert "ata_smart_start_self_test" in out.log
  assert has_line(out.result.stdout, "=== START OF OFFLINE IMMEDIATE AND SELF-TEST SECTION ===")
  assert has_line(out.result.stdout, "Sending command: \"Execute SMART Short self-test routine immediately in off-line mode\".")
  assert has_line(out.result.stdout, "Drive command \"Execute SMART Short self-test routine immediately in off-line mode\" successful.")
  assert has_line(out.result.stdout, "Testing has begun.")
  assert has_line(out.result.stdout, "Please wait 2 minutes for test to complete.")
  assert has_line(out.result.stdout, "Use smartctl -X to abort test.")
}

test test_long_conveyance_and_offline_self_tests_use_their_subcommands { |ctx|
  var lines = ata_fixture(smart_data_page(200))
  lines += [
    ata_quiet(smart_cdb(h("d4"), 0, 2, false), h("4f"), h("c2")),
    ata_quiet(smart_cdb(h("d4"), 0, 3, false), h("4f"), h("c2")),
    ata_quiet(smart_cdb(h("d4"), 0, 0, false), h("4f"), h("c2")),
  ]
  let long = smartctl(ctx, lines, ["-d", "sat", "-t", "long"])
  assert long.status == 0, long.stderr
  assert "Execute SMART Extended self-test routine immediately in off-line mode" in long.stdout
  assert has_line(long.stdout, "Please wait 60 minutes for test to complete.")
  let conveyance = smartctl(ctx, lines, ["-d", "sat", "-t", "conveyance"])
  assert "Execute SMART Conveyance self-test routine immediately in off-line mode" in conveyance.stdout
  assert has_line(conveyance.stdout, "Please wait 5 minutes for test to complete.")
  let offline = smartctl(ctx, lines, ["-d", "sat", "-t", "offline"])
  assert "Execute SMART Immediate Offline routine immediately in off-line mode" in offline.stdout
  assert has_line(offline.stdout, "Please wait 432 seconds for test to complete.")
}

test test_abort_sends_the_abort_subcommand { |ctx|
  var lines = ata_fixture(smart_data_page(200))
  lines += [ata_quiet(smart_cdb(h("d4"), 0, 127, false), h("4f"), h("c2"))]
  let out = smartctl(ctx, lines, ["-d", "sat", "-X"])
  assert out.status == 0, out.stderr
  assert has_line(out.stdout, "Sending command: \"Abort SMART off-line mode self-test routine\".")
  assert has_line(out.stdout, "Self-testing aborted!")
}

test test_smart_enable_and_disable_commands { |ctx|
  var lines = ata_fixture(smart_data_page(200))
  lines += [
    ata_quiet(smart_cdb(h("d8"), 0, 0, false), h("4f"), h("c2")),
    ata_quiet(smart_cdb(h("d9"), 0, 0, false), h("4f"), h("c2")),
  ]
  let on = smartctl_logged(ctx, lines, ["-d", "sat", "-s", "on"])
  assert on.result.status == 0, on.result.stderr
  assert "ata_smart_enable" in on.log
  assert has_line(on.result.stdout, "=== START OF ENABLE/DISABLE COMMANDS SECTION ===")
  assert has_line(on.result.stdout, "SMART Enabled.")
  let off = smartctl_logged(ctx, lines, ["-d", "sat", "-s", "off"])
  assert "ata_smart_disable" in off.log
  assert has_line(off.result.stdout, "SMART Disabled. Use option -s with argument 'on' to enable it.")
}

test test_disabled_smart_stops_read_commands { |ctx|
  var page = identify_page().base64().base64_decode()?
  var cells_in: List[Int] = []
  for index in range(page.len()) { cells_in += [page.byte_at(index) ?? 0] }
  cells_in[170] = 0
  var lines = ata_fixture(smart_data_page(200))
  lines[0] = identify_line(sealed(cells_in))
  let out = smartctl(ctx, lines, ["-d", "sat", "-H"])
  assert out.status == 4, out.stderr
  assert has_line(out.stdout, "SMART Disabled. Use option -s with argument 'on' to enable it.")
  assert "overall-health" not in out.stdout
}

test test_json_report_has_smartctl_key_order_and_decoded_values { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-j", "-i", "-H", "-A"])
  assert out.status == 0, out.stderr
  let root = json.decode(out.stdout)?
  assert json.get(root, ["json_format_version"])? == [1, 0]
  assert json.get(root, ["smartctl", "version"])? == [7, 5]
  assert json.get(root, ["smartctl", "exit_status"])? == 0
  assert json.get(root, ["device", "type"])? == "sat"
  assert json.get(root, ["device", "protocol"])? == "ATA"
  assert json.get(root, ["model_name"])? == "WDC WD10EZEX-00BN5A0"
  assert json.get(root, ["user_capacity", "bytes"])? == 1000204886016
  assert json.get(root, ["smart_status", "passed"])? == true
  assert json.get(root, ["ata_smart_attributes", "table", 1, "name"])? == "Spin_Up_Time"
  assert json.get(root, ["ata_smart_attributes", "table", 1, "raw", "value"])? == 6823
  assert json.get(root, ["power_on_time", "hours"])? == 7700
  assert json.get(root, ["temperature", "current"])? == 40
  # Keys keep smartctl's order rather than sorting alphabetically.
  let first = out.stdout.find("\"json_format_version\"") ?? -1
  let second = out.stdout.find("\"smartctl\"") ?? -1
  let third = out.stdout.find("\"local_time\"") ?? -1
  let fourth = out.stdout.find("\"device\"") ?? -1
  assert first >= 0 and first < second and second < third and third < fourth
  assert out.stdout.starts_with("{\n  \"json_format_version\": [\n    1,\n    0\n  ],\n  \"smartctl\": {")
}

test test_json_compact_is_one_line { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "--json=c", "-H"])
  assert out.status == 0, out.stderr
  assert out.stdout.count_lines() == 1
  assert out.stdout.starts_with("{\"json_format_version\":[1,0],\"smartctl\":{\"version\":[7,5]")
}

test test_json_failure_is_reported_in_messages { |ctx|
  let out = smartctl_plain(ctx, ["-j", "-a", "/nonexistent/sda"])
  assert out.status == 2, out.stderr
  let root = json.decode(out.stdout)?
  assert json.get(root, ["smartctl", "exit_status"])? == 2
  let message = json.get(root, ["smartctl", "messages", 0, "string"])?.require(Str)?
  assert message.starts_with("Smartctl open device: /nonexistent/sda failed:")
}

test test_command_line_errors_use_smartctl_wording_and_status { |ctx|
  let missing = smartctl_plain(ctx, ["-a"])
  assert missing.status == 1
  assert "ERROR: smartctl requires a device name as the final command-line argument.\n\n\nUse smartctl -h to get a usage summary\n\n" in missing.stdout

  let two = smartctl_plain(ctx, ["-a", "/dev/sda", "/dev/sdb"])
  assert two.status == 1
  assert "ERROR: smartctl takes ONE device name as the final command-line argument.\nYou have provided 2 device names:\n/dev/sda\n/dev/sdb\n\nUse smartctl -h to get a usage summary\n" in two.stdout

  let bad_test = smartctl_plain(ctx, ["-t", "bogus", "/dev/sda"])
  assert bad_test.status == 1
  assert "=======> INVALID ARGUMENT TO -t: bogus\n=======> VALID ARGUMENTS ARE: offline, short, long, conveyance, force, vendor,N, select,M-N, pending,N, afterselect,[on|off] <=======\n\nUse smartctl -h to get a usage summary\n" in bad_test.stdout

  let unknown = smartctl_plain(ctx, ["--bogus", "/dev/sda"])
  assert unknown.status == 1
  assert "=======> UNRECOGNIZED OPTION: bogus\n\nUse smartctl -h to get a usage summary\n" in unknown.stdout

  let short_unknown = smartctl_plain(ctx, ["-y", "/dev/sda"])
  assert "=======> UNRECOGNIZED OPTION: y\n" in short_unknown.stdout

  let bad_switch = smartctl_plain(ctx, ["-s", "maybe", "/dev/sda"])
  assert bad_switch.status == 1
  assert "=======> INVALID ARGUMENT TO -s: maybe\n=======> VALID ARGUMENTS ARE: on, off, aam," in bad_switch.stdout

  let bad_type = smartctl_plain(ctx, ["-d", "bogus", "/dev/sda"])
  assert bad_type.status == 1
  assert "/dev/sda: Unknown device type 'bogus'\n=======> VALID ARGUMENTS ARE: ata, scsi[+TYPE]" in bad_type.stdout

  let sat_size = smartctl_plain(ctx, ["-d", "sat,13", "/dev/sda"])
  assert sat_size.status == 1
  assert "/dev/sda: Option '-d sat[,auto][,N]' requires N to be 0, 12 or 16" in sat_size.stdout

  let undetected = smartctl_plain(ctx, ["-a", "/dev/null"])
  assert undetected.status == 1
  assert "/dev/null: Unable to detect device type\nPlease specify device type with the -d option.\n" in undetected.stdout
}

test test_unsupported_options_fail_before_any_device_command { |ctx|
  for option in [["-o", "on"], ["-S", "on"], ["-C"], ["-g", "all"], ["-v", "9,minutes"], ["-n", "standby"], ["-r", "ioctl"], ["--identify"], ["-l", "scttemp"], ["-l", "xerror,5"], ["-t", "select,0-100"], ["-s", "wcache,on"], ["-T", "permissive"], ["-F", "samsung"], ["-B", "drivedb.h"], ["-P", "showall"], ["-f", "hex"], ["-d", "usbcypress"], ["-q", "bogus"]] {
    let out = smartctl_logged(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-a"] + option)
    assert out.result.status == 1, f"{option.join(" ")}: {out.result.stdout}{out.result.stderr}"
    assert out.log == "", f"{option.join(" ")} reached the device"
  }
}

test test_only_one_test_or_abort_and_one_device_type_are_allowed { |ctx|
  let two_tests = smartctl_plain(ctx, ["-t", "short", "-t", "long", "/dev/sda"])
  assert two_tests.status == 1
  assert "ERROR: smartctl can only run a single test type (or abort) at a time." in two_tests.stdout
  let abort_and_test = smartctl_plain(ctx, ["-t", "short", "-X", "/dev/sda"])
  assert abort_and_test.status == 1
  assert "ERROR: smartctl can only run a single test type (or abort) at a time." in abort_and_test.stdout
  let two_types = smartctl_plain(ctx, ["-d", "ata", "-d", "sat", "/dev/sda"])
  assert two_types.status == 1
  assert "ERROR: multiple -d TYPE options are only allowed with --scan" in two_types.stdout
  let question = smartctl_plain(ctx, ["-?"])
  assert question.status == 0
  assert "Usage: smartctl [options] device" in question.stdout
}

test test_help_and_version_need_no_device { |ctx|
  let help = smartctl_plain(ctx, ["-h"])
  assert help.status == 0
  assert "Usage: smartctl [options] device" in help.stdout
  assert "-t TEST, --test=TEST" in help.stdout
  let version = smartctl_plain(ctx, ["-V"])
  assert version.status == 0
  assert version.stdout.starts_with("smartctl 7.5 2025-04-30 r5714 [")
  assert "ABSOLUTELY NO WARRANTY" in version.stdout
  let long_form = smartctl_plain(ctx, ["--vers"])
  assert "ABSOLUTELY NO WARRANTY" in long_form.stdout
}

test test_device_open_failure_exits_2 { |ctx|
  let out = smartctl_plain(ctx, ["-d", "sat", "-i", "/nonexistent/sda"])
  assert out.status == 2
  assert "Smartctl open device: /nonexistent/sda failed:" in out.stdout
}

test test_failed_identify_names_the_kernel_error { |ctx|
  let out = smartctl(ctx, [], ["-d", "sat", "-i"])
  assert out.status == 2
  assert "Read Device Identity failed:" in out.stdout
  assert "A mandatory SMART command failed: exiting. To continue, add one or more '-T permissive' options." in out.stdout
}

test test_auto_detection_uses_inquiry_to_find_sat { |ctx|
  let root = test.temp_dir(ctx)?
  let device = fp"{root}/sda"
  device.write(b"")
  let inquiry = scsi_line(b"\x12\x00\x00\x00\x24\x00", 36, scsi_inquiry_page("ATA"))
  var lines = ata_fixture(smart_data_page(200))
  lines += [inquiry]
  let fixture = fp"{root}/fixture.jsonl"
  fixture.write(json.encode_lines(lines)?)
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  let out = test.run_script(ctx, source, ["-i", "-j", device.display()], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "smartctl")?
  assert out.status == 0, out.stderr
  let parsed = json.decode(out.stdout)?
  assert json.get(parsed, ["device", "type"])? == "sat"
  assert json.get(parsed, ["device", "info_name"])? == f"{device.display()} [SAT]"
  assert json.get(parsed, ["model_name"])? == "WDC WD10EZEX-00BN5A0"
}

test test_nvme_info_health_attributes_and_logs { |ctx|
  let nvme_device = test.temp_dir(ctx)?
  fp"{nvme_device}/nvme0".write(b"")
  let fixture = fp"{nvme_device}/fixture.jsonl"
  fixture.write(json.encode_lines(nvme_fixture())?)
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  let out = test.run_script(ctx, source, ["-a", fp"{nvme_device}/nvme0".display()], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "smartctl")?
  # Two logged errors set the error-log bit.
  assert out.status == 64, out.stderr
  assert has_line(out.stdout, "Model Number:                       Samsung SSD 970 EVO 500GB")
  assert has_line(out.stdout, "Serial Number:                      S466NX0K123456")
  assert has_line(out.stdout, "PCI Vendor/Subsystem ID:            0x144d")
  assert has_line(out.stdout, "IEEE OUI Identifier:                0x002538")
  assert has_line(out.stdout, "Total NVM Capacity:                 500,107,862,016 [500 GB]")
  assert has_line(out.stdout, "NVMe Version:                       1.3")
  assert has_line(out.stdout, "Namespace 1 Size/Capacity:          500,107,862,016 [500 GB]")
  assert has_line(out.stdout, "Namespace 1 IEEE EUI-64:            002538 5b21b2c3d4")
  assert has_line(out.stdout, "Firmware Updates (0x16):            3 Slots, no Reset required")
  assert has_line(out.stdout, "Optional Admin Commands (0x0017):   Security Format Frmw_DL Self_Test")
  assert has_line(out.stdout, "=== START OF SMART DATA SECTION ===")
  assert has_line(out.stdout, "SMART overall-health self-assessment test result: PASSED")
  assert has_line(out.stdout, "SMART/Health Information (NVMe Log 0x02, NSID 0xffffffff)")
  assert has_line(out.stdout, "Temperature:                        35 Celsius")
  assert has_line(out.stdout, "Data Units Read:                    4,567,890 [2.33 TB]")
  assert has_line(out.stdout, "Data Units Written:                 3,456,789 [1.76 TB]")
  assert has_line(out.stdout, "Error Information (NVMe Log 0x01, 16 of 64 entries)")
  assert has_line(out.stdout, "  0          2     0  0x0015  0x4002  0x028            0     -     -  Invalid Field in Command")
  assert has_line(out.stdout, "Self-test Log (NVMe Log 0x06, NSID 0xffffffff)")
  assert has_line(out.stdout, "Maximum Data Transfer Size:         32 Pages")
  assert has_line(out.stdout, "Warning  Comp. Temp. Threshold:     84 Celsius")
  assert has_line(out.stdout, "Supported Power States")
  assert has_line(out.stdout, " 0 +     6.20W        -        -   0  0  0  0        0       0")
  assert has_line(out.stdout, " 3 -   0.0400W        -        -   0  0  0  0      210    1500")
  assert has_line(out.stdout, "Supported LBA Sizes (NSID 0x1)")
  assert has_line(out.stdout, " 1 -    4096       0         1")
  assert "Namespace 1 Features" not in out.stdout
  assert has_line(out.stdout, "Self-test status: No self-test in progress")
}

test test_nvme_json_and_failed_health { |ctx|
  let root = test.temp_dir(ctx)?
  fp"{root}/nvme0".write(b"")
  var lines = nvme_fixture()
  lines[3] = nvme_line(2, 4294967295, 127 * 65536 + 2, nvme_health_page(4))
  let fixture = fp"{root}/fixture.jsonl"
  fixture.write(json.encode_lines(lines)?)
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  let out = test.run_script(ctx, source, ["-j", "-i", "-H", "-A", fp"{root}/nvme0".display()], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "smartctl")?
  assert out.status == 8, out.stderr
  let parsed = json.decode(out.stdout)?
  assert json.get(parsed, ["device", "type"])? == "nvme"
  assert json.get(parsed, ["model_name"])? == "Samsung SSD 970 EVO 500GB"
  assert json.get(parsed, ["smart_status", "passed"])? == false
  assert json.get(parsed, ["smart_status", "nvme", "value"])? == 4
  assert json.get(parsed, ["nvme_smart_health_information_log", "temperature"])? == 35
  assert json.get(parsed, ["nvme_smart_health_information_log", "data_units_read"])? == 4567890
  assert json.get(parsed, ["nvme_namespaces", 0, "size", "bytes"])? == 500107862016
}

test test_nvme_self_test_commands { |ctx|
  let root = test.temp_dir(ctx)?
  fp"{root}/nvme0".write(b"")
  let fixture = fp"{root}/fixture.jsonl"
  fixture.write(json.encode_lines(nvme_fixture())?)
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  let scope = {XSH_MODULE_PATH: ctx.core_dir.display()}
  let short = test.run_script(ctx, source, ["-t", "short", fp"{root}/nvme0".display()], scope, b"", "smartctl")?
  assert short.status == 0, short.stderr
  assert has_line(short.stdout, "Self-test has begun")
  assert has_line(short.stdout, "Use smartctl -X to abort test")
  let long = test.run_script(ctx, source, ["-t", "long", fp"{root}/nvme0".display()], scope, b"", "smartctl")?
  assert long.status == 0, long.stderr
  let abort = test.run_script(ctx, source, ["-X", fp"{root}/nvme0".display()], scope, b"", "smartctl")?
  assert abort.status == 0, abort.stderr
  assert has_line(abort.stdout, "Self-test aborted!")
}

test test_scsi_identity_health_and_temperature { |ctx|
  let out = smartctl(ctx, scsi_fixture("SEAGATE", 0), ["-d", "scsi", "-i", "-H", "-A"])
  assert out.status == 0, out.stderr
  assert has_line(out.stdout, "Vendor:               SEAGATE")
  assert has_line(out.stdout, "Product:              ST300MM0006")
  assert has_line(out.stdout, "Compliance:           SPC-4")
  assert has_line(out.stdout, "User Capacity:        300,000,000,000 bytes [300 GB]")
  assert has_line(out.stdout, "Serial number:        S0K1ABCD")
  assert has_line(out.stdout, "SMART Health Status: OK")
  assert has_line(out.stdout, "Current Drive Temperature:     31 C")
  assert has_line(out.stdout, "Drive Trip Temperature:        68 C")
  let failing = smartctl(ctx, scsi_fixture("SEAGATE", h("5d")), ["-d", "scsi", "-H"])
  assert failing.status == 8, failing.stderr
  assert "FAILURE PREDICTION THRESHOLD EXCEEDED" in failing.stdout
}

test test_scsi_rejects_options_it_cannot_serve { |ctx|
  for option in [["-c"], ["-l", "error"], ["-t", "short"], ["-s", "on"], ["-x"]] {
    let out = smartctl_logged(ctx, scsi_fixture("SEAGATE", 0), ["-d", "scsi"] + option)
    assert out.result.status == 1, option.join(" ")
    assert out.log == ""
  }
}

test test_quiet_modes { |ctx|
  let silent = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-q", "silent", "-H"])
  assert silent.status == 0
  assert silent.stdout == ""
  let errors = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-q", "errorsonly", "-H", "-A"])
  assert errors.status == 0
  assert errors.stdout == ""
  let noserial = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-q", "noserial", "-i"])
  assert "Serial Number" not in noserial.stdout
  assert "LU WWN Device Id" not in noserial.stdout
  assert "Device Model:     WDC WD10EZEX-00BN5A0" in noserial.stdout
  let json_run = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-q", "noserial", "-i", "-j"])
  assert "serial_number" not in json_run.stdout
  assert "\"wwn\"" not in json_run.stdout
}

test test_checksum_options { |ctx|
  var broken = smart_data_page(200)
  var cells_in: List[Int] = []
  for index in range(broken.len()) { cells_in += [broken.byte_at(index) ?? 0] }
  cells_in[511] = (cells_in[511] + 1) % 256
  var lines = ata_fixture(smart_data_page(200))
  lines[1] = ata_data(smart_cdb(h("d0"), 1, 0, true), raw_page(cells_in))
  let warn = smartctl(ctx, lines, ["-d", "sat", "-A"])
  assert warn.status == 4, warn.stderr
  assert "Warning! SMART Attribute Data Structure error: invalid SMART checksum." in warn.stdout
  assert "Reallocated_Sector_Ct" in warn.stdout
  let ignored = smartctl(ctx, lines, ["-d", "sat", "-A", "-b", "ignore"])
  assert ignored.status == 0, ignored.stderr
  assert "invalid SMART checksum" not in ignored.stdout
  let stopped = smartctl(ctx, lines, ["-d", "sat", "-A", "-b", "exit"])
  assert stopped.status == 4
  assert "Reallocated_Sector_Ct" not in stopped.stdout
}

test test_xerror_falls_back_to_the_summary_log_with_a_notice { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-d", "sat", "-l", "xerror"])
  assert out.status == 0, out.stderr
  assert has_line(out.stdout, "SMART Error Log Version: 1")
  assert "READ LOG EXT is not available" in out.stderr
}

test test_scan_lists_candidates_in_smartctl_format { |ctx|
  let out = smartctl_plain(ctx, ["--scan"])
  assert out.status == 0, out.stderr
  for line in out.stdout.lines() {
    assert rx"^/dev/[a-z0-9]+ -d (scsi|nvme) # /dev/[a-z0-9]+, (SCSI|NVMe) device$".matches(line), line
  }
  let json_scan = smartctl_plain(ctx, ["--scan", "-j"])
  assert json_scan.status == 0, json_scan.stderr
  let parsed = json.decode(json_scan.stdout)?
  assert json.get(parsed, ["smartctl", "argv"])? == ["smartctl", "--scan", "-j"]
  assert json.get(parsed, ["devices"])? is List[Any]
}

# sysfs lists devices whose node was never created (a container shares /sys
# but not /dev); a scan must offer only nodes that exist.
test test_scan_offers_only_devices_that_have_a_node { |ctx|
  let root = test.temp_dir(ctx, name: "scan-roots")?
  let sys = fp"{root}/sys"
  let dev = fp"{root}/dev"
  for name in ["sda", "sdb", "nvme0n1", "nvme1n1"] { fp"{sys}/block/{name}".mkdir() }
  for name in ["nvme0", "nvme1"] { fp"{sys}/class/nvme/{name}".mkdir() }
  dev.mkdir()
  for name in ["sda", "nvme0", "nvme0n1"] { fp"{dev}/{name}".write(b"") }
  let present = smart.present_candidates(sys, dev)?
  assert [item.name for item in present] == ["sda", "nvme0", "nvme0n1"]
  assert [item.path for item in present] == [fp"{dev}/sda", fp"{dev}/nvme0", fp"{dev}/nvme0n1"]
  for name in ["sdb", "nvme1", "nvme1n1"] { fp"{dev}/{name}".write(b"") }
  assert smart.present_candidates(sys, dev)?.len() == 6
}

test test_option_abbreviations_and_bundles { |ctx|
  let out = smartctl(ctx, ata_fixture(smart_data_page(200)), ["--dev=sat", "-iH", "--hea"])
  assert out.status == 0, out.stderr
  assert "=== START OF INFORMATION SECTION ===" in out.stdout
  assert "test result: PASSED" in out.stdout
  let attached = smartctl(ctx, ata_fixture(smart_data_page(200)), ["-dsat", "-lselftest"])
  assert attached.status == 128, attached.stderr
  assert "SMART Self-test log structure revision number 1" in attached.stdout
}

test test_decoders_format_capacity_with_three_significant_digits {
  assert smart.capacity_text(1000204886016) == "1.00 TB"
  assert smart.capacity_text(2048408248320) == "2.04 TB"
  assert smart.capacity_text(15362991415296) == "15.3 TB"
  assert smart.capacity_text(500107862016) == "500 GB"
  assert smart.capacity_text(8001563222016) == "8.00 TB"
  assert smart.capacity_text(51200000000) == "51.2 GB"
  assert smart.thousands(0) == "0"
  assert smart.thousands(1000) == "1,000"
  assert smart.thousands(1000204886016) == "1,000,204,886,016"
}

test test_decoders_render_raw_temperatures {
  assert smart.raw_text(194, [30, 0, 0, 0, 0, 0]) == "30"
  assert smart.raw_text(194, [36, 0, 20, 0, 0, 0]) == "36 (0 20 0 0 0)"
  assert smart.raw_text(194, [38, 0, 20, 45, 1, 0]) == "38 (Min/Max 20/45 #1)"
  assert smart.raw_text(9, [h("14"), h("1e"), 0, 0, 0, 0]) == "7700"
  assert smart.raw_text(1, [1, 2, 3, 4, 5, 6]) == "6618611909121"
  assert smart.attribute_name(5) == "Reallocated_Sector_Ct"
  assert smart.attribute_name(231) == "Temperature_Celsius"
  assert smart.attribute_name(77) == "Unknown_Attribute"
}

test test_decoders_name_ata_versions_and_error_commands {
  var words = cells(512)
  words = with_field(words, 160, 2, h("03fe"))
  words = with_field(words, 162, 2, h("0039"))
  let version = smart.ata_version(smart.words(raw_page(words)))
  assert version.text == "ACS-2, ATA8-ACS T13/1699-D revision 4c"
  words = with_field(words, 162, 2, 0)
  assert smart.ata_version(smart.words(raw_page(words))).text == "ACS-2 (minor revision not indicated)"
  assert smart.command_name(h("c8"), 0) == "READ DMA"
  assert smart.command_name(h("ef"), 3) == "SET FEATURES [Set transfer mode]"
  assert smart.command_name(h("b0"), h("d0")) == "SMART READ DATA"
  assert smart.power_up_time(15 * 86400000 + 1180376) == "15d+00:19:40.376"
  assert smart.power_up_time(4000) == "00:00:04.000"
  assert smart.error_flags(h("44")) == ["UNC", "ABRT"]
}

test test_json_self_test_start_reports_information_messages { |ctx|
  var lines = ata_fixture(smart_data_page(200))
  lines += [ata_quiet(smart_cdb(h("d4"), 0, 1, false), h("4f"), h("c2"))]
  let out = smartctl(ctx, lines, ["-d", "sat", "-j", "-t", "short"])
  assert out.status == 0, out.stderr
  let parsed = json.decode(out.stdout)?
  var texts: List[Str] = []
  var severities: List[Str] = []
  for index in range(6) {
    texts += [json.get(parsed, ["smartctl", "messages", index, "string"])?.require(Str)?]
    severities += [json.get(parsed, ["smartctl", "messages", index, "severity"])?.require(Str)?]
  }
  assert "Testing has begun." in texts
  assert "Use smartctl -X to abort test." in texts
  assert "Please wait 2 minutes for test to complete." in texts
  assert severities[0] == "information"
}

test test_json_error_log_table_is_inside_the_summary { |ctx|
  var lines = ata_fixture(smart_data_page(200))
  lines[4] = ata_data(smart_cdb(h("d5"), 1, 1, true), error_log_page(1))
  let out = smartctl(ctx, lines, ["-d", "sat", "-j", "-l", "error"])
  assert out.status == 64, out.stderr
  let parsed = json.decode(out.stdout)?
  assert json.get(parsed, ["ata_smart_error_log", "summary", "count"])? == 1
  assert json.get(parsed, ["ata_smart_error_log", "summary", "logged_count"])? == 1
  assert json.get(parsed, ["ata_smart_error_log", "summary", "table", 0, "lifetime_hours"])? == 1234
  assert json.get(parsed, ["ata_smart_error_log", "summary", "table", 0, "previous_commands", 0, "command_name"])? == "READ DMA EXT"
}

test test_nvme_namespace_id_is_hexadecimal { |ctx|
  let root = test.temp_dir(ctx)?
  fp"{root}/nvme0".write(b"")
  let fixture = fp"{root}/fixture.jsonl"
  # Namespace 2 is identified instead of the namespace the device node names.
  var lines = nvme_fixture()
  lines += [nvme_line(6, 2, 0, nvme_namespace_page())]
  fixture.write(json.encode_lines(lines)?)
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  let scope = {XSH_MODULE_PATH: ctx.core_dir.display()}
  let named = test.run_script(ctx, source, ["-d", "nvme,0x2", "-i", fp"{root}/nvme0".display()], scope, b"", "smartctl")?
  assert named.status == 0, named.stderr
  assert "Namespace 2 Size/Capacity:" in named.stdout
  let decimal = test.run_script(ctx, source, ["-d", "nvme,2", "-i", fp"{root}/nvme0".display()], scope, b"", "smartctl")?
  assert decimal.status == 1
  assert "Invalid NVMe namespace id in 'nvme,2'" in decimal.stdout
}

test test_nvme_rejects_logs_it_does_not_have { |ctx|
  let root = test.temp_dir(ctx)?
  fp"{root}/nvme0".write(b"")
  let fixture = fp"{root}/fixture.jsonl"
  fixture.write(json.encode_lines(nvme_fixture())?)
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  let scope = {XSH_MODULE_PATH: ctx.core_dir.display()}
  for option in [["-l", "directory"], ["-l", "selective"], ["-s", "on"]] {
    let out = test.run_script(ctx, source, option + [fp"{root}/nvme0".display()], scope, b"", "smartctl")?
    assert out.status == 1, option.join(" ")
    assert "not supported for NVMe devices" in out.stderr
  }
}

test test_sat_auto_probes_with_inquiry_first { |ctx|
  let out = smartctl(ctx, [], ["-d", "sat,auto", "-i"])
  assert out.status == 2
  assert "failed: INQUIRY [SAT]:" in out.stdout
}

test test_scan_type_filter_and_unknown_type { |ctx|
  let nvme_only = smartctl_plain(ctx, ["--scan", "-d", "nvme"])
  assert nvme_only.status == 0, nvme_only.stderr
  for line in nvme_only.stdout.lines() { assert " -d nvme # " in line }
  let scsi_only = smartctl_plain(ctx, ["--scan", "-d", "scsi"])
  for line in scsi_only.stdout.lines() { assert " -d scsi # " in line }
  let unknown = smartctl_plain(ctx, ["--scan", "-d", "bogus"])
  assert unknown.status == 1
  assert "Unknown device type 'bogus'" in unknown.stdout
}

test test_ata_brief_and_badsum_options_do_not_apply_to_other_transports { |ctx|
  for option in [["-f", "brief"], ["-b", "ignore"]] {
    let scsi_run = smartctl_logged(ctx, scsi_fixture("SEAGATE", 0), ["-d", "scsi", "-H"] + option)
    assert scsi_run.result.status == 1, option.join(" ")
    assert scsi_run.log == ""
  }
}

test test_twelve_byte_sat_form_is_selected_by_the_device_type { |ctx|
  let identify_12 = bytes.from_ints([h("a1"), 8, h("2e"), 0, 1, 0, 0, 0, 0, h("ec"), 0, 0]) ?? b""
  let lines: List[Any] = [sg_line(identify_12, identify_page(), ata_sense(0, 0), "from_device")]
  let out = smartctl(ctx, lines, ["-d", "sat,12", "-i"])
  assert out.status == 0, out.stderr
  assert has_line(out.stdout, "Device Model:     WDC WD10EZEX-00BN5A0")
  let sixteen = smartctl(ctx, lines, ["-d", "sat,16", "-i"])
  assert sixteen.status == 2
}

test test_nvme_noserial_omits_serial_and_eui { |ctx|
  let root = test.temp_dir(ctx)?
  fp"{root}/nvme0".write(b"")
  let fixture = fp"{root}/fixture.jsonl"
  fixture.write(json.encode_lines(nvme_fixture())?)
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let source = fp"{ctx.core_dir}/smartctl.xsh".read_text()?
  let out = test.run_script(ctx, source, ["-q", "noserial", "-i", fp"{root}/nvme0".display()], {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "smartctl")?
  assert out.status == 0, out.stderr
  assert "Model Number:" in out.stdout
  assert "Serial Number" not in out.stdout
  assert "EUI-64" not in out.stdout
}

test test_device_without_smart_capability_is_reported { |ctx|
  var cells_in: List[Int] = []
  let page = identify_page()
  for index in range(page.len()) { cells_in += [page.byte_at(index) ?? 0] }
  # Word 82 bit 0 clear: SMART is not supported.
  cells_in[164] = 0
  var lines = ata_fixture(smart_data_page(200))
  lines[0] = identify_line(sealed(cells_in))
  let out = smartctl(ctx, lines, ["-d", "sat", "-A"])
  assert out.status == 4, out.stderr
  assert "SMART support is: Unavailable - device lacks SMART capability." in out.stdout
  assert "Reallocated_Sector_Ct" not in out.stdout
}

test test_nvme_failed_health_names_each_warning_bit {
  let log = nvme.health(nvme_health_page(h("0b")))
  let text = nvme.health_text(log)
  assert text.starts_with("SMART overall-health self-assessment test result: FAILED!\n")
  assert "- available spare has fallen below threshold\n" in text
  assert "- temperature is above or below threshold\n" in text
  assert "- media has been placed in read only mode\n" in text
  assert "- NVM subsystem reliability" not in text
  let unknown = nvme.health_text(nvme.health(nvme_health_page(h("80"))))
  assert "- unknown critical warning(s) (0x80)" in unknown
}

test test_old_failed_self_tests_are_outdated_by_a_newer_successful_extended_test { |ctx|
  var page = cells(512)
  page[0] = 1
  # Slot 0: failed short test; slot 1: successful extended test, newer.
  page[2] = 1
  page[3] = h("71")
  page[2 + 24] = 2
  page[2 + 24 + 1] = 0
  page = with_field(page, 2 + 24 + 2, 2, 2000)
  page = with_field(page, 2 + 2, 2, 1000)
  page[508] = 2
  let decoded = smart.self_test_log(sealed(page))
  assert decoded.error_count == 1
  assert decoded.outdated_count == 1
  assert decoded.outdated_by == 1
  let text = ata.self_test_log_text(decoded)
  assert "1 of 1 failed self-tests are outdated by newer successful extended offline self-test # 1\n" in text

  var lines = ata_fixture(smart_data_page(200))
  lines[5] = ata_data(smart_cdb(h("d5"), 1, 6, true), sealed(page))
  let out = smartctl(ctx, lines, ["-d", "sat", "-l", "selftest"])
  assert out.status == 0, out.stderr
  assert "failed self-tests are outdated" in out.stdout
}

test test_capabilities_text_covers_missing_features {
  var page = cells(512)
  page[367] = 0
  page = with_field(page, 368, 2, 0)
  let bare = ata.capabilities_text(smart.smart_values(sealed(page), null), identify_page())
  assert "\tOff-line data collection not supported.\n" in bare
  assert "(0x0000)\tAutomatic saving of SMART data\t\t\t\t\tis not implemented.\n" in bare
  assert "recommended polling time: \t        Not Supported.\n" in bare
  assert "Conveyance self-test routine\nrecommended polling time: \t        Not Supported.\n" in bare
  page[367] = h("10")
  page = with_field(page, 368, 2, 2)
  page[370] = 0
  let partial = ata.capabilities_text(smart.smart_values(sealed(page), null), raw_page(cells(512)))
  assert "No SMART execute Offline immediate." in partial
  assert "(0x0002)\tDoes not save SMART data before\n\t\t\t\t\tentering power-saving mode.\n\t\t\t\t\tSupports SMART auto save timer.\n" in partial
  assert "Error logging NOT supported." in partial
  assert "No General Purpose Logging support." in partial
}

test test_selective_log_columns_widen_for_large_spans_and_directories_merge_ranges {
  var page = cells(512)
  page[0] = 1
  page = with_field(page, 2, 8, 0)
  page = with_field(page, 10, 8, 1953525167)
  let wide = ata.selective_log_text(smart.selective_log(sealed(page)), 0)
  assert " SPAN     MIN_LBA     MAX_LBA  CURRENT_TEST_STATUS\n" in wide
  assert "    1           0  1953525167  Not_testing\n" in wide

  var dir = cells(512)
  dir[0] = 1
  for address in range(h("80"), h("a0")) { dir[address * 2] = 1 }
  let text = ata.directory_text(smart.log_directory(raw_page(dir)))
  assert "0x80-0x9f      SL  R/W      1  Host vendor specific log\n" in text
}

test test_nvme_version_below_1_2_is_shown_as_a_bound {
  var page = cells(4096)
  page = with_field(page, 80, 4, h("00010100"))
  let controller = nvme.controller(raw_page(page))
  let text = nvme.info_text(controller, null, 1, "now", true)
  assert "NVMe Version:                       <1.2\n" in text
  assert "Controller ID" not in text
}

test test_nvme_namespace_size_and_capacity_split_when_they_differ {
  var page = cells(4096)
  page = with_field(page, 0, 8, 1000)
  page = with_field(page, 8, 8, 900)
  page[25] = 0
  page[130] = 9
  let space = nvme.namespace(raw_page(page))
  let text = nvme.info_text(nvme.controller(raw_page(cells(4096))), space, 1, "now", true)
  assert "Namespace 1 Size:                   512,000 [512 KB]\n" in text
  assert "Namespace 1 Capacity:               460,800 [460 KB]\n" in text
}
