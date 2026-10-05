use core.lib.sys_source as src
use core.lib.system_report as report

test test_sys_source_read_withholds_values_it_cannot_prove_complete {
  let root = fs.tempdir()?
  defer root.close()?
  root.write(p"text", "  value \n")
  root.write(p"binary", b"\xff\xfe")
  root.write(p"long", "0123456789")

  let trimmed = src.read_source_text(root, p"text")
  assert trimmed.observation.state == report.Observed and trimmed.observation.value == "value"
  let kept = src.read_source_text(root, p"text", preserve_whitespace: true)
  assert kept.observation.value == "  value \n"
  let binary = src.read_source_text(root, p"binary")
  assert binary.observation.state == report.Malformed and binary.observation.value == null
  assert binary.observation.raw_bytes_base64 == "//4="
  let truncated = src.read_source_text(root, p"long", max_bytes: 4)
  assert truncated.observation.state == report.Truncated, "a prefix never becomes a value"
  assert truncated.observation.value == null
  let missing = src.read_source_text(root, p"missing")
  assert missing.observation.state == report.Absent
  assert src.observed_text(missing) == null
  assert src.observed_text(trimmed) == "value"
}

test test_sys_source_numbers_reject_prefixes_signs_and_json_unsafe_values {
  let root = fs.tempdir()?
  defer root.close()?
  for item in [
    {file: "ok", value: "42"},
    {file: "negative", value: "-7"},
    {file: "hex", value: "0x10"},
    {file: "plus", value: "+1"},
    {file: "huge", value: "9007199254740992"},
  ] {
    root.write(fp"{item.file}", f"{item.value}\n")
  }

  assert src.bounded_number(src.read_source_text(root, p"ok"), true).value == 42
  assert src.bounded_number(src.read_source_text(root, p"negative"), false).value == -7
  assert src.bounded_number(src.read_source_text(root, p"negative"), true).error_kind == "negative_integer"
  assert src.bounded_number(src.read_source_text(root, p"hex"), true).state == report.Malformed
  assert src.bounded_number(src.read_source_text(root, p"plus"), true).state == report.Malformed
  assert src.bounded_number(src.read_source_text(root, p"huge"), true).state == report.RangeFailure
  let absent = src.bounded_number(src.read_source_text(root, p"missing"), true)
  assert absent.value == null and absent.state == null, "absence is not an error"
  assert src.parse_integer("12") == 12 and src.parse_integer("1 2") == null and src.parse_integer(null) == null
  assert src.parse_bool01("1") == true and src.parse_bool01("0") == false and src.parse_bool01("2") == null
  assert src.parse_words(" a  b ") == ["a", "b"] and src.split_csv("") == [] and src.split_csv("a,b") == ["a", "b"]
}

test test_sys_source_links_separate_unbound_failed_and_present_states {
  let root = fs.tempdir()?
  defer root.close()?
  root.mkdir(p"dev/real/driver_target", parents: true)
  root.symlink(p"driver_target", p"dev/real/driver")
  root.mkdir(p"class/entry", parents: true)
  root.symlink(../dev/real, p"class/entry/device")

  assert src.driver_name(root, p"dev/real/driver").observation.value == "driver_target"
  assert src.driver_name(root, p"dev/real/unbound").observation.state == report.Absent
  let parent = src.class_parent_target(root, p"class/entry")
  assert parent.state == report.Observed
  assert parent.target?.display() == "../dev/real"
  let missing = src.class_entry_target(root, p"class/none")
  assert missing.state == report.Disappeared
  let directory = src.class_entry_target(root, p"dev/real")
  assert directory.state == report.Observed and directory.target == null, "rooted fixtures may use directories"
}

test test_sys_source_issues_carry_a_section_only_when_attached {
  let issue = src.issue("devices.sda.size", report.Malformed, "invalid_integer", null)
  assert issue.detail.state == report.Malformed
  let detailed = src.issue_with_detail("mounts.1.usage", report.Malformed, "invalid_mount_target", null, "not absolute")
  assert detailed.detail.value == "not absolute"
  let tagged = src.with_section("storage", [issue, detailed])
  assert [item.section for item in tagged] == ["storage", "storage"]
  assert tagged[0].field == "devices.sda.size" and tagged[1].error_kind == "invalid_mount_target"
  let issues = src.append_text_issue([], "x", src.read_source_text(fs.tempdir()?, p"gone"))
  assert issues == [], "an absent source is not an issue"
}
