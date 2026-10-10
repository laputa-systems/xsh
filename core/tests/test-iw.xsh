use core.lib.nl80211 as nl

type IwRun = {success: Bool, status: Int, stdout: Str, stderr: Str, calls: Str}

# The family id the recorded kernel assigns nl80211, and the multicast group
# ids it publishes. Any numbers would do; these are distinct on purpose.
const FAMILY = 28
const GROUP_CONFIG = 19
const GROUP_SCAN = 20
const GROUP_REGULATORY = 21
const GROUP_MLME = 22

# nl80211 attribute numbers used to build recorded replies.
const A_WIPHY = 1
const A_WIPHY_NAME = 2
const A_IFINDEX = 3
const A_IFNAME = 4
const A_IFTYPE = 5
const A_MAC = 6
const A_STA_INFO = 21
const A_WIPHY_BANDS = 22
const A_SUPPORTED_IFTYPES = 32
const A_REG_ALPHA2 = 33
const A_WIPHY_FREQ = 38
const A_MAX_NUM_SCAN_SSIDS = 43
const A_SCAN_FREQUENCIES = 44
const A_SCAN_SSIDS = 45
const A_BSS = 47
const A_SUPPORTED_COMMANDS = 50
const A_SSID = 52
const A_MAX_SCAN_IE_LEN = 56
const A_CIPHER_SUITES = 57
const A_WIPHY_RETRY_SHORT = 61
const A_WIPHY_RETRY_LONG = 62
const A_WIPHY_COVERAGE_CLASS = 89
const A_WIPHY_TX_POWER_LEVEL = 98
const A_WDEV = 153
const A_CHANNEL_WIDTH = 159
const A_CENTER_FREQ1 = 160
const A_SPLIT_WIPHY_DUMP = 174

proc genl(cmd: Int, attrs: List[Bytes]) [error] -> Result[Bytes, Error] {
  bytes.concat([bytes.from_ints([cmd, 1, 0, 0])?] + attrs)
}

# The payload `lib/nl80211.xsh` sends for `cmd` with `attrs`.
proc request_payload(cmd: Int, attrs: List[Bytes]) [error] -> Result[Bytes, Error] {
  bytes.concat([bytes.from_ints([cmd, 0, 0, 0])?] + attrs)
}

proc reply_of(cmd: Int, attrs: List[Bytes]) [error] -> Result[Record, Error] {
  {type: FAMILY, flags: 2, payload: genl(cmd, attrs)?.base64()}
}

proc family_line() -> Record {
  {op: "genl_family", name: "nl80211", id: FAMILY}
}

# The control-family reply that lists nl80211's multicast groups.
proc groups_line() [error] -> Result[Record, Error] {
  let groups = [
    nl.attr_nested(1, [nl.attr_str(1, "config")?, nl.attr_u32(2, GROUP_CONFIG)?])?,
    nl.attr_nested(2, [nl.attr_str(1, "scan")?, nl.attr_u32(2, GROUP_SCAN)?])?,
    nl.attr_nested(3, [nl.attr_str(1, "regulatory")?, nl.attr_u32(2, GROUP_REGULATORY)?])?,
    nl.attr_nested(4, [nl.attr_str(1, "mlme")?, nl.attr_u32(2, GROUP_MLME)?])?,
  ]
  let body = genl(
    1,
    [nl.attr_u16(1, FAMILY)?, nl.attr_str(2, "nl80211")?, nl.attr_nested(7, groups)?],
  )?
  {op: "netlink_request", protocol: 16, type: 16, cmd: 3, replies: [{type: 16, flags: 0, payload: body.base64()}]}
}

# A recorded request: matched on command, flags, and exact attributes.
proc request_line(cmd: Int, attrs: List[Bytes], flags: Int, replies: List[Record]) [error] -> Result[Record, Error] {
  {
    op: "netlink_request",
    type: FAMILY,
    flags: flags,
    payload: request_payload(cmd, attrs)?.base64(),
    replies: replies,
  }
}

proc dump_line(cmd: Int, attrs: List[Bytes], replies: List[Record]) [error] -> Result[Record, Error] {
  request_line(cmd, attrs, 773, replies)
}

proc single_line(cmd: Int, attrs: List[Bytes], replies: List[Record]) [error] -> Result[Record, Error] {
  request_line(cmd, attrs, 5, replies)
}

proc event_line(cmd: Int, attrs: List[Bytes], multicast: Int) [error] -> Result[Record, Error] {
  {op: "netlink_event", type: FAMILY, payload: genl(cmd, attrs)?.base64(), group: multicast}
}

proc mac(last: Int) [error] -> Result[Bytes, Error] {
  bytes.from_ints([2, 0, 0, 0, 0, last])
}

# One interface message as GET_INTERFACE reports it.
proc interface_reply(name: Str, index: Int, phy: Int, kind: Int, last_mac: Int) [error] -> Result[Record, Error] {
  reply_of(
    5,
    [
      nl.attr_u32(A_WIPHY, phy)?,
      nl.attr_str(A_IFNAME, name)?,
      nl.attr_u32(A_IFINDEX, index)?,
      nl.attr_u32(A_IFTYPE, kind)?,
      nl.attr_u64(A_WDEV, phy * 4294967296 + 1)?,
      nl.attr(A_MAC, mac(last_mac)?)?,
    ],
  )
}

proc wlan0_reply() [error] -> Result[Record, Error] {
  reply_of(
    5,
    [
      nl.attr_u32(A_WIPHY, 0)?,
      nl.attr_str(A_IFNAME, "wlan0")?,
      nl.attr_u32(A_IFINDEX, 3)?,
      nl.attr_u32(A_IFTYPE, 2)?,
      nl.attr_u64(A_WDEV, 1)?,
      nl.attr(A_MAC, mac(1)?)?,
      nl.attr(A_SSID, bytes.from_text("MyNet"))?,
      nl.attr_u32(A_WIPHY_FREQ, 2437)?,
      nl.attr_u32(A_CHANNEL_WIDTH, 1)?,
      nl.attr_u32(A_CENTER_FREQ1, 2437)?,
      nl.attr_u32(A_WIPHY_TX_POWER_LEVEL, 2000)?,
    ],
  )
}

proc fixture(ctx: TestContext, name: Str, lines: List[Record]) [fs, error] -> Result[Path, Error] {
  let file = test.temp_file(ctx, name: f"{name}.jsonl", contents: b"")?
  var values: List[Any] = []
  for line in lines {
    values += [line]
  }
  file.write(json.encode_lines(values)?)
  file
}

