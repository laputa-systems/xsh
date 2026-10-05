##! Typed mount-table inventory read from `proc/self/mountinfo` in one rooted view.
##!
##! `collect` returns one record per valid mountinfo row with numeric mount and
##! parent IDs, the `major:minor` device number, decoded root, target, and source
##! text, mount and superblock options, optional propagation fields, and the
##! filesystem type. It keeps raw option and source text: credential and
##! host-path redaction is a renderer decision (`system-report` applies its own),
##! never a collector decision. Optional capacity comes from the same rooted view
##! and is queried only for mounts proven not to be shadowed or automounted.
use sys_source as src
use system_report as report

## Describes one mountinfo row, including its optional capacity observation.
export type MountEntry = {
  mount_id: Int,
  parent_id: Int,
  major: Int,
  minor: Int,
  root: Str,
  target: Str,
  mount_options: List[Str],
  optional_fields: List[Str],
  filesystem: Str,
  source: Str,
  super_options: List[Str],
  usage_state: report.ObservationState,
  usage_total_bytes: Int?,
  usage_used_bytes: Int?,
  usage_available_bytes: Int?,
}

## Retains valid mount entries, the mountinfo source state, and field issues.
export type MountTable = {
  source_state: report.ObservationState,
  mounts: List[MountEntry],
  issues: List[src.Issue],
}

## Decodes the octal escapes mountinfo applies to whitespace and backslashes.
export pure decode_mount_field(value: Str) -> Str {
  value.replace("\\040", " ")
    .replace("\\011", "\t")
    .replace("\\012", "\n")
    .replace("\\134", "\\")
}

pure mount_usage_eligible(filesystem: Str) -> Bool {
  filesystem in [
    "btrfs",
    "exfat",
    "ext2",
    "ext3",
    "ext4",
    "f2fs",
    "ntfs",
    "ntfs3",
    "overlay",
    "tmpfs",
    "vfat",
    "xfs",
  ]
}

type MountUsageRow = {mount_id: Int, parent_id: Int, target: Str, filesystem: Str}

type MountUsageIndex = {rows: List[MountUsageRow], by_id: Map[Int, Int], target_counts: Map[Int], valid_graph: Bool}

pure mount_usage_index(mountinfo: Str) -> MountUsageIndex {
  var rows: List[MountUsageRow] = []
  var by_id: Map[Int, Int] = {}
  var target_counts: Map[Int] = {}
  var valid_graph = true
  for line in mountinfo.lines() {
    let fields = src.parse_words(line)
    var separator = 0
    while separator < fields.len() and fields[separator] != "-" {
      separator += 1
    }

    if separator < 6 or separator + 3 >= fields.len() {
      valid_graph = false
      continue
    }

    let mount_id = src.parse_integer(fields[0]) ?? -1
    let parent_id = src.parse_integer(fields[1]) ?? -1
    if mount_id < 0 or parent_id < 0 or mount_id > 9007199254740991 or parent_id > 9007199254740991 {
      valid_graph = false
      continue
    }

    let numbers = fields[2].split(":")
    let major = src.parse_integer(numbers.get(0) ?? "") ?? -1
    let minor = src.parse_integer(numbers.get(1) ?? "") ?? -1
    let target = decode_mount_field(fields[4])
    if major < 0 or minor < 0 or major > 9007199254740991 or minor > 9007199254740991 or ! target.starts_with("/") {
      valid_graph = false
      continue
    }

    if mount_id in by_id {
      valid_graph = false
    }

    by_id = by_id.set(mount_id, if mount_id in by_id { -1 } else { rows.len() })
    target_counts = target_counts.set(target, (target_counts.get(target) ?? 0) + 1)
    rows += [
      {
        mount_id: mount_id,
        parent_id: parent_id,
        target: target,
        filesystem: fields[separator + 1],
      },
    ]
  }

  {rows: rows, by_id: by_id, target_counts: target_counts, valid_graph: valid_graph}
}

# A target can resolve through a different mount when its own or an ancestor path is shadowed.
pure mount_usage_safe(index: MountUsageIndex, mount_id: Int) -> Bool {
  guard index.valid_graph else {
    return false
  }

  var current_id = mount_id
  var seen = set.empty()
  var depth = 0
  while depth < index.rows.len() {
    let key = f"{current_id}"
    return false when key in seen

    seen = set.add(seen, key)
    let row_index = index.by_id.get(current_id) ?? -1
    return false when row_index < 0

    let entry = index.rows[row_index]
    if ! mount_usage_eligible(entry.filesystem) or (index.target_counts.get(entry.target) ?? 0) != 1 {
      return false
    }

    return true when entry.parent_id == 0

    return false when entry.parent_id == current_id

    return true when entry.parent_id not in index.by_id

    current_id = entry.parent_id
    depth += 1
  }

  false
}

