use core.lib.sys_mount as mounts
use core.lib.system_report as report

proc write_mountinfo(root: FsRoot, text: Str) [fs, error] {
  root.mkdir(p"proc/self", parents: true)
  root.write(p"proc/self/mountinfo", text)
}

test test_sys_mount_collect_keeps_typed_identity_and_raw_option_text {
  let root = fs.tempdir()?
  defer root.close()?
  write_mountinfo(
    root,
    """12 1 8:1 / /mnt/a\\040b rw,relatime shared:3 master:4 - ext4 /dev/sda1 rw,errors=remount-ro,password=private-secret
13 12 259:7 /sub /mnt/alias rw - btrfs /dev/nvme0n1p7 rw
""",
  )

  let table = mounts.collect(root)
  assert table.source_state == report.Observed
  assert table.issues == []
  assert table.mounts.len() == 2
  let first = table.mounts[0]
  assert first.mount_id == 12 and first.parent_id == 1
  assert first.major == 8 and first.minor == 1
  assert first.target == "/mnt/a b", "mountinfo octal escapes decode to the real path"
  assert first.root == "/"
  assert first.filesystem == "ext4"
  assert first.source == "/dev/sda1"
  assert first.mount_options == ["rw", "relatime"]
  assert first.optional_fields == ["shared:3", "master:4"]
  assert first.super_options == ["rw", "errors=remount-ro", "password=private-secret"], "redaction is a renderer policy"
  assert first.usage_state == report.NotRequested
  assert first.usage_total_bytes == null
  assert table.mounts[1].parent_id == 12
  assert table.mounts[1].root == "/sub"
  assert table.mounts[1].optional_fields == []
}

test test_sys_mount_collect_keeps_valid_rows_beside_malformed_and_out_of_range_rows {
  let root = fs.tempdir()?
  defer root.close()?
  write_mountinfo(
    root,
    """22 1 8:1 / /good rw - ext4 /dev/sda1 rw
not enough fields
23 1 x:y / /bad rw - ext4 /dev/sda2 rw
99999999999999999999 1 8:3 / /big rw - ext4 /dev/sda3 rw
24 1 8:4 / /also-good rw - xfs /dev/sda4 rw
""",
  )

  let table = mounts.collect(root)
  assert [entry.mount_id for entry in table.mounts] == [22, 24]
  let kinds = [item.error_kind ?? "" for item in table.issues]
  assert kinds == ["invalid_mountinfo_row", "invalid_mount_identity", "invalid_mount_identity"]
  assert [item.field for item in table.issues] == ["mounts.line.1", "mounts.line.2", "mounts.line.3"]
}

test test_sys_mount_collect_reports_an_absent_source_without_inventing_rows {
  let root = fs.tempdir()?
  defer root.close()?
  let table = mounts.collect(root)
  assert table.source_state == report.Absent
  assert table.mounts == []
  assert table.issues.len() == 1
  assert table.issues[0].field == "mounts"
  assert table.issues[0].state == report.Absent
}

test test_sys_mount_usage_is_opt_in_and_skips_shadowed_or_remote_mounts {
  let root = fs.tempdir()?
  defer root.close()?
  write_mountinfo(
    root,
    """1 0 8:1 / / rw - ext4 /dev/sda1 rw
2 1 0:2 / /mnt/remote rw - cifs //server/share rw
3 1 8:2 / /dup rw - ext4 /dev/sda2 rw
4 1 8:3 / /dup rw - ext4 /dev/sda3 rw
""",
  )

  let plain = mounts.collect(root)
  assert [entry.usage_state for entry in plain.mounts] == [
    report.NotRequested,
    report.NotRequested,
    report.NotRequested,
    report.NotRequested,
  ]

  let measured = mounts.collect(root, include_usage: true)
  let root_mount = measured.mounts[0]
  assert root_mount.usage_state == report.Observed
  assert (root_mount.usage_total_bytes ?? 0) > 0
  assert measured.mounts[1].usage_state == report.NotRequested, "network filesystems are never queried"
  assert measured.mounts[2].usage_state == report.NotRequested, "a target mounted twice resolves ambiguously"
  assert measured.mounts[3].usage_state == report.NotRequested
}

test test_sys_mount_decode_field_handles_the_four_kernel_escapes {
  assert mounts.decode_mount_field("a\\040b\\011c\\012d\\134e") == "a b\tc\nd\\e"
  assert mounts.decode_mount_field("plain") == "plain"
}