# Runs the applet under the linux fake with `lines` as the recorded kernel.
proc run_iw(ctx: TestContext, name: Str, lines: List[Record], argv: List[Str]) [fs, process, error] -> Result[IwRun, Error] {
  let recorded = fixture(ctx, name, lines)?
  let log = test.temp_file(ctx, name: f"{name}.log", contents: b"")?
  test.linux_fake(ctx, {netlink_fixture: recorded, log: log})?
  let source = fp"{ctx.core_dir}/iw.xsh".read_text()?
  let ran = test.run_script(ctx, source, argv, {XSH_MODULE_PATH: ctx.core_dir}, b"", "iw")?
  {success: ran.success, status: ran.status, stdout: ran.stdout, stderr: ran.stderr, calls: log.read_text()?}
}

test test_help_lists_the_supported_commands_in_iw_layout { |ctx|
  let source = fp"{ctx.core_dir}/iw.xsh".read_text()?
  let full = test.expect(ctx, source, status: 0, args: ["help"], env: {XSH_MODULE_PATH: ctx.core_dir})?
  assert full.stdout.starts_with("Usage:\tiw [options] command\nOptions:\n\t--version\tshow version (6.17)\nCommands:\n")
  assert "\tdev <devname> info\n\t\tShow information for this interface.\n\n" in full.stdout
  assert "\treg set <ISO/IEC 3166-1 alpha2>\n\t\tNotify the kernel about the current regulatory domain.\n\n" in full.stdout
  assert full.stdout.ends_with("\nDo NOT screenscrape this tool, we don't consider its output stable.\n\n")
  # Commands this applet does not implement are not advertised.
  assert "<devname> connect" not in full.stdout and "ibss join" not in full.stdout and "ap start" not in full.stdout

  # The text iw 6.17 prints for `help reg get`, less its --debug option line.
  let reg_get = test.expect(ctx, source, status: 0, args: ["help", "reg", "get"], env: {XSH_MODULE_PATH: ctx.core_dir})?
  let reg_get_expected = [
    "Usage:\tiw [options] command",
    "Options:",
    "\t--version\tshow version (6.17)",
    "Commands:",
    "\tphy <phyname> reg get",
    "\t\tPrint out the devices' current regulatory domain information.",
    "",
    "\treg get",
    "\t\tPrint out the kernel's current regulatory domain information.",
    "",
    "",
    "Commands that use the netdev ('dev') can also be given the",
    "'wdev' instead to identify the device.",
    "",
    "You can omit the 'phy' or 'dev' if the identification is unique,",
    "e.g. \"iw wlan0 info\" or \"iw phy0 info\". (Don't when scripting.)",
    "",
    "Do NOT screenscrape this tool, we don't consider its output stable.",
    "",
    "",
  ]
  assert reg_get.stdout == reg_get_expected.join("\n")

  let section = test.expect(ctx, source, status: 0, args: ["help", "reg"], env: {XSH_MODULE_PATH: ctx.core_dir})?
  assert "\treg get\n" in section.stdout and "\treg set <ISO/IEC 3166-1 alpha2>\n" in section.stdout
  assert "\tdev <devname> info" not in section.stdout

  let brief = test.expect(ctx, source, status: 0, env: {XSH_MODULE_PATH: ctx.core_dir})?
  assert "\tdev <devname> link\n" in brief.stdout
  assert "Show information for this interface." not in brief.stdout

  let none = test.expect(ctx, source, status: 0, args: ["help", "nonesuch"], env: {XSH_MODULE_PATH: ctx.core_dir})?
  assert "\tdev" not in none.stdout and none.stdout.starts_with("Usage:\tiw [options] command\n")
}

test test_version_unknown_commands_and_misplaced_arguments { |ctx|
  let source = fp"{ctx.core_dir}/iw.xsh".read_text()?
  let module_env = {XSH_MODULE_PATH: ctx.core_dir}
  let version = test.expect(ctx, source, status: 0, args: ["--version"], env: module_env)?
  assert version.stdout == "iw version 6.17\n"
  for argv in [["foo"], ["--bogus"], ["reg"], ["reg", "get", "extra"], ["list", "extra"], ["dev", "wlan0"], ["dev", "info"], ["wlan0", "info"]] {
    let usage = test.expect(ctx, source, status: 1, args: argv, env: module_env, stdout: ["Usage:\tiw [options] command\n"])?
    assert usage.stderr == ""
  }
  let regset = test.expect(ctx, source, status: 1, args: ["reg", "set"], env: module_env)?
  assert regset.stdout == "Usage:\tiw [options] reg set <ISO/IEC 3166-1 alpha2>\n\nNotify the kernel about the current regulatory domain.\n"
}

test test_reg_get_prints_the_recorded_kernel_domain { |ctx|
  # The reply is the kernel's own answer to GET_REG on a host without
  # wireless hardware, and the expected text is what iw 6.17 printed for it.
  let recorded = "HwEAAAcAIQAwMAAA5AEiADwAAAAIAAEAAAAAAAgAAgDQpiQACAADAEC4JQAIAAQAQJwAAAgABQBYAgAACAAGANAHAAAIAAcAAAAAADwAAQAIAAEAgAgAAAgAAgCofSUACAADAFDfJQAIAAQAIE4AAAgABQBYAgAACAAGANAHAAAIAAcAAAAAADwAAgAIAAEAgQAAAAgAAgAQwCUACAADADAOJgAIAAQAIE4AAAgABQBYAgAACAAGANAHAAAIAAcAAAAAADwAAwAIAAEAgAgAAAgAAgBQ404ACAADANAbUAAIAAQAgDgBAAgABQBYAgAACAAGANAHAAAIAAcAAAAAADwABAAIAAEAkAgAAAgAAgDQG1AACAADAFBUUQAIAAQAgDgBAAgABQBYAgAACAAGANAHAAAIAAcAAAAAADwABQAIAAEAkAAAAAgAAgBQxVMACAADANBuVwAIAAQAAHECAAgABQBYAgAACAAGANAHAAAIAAcAAAAAADwABgAIAAEAgAAAAAgAAgBYglcACAADAPgIWQAIAAQAgDgBAAgABQBYAgAACAAGANAHAAAIAAcAAAAAADwABwAIAAEAAAAAAAgAAgDAaWkDCAADAEBKzAMIAAQAgPUgAAgABQAAAAAACAAGAAAAAAAIAAcAAAAAAA=="
  let lines: List[Record] = [
    family_line(),
    {op: "netlink_request", type: FAMILY, cmd: 31, flags: 773, replies: [{type: FAMILY, flags: 2, payload: recorded}]},
  ]
  let output = run_iw(ctx, "reg-get", lines, ["reg", "get"])?
  assert output.success, output.stderr
  let expected = [
    "global",
    "country 00: DFS-UNSET",
    "\t(2402 - 2472 @ 40), (6, 20), (N/A)",
    "\t(2457 - 2482 @ 20), (6, 20), (N/A), AUTO-BW, PASSIVE-SCAN",
    "\t(2474 - 2494 @ 20), (6, 20), (N/A), NO-OFDM, PASSIVE-SCAN",
    "\t(5170 - 5250 @ 80), (6, 20), (N/A), AUTO-BW, PASSIVE-SCAN",
    "\t(5250 - 5330 @ 80), (6, 20), (0 ms), DFS, AUTO-BW, PASSIVE-SCAN",
    "\t(5490 - 5730 @ 160), (6, 20), (0 ms), DFS, PASSIVE-SCAN",
    "\t(5735 - 5835 @ 80), (6, 20), (N/A), PASSIVE-SCAN",
    "\t(57240 - 63720 @ 2160), (N/A, 0), (N/A)",
    "",
  ]
  assert output.stdout == expected.join("\n") + "\n"
}

