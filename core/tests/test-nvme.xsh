# nvme runs over the storage transport's fake: every device exchange comes from
# a recorded fixture file, so these tests never reach a real device. The device
# operand is /dev/null, a character device the fake never reads or writes.

type Ran = {status: Int, stdout: Str, stderr: Str, bytes: Bytes}

# One recorded Identify, Get Log Page or Get Features exchange.
proc admin_line(
  opcode: Int,
  nsid = 0,
  cdw10 = 0,
  cdw11 = 0,
  cdw12 = 0,
  cdw13 = 0,
  cdw14 = 0,
  direction = "none",
  data_len = 0,
  status = 0,
  result = 0,
  data = b"",
  errno: Int? = null,
) -> Record {
  {
    op: "nvme_admin",
    opcode: opcode,
    nsid: nsid,
    cdw10: cdw10,
    cdw11: cdw11,
    cdw12: cdw12,
    cdw13: cdw13,
    cdw14: cdw14,
    direction: direction,
    data_len: data_len,
    status: status,
    result: result,
    data: data.base64(),
    errno: errno,
  }
}

proc identify_line(nsid: Int, cns: Int, data: Bytes, cdw11 = 0) -> Record {
  admin_line(6, nsid: nsid, cdw10: cns, cdw11: cdw11, direction: "from_device", data_len: 4096, data: data)
}

# A log page read of `length` bytes at offset zero; the dword count is
# zero-based and split across cdw10 and cdw11.
proc log_line(log_id: Int, length: Int, data: Bytes, nsid = 4294967295) -> Record {
  let dwords = length / 4 - 1
  admin_line(2, nsid: nsid, cdw10: dwords % 65536 * 65536 + log_id, cdw11: dwords / 65536, direction: "from_device", data_len: length, data: data)
}

# Replaces `value.len()` bytes of `data` at `offset`.
proc put(data: Bytes, offset: Int, value: Bytes) [error] -> Result[Bytes, Error] {
  Ok(bytes.concat([data.slice(0, length: offset), value, data.slice(offset + value.len())]))
}

proc le(value: Int, width: Int) [error] -> Result[Bytes, Error] {
  bytes.pack_le(value, width)
}

# The text field `text` space padded to `width` bytes.
proc padded(text: Str, width: Int) -> Bytes {
  bytes.from_text(text + "                                                                                ".byte_slice(0, length: width - text.byte_len()))
}

proc controller_page() [error] -> Result[Bytes, Error] {
  var data = bytes.zero(4096)?
  data = put(data, 0, le(5197, 2)?)?
  data = put(data, 2, le(5197, 2)?)?
  data = put(data, 4, padded("S5GXNX0T000001", 20))?
  data = put(data, 24, padded("Example NVMe SSD 1TB", 40))?
  data = put(data, 64, padded("5B2QGXA7", 8))?
  data = put(data, 72, b"\x02\x38\x25\x00")?
  data = put(data, 77, b"\x09")?
  data = put(data, 78, le(6, 2)?)?
  data = put(data, 80, le(66560, 4)?)?
  data = put(data, 256, le(23, 2)?)?
  data = put(data, 260, b"\x16\x2f\x3f\x01")?
  data = put(data, 263, b"\x01")?
  data = put(data, 266, le(358, 2)?)?
  data = put(data, 268, le(358, 2)?)?
  data = put(data, 512, b"\x66\x44")?
  data = put(data, 516, le(1, 4)?)?
  data = put(data, 520, le(95, 2)?)?
  data = put(data, 525, b"\x01")?
  # Power state 0: 8.49 W, operational; state 1: 4.60 W, non-operational.
  data = put(data, 2048, le(849, 2)?)?
  data = put(data, 2048 + 4, le(0, 4)?)?
  data = put(data, 2048 + 32, le(460, 2)?)?
  data = put(data, 2048 + 32 + 3, b"\x02")?
  data = put(data, 2048 + 32 + 4, le(5, 4)?)?
  data = put(data, 2048 + 32 + 8, le(5, 4)?)?
  data = put(data, 3072, b"VENDOR")?
  Ok(data)
}

# Runs core/nvme.xsh with the given fixture lines under the linux fake and
# returns what the process printed; `log` receives the fake's call log.
proc nvme(ctx: TestContext, lines: List[Record], args: List[Str], log: Path) [fs, process, error] -> Result[Ran, Error] {
  let root = test.temp_dir(ctx, name: "nvme-run")?
  let fixture = fp"{root}/fixture.jsonl"
  var recorded: List[Any] = []
  for line in lines {
    recorded += [line]
  }
  fixture.write(json.encode_lines(recorded)?)
  test.linux_fake(ctx, {storage_fixture: fixture, log: log})?
  let source = fp"{ctx.core_dir}/nvme.xsh".read_text()?
  let out = test.run_script(ctx, source, args, {XSH_MODULE_PATH: ctx.core_dir}, b"", "nvme")?
  Ok({status: out.status, stdout: out.stdout, stderr: out.stderr, bytes: out.stdout_bytes})
}

proc quiet_log(ctx: TestContext) [fs, error] -> Result[Path, Error] {
  test.temp_file(ctx, name: "nvme-calls", contents: b"")
}

# Pages for the log and feature tests.

proc smart_page() [error] -> Result[Bytes, Error] {
  var data = bytes.zero(512)?
  data = put(data, 0, b"\x04")?
  data = put(data, 1, le(311, 2)?)?
  data = put(data, 3, b"\x64\x0a\x02")?
  data = put(data, 6, b"\x01")?
  data = put(data, 32, le(4102436, 8)?)?
  data = put(data, 48, le(2147823, 8)?)?
  data = put(data, 64, le(35367119, 8)?)?
  data = put(data, 80, le(65263262, 8)?)?
  data = put(data, 96, le(1061, 8)?)?
  data = put(data, 112, le(189, 8)?)?
  data = put(data, 128, le(1953, 8)?)?
  data = put(data, 144, le(75, 8)?)?
  data = put(data, 176, le(3, 8)?)?
  data = put(data, 192, le(7, 4)?)?
  data = put(data, 196, le(9, 4)?)?
  data = put(data, 200, le(312, 2)?)?
  data = put(data, 204, le(315, 2)?)?
  data = put(data, 216, le(11, 4)?)?
  Ok(data)
}

# Error entry `count` failing command 14 on queue 2 with an invalid field.
proc error_entry(count: Int) [error] -> Result[Bytes, Error] {
  var data = bytes.zero(64)?
  data = put(data, 0, le(count, 8)?)?
  data = put(data, 8, le(2, 2)?)?
  data = put(data, 10, le(14, 2)?)?
  data = put(data, 12, le(2 * 2 + 1, 2)?)?
  data = put(data, 14, le(65535, 2)?)?
  data = put(data, 24, le(1, 4)?)?
  Ok(data)
}