## Reads the mount table; `include_usage` also queries capacity for eligible local mounts.
export proc collect(root: FsRoot, include_usage: Bool = false) [fs, error] -> MountTable {
  var issues: List[src.Issue] = []
  let mount_source = src.read_source_text(root, p"proc/self/mountinfo", max_bytes: 4194304)
  var mounts: List[MountEntry] = []
  if mount_source.observation.state != report.Observed or mount_source.observation.value == null {
    issues += [src.issue("mounts", mount_source.observation.state, mount_source.error_kind, mount_source.errno)]
  } else {
    let usage_index = mount_usage_index(mount_source.observation.value)
    for line_item in mount_source.observation.value.lines() |> enumerate() {
      let {index: line_index, value: line, ..} = line_item
      let fields = src.parse_words(line)
      var separator = 0
      while separator < fields.len() and fields[separator] != "-" {
        separator += 1
      }

      if separator < 6 or separator + 3 >= fields.len() {
        issues += [src.issue(f"mounts.line.{line_index}", report.Malformed, "invalid_mountinfo_row", null)]
        continue
      }

      let ids = src.parse_integer(fields[0]) ?? -1
      let parent_id = src.parse_integer(fields[1]) ?? -1
      let device_ids = fields[2].split(":")
      let major = src.parse_integer(device_ids.get(0) ?? "") ?? -1
      let minor = src.parse_integer(device_ids.get(1) ?? "") ?? -1
      if ids < 0 or parent_id < 0 or major < 0 or minor < 0 {
        issues += [src.issue(f"mounts.line.{line_index}", report.Malformed, "invalid_mount_identity", null)]
        continue
      }

      if ids > 9007199254740991 or parent_id > 9007199254740991 or major > 9007199254740991 or minor > 9007199254740991 {
        issues += [
          src.issue(f"mounts.line.{line_index}", report.RangeFailure, "mount_identity_out_of_json_range", null),
        ]
        continue
      }

      var optional_fields: List[Str] = []
      var index = 6
      while index < separator {
        optional_fields += [decode_mount_field(fields[index])]
        index += 1
      }

      let target = decode_mount_field(fields[4])
      let source = decode_mount_field(fields[separator + 2])
      var usage_state = report.NotRequested
      var usage_total_bytes: Int? = null
      var usage_used_bytes: Int? = null
      var usage_available_bytes: Int? = null
      if include_usage and mount_usage_safe(usage_index, ids) {
        if ! target.starts_with("/") {
          usage_state = report.Malformed
          issues += [
            src.issue_with_detail(
              f"mounts.{ids}.usage",
              usage_state,
              "invalid_mount_target",
              null,
              "The mount target was not absolute.",
            ),
          ]
        } else {
          var usage_path: Path? = null
          if target == "/" {
            usage_path = p"."
          } else {
            if let Ok(relative_path) = fp"{target}".strip_prefix(/) {
              usage_path = relative_path
            } else {
              usage_path = null
            }
          }

          if usage_path == null {
            usage_state = report.Malformed
            issues += [
              src.issue_with_detail(
                f"mounts.{ids}.usage",
                usage_state,
                "invalid_mount_target",
                null,
                "The mount target could not be made relative to the observation root.",
              ),
            ]
          } else {
            let usage = root.filesystem_stats(usage_path)?
            usage_state = match usage.state {
              "observed" => report.Observed,
              "absent" => report.Disappeared,
              "permission_denied" => report.PermissionDenied,
              "malformed" => report.Malformed,
              "range_failure" => report.RangeFailure,
              else => report.ReadFailure,
            }
            usage_total_bytes = usage.total_bytes
            usage_used_bytes = usage.used_bytes
            usage_available_bytes = usage.available_bytes
            if usage.state != "observed" {
              issues += [
                src.issue(
                  f"mounts.{ids}.usage",
                  usage_state,
                  usage.error_kind,
                  usage.errno,
                ),
              ]
            }
          }
        }
      }

      mounts += [
        {
          mount_id: ids,
          parent_id: parent_id,
          major: major,
          minor: minor,
          root: decode_mount_field(fields[3]),
          target: target,
          mount_options: src.split_csv(fields[5]),
          optional_fields: optional_fields,
          filesystem: fields[separator + 1],
          source: source,
          super_options: src.split_csv(fields[separator + 3]),
          usage_state: usage_state,
          usage_total_bytes: usage_total_bytes,
          usage_used_bytes: usage_used_bytes,
          usage_available_bytes: usage_available_bytes,
        },
      ]
    }
  }

  {source_state: mount_source.observation.state, mounts: mounts, issues: issues}
}