test test_reg_get_for_a_wiphy_names_it_and_a_self_managed_domain { |ctx|
  let rule = nl.attr_nested(
    1,
    [nl.attr_u32(1, 0)?, nl.attr_u32(2, 2402000)?, nl.attr_u32(3, 2472000)?, nl.attr_u32(4, 40000)?, nl.attr_u32(5, 300)?, nl.attr_u32(6, 1500)?],
  )?
  let domain = reply_of(
    31,
    [
      nl.attr_u32(A_WIPHY, 2)?,
      nl.attr(A_REG_ALPHA2, b"DE\0\0")?,
      nl.attr_u32(216, 1)?,
      nl.attr_u8(146, 2)?,
      nl.attr_nested(34, [rule])?,
    ],
  )?
  let lines: List[Record] = [
    family_line(),
    {op: "netlink_request", type: FAMILY, cmd: 31, flags: 773, replies: [domain]},
    {op: "netlink_request", type: FAMILY, cmd: 31, flags: 5, replies: [domain]},
  ]
  let output = run_iw(ctx, "reg-self-managed", lines, ["reg", "get"])?
  assert output.success, output.stderr
  assert output.stdout == "phy#2 (self-managed)\ncountry DE: DFS-ETSI\n\t(2402 - 2472 @ 40), (3, 15), (N/A)\n\n"
}

test test_reg_set_sends_the_alpha2_and_reports_a_kernel_refusal { |ctx|
  let alpha2 = nl.attr_str(A_REG_ALPHA2, "US")?
  let accepted: List[Record] = [family_line(), single_line(27, [alpha2], [])?]
  let ok = run_iw(ctx, "reg-set", accepted, ["reg", "set", "US"])?
  assert ok.success and ok.stdout == "", ok.stderr
  let calls = ok.calls
  assert "\"op\":\"netlink_request\"" in calls
  assert request_payload(27, [alpha2])?.base64() in calls

  let refused: List[Record] = [family_line(), {op: "netlink_request", type: FAMILY, cmd: 27, flags: 5, errno: 1}]
  let denied = run_iw(ctx, "reg-set-denied", refused, ["reg", "set", "US"])?
  assert denied.status == 255
  assert denied.stderr == "command failed: Operation not permitted (-1)\n"

  let source = fp"{ctx.core_dir}/iw.xsh".read_text()?
  let bad = test.expect(ctx, source, status: 2, args: ["reg", "set", "USA"], env: {XSH_MODULE_PATH: ctx.core_dir})?
  assert bad.stderr == "not a valid ISO/IEC 3166-1 alpha2\nSpecial non-alpha2 usable entries:\n\t00\tWorld Regulatory domain\n"
}

test test_reg_reload_sends_one_request { |ctx|
  let lines: List[Record] = [family_line(), single_line(126, [], [])?]
  let output = run_iw(ctx, "reg-reload", lines, ["reg", "reload"])?
  assert output.success and output.stdout == "", output.stderr
  assert request_payload(126, [])?.base64() in output.calls
}

test test_dev_lists_interfaces_under_their_phy { |ctx|
  let lines: List[Record] = [
    family_line(),
    dump_line(
      5,
      [],
      [wlan0_reply()?, interface_reply("wlan1", 4, 0, 2, 2)?, interface_reply("mon0", 5, 1, 6, 3)?],
    )?,
  ]
  let output = run_iw(ctx, "dev", lines, ["dev"])?
  assert output.success, output.stderr
  let expected = [
    "phy#0",
    "\tInterface wlan0",
    "\t\tifindex 3",
    "\t\twdev 0x1",
    "\t\taddr 02:00:00:00:00:01",
    "\t\tssid MyNet",
    "\t\ttype managed",
    "\t\tchannel 6 (2437 MHz), width: 20 MHz, center1: 2437 MHz",
    "\t\ttxpower 20.00 dBm",
    "\tInterface wlan1",
    "\t\tifindex 4",
    "\t\twdev 0x1",
    "\t\taddr 02:00:00:00:00:02",
    "\t\ttype managed",
    "phy#1",
    "\tInterface mon0",
    "\t\tifindex 5",
    "\t\twdev 0x100000001",
    "\t\taddr 02:00:00:00:00:03",
    "\t\ttype monitor",
  ]
  assert output.stdout == expected.join("\n") + "\n"
}

test test_dev_info_resolves_the_name_and_unknown_names_fail_like_iw { |ctx|
  let by_index = single_line(5, [nl.attr_u32(A_IFINDEX, 3)?], [wlan0_reply()?])?
  let lines: List[Record] = [
    family_line(),
    dump_line(5, [], [wlan0_reply()?])?,
    by_index,
    dump_line(1, [nl.attr_flag(A_SPLIT_WIPHY_DUMP)?], [])?,
  ]
  let info = run_iw(ctx, "dev-info", lines, ["dev", "wlan0", "info"])?
  assert info.success, info.stderr
  let expected = [
    "Interface wlan0",
    "\tifindex 3",
    "\twdev 0x1",
    "\taddr 02:00:00:00:00:01",
    "\tssid MyNet",
    "\ttype managed",
    "\twiphy 0",
    "\tchannel 6 (2437 MHz), width: 20 MHz, center1: 2437 MHz",
    "\ttxpower 20.00 dBm",
  ]
  assert info.stdout == expected.join("\n") + "\n"

  let missing = run_iw(ctx, "dev-missing", lines, ["dev", "wlan9", "info"])?
  assert missing.status == 237 and missing.stdout == ""
  assert missing.stderr == "command failed: No such device (-19)\n"
  # The device is resolved before the command words are read, as in iw.
  let unknown = run_iw(ctx, "dev-missing-command", lines, ["dev", "wlan9", "bogus"])?
  assert unknown.status == 237
  let no_phy = run_iw(ctx, "phy-missing", lines, ["phy", "phy9", "info"])?
  assert no_phy.status == 254 and no_phy.stderr == "command failed: No such file or directory (-2)\n"

  let implicit = run_iw(ctx, "dev-implicit", lines, ["wlan0", "info"])?
  assert implicit.success and implicit.stdout == info.stdout, implicit.stderr
  let bad_word = run_iw(ctx, "dev-bad-word", lines, ["dev", "wlan0", "bogus"])?
  assert bad_word.status == 1 and bad_word.stdout.starts_with("Usage:\tiw [options] command\n")
}