proc ns_page() [error] -> Result[Bytes, Error] {
  var data = bytes.zero(4096)?
  data = put(data, 0, le(1000215216, 8)?)?
  data = put(data, 8, le(1000215216, 8)?)?
  data = put(data, 16, le(500000000, 8)?)?
  data = put(data, 24, b"\x0a\x01\x00")?
  data = put(data, 128, b"\x00\x00\x09\x01")?
  data = put(data, 132, b"\x00\x00\x0c\x00")?
  data = put(data, 104, b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10")?
  Ok(data)
}

type Install = {log: Path}

proc ran_ok(ran: Ran) -> Ran {
  assert ran.status == 0, f"status {ran.status}: {ran.stderr}"
  ran
}

test test_nvme_help_and_version_need_no_device { |ctx|
  let log = quiet_log(ctx)?
  let version = ran_ok(nvme(ctx, [], ["version"], log)?)
  assert version.stdout == "nvme version 2.16 (xsh)\n"
  let bare = ran_ok(nvme(ctx, [], [], log)?)
  assert bare.stdout.starts_with("nvme-2.16\nusage: nvme <command> [<device>] [<args>]\n")
  assert "  id-ctrl                              Send NVMe Identify Controller\n" in bare.stdout
  assert "  smart-log                            Retrieve SMART Log, show it\n" in bare.stdout
  let topic = ran_ok(nvme(ctx, [], ["help", "smart-log"], log)?)
  assert topic.stdout.starts_with("\u{1b}[1mUsage: nvme smart-log <device> [OPTIONS]\u{1b}[0m\n\nRetrieve SMART log")
  assert "\u{1b}[1mOptions:\u{1b}[0m\n  [  --verbose, -v ]" in topic.stdout
  # nvme-cli exits 1 after printing a command's own help.
  let own = nvme(ctx, [], ["id-ctrl", "--help"], log)?
  assert own.status == 1
  assert own.stdout.starts_with("\u{1b}[1mUsage: nvme id-ctrl <device> [OPTIONS]")
  assert log.read_text()? == ""
}

test test_nvme_refuses_changing_and_unknown_commands_without_touching_a_device { |ctx|
  let log = quiet_log(ctx)?
  for cmd in ["format", "fw-commit", "fw-download", "sanitize"] {
    let ran = nvme(ctx, [], [cmd, "/dev/null"], log)?
    assert ran.status == 1
    assert f"nvme: {cmd} is refused" in ran.stderr, ran.stderr
  }
  let unsupported = nvme(ctx, [], ["get-log", "/dev/null"], log)?
  assert unsupported.status == 1
  assert unsupported.stderr == "nvme: get-log is not supported\n"
  let unknown = nvme(ctx, [], ["frobnicate"], log)?
  assert unknown.status == 1
  assert unknown.stderr.starts_with("ERROR: Invalid sub-command 'frobnicate'\nnvme-2.16\nusage:")
  assert log.read_text()? == ""
}

test test_nvme_option_errors_follow_nvme_cli_wording { |ctx|
  let log = quiet_log(ctx)?
  let usage_head = "\u{1b}[1mUsage: nvme id-ns <device> [OPTIONS]\u{1b}[0m\n"
  let missing = nvme(ctx, [], ["id-ns", "/dev/null", "-n"], log)?
  assert missing.status == 1
  assert missing.stderr.starts_with(f"id-ns: option requires an argument: n\n{usage_head}"), missing.stderr
  let unknown = nvme(ctx, [], ["id-ns", "--bogus", "/dev/null"], log)?
  assert unknown.stderr.starts_with(f"id-ns: unrecognized option: bogus\n{usage_head}")
  let short = nvme(ctx, [], ["id-ns", "-x", "/dev/null"], log)?
  assert short.stderr.starts_with(f"id-ns: unrecognized option: x\n{usage_head}")
  let no_device = nvme(ctx, [], ["id-ns"], log)?
  assert no_device.stderr.starts_with(f"id-ns: Invalid argument\n{usage_head}")
  let not_a_device = nvme(ctx, [], ["id-ns", "/etc/passwd"], log)?
  assert not_a_device.stderr.starts_with(f"/etc/passwd is not a block or character device\n{usage_head}")
  let absent = nvme(ctx, [], ["id-ns", "/nonexistent-nvme-node"], log)?
  assert absent.stderr.starts_with(f"/nonexistent-nvme-node: No such file or directory\n{usage_head}")
  let word = nvme(ctx, [], ["id-ns", "-n", "abc", "/dev/null"], log)?
  assert word.stderr == "Expected word argument for 'namespace-id' but got 'abc'!\n"
  let byte = nvme(ctx, [], ["get-feature", "-f", "256", "/dev/null"], log)?
  assert byte.stderr == "Expected byte argument for 'feature-id' but got '256'!\n"
  let negative = nvme(ctx, [], ["get-feature", "-f", "-1", "/dev/null"], log)?
  assert negative.stderr == "Expected byte argument for 'feature-id' but got '-1'!\n"
  let integer = nvme(ctx, [], ["list-ns", "-y", "x", "/dev/null"], log)?
  assert integer.stderr == "Expected integer argument for 'csi' but got 'x'!\n"
  let format = nvme(ctx, [], ["id-ns", "-n", "1", "-o", "xml", "/dev/null"], log)?
  assert format.status == 1
  assert format.stderr == "Invalid output format\n"
  let extra = nvme(ctx, [], ["id-ns", "/dev/null", "/dev/null"], log)?
  assert extra.stderr.starts_with(f"id-ns: unexpected extra argument: /dev/null\n{usage_head}")
  assert log.read_text()? == ""
}

test test_nvme_id_ctrl_normal_output_and_human_readable_fields { |ctx|
  let log = quiet_log(ctx)?
  let ran = ran_ok(nvme(ctx, [identify_line(0, 1, controller_page()?)], ["id-ctrl", "/dev/null"], log)?)
  let lines = ran.stdout.lines()
  assert lines[0] == "NVME Identify Controller:"
  assert lines[1] == "vid       : 0x144d"
  assert lines[3] == "sn        : S5GXNX0T000001      "
  assert lines[4] == "mn        : Example NVMe SSD 1TB                    "
  assert lines[5] == "fr        : 5B2QGXA7"
  assert "ieee      : 002538" in lines
  assert "mdts      : 9" in lines
  assert "cntlid    : 0x6" in lines
  assert "ver       : 0x10400" in lines
  assert "oacs      : 0x17" in lines
  assert "frmw      : 0x16" in lines
  assert "wctemp    : 358" in lines
  assert "sqes      : 0x66" in lines
  assert "nn        : 1" in lines
  assert "subnqn    : " in lines
  assert "ps    0 : mp:8.49W operational enlat:0 exlat:0 rrt:0 rrl:0" in lines
  assert "          rwt:0 rwl:0 idle_power:- active_power:-" in lines
  assert "ps    1 : mp:4.60W non-operational enlat:5 exlat:5 rrt:0 rrl:0" in lines
  assert "vs[]:" not in lines
  assert "  [4:4] : 0x1\tDevice Self-test Supported" not in lines

  let human = ran_ok(nvme(ctx, [identify_line(0, 1, controller_page()?)], ["id-ctrl", "-H", "/dev/null"], log)?)
  let detail = human.stdout.lines()
  assert "  [4:4] : 0x1\tDevice Self-test Supported" in detail
  assert "  [3:3] : 0x1\tNS Management and Attachment Supported" not in detail
  assert "  [2:2] : 0x1\tFW Commit and Download Supported" in detail
  assert "  [4:4] : 0x1\tFirmware Activate Without Reset Supported" in detail
  assert "  [3:1] : 0x3\tNumber of Firmware Slots" in detail
  assert "  [0:0] : 0\tFirmware Slot 1 Read/Write" in detail
  assert "  [7:4] : 0x6\tMax SQ Entry Size (64)" in detail
  assert "  [3:0] : 0x6\tMin SQ Entry Size (64)" in detail
  assert "  [7:4] : 0x4\tMax CQ Entry Size (16)" in detail
  assert "  [0:0] : 0x1\tVolatile Write Cache Present" in detail
  assert "  [2:2] : 0x1\tData Set Management Supported" in detail
  assert "  [3:3] : 0x1\tWrite Zeroes Supported" in detail
  assert "  [5:5] : 0\tReservations Not Supported" in detail
}

test test_nvme_id_ctrl_vendor_json_and_binary_forms { |ctx|
  let log = quiet_log(ctx)?
  let page = controller_page()?
  let vendor = ran_ok(nvme(ctx, [identify_line(0, 1, page)], ["id-ctrl", "-V", "/dev/null"], log)?)
  let lines = vendor.stdout.lines()
  var at = 0
  while lines[at] != "vs[]:" {
    at += 1
  }
  assert lines[at + 1] == "       0  1  2  3  4  5  6  7  8  9  a  b  c  d  e  f"
  assert lines[at + 2] == "0000: 56 45 4e 44 4f 52 00 00 00 00 00 00 00 00 00 00 \"VENDOR..........\""
  assert lines.len() == at + 2 + 64

  let printed = ran_ok(nvme(ctx, [identify_line(0, 1, page)], ["id-ctrl", "-o", "json", "/dev/null"], log)?)
  assert printed.stdout.starts_with("{\n  \"vid\":5197,\n  \"ssvid\":5197,\n  \"sn\":\"S5GXNX0T000001      \",\n")
  assert "  \"ieee\":9528,\n" in printed.stdout
  assert "  \"psds\":[\n    {\n      \"max_power\":849,\n      \"flags\":0,\n" in printed.stdout
  assert printed.stdout.ends_with("    }\n  ]\n}\n")
  let decoded = json.decode(printed.stdout)?
  assert decoded.nn == 1 and decoded.psds.len() == 2

  let raw = ran_ok(nvme(ctx, [identify_line(0, 1, page)], ["id-ctrl", "-b", "/dev/null"], log)?)
  assert raw.bytes == page
  let as_binary = ran_ok(nvme(ctx, [identify_line(0, 1, page)], ["id-ctrl", "--output-format=binary", "/dev/null"], log)?)
  assert as_binary.bytes == page
  let refused = nvme(ctx, [identify_line(0, 1, page)], ["id-ctrl", "-V", "-o", "json", "/dev/null"], log)?
  assert refused.status == 1
  assert refused.stdout.starts_with("{\n  \"error\":\"id-ctrl: --vendor-specific is not available in json output\"")
}

test test_nvme_device_failures_are_reported_as_nvme_cli_reports_them { |ctx|
  let log = quiet_log(ctx)?
  # SC 0x02 (invalid field) with do-not-retry set, then an ioctl failure.
  let status = nvme(ctx, [admin_line(6, cdw10: 1, direction: "from_device", data_len: 4096, status: 16386)], ["id-ctrl", "/dev/null"], log)?
  assert status.status == 1
  assert status.stderr == "NVMe status: Invalid Field in Command: A reserved coded value or an unsupported value in a defined field(0x4002)\n", status.stderr
  let errno = nvme(ctx, [admin_line(6, cdw10: 1, direction: "from_device", data_len: 4096, errno: 25)], ["id-ctrl", "/dev/null"], log)?
  assert errno.status == 1
  assert errno.stderr == "identify controller: Not a tty\n"
  let printed = nvme(ctx, [admin_line(6, cdw10: 1, direction: "from_device", data_len: 4096, errno: 25)], ["id-ctrl", "-o", "json", "/dev/null"], log)?
  assert printed.status == 1
  assert printed.stdout == "{\n  \"error\":\"identify controller: Not a tty\"\n}\n"
  # A command the fixture does not know is the fake's own refusal.
  let unmatched = nvme(ctx, [], ["id-ctrl", "/dev/null"], log)?
  assert unmatched.status == 1
  assert unmatched.stderr.starts_with("identify controller: ")
}

test test_nvme_id_ns_uses_the_device_namespace_and_decodes_formats { |ctx|
  let log = quiet_log(ctx)?
  let lines = [{op: "nvme_namespace_id", nsid: 1}, identify_line(1, 0, ns_page()?)]
  let ran = ran_ok(nvme(ctx, lines, ["id-ns", "/dev/null"], log)?)
  let text = ran.stdout.lines()
  assert text[0] == "NVME Identify Namespace 1:"
  assert text[1] == "nsze    : 0x3b9e12b0"
  assert text[2] == "ncap    : 0x3b9e12b0"
  assert text[3] == "nuse    : 0x1dcd6500"
  assert "nsfeat  : 0xa" in text
  assert "nlbaf   : 1" in text
  assert "flbas   : 0" in text
  assert "npwg    : 0" not in text
  assert "nguid   : 0102030405060708090a0b0c0d0e0f10" in text
  assert "lbaf  0 : ms:0   lbads:9  rp:0x1 (in use)" in text
  assert "lbaf  1 : ms:0   lbads:12 rp:0 " in text

  let explicit = ran_ok(nvme(ctx, [identify_line(7, 0, ns_page()?)], ["id-ns", "-n", "7", "/dev/null"], log)?)
  assert explicit.stdout.starts_with("NVME Identify Namespace 7:\n")
  let forced = ran_ok(nvme(ctx, [identify_line(7, 17, ns_page()?)], ["id-ns", "-n", "7", "--force", "/dev/null"], log)?)
  assert forced.stdout.starts_with("NVME Identify Namespace 7:\n")
  let missing = nvme(ctx, [{op: "nvme_namespace_id", errno: 25}], ["id-ns", "/dev/null"], log)?
  assert missing.stderr == "get-namespace-id: Not a tty\n"

  let human = ran_ok(nvme(ctx, [identify_line(7, 0, ns_page()?)], ["id-ns", "-n", "7", "-H", "/dev/null"], log)?)
  assert "  [1:1] : 0x1\tNamespace uses NAWUN, NAWUPF, and NACWU" in human.stdout.lines()
  assert "  [3:3] : 0x1\tNGUID and EUI64 fields if non-zero, Never Reused" in human.stdout.lines()
  assert "lbaf  0 : ms:0   lbads:9  rp:0x1 Better (in use)" in human.stdout.lines()

  let printed = ran_ok(nvme(ctx, [identify_line(7, 0, ns_page()?)], ["id-ns", "-n", "7", "-o", "json", "/dev/null"], log)?)
  let decoded = json.decode(printed.stdout)?
  assert decoded.nsze == 1000215216 and decoded.nlbaf == 1
  assert decoded.nguid == "0102030405060708090a0b0c0d0e0f10"
  assert decoded.lbafs[0].ds == 9 and decoded.lbafs[1].ds == 12
}

test test_nvme_smart_log_decodes_counters_and_temperatures { |ctx|
  let log = quiet_log(ctx)?
  let lines = [log_line(2, 512, smart_page()?)]
  let ran = ran_ok(nvme(ctx, lines, ["smart-log", "/dev/null"], log)?)
  let expected = """Smart Log for NVME device:null namespace-id:ffffffff
critical_warning\t\t\t: 0x4
temperature\t\t\t\t: 38 \u{00b0}C (311 K)
available_spare\t\t\t\t: 100%
available_spare_threshold\t\t: 10%
percentage_used\t\t\t\t: 2%
endurance group critical warning summary: 0x1
Data Units Read\t\t\t\t: 4,102,436 (2.10 TB)
Data Units Written\t\t\t: 2,147,823 (1.10 TB)
host_read_commands\t\t\t: 35,367,119
host_write_commands\t\t\t: 65,263,262
controller_busy_time\t\t\t: 1,061
power_cycles\t\t\t\t: 189
power_on_hours\t\t\t\t: 1,953
unsafe_shutdowns\t\t\t: 75
media_errors\t\t\t\t: 0
num_err_log_entries\t\t\t: 3
Warning Temperature Time\t\t: 7
Critical Composite Temperature Time\t: 9
Temperature Sensor 1\t\t\t: 39 \u{00b0}C (312 K)
Temperature Sensor 3\t\t\t: 42 \u{00b0}C (315 K)
Thermal Management T1 Trans Count\t: 11
Thermal Management T2 Trans Count\t: 0
Thermal Management T1 Total Time\t: 0
Thermal Management T2 Total Time\t: 0
"""
  assert ran.stdout == expected, ran.stdout
  let human = ran_ok(nvme(ctx, lines, ["smart-log", "-H", "/dev/null"], log)?)
  assert "\t[2:2]\t: 0x1\tReliability has been degraded" in human.stdout.lines()
  assert "\t[3:3]\t: 0\tMedia has not been placed in read only mode" in human.stdout.lines()
  let printed = ran_ok(nvme(ctx, lines, ["smart-log", "-o", "json", "/dev/null"], log)?)
  let decoded = json.decode(printed.stdout)?
  assert decoded.critical_warning == 4 and decoded.temperature == 311
  assert decoded.avail_spare == 100 and decoded.spare_thresh == 10 and decoded.percent_used == 2
  assert decoded.data_units_read == 4102436 and decoded.power_on_hours == 1953
  assert decoded.temperature_sensor_1 == 312 and decoded.temperature_sensor_3 == 315
  assert "temperature_sensor_2" not in printed.stdout
  assert decoded.thm_temp1_trans_count == 11
  let raw = ran_ok(nvme(ctx, lines, ["smart-log", "--raw-binary", "/dev/null"], log)?)
  assert raw.bytes == smart_page()?
}

test test_nvme_smart_log_counters_wider_than_64_bits_are_exact { |ctx|
  let log = quiet_log(ctx)?
  let page = put(smart_page()?, 32, b"\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff")?
  let ran = ran_ok(nvme(ctx, [log_line(2, 512, page, nsid: 1)], ["smart-log", "-n", "1", "/dev/null"], log)?)
  assert "namespace-id:1\n" in ran.stdout
  assert "Data Units Read\t\t\t\t: 340,282,366,920,938,463,463,374,607,431,768,211,455 (" in ran.stdout, ran.stdout
}

test test_nvme_error_log_prints_entries_up_to_what_the_controller_keeps { |ctx|
  let log = quiet_log(ctx)?
  # ELPE 1 keeps two entries, so asking for 64 reads 2 * 64 bytes.
  var ctrl = bytes.zero(4096)?
  ctrl = put(ctrl, 262, b"\x01")?
  let page = bytes.concat([error_entry(8)?, error_entry(0)?])
  let lines = [identify_line(0, 1, ctrl), log_line(1, 128, page)]
  let ran = ran_ok(nvme(ctx, lines, ["error-log", "/dev/null"], log)?)
  let expected = """Error Log Entries for device:null entries:2
.................
 Entry[ 0]   
.................
error_count\t: 8
sqid\t\t: 2
cmdid\t\t: 0xe
status_field\t: 0x2(Invalid Field in Command: A reserved coded value or an unsupported value in a defined field)
phase_tag\t: 0x1
parm_err_loc\t: 0xffff
lba\t\t: 0
nsid\t\t: 0x1
vs\t\t: 0
trtype\t\t: The transport type is not indicated or the error is not transport related.
cs\t\t: 0
trtype_spec_info: 0
.................
 Entry[ 1]   
.................
error_count\t: 0
sqid\t\t: 2
cmdid\t\t: 0xe
status_field\t: 0x2(Invalid Field in Command: A reserved coded value or an unsupported value in a defined field)
phase_tag\t: 0x1
parm_err_loc\t: 0xffff
lba\t\t: 0
nsid\t\t: 0x1
vs\t\t: 0
trtype\t\t: The transport type is not indicated or the error is not transport related.
cs\t\t: 0
trtype_spec_info: 0
.................
"""
  assert ran.stdout == expected, ran.stdout

  let one = [identify_line(0, 1, ctrl), log_line(1, 64, error_entry(8)?)]
  let printed = ran_ok(nvme(ctx, one, ["error-log", "-e", "1", "-o", "json", "/dev/null"], log)?)
  let decoded = json.decode(printed.stdout)?
  assert decoded.errors.len() == 1
  assert decoded.errors[0].error_count == 8 and decoded.errors[0].status_field == 2 and decoded.errors[0].phase_tag == 1
  let raw = ran_ok(nvme(ctx, one, ["error-log", "-e", "1", "-b", "/dev/null"], log)?)
  assert raw.bytes == error_entry(8)?
  let zero = nvme(ctx, one, ["error-log", "-e", "0", "/dev/null"], log)?
  assert zero.stderr == "non-zero log-entries is required param\n"
}

test test_nvme_error_log_pages_in_4k_transfers { |ctx|
  let log = quiet_log(ctx)?
  var ctrl = bytes.zero(4096)?
  ctrl = put(ctrl, 262, b"\xff")?
  # 65 entries are 4160 bytes: a 4096-byte read at offset 0, then 64 bytes
  # at byte offset 4096 carried in dword 12.
  let first = bytes.zero(4096)?
  let second = error_entry(65)?
  let lines = [
    identify_line(0, 1, ctrl),
    log_line(1, 4096, first),
    admin_line(2, nsid: 4294967295, cdw10: 15 * 65536 + 1, cdw12: 4096, direction: "from_device", data_len: 64, data: second),
  ]
  let ran = ran_ok(nvme(ctx, lines, ["error-log", "-e", "65", "-b", "/dev/null"], log)?)
  assert ran.bytes == bytes.concat([first, second])
}

test test_nvme_self_test_log_lists_used_results { |ctx|
  let log = quiet_log(ctx)?
  var page = bytes.zero(564)?
  page = put(page, 0, b"\x01\x32")?
  # Result 0: short test (code 1) failed (result 7) with LBA and status.
  page = put(page, 4, b"\x17\x03\x0f\x00")?
  page = put(page, 8, le(28, 8)?)?
  page = put(page, 16, le(1, 4)?)?
  page = put(page, 20, le(4096, 8)?)?
  page = put(page, 28, b"\x02\x81")?
  # Result 1: extended test (code 2) completed (result 0), nothing valid.
  page = put(page, 32, b"\x20\x00\x00\x00")?
  page = put(page, 36, le(100, 8)?)?
  # The remaining entries are unused (result 0xf).
  var index = 2
  while index < 20 {
    page = put(page, 4 + index * 28, b"\x0f")?
    index += 1
  }
  let lines = [log_line(6, 564, page)]
  let ran = ran_ok(nvme(ctx, lines, ["self-test-log", "/dev/null"], log)?)
  let expected = """Device Self Test Log for NVME device:null
Current operation  : 0x1
Current Completion : 50%
Self Test Result[0]:
  Operation Result             : 0x7
  Self Test Code               : 0x1
  Valid Diagnostic Information : 0xf
  Power on hours (POH)         : 0x1c
  Namespace Identifier         : 0x1
  Failing LBA                  : 0x1000
  Status Code Type             : 0x2
  Status Code                  : 0x81
  Vendor Specific              : 0 0
Self Test Result[1]:
  Operation Result             : 0
  Self Test Code               : 0x2
  Valid Diagnostic Information : 0
  Power on hours (POH)         : 0x64
  Vendor Specific              : 0 0
"""
  assert ran.stdout == expected, ran.stdout
  # -e shortens the transfer to the header and that many entries.
  let two = put(page.slice(0, length: 60), 0, b"\x00\x00")?
  let short = ran_ok(nvme(ctx, [log_line(6, 60, two)], ["self-test-log", "-e", "2", "/dev/null"], log)?)
  assert short.stdout.starts_with("Device Self Test Log for NVME device:null\nCurrent operation  : 0\nSelf Test Result[0]:")
  let printed = ran_ok(nvme(ctx, lines, ["self-test-log", "-o", "json", "/dev/null"], log)?)
  let decoded = json.decode(printed.stdout)?
  assert decoded["Current Device Self-Test Completion"] == 50
  assert decoded["Self Test Results"].len() == 2
  assert decoded["Self Test Results"][0]["Failing LBA"] == 4096
}

test test_nvme_fw_log_shows_the_active_slot_and_revisions { |ctx|
  let log = quiet_log(ctx)?
  var page = bytes.zero(512)?
  page = put(page, 0, b"\x12")?
  page = put(page, 8, b"5B2QGXA7")?
  page = put(page, 16, b"6B2QGXA7")?
  let ran = ran_ok(nvme(ctx, [log_line(3, 512, page)], ["fw-log", "/dev/null"], log)?)
  assert ran.stdout == "Firmware Log for device:null\nafi  : 0x12\nfrs1 : 0x3741584751324235 (5B2QGXA7)\nfrs2 : 0x3741584751324236 (6B2QGXA7)\n"
}

test test_nvme_get_feature_values_and_decoded_fields { |ctx|
  let log = quiet_log(ctx)?
  # Temperature threshold: the feature value is the completion dword.
  let temperature = [admin_line(10, cdw10: 4, result: 343)]
  let plain = ran_ok(nvme(ctx, temperature, ["get-feature", "-f", "4", "/dev/null"], log)?)
  assert plain.stdout == "get-feature:0x04 (Temperature Threshold), Current value:0x00000157\n", plain.stdout
  let human = ran_ok(nvme(ctx, temperature, ["get-feature", "-f", "0x4", "-H", "/dev/null"], log)?)
  assert human.stdout == """get-feature:0x04 (Temperature Threshold), Current value:0x00000157
\tThreshold Temperature Select (TMPSEL): Composite temperature
\tThreshold Type Select         (THSEL): Over temperature threshold
\tTemperature Threshold         (TMPTH): 343 K (70 C)
""", human.stdout
  # Select 1 is the default value; cdw11 and uuid index pass through.
  let select = [admin_line(10, nsid: 1, cdw10: 260, cdw11: 5, cdw14: 3, result: 7)]
  let chosen = ran_ok(nvme(ctx, select, ["get-feature", "-f", "4", "-n", "1", "-s", "1", "-c", "5", "-U", "3", "/dev/null"], log)?)
  assert chosen.stdout == "get-feature:0x04 (Temperature Threshold), Default value:0x00000007\n"
  # Select 3 reports capabilities.
  let capabilities = [admin_line(10, cdw10: 774, result: 5)]
  let supported = ran_ok(nvme(ctx, capabilities, ["get-feature", "-f", "6", "-s", "3", "/dev/null"], log)?)
  assert supported.stdout == """get-feature:0x06 (Volatile Write Cache), Supported capabilities value:0x00000005
\tfeature is saveable
\tfeature is not namespace specific
\tfeature is changeable
""", supported.stdout
  # Timestamp carries an eight-byte data buffer shown as a hexdump.
  let timestamp = [admin_line(10, cdw10: 14, direction: "from_device", data_len: 8, data: b"\x01\x02\x03\x04\x05\x06\x00\x00")]
  let dumped = ran_ok(nvme(ctx, timestamp, ["get-feature", "-f", "14", "/dev/null"], log)?)
  let lines = dumped.stdout.lines()
  assert lines[0] == "get-feature:0x0e (Timestamp), Current value:0x00000000"
  assert lines[1] == "       0  1  2  3  4  5  6  7  8  9  a  b  c  d  e  f"
  assert lines[2] == "0000: 01 02 03 04 05 06 00 00" + "                          " + " \"........\"".byte_slice(1)
  let raw = ran_ok(nvme(ctx, timestamp, ["get-feature", "-f", "14", "-b", "/dev/null"], log)?)
  assert raw.bytes == b"\x01\x02\x03\x04\x05\x06\x00\x00"
  # An explicit data length overrides the feature's own.
  let sized = [admin_line(10, cdw10: 4, direction: "from_device", data_len: 4, data: b"\xaa\xbb\xcc\xdd")]
  let forced = ran_ok(nvme(ctx, sized, ["get-feature", "-f", "4", "-l", "4", "-b", "/dev/null"], log)?)
  assert forced.bytes == b"\xaa\xbb\xcc\xdd"
}

test test_nvme_get_feature_refuses_what_it_cannot_decode_or_send { |ctx|
  let log = quiet_log(ctx)?
  let select = nvme(ctx, [], ["get-feature", "-f", "4", "-s", "4", "/dev/null"], log)?
  assert select.status == 1
  assert select.stderr == "invalid 'select' param:4\n"
  let changed = nvme(ctx, [], ["get-feature", "-f", "4", "-C", "/dev/null"], log)?
  assert changed.status == 1
  assert changed.stderr == "get-feature: --changed is not supported\n"
  let printed = nvme(ctx, [admin_line(10, cdw10: 4, result: 1)], ["get-feature", "-f", "4", "-o", "json", "/dev/null"], log)?
  assert printed.status == 1
  assert printed.stderr == "get-feature: json output is not supported\n"
  assert log.read_text()? == ""
}

test test_nvme_list_ns_prints_active_namespace_ids { |ctx|
  let log = quiet_log(ctx)?
  let page = bytes.concat([le(1, 4)?, le(3, 4)?, le(12, 4)?, bytes.zero(4084)?])
  # The list starts after nsid - 1, so -n 1 asks for ids greater than 0.
  let ran = ran_ok(nvme(ctx, [identify_line(0, 2, page)], ["list-ns", "/dev/null"], log)?)
  assert ran.stdout == "[   0]:0x1\n[   1]:0x3\n[   2]:0xc\n", ran.stdout
  let printed = ran_ok(nvme(ctx, [identify_line(0, 2, page)], ["list-ns", "-o", "json", "/dev/null"], log)?)
  let decoded = json.decode(printed.stdout)?
  assert decoded.nsid_list.len() == 3
  assert decoded.nsid_list[0].nsid == 1 and decoded.nsid_list[2].nsid == 12
  assert printed.stdout == "{\n  \"nsid_list\":[\n    {\n      \"nsid\":1\n    },\n    {\n      \"nsid\":3\n    },\n    {\n      \"nsid\":12\n    }\n  ]\n}\n"
  let all = ran_ok(nvme(ctx, [identify_line(4, 16, page)], ["list-ns", "-a", "-n", "5", "/dev/null"], log)?)
  assert all.stdout.starts_with("[   0]:0x1\n")
  # A command set identifier moves to the command-set-specific list.
  let by_csi = ran_ok(nvme(ctx, [identify_line(0, 26, page, cdw11: 33554432)], ["list-ns", "-y", "2", "/dev/null"], log)?)
  assert by_csi.stdout.starts_with("[   0]:0x1\n")
  let none = ran_ok(nvme(ctx, [identify_line(0, 2, bytes.zero(4096)?)], ["list-ns", "/dev/null"], log)?)
  assert none.stdout == ""
  let zero = nvme(ctx, [], ["list-ns", "-n", "0", "/dev/null"], log)?
  assert zero.status == 1
  assert zero.stderr == "list-ns: namespace id 0 is invalid\n"
}

# A sysfs and device tree with three NVMe namespaces: nvme0n1 and nvme0n2 have
# device nodes (the fixture answers Identify Namespace for nsid 1 only, and the
# fake refuses the other), nvme1n1 has no node and is read from sysfs alone.
proc namespace_tree(ctx: TestContext) [fs, process, error] -> Result[Path, Error] {
  let root = test.temp_dir(ctx, name: "nvme-tree")?
  let sys = fp"{root}/sys"
  let dev = fp"{root}/dev"
  dev.mkdir()
  for name in ["nvme0n1", "nvme1n1", "nvme0n2"] {
    fp"{sys}/block/{name}/queue".mkdir()
    fp"{sys}/block/{name}/device".mkdir()
  }
  # nvme1n1 has no device node, so it can only be described from sysfs.
  for name in ["nvme0n1", "nvme0n2"] {
    fp"{dev}/{name}".write(b"")
  }
  fp"{sys}/block/nvme0n1/size".write("1000215216\n")
  fp"{sys}/block/nvme0n1/nsid".write("1\n")
  fp"{sys}/block/nvme0n1/queue/logical_block_size".write("512\n")
  fp"{sys}/block/nvme0n1/device/model".write("Example NVMe SSD 1TB\n")
  fp"{sys}/block/nvme0n1/device/serial".write("S5GXNX0T000001\n")
  fp"{sys}/block/nvme0n1/device/firmware_rev".write("5B2QGXA7\n")
  fp"{sys}/block/nvme0n2/size".write("2097152\n")
  fp"{sys}/block/nvme0n2/nsid".write("2\n")
  fp"{sys}/block/nvme0n2/queue/logical_block_size".write("512\n")
  fp"{sys}/block/nvme0n2/device/model".write("Example NVMe SSD 1TB\n")
  fp"{sys}/block/nvme0n2/device/serial".write("S5GXNX0T000001\n")
  fp"{sys}/block/nvme0n2/device/firmware_rev".write("5B2QGXA7\n")
  fp"{sys}/block/nvme1n1/size".write("7814037168\n")
  fp"{sys}/block/nvme1n1/queue/logical_block_size".write("4096\n")
  fp"{sys}/block/nvme1n1/device/model".write("Second Drive\n")
  fp"{sys}/block/nvme1n1/device/serial".write("SN2\n")
  fp"{sys}/block/nvme1n1/device/firmware_rev".write("FW2\n")
  Ok(root)
}

proc nvme_list(ctx: TestContext, root: Path, lines: List[Record], args: List[Str]) [fs, process, error] -> Result[Ran, Error] {
  let work = test.temp_dir(ctx, name: "nvme-list-run")?
  let fixture = fp"{work}/fixture.jsonl"
  var recorded: List[Any] = []
  for line in lines {
    recorded += [line]
  }
  fixture.write(json.encode_lines(recorded)?)
  test.linux_fake(ctx, {storage_fixture: fixture})?
  let source = "use lib.nvme_cli\nproc main(...argv: List[Str]) {\n  nvme_cli.dispatch(argv[2..], fp\"{argv[0]}\", fp\"{argv[1]}\")\n}\n"
  var words: List[Union[Str, Path]] = [fp"{root}/sys", fp"{root}/dev", "list"]
  for word in args {
    words += [word]
  }
  let out = test.run_script(ctx, source, words, {XSH_MODULE_PATH: ctx.core_dir}, b"", "nvme-list")?
  Ok({status: out.status, stdout: out.stdout, stderr: out.stderr, bytes: out.stdout_bytes})
}

test test_nvme_list_prints_the_nvme_cli_table_and_json { |ctx|
  let root = namespace_tree(ctx)?
  let ran = ran_ok(nvme_list(ctx, root, [identify_line(1, 0, ns_page()?)], [])?)
  let expected = [
    "Node                  Generic               SN                   Model                                    Namespace  Usage                      Format           FW Rev  ",
    "--------------------- --------------------- -------------------- ---------------------------------------- ---------- -------------------------- ---------------- --------",
    "nvme0n1               ng0n1                 S5GXNX0T000001       Example NVMe SSD 1TB                     0x1        256.00  GB / 512.11  GB    512   B +  0 B   5B2QGXA7",
    "nvme0n2               ng0n2                 S5GXNX0T000001       Example NVMe SSD 1TB                     0x2          1.07  GB /   1.07  GB    512   B +  0 B   5B2QGXA7",
    "nvme1n1               ng1n1                 SN2                  Second Drive                             0x1          4.00  TB /   4.00  TB      4 KiB +  0 B   FW2     ",
  ]
  # The first namespace takes its size and usage from Identify Namespace
  # (500000000 of 1000215216 blocks in use); the others come from sysfs.
  assert ran.stdout.lines() == expected, ran.stdout
  assert ran.stdout.ends_with("FW2     \n")

  let printed = ran_ok(nvme_list(ctx, root, [identify_line(1, 0, ns_page()?)], ["-o", "json"])?)
  let decoded = json.decode(printed.stdout)?
  assert decoded.Devices.len() == 3
  let first = decoded.Devices[0]
  assert first.NameSpace == 1 and first.DevicePath == "nvme0n1" and first.GenericPath == "ng0n1"
  assert first.ModelNumber == "Example NVMe SSD 1TB" and first.SerialNumber == "S5GXNX0T000001"
  assert first.Firmware == "5B2QGXA7"
  assert first.UsedBytes == 256000000000 and first.MaximumLBA == 1000215216
  assert first.PhysicalSize == 512110190592 and first.SectorSize == 512
  let third = decoded.Devices[2]
  assert third.DevicePath == "nvme1n1" and third.SectorSize == 4096
  assert third.PhysicalSize == 7814037168 * 512 and third.UsedBytes == third.PhysicalSize
  assert printed.stdout.starts_with("{\n  \"Devices\":[\n    {\n      \"NameSpace\":1,\n")
}

test test_nvme_list_edge_cases { |ctx|
  let root = test.temp_dir(ctx, name: "nvme-empty")?
  fp"{root}/sys/block".mkdir()
  fp"{root}/dev".mkdir()
  let none = ran_ok(nvme_list(ctx, root, [], [])?)
  assert none.stdout == ""
  assert none.stderr == "No NVMe devices detected.\n"
  let empty = ran_ok(nvme_list(ctx, root, [], ["-o", "json"])?)
  assert empty.stdout == "{\n  \"Devices\":[]\n}\n"
  let binary = nvme_list(ctx, root, [], ["-o", "binary"])?
  assert binary.status == 1 and binary.stderr == "Invalid output format\n"
  let verbose = nvme_list(ctx, root, [], ["-v"])?
  assert verbose.status == 1 and verbose.stderr == "list: verbose listing is not supported\n"
  let operand = nvme_list(ctx, root, [], ["/dev/nvme0n1"])?
  assert operand.status == 1
  assert operand.stderr.starts_with("list: unexpected extra argument: /dev/nvme0n1\n")
}

test test_nvme_device_self_test_starts_and_aborts_with_narrow_validation { |ctx|
  let log = quiet_log(ctx)?
  let short = ran_ok(nvme(ctx, [admin_line(20, nsid: 4294967295, cdw10: 1)], ["device-self-test", "-s", "1", "/dev/null"], log)?)
  assert short.stdout == "Short Device self-test started\n"
  let extended = ran_ok(nvme(ctx, [admin_line(20, nsid: 1, cdw10: 2)], ["device-self-test", "-s", "2", "-n", "1", "/dev/null"], log)?)
  assert extended.stdout == "Extended Device self-test started\n"
  let abort = ran_ok(nvme(ctx, [admin_line(20, nsid: 4294967295, cdw10: 15)], ["device-self-test", "-s", "15", "/dev/null"], log)?)
  assert abort.stdout == "Aborting device self-test operation\n"
  # A device-reported refusal (self-test already running) is a status error.
  let busy = nvme(ctx, [admin_line(20, nsid: 4294967295, cdw10: 1, status: 29 + 256)], ["device-self-test", "-s", "1", "/dev/null"], log)?
  assert busy.status == 1
  assert busy.stderr.starts_with("NVMe status: Device Self-test In Progress: "), busy.stderr
  assert busy.stderr.ends_with("(0x11d)\n")
  let mark = log.read_text()?.byte_len()

  # Codes that start vendor-specific or host-initiated work, a wait, and
  # codes outside the defined set never reach the device.
  for code in ["3", "14", "7"] {
    let refused = nvme(ctx, [], ["device-self-test", "-s", code, "/dev/null"], log)?
    assert refused.status == 1
    assert "is not supported (0 shows the state, 1 short, 2 extended, 15 abort)" in refused.stderr, refused.stderr
  }
  let waiting = nvme(ctx, [], ["device-self-test", "-s", "1", "-w", "/dev/null"], log)?
  assert waiting.status == 1
  assert waiting.stderr == "device-self-test: --wait is not supported; poll self-test-log instead\n"
  assert log.read_text()?.byte_len() == mark
}

test test_nvme_device_self_test_zero_shows_the_running_operation { |ctx|
  let log = quiet_log(ctx)?
  var page = bytes.zero(4)?
  page = put(page, 0, b"\x02\x2d")?
  let running = ran_ok(nvme(ctx, [log_line(6, 4, page)], ["device-self-test", "/dev/null"], log)?)
  assert running.stdout == "Current operation  : 0x2 (Extended device self-test operation in progress)\nCurrent Completion : 45%\n", running.stdout
  let idle = ran_ok(nvme(ctx, [log_line(6, 4, bytes.zero(4)?)], ["device-self-test", "-s", "0", "/dev/null"], log)?)
  assert idle.stdout == "Current operation  : 0 (No device self-test operation in progress)\n"
}

test test_nvme_set_feature_is_limited_to_one_dword_features { |ctx|
  let log = quiet_log(ctx)?
  let changed = ran_ok(nvme(ctx, [admin_line(9, cdw10: 6, cdw11: 1)], ["set-feature", "-f", "6", "-V", "1", "/dev/null"], log)?)
  assert changed.stdout == "set-feature:0x06 (Volatile Write Cache), value:0x00000001\n"
  let saved = ran_ok(nvme(ctx, [admin_line(9, nsid: 1, cdw10: 2147483652, cdw11: 343)], ["set-feature", "-f", "0x4", "-V", "343", "-n", "1", "-s", "/dev/null"], log)?)
  assert saved.stdout == "set-feature:0x04 (Temperature Threshold), value:0x00000157\n"
  let rejected = nvme(ctx, [admin_line(9, cdw10: 4, cdw11: 5, status: 256 + 14)], ["set-feature", "-f", "4", "-V", "5", "/dev/null"], log)?
  assert rejected.status == 1
  assert rejected.stderr.starts_with("NVMe status: Feature Not Changeable: "), rejected.stderr
  let before = log.read_text()?.byte_len()

  let refusals = [
    {args: ["set-feature", "-V", "1", "/dev/null"], message: "feature-id required param\n"},
    {args: ["set-feature", "-f", "6", "/dev/null"], message: "value required param\n"},
    {args: ["set-feature", "-f", "12", "-V", "1", "/dev/null"], message: "set-feature: feature 0x0c cannot be changed with this nvme; settable features: 0x04, 0x05, 0x06, 0x08, 0x0a, 0x0b, 0x0f\n"},
    {args: ["set-feature", "-f", "6", "-V", "1", "-d", "data.bin", "/dev/null"], message: "set-feature: --data is not supported: only features with a one-dword value can be set\n"},
    {args: ["set-feature", "-f", "6", "-V", "1", "-c", "1", "/dev/null"], message: "set-feature: --cdw12 is not supported: only features with a one-dword value can be set\n"},
    {args: ["set-feature", "-f", "6", "-V", "1", "-o", "json", "/dev/null"], message: "set-feature: only the normal output format is available\n"},
  ]
  for case in refusals {
    let ran = nvme(ctx, [], case.args, log)?
    assert ran.status == 1
    assert ran.stderr == case.message, ran.stderr
  }
  assert log.read_text()?.byte_len() == before
}

test test_nvme_common_options_are_honoured_or_refused { |ctx|
  let log = quiet_log(ctx)?
  let page = controller_page()?
  let lines = [identify_line(0, 1, page)]
  # --dry-run sends nothing and succeeds quietly.
  let dry = ran_ok(nvme(ctx, lines, ["id-ctrl", "--dry-run", "/dev/null"], log)?)
  assert dry.stdout == "" and log.read_text()? == ""
  # --verbose traces the command on stderr before sending it.
  let verbose = ran_ok(nvme(ctx, lines, ["id-ctrl", "-v", "/dev/null"], log)?)
  assert verbose.stderr == "id-ctrl: opcode 0x6 nsid 0 cdw10 0x1 cdw11 0 cdw12 0 cdw13 0 cdw14 0 data_len 4096\n", verbose.stderr
  assert verbose.stdout.starts_with("NVME Identify Controller:\n")
  # --timeout and --no-retries are accepted; a bad timeout is a parse error.
  let timed = ran_ok(nvme(ctx, lines, ["id-ctrl", "-t", "500", "--no-retries", "/dev/null"], log)?)
  assert timed.stdout.starts_with("NVME Identify Controller:\n")
  let bad = nvme(ctx, lines, ["id-ctrl", "--timeout=soon", "/dev/null"], log)?
  assert bad.stderr == "Expected word argument for 'timeout' but got 'soon'!\n"
  let version_one = ran_ok(nvme(ctx, lines, ["id-ctrl", "--output-format-version=1", "/dev/null"], log)?)
  assert version_one.stdout.starts_with("NVME Identify Controller:\n")
  let version_two = nvme(ctx, lines, ["id-ctrl", "--output-format-version=2", "/dev/null"], log)?
  assert version_two.status == 1
  assert version_two.stderr == "--output-format-version=2 is not supported: only version 1 is produced\n"
}

test test_nvme_option_syntax_follows_getopt_long { |ctx|
  let log = quiet_log(ctx)?
  let lines = [identify_line(0, 1, controller_page()?)]
  let attached = ran_ok(nvme(ctx, lines, ["id-ctrl", "-ojson", "/dev/null"], log)?)
  assert attached.stdout.starts_with("{\n  \"vid\":5197,")
  let abbreviated = ran_ok(nvme(ctx, lines, ["id-ctrl", "--raw", "/dev/null"], log)?)
  assert abbreviated.bytes == controller_page()?
  let separated = ran_ok(nvme(ctx, lines, ["id-ctrl", "--output-format", "json", "/dev/null"], log)?)
  assert separated.stdout == attached.stdout
  let clustered = ran_ok(nvme(ctx, lines, ["id-ctrl", "-HV", "/dev/null"], log)?)
  assert "  [4:4] : 0x1\tDevice Self-test Supported" in clustered.stdout.lines()
  assert "vs[]:" in clustered.stdout.lines()
  let after_operand = ran_ok(nvme(ctx, lines, ["id-ctrl", "/dev/null", "-o", "json"], log)?)
  assert after_operand.stdout == attached.stdout
  let ambiguous = nvme(ctx, lines, ["id-ctrl", "--output", "json", "/dev/null"], log)?
  assert ambiguous.stderr.starts_with("id-ctrl: option is ambiguous: output\n")
  let value_for_flag = nvme(ctx, lines, ["id-ctrl", "--human-readable=yes", "/dev/null"], log)?
  assert value_for_flag.stderr.starts_with("id-ctrl: option doesn't allow an argument: human-readable\n")
  let after_double_dash = nvme(ctx, lines, ["id-ctrl", "--", "-H"], log)?
  assert after_double_dash.stderr.starts_with("-H: No such file or directory\n")
}

test test_nvme_id_ns_vendor_region_and_fw_log_json { |ctx|
  let log = quiet_log(ctx)?
  var page = ns_page()?
  page = put(page, 384, b"NSVENDOR")?
  let vendor = ran_ok(nvme(ctx, [identify_line(1, 0, page)], ["id-ns", "-n", "1", "-V", "/dev/null"], log)?)
  let lines = vendor.stdout.lines()
  var at = 0
  while lines[at] != "vs[]:" {
    at += 1
  }
  assert lines[at + 2] == "0000: 4e 53 56 45 4e 44 4f 52 00 00 00 00 00 00 00 00 \"NSVENDOR........\""
  # 3712 vendor bytes are 232 rows after the header.
  assert lines.len() == at + 2 + 232
  let refused = nvme(ctx, [identify_line(1, 0, page)], ["id-ns", "-n", "1", "-V", "-o", "json", "/dev/null"], log)?
  assert refused.status == 1

  var firmware = bytes.zero(512)?
  firmware = put(firmware, 0, b"\x01")?
  firmware = put(firmware, 8, b"5B2QGXA7")?
  let printed = ran_ok(nvme(ctx, [log_line(3, 512, firmware)], ["fw-log", "-o", "json", "/dev/null"], log)?)
  let decoded = json.decode(printed.stdout)?
  assert decoded["Firmware Log"]["Active Slot"] == 1
  assert decoded["Firmware Log"].frs1 == "5B2QGXA7"
}