proc phy_messages() [error] -> Result[List[Record], Error] {
  let first = reply_of(
    3,
    [
      nl.attr_u32(A_WIPHY, 0)?,
      nl.attr_str(A_WIPHY_NAME, "phy0")?,
      nl.attr(A_MAX_NUM_SCAN_SSIDS, bytes.from_ints([4])?)?,
      nl.attr_u16(A_MAX_SCAN_IE_LEN, 2257)?,
      nl.attr(A_WIPHY_RETRY_SHORT, bytes.from_ints([7])?)?,
      nl.attr(A_WIPHY_RETRY_LONG, bytes.from_ints([4])?)?,
      nl.attr(A_WIPHY_COVERAGE_CLASS, bytes.from_ints([0])?)?,
      nl.attr(A_CIPHER_SUITES, bytes.concat([bytes.from_ints([0, 15, 172, 4])?, bytes.from_ints([0, 15, 172, 2])?]))?,
      nl.attr_nested(A_SUPPORTED_IFTYPES, [nl.attr_flag(2)?, nl.attr_flag(3)?, nl.attr_flag(6)?])?,
    ],
  )?
  let freq_2412 = nl.attr_nested(0, [nl.attr_u32(1, 2412)?, nl.attr_u32(6, 2000)?])?
  let freq_2467 = nl.attr_nested(1, [nl.attr_u32(1, 2467)?, nl.attr_flag(2)?])?
  let freq_5260 = nl.attr_nested(2, [nl.attr_u32(1, 5260)?, nl.attr_u32(6, 2300)?, nl.attr_flag(3)?, nl.attr_flag(5)?])?
  let rate_10 = nl.attr_nested(0, [nl.attr_u32(1, 10)?, nl.attr_flag(2)?])?
  let rate_60 = nl.attr_nested(1, [nl.attr_u32(1, 60)?])?
  let band = nl.attr_nested(
    0,
    [
      nl.attr(4, bytes.pack_le(6627, 2)?)?,
      nl.attr(5, bytes.from_ints([3])?)?,
      nl.attr(6, bytes.from_ints([5])?)?,
      nl.attr(3, bytes.concat([bytes.from_ints([255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 44, 1, 1, 0, 0, 0])?]))?,
      nl.attr_nested(1, [freq_2412, freq_2467, freq_5260])?,
      nl.attr_nested(2, [rate_10, rate_60])?,
    ],
  )?
  let second = reply_of(3, [nl.attr_u32(A_WIPHY, 0)?, nl.attr_nested(A_WIPHY_BANDS, [band])?])?
  let third = reply_of(
    3,
    [
      nl.attr_u32(A_WIPHY, 0)?,
      nl.attr_nested(A_SUPPORTED_COMMANDS, [nl.attr_u32(1, 5)?, nl.attr_u32(2, 33)?, nl.attr_u32(3, 31)?])?,
    ],
  )?
  [first, second, third]
}

test test_list_and_phy_info_merge_a_split_wiphy_dump { |ctx|
  let split = nl.attr_flag(A_SPLIT_WIPHY_DUMP)?
  let by_index = nl.attr_u32(A_WIPHY, 0)?
  let lines: List[Record] = [
    family_line(),
    dump_line(1, [split], phy_messages()?)?,
    dump_line(1, [split, by_index], phy_messages()?)?,
  ]
  let expected = [
    "Wiphy phy0",
    "\twiphy index: 0",
    "\tmax # scan SSIDs: 4",
    "\tmax scan IEs length: 2257 bytes",
    "\tRetry short limit: 7",
    "\tRetry long limit: 4",
    "\tCoverage class: 0 (up to 0m)",
    "\tSupported Ciphers:",
    "\t\t* CCMP-128 (00-0f-ac:4)",
    "\t\t* TKIP (00-0f-ac:2)",
    "\tSupported interface modes:",
    "\t\t * managed",
    "\t\t * AP",
    "\t\t * monitor",
    "\twiphy index: 0",
    "\tBand 1:",
    "\t\tCapabilities: 0x19e3",
    "\t\t\tRX LDPC",
    "\t\t\tHT20/HT40",
    "\t\t\tStatic SM Power Save",
    "\t\t\tRX HT20 SGI",
    "\t\t\tRX HT40 SGI",
    "\t\t\tTX STBC",
    "\t\t\tRX STBC 1-stream",
    "\t\t\tMax AMSDU length: 7935 bytes",
    "\t\t\tDSSS/CCK HT40",
    "\t\tMaximum RX AMPDU length 65535 bytes (exponent: 0x003)",
    "\t\tMinimum RX AMPDU time spacing: 4 usec (0x05)",
    "\t\tHT Max RX data rate: 300 Mbps",
    "\t\tHT TX/RX MCS rate indexes supported: 0-15",
    "\t\tFrequencies:",
    "\t\t\t* 2412 MHz [1] (20.0 dBm)",
    "\t\t\t* 2467 MHz [12] (disabled)",
    "\t\t\t* 5260 MHz [52] (23.0 dBm) (no IR, radar detection)",
    "\t\tBitrates (non-HT):",
    "\t\t\t* 1.0 Mbps (short preamble supported)",
    "\t\t\t* 6.0 Mbps",
    "\twiphy index: 0",
    "\tSupported commands:",
    "\t\t * get_interface",
    "\t\t * trigger_scan",
    "\t\t * get_reg",
  ]
  for argv in [["list"], ["phy"]] {
    let output = run_iw(ctx, "list", lines, argv)?
    assert output.success, output.stderr
    assert output.stdout == expected.join("\n") + "\n"
  }
}

proc bss_reply(bssid_last: Int, status: Int?, ies: Bytes, boottime: Int?) [error] -> Result[Record, Error] {
  var fields = [
    nl.attr(1, mac(bssid_last)?)?,
    nl.attr_u32(2, 2437)?,
    nl.attr_u64(3, 90061000000)?,
    nl.attr_u16(4, 100)?,
    nl.attr_u16(5, 1073)?,
    nl.attr(6, ies)?,
    nl.attr_u32(7, -4500 + 4294967296)?,
    nl.attr_u32(10, 120)?,
  ]
  if status != null {
    fields += [nl.attr_u32(9, status)?]
  }
  if boottime != null {
    fields += [nl.attr_u64(15, boottime)?]
  }
  reply_of(
    34,
    [nl.attr_u32(A_IFINDEX, 3)?, nl.attr_nested(A_BSS, fields)?],
  )
}

proc ie(id: Int, data: List[Int]) [error] -> Result[Bytes, Error] {
  bytes.concat([bytes.from_ints([id, data.len()])?, bytes.from_ints(data)?])
}

proc beacon_elements() [error] -> Result[Bytes, Error] {
  bytes.concat([
    ie(0, [77, 121, 78, 101, 116])?,
    ie(1, [130, 132, 139, 150, 12, 18, 24, 36])?,
    ie(3, [6])?,
    ie(5, [0, 1, 0, 0])?,
    ie(7, [85, 83, 32, 1, 11, 30])?,
    ie(42, [4])?,
    ie(
      48,
      [1, 0, 0, 15, 172, 4, 1, 0, 0, 15, 172, 4, 1, 0, 0, 15, 172, 2, 12, 0],
    )?,
    ie(50, [48, 72, 96, 108])?,
    ie(45, [239, 9, 27])?,
  ])
}

test test_scan_dump_prints_bss_blocks_in_iw_layout { |ctx|
  let scan_request = nl.attr_u32(A_IFINDEX, 3)?
  let lines: List[Record] = [
    family_line(),
    dump_line(5, [], [wlan0_reply()?])?,
    dump_line(32, [scan_request], [bss_reply(10, null, beacon_elements()?, 4000000000)?])?,
  ]
  let output = run_iw(ctx, "scan-dump", lines, ["dev", "wlan0", "scan", "dump"])?
  assert output.success, output.stderr
  let expected = [
    "BSS 02:00:00:00:00:0a(on wlan0)",
    "\tlast seen: 4.000s [boottime]",
    "\tTSF: 90061000000 usec (1d, 01:01:01)",
    "\tfreq: 2437",
    "\tbeacon interval: 100 TUs",
    "\tcapability: ESS Privacy ShortPreamble ShortSlotTime (0x0431)",
    "\tsignal: -45.00 dBm",
    "\tlast seen: 120 ms ago",
    "\tSSID: MyNet",
    "\tSupported rates: 1.0* 2.0* 5.5* 11.0* 6.0 9.0 12.0 18.0 ",
    "\tDS Parameter set: channel 6",
    "\tTIM: DTIM Count 0 DTIM Period 1 Bitmap Control 0x0 Bitmap[0] 0x0",
    "\tCountry: US\tEnvironment: Indoor/Outdoor",
    "\t\tChannels [1 - 11] @ 30 dBm",
    "\tERP: <barker preamble mode>",
    "\tRSN:\t * Version: 1",
    "\t\t * Group cipher: CCMP",
    "\t\t * Pairwise ciphers: CCMP",
    "\t\t * Authentication suites: PSK",
    "\t\t * Capabilities: 16-PTKSA-RC 1-GTKSA-RC (0x000c)",
    "\tExtended supported rates: 24.0 36.0 48.0 54.0 ",
  ]
  # Without -u the element this decoder has no printer for stays silent.
  assert output.stdout == expected.join("\n") + "\n"

  let unknown = run_iw(ctx, "scan-dump-u", lines, ["dev", "wlan0", "scan", "dump", "-u"])?
  assert unknown.success, unknown.stderr
  assert unknown.stdout == (expected + ["\tUnknown IE (45): ef 09 1b"]).join("\n") + "\n"
}

test test_scan_dump_reports_both_element_sets_and_status { |ctx|
  let beacon = ie(0, [32, 66])?
  let response = bytes.concat([ie(0, [32, 66])?, ie(3, [11])?])
  let fields = [
    nl.attr(1, mac(11)?)?,
    nl.attr_u32(9, 1)?,
    nl.attr(6, response)?,
    nl.attr(11, beacon)?,
  ]
  let message = reply_of(34, [nl.attr_u32(A_IFINDEX, 3)?, nl.attr_nested(A_BSS, fields)?])?
  let lines: List[Record] = [
    family_line(),
    dump_line(5, [], [wlan0_reply()?])?,
    dump_line(32, [nl.attr_u32(A_IFINDEX, 3)?], [message])?,
  ]
  let output = run_iw(ctx, "scan-both", lines, ["dev", "wlan0", "scan", "dump"])?
  assert output.success, output.stderr
  let expected = [
    "BSS 02:00:00:00:00:0b(on wlan0) -- associated",
    "\tInformation elements from Probe Response frame:",
    "\tSSID: \\x20B",
    "\tDS Parameter set: channel 11",
    "\tInformation elements from Beacon frame:",
    "\tSSID: \\x20B",
  ]
  assert output.stdout == expected.join("\n") + "\n"
}

test test_scan_triggers_subscribes_waits_and_dumps { |ctx|
  let trigger = [
    nl.attr_u32(A_IFINDEX, 3)?,
    nl.attr_nested(A_SCAN_SSIDS, [nl.attr(0, bytes.from_text("MyNet"))?])?,
    nl.attr_nested(A_SCAN_FREQUENCIES, [nl.attr_u32(0, 2412)?, nl.attr_u32(1, 2437)?])?,
    nl.attr_u32(158, 3)?,
  ]
  let dump = bss_reply(10, null, beacon_elements()?, null)?
  let lines: List[Record] = [
    family_line(),
    groups_line()?,
    dump_line(5, [], [wlan0_reply()?])?,
    single_line(33, trigger, [])?,
    event_line(33, [nl.attr_u32(A_IFINDEX, 3)?, nl.attr_u32(A_WIPHY, 0)?], GROUP_SCAN)?,
    event_line(34, [nl.attr_u32(A_IFINDEX, 9)?], GROUP_SCAN)?,
    event_line(34, [nl.attr_u32(A_IFINDEX, 3)?], GROUP_SCAN)?,
    dump_line(32, [nl.attr_u32(A_IFINDEX, 3)?], [dump])?,
  ]
  let argv = ["dev", "wlan0", "scan", "freq", "2412", "2437", "ssid", "MyNet", "lowpri", "flush"]
  let output = run_iw(ctx, "scan", lines, argv)?
  assert output.success, output.stderr
  assert output.stdout.starts_with("BSS 02:00:00:00:00:0a(on wlan0)\n\tTSF: 90061000000 usec")
  let calls = output.calls
  # The scan group is joined before the trigger is sent.
  let joined = calls.find("\"op\":\"setsockopt\"") ?? -1
  let sent = calls.find(request_payload(33, trigger)?.base64()) ?? -1
  assert joined >= 0 and sent > joined
  assert f"\"value\":\"{GROUP_SCAN}\"" in calls
}

test test_scan_abort_event_and_abort_command { |ctx|
  let trigger = [nl.attr_u32(A_IFINDEX, 3)?, nl.attr_nested(A_SCAN_SSIDS, [nl.attr(0, b"")?])?]
  let lines: List[Record] = [
    family_line(),
    groups_line()?,
    dump_line(5, [], [wlan0_reply()?])?,
    single_line(33, trigger, [])?,
    event_line(35, [nl.attr_u32(A_IFINDEX, 3)?], GROUP_SCAN)?,
    single_line(114, [nl.attr_u32(A_IFINDEX, 3)?], [])?,
  ]
  let aborted = run_iw(ctx, "scan-aborted", lines, ["dev", "wlan0", "scan"])?
  assert aborted.success and aborted.stdout == "scan aborted!\n", aborted.stderr
  let command = run_iw(ctx, "scan-abort", lines, ["dev", "wlan0", "scan", "abort"])?
  assert command.success and command.stdout == "", command.stderr
  assert request_payload(114, [nl.attr_u32(A_IFINDEX, 3)?])?.base64() in command.calls
}

proc sta_info_reply(last: Int) [error] -> Result[Record, Error] {
  let flags = bytes.concat([bytes.pack_le(254, 4)?, bytes.pack_le(170, 4)?])
  let tx_rate = nl.attr_nested(8, [nl.attr_u32(5, 1444)?, nl.attr(2, bytes.from_ints([15])?)?, nl.attr_flag(4)?])?
  let rx_rate = nl.attr_nested(14, [nl.attr_u16(1, 1300)?, nl.attr(2, bytes.from_ints([14])?)?])?
  let parameters = nl.attr_nested(
    15,
    [nl.attr(4, bytes.from_ints([2])?)?, nl.attr_u16(5, 100)?, nl.attr_flag(3)?],
  )?
  let info = nl.attr_nested(
    A_STA_INFO,
    [
      nl.attr_u32(1, 1520)?,
      nl.attr_u32(2, 123456)?,
      nl.attr_u32(9, 789)?,
      nl.attr_u32(3, 654321)?,
      nl.attr_u32(10, 321)?,
      nl.attr_u32(11, 4)?,
      nl.attr_u32(12, 1)?,
      nl.attr(7, bytes.from_ints([211])?)?,
      nl.attr_nested(25, [nl.attr(1, bytes.from_ints([211])?)?, nl.attr(2, bytes.from_ints([209])?)?])?,
      nl.attr(13, bytes.from_ints([212])?)?,
      nl.attr_u32(27, 54375)?,
      tx_rate,
      rx_rate,
      nl.attr(17, flags)?,
      parameters,
      nl.attr_u32(16, 1234)?,
    ],
  )?
  reply_of(19, [nl.attr_u32(A_IFINDEX, 3)?, nl.attr(A_MAC, mac(last)?)?, info])
}

test test_station_dump_and_get_print_every_decoded_field { |ctx|
  let expected = [
    "Station 02:00:00:00:00:02 (on wlan0)",
    "\tinactive time:\t1520 ms",
    "\trx bytes:\t123456",
    "\trx packets:\t789",
    "\ttx bytes:\t654321",
    "\ttx packets:\t321",
    "\ttx retries:\t4",
    "\ttx failed:\t1",
    "\tsignal:  \t-45 [-45, -47] dBm",
    "\tsignal avg:\t-44 dBm",
    "\ttx bitrate:\t144.4 MBit/s MCS 15 short GI",
    "\trx bitrate:\t130.0 MBit/s MCS 14",
    "\texpected throughput:\t54.375Mbps",
    "\tDTIM period:\t2",
    "\tbeacon interval:100",
    "\tshort slot time:\tyes",
    "\tconnected time:\t1234 seconds",
    "\tauthorized:\tyes",
    "\tauthenticated:\tyes",
    "\tassociated:\tyes",
    "\tpreamble:\tlong",
    "\tWMM/WME:\tyes",
    "\tMFP:\t\tno",
    "\tTDLS peer:\tno",
  ]
  let target_mac = nl.attr(A_MAC, mac(2)?)?
  let lines: List[Record] = [
    family_line(),
    dump_line(5, [], [wlan0_reply()?])?,
    dump_line(17, [nl.attr_u32(A_IFINDEX, 3)?], [sta_info_reply(2)?, sta_info_reply(2)?])?,
    single_line(17, [nl.attr_u32(A_IFINDEX, 3)?, target_mac], [sta_info_reply(2)?])?,
  ]
  let dump = run_iw(ctx, "station-dump", lines, ["dev", "wlan0", "station", "dump"])?
  assert dump.success, dump.stderr
  # The last line of each station is the wall-clock time `iw` prints.
  let stations = dump.stdout.lines()
  assert stations.len() == 2 * (expected.len() + 1)
  assert stations[0..expected.len()] == expected
  assert stations[expected.len() + 1..2 * expected.len() + 1] == expected
  assert rx"^\tcurrent time:\t\d+ ms$".matches(stations[expected.len()])
  let one = run_iw(ctx, "station-get", lines, ["dev", "wlan0", "station", "get", "02:00:00:00:00:02"])?
  assert one.success and one.stdout.lines()[0..expected.len()] == expected, one.stderr
  let invalid = run_iw(ctx, "station-bad-mac", lines, ["dev", "wlan0", "station", "get", "nope"])?
  assert invalid.status == 2 and invalid.stderr == "invalid mac address\n"
}

test test_link_reports_connection_and_not_connected { |ctx|
  let ssid = ie(0, [77, 121, 78, 101, 116])?
  let associated = bss_reply(10, 1, ssid, null)?
  let neighbour = bss_reply(11, null, ssid, null)?
  let station = sta_info_reply(10)?
  let scan_request = nl.attr_u32(A_IFINDEX, 3)?
  let connected_lines: List[Record] = [
    family_line(),
    dump_line(5, [], [wlan0_reply()?])?,
    dump_line(32, [scan_request], [neighbour, associated])?,
    single_line(17, [scan_request, nl.attr(A_MAC, mac(10)?)?], [station])?,
  ]
  let connected = run_iw(ctx, "link", connected_lines, ["dev", "wlan0", "link"])?
  assert connected.success, connected.stderr
  let expected = [
    "Connected to 02:00:00:00:00:0a (on wlan0)",
    "\tSSID: MyNet",
    "\tfreq: 2437.0",
    "\tRX: 123456 bytes (789 packets)",
    "\tTX: 654321 bytes (321 packets)",
    "\tsignal: -45 dBm",
    "\trx bitrate: 130.0 MBit/s MCS 14",
    "\ttx bitrate: 144.4 MBit/s MCS 15 short GI",
    "\tbss flags: short-slot-time",
    "\tdtim period: 2",
    "\tbeacon int: 100",
  ]
  assert connected.stdout == expected.join("\n") + "\n"

  let idle_lines: List[Record] = [
    family_line(),
    dump_line(5, [], [wlan0_reply()?])?,
    dump_line(32, [scan_request], [neighbour])?,
  ]
  let idle = run_iw(ctx, "link-idle", idle_lines, ["dev", "wlan0", "link"])?
  assert idle.success and idle.stdout == "Not connected.\n", idle.stderr
}

test test_set_commands_send_one_request_each { |ctx|
  let device = nl.attr_u32(A_IFINDEX, 3)?
  let lines: List[Record] = [
    family_line(),
    dump_line(5, [], [wlan0_reply()?])?,
    dump_line(1, [nl.attr_flag(A_SPLIT_WIPHY_DUMP)?], [{type: FAMILY, flags: 2, payload: genl(3, [nl.attr_u32(A_WIPHY, 0)?, nl.attr_str(A_WIPHY_NAME, "phy0")?])?.base64()}])?,
    single_line(6, [device, nl.attr_u32(A_IFTYPE, 6)?], [])?,
    single_line(2, [device, nl.attr_u32(97, 2)?, nl.attr_u32(98, 1500)?], [])?,
    single_line(2, [device, nl.attr_u32(97, 0)?], [])?,
    single_line(2, [device, nl.attr_u32(A_WIPHY_FREQ, 2437)?, nl.attr_u32(A_CHANNEL_WIDTH, 2)?, nl.attr_u32(A_CENTER_FREQ1, 2447)?], [])?,
    single_line(2, [device, nl.attr_u32(A_WIPHY_FREQ, 5180)?, nl.attr_u32(A_CHANNEL_WIDTH, 1)?, nl.attr_u32(A_CENTER_FREQ1, 5180)?], [])?,
    single_line(2, [nl.attr_u32(A_WIPHY, 0)?, nl.attr_u32(97, 1)?, nl.attr_u32(98, 1000)?], [])?,
    single_line(2, [device, nl.attr_u32(A_WIPHY_FREQ, 5180)?, nl.attr_u32(A_CHANNEL_WIDTH, 2)?, nl.attr_u32(A_CENTER_FREQ1, 5170)?], [])?,
    single_line(2, [device, nl.attr_u32(A_WIPHY_FREQ, 2412)?, nl.attr_u32(A_CHANNEL_WIDTH, 0)?, nl.attr_u32(A_CENTER_FREQ1, 2412)?], [])?,
    single_line(2, [nl.attr_u32(A_WIPHY, 0)?, nl.attr_u32(A_WIPHY_FREQ, 2484)?, nl.attr_u32(A_CHANNEL_WIDTH, 1)?, nl.attr_u32(A_CENTER_FREQ1, 2484)?], [])?,
  ]
  let cases = [
    ["dev", "wlan0", "set", "type", "monitor"],
    ["dev", "wlan0", "set", "txpower", "fixed", "1500"],
    ["dev", "wlan0", "set", "txpower", "auto"],
    ["dev", "wlan0", "set", "channel", "6", "HT40+"],
    ["dev", "wlan0", "set", "freq", "5180", "HT20"],
    ["phy", "phy0", "set", "txpower", "limit", "1000"],
    ["dev", "wlan0", "set", "channel", "36", "HT40-"],
    ["dev", "wlan0", "set", "freq", "2412"],
    ["phy", "phy0", "set", "channel", "14", "HT20"],
  ]
  for argv in cases {
    let output = run_iw(ctx, "set", lines, argv)?
    assert output.success and output.stdout == "", f"{argv.join(" ")}: {output.stderr}"
  }
  let invalid = [
    ["dev", "wlan0", "set", "type", "bogus"],
    ["dev", "wlan0", "set", "type"],
    ["dev", "wlan0", "set", "txpower", "fixed"],
    ["dev", "wlan0", "set", "txpower", "huge", "1"],
    ["dev", "wlan0", "set", "channel", "x"],
    ["dev", "wlan0", "set", "channel", "6", "HT80"],
    ["dev", "wlan0", "set", "channel", "0"],
  ]
  for argv in invalid {
    let output = run_iw(ctx, "set-invalid", lines, argv)?
    assert output.status != 0, argv.join(" ")
  }
  let bogus = run_iw(ctx, "set-bogus-type", lines, ["dev", "wlan0", "set", "type", "bogus"])?
  assert bogus.status == 2 and bogus.stderr == "invalid interface type bogus\n"
  let missing = run_iw(ctx, "set-missing-level", lines, ["dev", "wlan0", "set", "txpower", "fixed"])?
  assert missing.status == 2 and missing.stderr == "Missing TX power level argument.\n"
  let invalid_setting = run_iw(ctx, "set-bad-setting", lines, ["dev", "wlan0", "set", "txpower", "huge", "1"])?
  assert invalid_setting.status == 2 and invalid_setting.stderr == "Invalid parameter: huge\n"
  let usage = run_iw(ctx, "set-missing", lines, ["dev", "wlan0", "set", "txpower"])?
  assert usage.status == 1
  assert usage.stdout.starts_with("Usage:\tiw [options] dev <devname> set txpower <auto|fixed|limit> [<tx power in mBm>]\n\nSpecify transmit power level and setting type.\n")
}

test test_scan_trigger_builds_the_request_from_every_option { |ctx|
  let request = [
    nl.attr_u32(A_IFINDEX, 3)?,
    nl.attr_nested(A_SCAN_FREQUENCIES, [nl.attr_u32(0, 2412)?])?,
    nl.attr(42, bytes.concat([bytes.from_ints([0, 1, 255])?, bytes.from_ints([114, 4])?, bytes.from_text("mesh")]))?,
    nl.attr_u16(235, 20)?,
    nl.attr_flag(236)?,
    nl.attr_u32(158, 68)?,
  ]
  let lines: List[Record] = [
    family_line(),
    dump_line(5, [], [wlan0_reply()?])?,
    single_line(33, request, [])?,
  ]
  let argv = ["dev", "wlan0", "scan", "trigger", "freq", "2412", "duration", "20", "ies", "00:01:ff", "meshid", "mesh", "passive", "ap-force", "duration-mandatory", "coloc"]
  let output = run_iw(ctx, "scan-options", lines, argv)?
  # `trigger` returns once the kernel accepted the request, with no results.
  assert output.success and output.stdout == "", output.stderr
  for words in [["bogus"], ["duration"], ["ies", "zz"], ["ies", "00:"], ["passive", "ssid", "a"], ["-u"], ["meshid"]] {
    let bad = run_iw(ctx, "scan-bad", lines, ["dev", "wlan0", "scan", "trigger"] + words)?
    assert bad.status == 1 and "dev <devname> scan trigger" in bad.stdout, words.join(" ")
  }
  let coloc = run_iw(ctx, "scan-coloc", lines, ["dev", "wlan0", "scan", "coloc"])?
  assert coloc.status == 1 and "scan [-u]" in coloc.stdout
}

test test_scan_u_flag_reaches_the_results_of_a_waited_scan { |ctx|
  let trigger = [nl.attr_u32(A_IFINDEX, 3)?, nl.attr_nested(A_SCAN_SSIDS, [nl.attr(0, b"")?])?]
  let lines: List[Record] = [
    family_line(),
    groups_line()?,
    dump_line(5, [], [wlan0_reply()?])?,
    single_line(33, trigger, [])?,
    event_line(34, [nl.attr_u32(A_IFINDEX, 3)?], GROUP_SCAN)?,
    dump_line(32, [nl.attr_u32(A_IFINDEX, 3)?], [bss_reply(10, null, ie(45, [1, 2])?, null)?])?,
  ]
  let plain = run_iw(ctx, "scan-plain", lines, ["dev", "wlan0", "scan"])?
  assert plain.success and "Unknown IE" not in plain.stdout, plain.stderr
  let unknown = run_iw(ctx, "scan-unknown", lines, ["dev", "wlan0", "scan", "-u"])?
  assert unknown.success and "\tUnknown IE (45): 01 02\n" in unknown.stdout, unknown.stderr
}

test test_phy_selectors_by_index_by_name_and_implicit_form { |ctx|
  let split = nl.attr_flag(A_SPLIT_WIPHY_DUMP)?
  let by_index = nl.attr_u32(A_WIPHY, 0)?
  let domain = reply_of(
    31,
    [nl.attr_u32(A_WIPHY, 0)?, nl.attr(A_REG_ALPHA2, b"US\0\0")?, nl.attr_nested(34, [])?],
  )?
  let lines: List[Record] = [
    family_line(),
    dump_line(5, [], [wlan0_reply()?])?,
    dump_line(1, [split], phy_messages()?)?,
    dump_line(1, [split, by_index], phy_messages()?)?,
    single_line(31, [by_index], [domain])?,
  ]
  let reference = run_iw(ctx, "phy-name", lines, ["phy", "phy0", "info"])?
  assert reference.success and reference.stdout.starts_with("Wiphy phy0\n"), reference.stderr
  let numeric = run_iw(ctx, "phy-index", lines, ["phy", "phy#0", "info"])?
  assert numeric.success and numeric.stdout == reference.stdout, numeric.stderr
  let implicit = run_iw(ctx, "phy-implicit", lines, ["phy0", "info"])?
  assert implicit.success and implicit.stdout == reference.stdout, implicit.stderr
  let regulatory = run_iw(ctx, "phy-reg", lines, ["phy", "phy0", "reg", "get"])?
  assert regulatory.success, regulatory.stderr
  assert regulatory.stdout == "phy#0\ncountry US: DFS-UNSET\n\n"
}

test test_event_prints_subscribed_events_until_the_stream_ends { |ctx|
  let lines: List[Record] = [
    family_line(),
    groups_line()?,
    dump_line(5, [], [wlan0_reply()?])?,
    event_line(33, [nl.attr_u32(A_IFINDEX, 3)?, nl.attr_u32(A_WIPHY, 0)?], GROUP_SCAN)?,
    event_line(
      34,
      [
        nl.attr_u32(A_IFINDEX, 3)?,
        nl.attr_u32(A_WIPHY, 0)?,
        nl.attr_nested(A_SCAN_FREQUENCIES, [nl.attr_u32(0, 2412)?, nl.attr_u32(1, 2437)?])?,
        nl.attr_nested(A_SCAN_SSIDS, [nl.attr(0, b"")?])?,
      ],
      GROUP_SCAN,
    )?,
    event_line(36, [nl.attr_u8(48, 1)?, nl.attr(A_REG_ALPHA2, b"DE\0")?], GROUP_REGULATORY)?,
    event_line(46, [nl.attr_u32(A_IFINDEX, 3)?, nl.attr_u32(A_WIPHY, 0)?, nl.attr(A_MAC, mac(10)?)?, nl.attr_u16(72, 0)?], GROUP_MLME)?,
    event_line(48, [nl.attr_u32(A_IFINDEX, 3)?, nl.attr_u16(54, 3)?, nl.attr_flag(71)?], GROUP_MLME)?,
    event_line(41, [nl.attr_u32(A_IFINDEX, 3)?], GROUP_CONFIG)?,
    event_line(7, [nl.attr_u32(A_IFINDEX, 5)?, nl.attr_u32(A_IFTYPE, 6)?, nl.attr_u32(A_WIPHY, 1)?], GROUP_CONFIG)?,
    event_line(19, [nl.attr_u32(A_IFINDEX, 3)?, nl.attr(A_MAC, mac(2)?)?], GROUP_MLME)?,
    event_line(40, [nl.attr_u32(A_IFINDEX, 3)?], 99)?,
  ]
  let output = run_iw(ctx, "event", lines, ["event"])?
  let expected = [
    "wlan0 (phy #0): scan started",
    "wlan0 (phy #0): scan finished: 2412 2437, \"\"",
    "regulatory domain change: set to DE by a user request",
    "wlan0 (phy #0): connected to 02:00:00:00:00:0a",
    "wlan0: disconnected (by AP) reason: 3: Deauthenticated because sending station is leaving (or has left) the IBSS or ESS",
    "wlan0: unknown event 41 (michael_mic_failure)",
    "if5 (phy #1): new interface type monitor",
    "wlan0: new station 02:00:00:00:00:02",
  ]
  assert output.stdout == expected.join("\n") + "\n"
  # The recorded stream ends the way a receive timeout does.
  assert output.status == 245
  assert output.stderr == "command failed: Resource temporarily unavailable (-11)\n"

  let stamped = run_iw(ctx, "event-t", lines, ["event", "-t"])?
  assert rx"^\d+\.\d{6}: wlan0 \(phy #0\): scan started$".matches(stamped.stdout.lines()[0])
  let calendar = run_iw(ctx, "event-T", lines, ["event", "-T"])?
  assert rx"^\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{6}\]: wlan0 \(phy #0\): scan started$".matches(calendar.stdout.lines()[0])
  let relative = run_iw(ctx, "event-r", lines, ["event", "-r"])?
  assert rx"^\+\d+\.\d{6}: wlan0 \(phy #0\): scan started$".matches(relative.stdout.lines()[0])
  let source = fp"{ctx.core_dir}/iw.xsh".read_text()?
  let bad = test.expect(ctx, source, status: 1, args: ["event", "-f"], env: {XSH_MODULE_PATH: ctx.core_dir})?
  assert bad.stdout.starts_with("Usage:\tiw [options] event [-t|-T|-r]\n\nMonitor events from the kernel.\n")
}

test test_missing_nl80211_family_and_unrecorded_requests_fail_loudly { |ctx|
  let absent: List[Record] = [{op: "genl_family", name: "nl80211", errno: 2}]
  let missing = run_iw(ctx, "no-family", absent, ["dev"])?
  assert missing.status == 1 and missing.stderr == "nl80211 not found.\n"

  # A request the fixture does not record never reaches a kernel.
  let unrecorded = run_iw(ctx, "unrecorded", [family_line()], ["dev"])?
  assert unrecorded.status == 1
  assert "no recorded response" in unrecorded.stderr
}
