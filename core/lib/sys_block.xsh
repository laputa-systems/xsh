##! Typed block-device inventory read from `sys/class/block` in one rooted view.
##!
##! `collect` returns one record per class entry with its identity (`name`,
##! `major:minor`), kind (`disk`, `partition`, `virtual`, `unknown`), size and
##! sector geometry, queue and I/O counters, the PCI address of its controller,
##! and layered relationships: the containing disk of a partition and the
##! holder and slave devices of stacked devices, both as names and as indexes
##! into `devices`. A malformed device never discards its neighbors. Model and
##! firmware text stays raw; naming policy and redaction belong to renderers.
use sys_pci
use sys_source as src
use system_report as report

## Keeps the active block scheduler with every scheduler offered by the device.
export type BlockScheduler = {active: Str, available: List[Str]}

## Describes one block device; indexes refer to the `devices` list of the same inventory.
export type BlockDevice = {
  name: Str,
  major: Int?,
  minor: Int?,
  kind: Str,
  size_bytes: Int?,
  logical_sector_bytes: Int?,
  physical_sector_bytes: Int?,
  removable: Bool?,
  rotational: Bool?,
  read_only: Bool?,
  model: report.TextObservation,
  firmware: report.TextObservation,
  parent_name: Str?,
  parent_pci_address: Str?,
  holder_names: List[Str],
  slave_names: List[Str],
  parent_device_index: Int?,
  holder_indices: List[Int],
  slave_indices: List[Int],
  active_scheduler: Str?,
  available_schedulers: List[Str],
  read_ahead_kb: Int?,
  discard_granularity_bytes: Int?,
  discard_max_bytes: Int?,
  io_counters: List[report.MemoryCounter],
}

## Retains block devices, the class listing state, and field-level issues.
export type BlockInventory = {
  listing_state: Str,
  enumeration_succeeded: Bool,
  devices: List[BlockDevice],
  issues: List[src.Issue],
}

## Requires exactly one bracketed active scheduler in one complete sysfs row.
export pure parse_scheduler(value: Str) -> BlockScheduler? {
  return null when value.trim() == "" or value.lines().len() != 1

  let choices = value.replace("\t", " ").split(" ") |> where .trim() != ""
  var available: List[Str] = []
  var active: Str? = null
  for choice in choices {
    var name = choice
    if choice.starts_with("[") and choice.ends_with("]") and choice.count_chars() >= 3 {
      guard active == null else {
        return null
      }

      name = choice.split("")
        |> drop(1)
        |> take(choice.count_chars() - 2).join("")
      active = name
    }

    return null when name == "" or "[" in name or "]" in name or name in available

    available += [name]
  }

  return null when active == null

  {active: active, available: available}
}

pure block_name_index(indices: Map[Int], name: Str?) -> Int? {
  return null when name == null or name not in indices

  indices.get(name) ?? 0
}

## Indexes devices by `major:minor`, keeping the first enumerated identity.
export pure index_by_number(devices: List[BlockDevice]) -> Map[Int] {
  var indices: Map[Int] = {}
  for index in range(devices.len()) {
    let device = devices[index]
    if device.major != null and device.minor != null {
      let key = f"{device.major}:{device.minor}"
      if key not in indices {
        indices = indices.set(key, index)
      }
    }
  }

  indices
}

## Looks up a device index by device number in a map built by `index_by_number`.
export pure index_of_number(indices: Map[Int], major: Int, minor: Int) -> Int? {
  let key = f"{major}:{minor}"
  return null when key not in indices

  indices.get(key) ?? 0
}

## Collects every class block entry with bounded reads and typed relationships.
export proc collect(root: FsRoot) [fs, error] -> BlockInventory {
  let listing = root.children(p"sys/class/block", max_entries: 4096)?
  var issues: List[src.Issue] = []
  var candidates: List[BlockDevice] = []
  var listed_names = set.empty()
  for device_path in listing.children {
    listed_names = set.add(listed_names, device_path.name())
  }

  if listing.state != "complete" {
    issues += [src.issue("devices", src.source_state(listing.state, false), listing.error_kind, listing.errno)]
  }

  for device_path in listing.children {
    let name = device_path.name()
    let dev = src.read_source_text(root, fp"{device_path}/dev", max_bytes: 4096)
    let dev_text = src.observed_text(dev)
    let dev_parts = (dev_text ?? "").split(":")
    var major: Int? = null
    var minor: Int? = null
    if dev_text == null {
      issues += [src.issue(f"devices.{name}.major_minor", dev.observation.state, dev.error_kind, dev.errno)]
    } else if dev_parts.len() != 2 {
      issues += [src.issue(f"devices.{name}.major_minor", report.Malformed, "invalid_device_number", null)]
    } else {
      let parsed_major = src.parse_integer(dev_parts[0]) ?? -1
      let parsed_minor = src.parse_integer(dev_parts[1]) ?? -1
      if parsed_major < 0 or parsed_minor < 0 {
        let out_of_range = (parsed_major < 0 and src.decimal_digits(dev_parts[0])) or (parsed_minor < 0 and src.decimal_digits(
          dev_parts[1],
        ))
        let state = if out_of_range { report.RangeFailure } else { report.Malformed }
        let error_kind = if out_of_range { "device_number_out_of_range" } else { "invalid_device_number" }
        issues += [src.issue(f"devices.{name}.major_minor", state, error_kind, null)]
      } else if parsed_major > 9007199254740991 or parsed_minor > 9007199254740991 {
        issues += [src.issue(f"devices.{name}.major_minor", report.RangeFailure, "device_number_out_of_range", null)]
      } else {
        major = parsed_major
        minor = parsed_minor
      }
    }

    let size = src.read_source_text(root, fp"{device_path}/size", max_bytes: 4096)
    let logical = src.read_source_text(root, fp"{device_path}/queue/logical_block_size", max_bytes: 4096)
    let physical = src.read_source_text(root, fp"{device_path}/queue/physical_block_size", max_bytes: 4096)
    let removable = src.read_source_text(root, fp"{device_path}/removable", max_bytes: 4096)
    let rotational = src.read_source_text(root, fp"{device_path}/queue/rotational", max_bytes: 4096)
    let read_only = src.read_source_text(root, fp"{device_path}/ro", max_bytes: 4096)
    let model = src.read_source_text(root, fp"{device_path}/device/model", max_bytes: 4096)
    let firmware = src.read_source_text(root, fp"{device_path}/device/firmware_rev", max_bytes: 4096)
    let fallback_firmware = if firmware.observation.state == report.Absent {
      src.read_source_text(root, fp"{device_path}/device/rev", max_bytes: 4096)
    } else {
      firmware
    }
    for field_source in [
      {
        field: "model",
        source: model,
      },
      {
        field: "firmware",
        source: fallback_firmware,
      },
    ] {
      let observed = field_source.source
      if observed.observation.state != report.Observed and observed.observation.state != report.Absent {
        issues += [
          src.issue(
            f"devices.{name}.{field_source.field}",
            observed.observation.state,
            observed.error_kind,
            observed.errno,
          ),
        ]
      }
    }

    let scheduler = src.read_source_text(root, fp"{device_path}/queue/scheduler", max_bytes: 4096)
    var active_scheduler: Str? = null
    var available_schedulers: List[Str] = []
    if scheduler.observation.state != report.Observed and scheduler.observation.state != report.Absent {
      issues += [
        src.issue(f"devices.{name}.scheduler", scheduler.observation.state, scheduler.error_kind, scheduler.errno),
      ]
    }

    if scheduler.observation.state == report.Observed {
      let parsed_scheduler = parse_scheduler(src.observed_text(scheduler) ?? "")
      if parsed_scheduler == null {
        issues += [src.issue(f"devices.{name}.scheduler", report.Malformed, "invalid_scheduler_selection", null)]
      } else {
        active_scheduler = parsed_scheduler.active
        available_schedulers = parsed_scheduler.available
      }
    }

    let read_ahead = src.read_source_text(root, fp"{device_path}/queue/read_ahead_kb", max_bytes: 4096)
    let discard_granularity = src.read_source_text(root, fp"{device_path}/queue/discard_granularity", max_bytes: 4096)
    let discard_max = src.read_source_text(root, fp"{device_path}/queue/discard_max_bytes", max_bytes: 4096)
    let logical_number = src.bounded_number(logical, true)
    let physical_number = src.bounded_number(physical, true)
    let removable_number = src.bounded_number(removable, true)
    let rotational_number = src.bounded_number(rotational, true)
    let read_only_number = src.bounded_number(read_only, true)
    let read_ahead_number = src.bounded_number(read_ahead, true)
    let discard_granularity_number = src.bounded_number(discard_granularity, true)
    let discard_max_number = src.bounded_number(discard_max, true)
    for field_number in [
      {
        field: "logical_sector_bytes",
        number: logical_number,
        boolean: false,
      },
      {
        field: "physical_sector_bytes",
        number: physical_number,
        boolean: false,
      },
      {
        field: "removable",
        number: removable_number,
        boolean: true,
      },
      {
        field: "rotational",
        number: rotational_number,
        boolean: true,
      },
      {
        field: "read_only",
        number: read_only_number,
        boolean: true,
      },
      {
        field: "read_ahead_kb",
        number: read_ahead_number,
        boolean: false,
      },
      {
        field: "discard_granularity_bytes",
        number: discard_granularity_number,
        boolean: false,
      },
      {
        field: "discard_max_bytes",
        number: discard_max_number,
        boolean: false,
      },
    ] {
      let parsed = field_number.number
      let field = f"devices.{name}.{field_number.field}"
      if parsed.state != null {
        issues += [src.issue(field, parsed.state ?? report.Malformed, parsed.error_kind, parsed.errno)]
      } else if field_number.boolean and (parsed.value ?? -1) not in [0, 1] and parsed.value != null {
        issues += [src.issue(field, report.Malformed, "invalid_boolean", null)]
      }
    }

    let stats = src.read_source_text(root, fp"{device_path}/stat", max_bytes: 4096)
    if stats.observation.state != report.Observed and stats.observation.state != report.Absent {
      issues += [src.issue(f"devices.{name}.stat", stats.observation.state, stats.error_kind, stats.errno)]
    }

    let stats_values = src.observed_text(stats) |> src.parse_words(_)
    let stat_field_count_valid = stats_values.len() in [11, 15, 17] or stats_values.len() > 17
    if stats.observation.state == report.Observed and ! stat_field_count_valid {
      issues += [src.issue(f"devices.{name}.stat", report.Malformed, "invalid_io_counter_count", null)]
    }

    var io_counters: List[report.MemoryCounter] = []
    let counter_names = [
      "read_ios",
      "read_merges",
      "read_sectors",
      "read_ms",
      "write_ios",
      "write_merges",
      "write_sectors",
      "write_ms",
      "in_flight",
      "io_ms",
      "weighted_io_ms",
      "discard_ios",
      "discard_merges",
      "discard_sectors",
      "discard_ms",
      "flush_ios",
      "flush_ms",
    ]
    let counter_units = [
      "requests",
      "requests",
      "sectors",
      "milliseconds",
      "requests",
      "requests",
      "sectors",
      "milliseconds",
      "requests",
      "milliseconds",
      "milliseconds",
      "requests",
      "requests",
      "sectors",
      "milliseconds",
      "requests",
      "milliseconds",
    ]
    var counter_index = 0
    while stat_field_count_valid and counter_index < stats_values.len() and counter_index < counter_names.len() {
      let counter_value = src.parse_integer(stats_values[counter_index]) ?? -1
      if counter_value >= 0 and counter_value <= 9007199254740991 {
        io_counters += [{name: counter_names[counter_index], value: counter_value, unit: counter_units[counter_index]}]
      } else {
        let out_of_range = src.decimal_digits(stats_values[counter_index])
        let state = if out_of_range { report.RangeFailure } else { report.Malformed }
        let error_kind = if out_of_range { "io_counter_out_of_range" } else { "invalid_io_counter" }
        issues += [src.issue(f"devices.{name}.stat.{counter_names[counter_index]}", state, error_kind, null)]
      }

      counter_index += 1
    }

    if stats_values.len() > counter_names.len() {
      issues += [src.issue(f"devices.{name}.stat", report.Unsupported, "unknown_io_counter_fields", null)]
    }

    let size_number = src.bounded_number(size, true)
    let sectors = size_number.value ?? -1
    if size.observation.state == report.Absent {
      issues += [src.issue(f"devices.{name}.size", report.Absent, size.error_kind, size.errno)]
    } else if size_number.state != null {
      issues += [
        src.issue(
          f"devices.{name}.size",
          size_number.state ?? report.Malformed,
          size_number.error_kind,
          size_number.errno,
        ),
      ]
    } else if sectors > 17592186044415 {
      issues += [src.issue(f"devices.{name}.size", report.RangeFailure, "byte_count_overflow", null)]
    }

    var is_partition = false
    match root.exists(fp"{device_path}/partition") {
      Ok(present) => is_partition = present
      Err(is PermissionDenied) => issues += [
        src.issue(f"devices.{name}.partition", report.PermissionDenied, "permission_denied", null),
      ]
      Err(_) => issues += [src.issue(f"devices.{name}.partition", report.ReadFailure, "partition_probe_failed", null)]
    }

    let target_source = src.class_entry_target(root, device_path)
    let target_path = target_source.target
    if target_source.state != report.Observed {
      issues += [
        src.issue(f"devices.{name}.sysfs_target", target_source.state, target_source.error_kind, target_source.errno),
      ]
    }

    let target = target_path?.display() ?? ""
    let kind = if is_partition {
      "partition"
    } else if target_path == null {
      "unknown"
    } else if "/virtual/" in target {
      "virtual"
    } else {
      "disk"
    }
    var parent_pci_address: Str? = null
    if target_path != null {
      parent_pci_address = sys_pci.address_in_target(target_path)
    }

    let holders_listing = root.children(fp"{device_path}/holders", max_entries: 4096)?
    let slaves_listing = root.children(fp"{device_path}/slaves", max_entries: 4096)?
    if holders_listing.state != "complete" {
      issues += [
        src.issue(
          f"devices.{name}.holders",
          src.source_state(holders_listing.state, false),
          holders_listing.error_kind,
          holders_listing.errno,
        ),
      ]
    }

    # Partitions expose holders but have no slaves directory of their own.
    if slaves_listing.state != "complete" and ! (is_partition and slaves_listing.state == "absent") {
      issues += [
        src.issue(
          f"devices.{name}.slaves",
          src.source_state(slaves_listing.state, false),
          slaves_listing.error_kind,
          slaves_listing.errno,
        ),
      ]
    }

    var holders: List[Str] = []
    var slaves: List[Str] = []
    for holder in holders_listing.children {
      holders += [holder.name()]
    }

    for slave in slaves_listing.children {
      slaves += [slave.name()]
    }

    var parent_name: Str? = null
    let target_components = target.split("/")
    for component in target_components {
      if component != name and component != "block" and component in listed_names {
        parent_name = component
      }
    }

    var sector_bytes: Int? = null
    if sectors >= 0 and sectors <= 17592186044415 {
      sector_bytes = sectors * 512
    }

    candidates += [
      {
        name: name,
        major: major,
        minor: minor,
        kind: kind,
        size_bytes: sector_bytes,
        logical_sector_bytes: logical_number.value,
        physical_sector_bytes: physical_number.value,
        removable: if (removable_number.value ?? -1) in [
          0,
          1,
        ] {
          src.parse_bool01(src.observed_text(removable))
        } else {
          null
        },
        rotational: if (rotational_number.value ?? -1) in [
          0,
          1,
        ] {
          src.parse_bool01(src.observed_text(rotational))
        } else {
          null
        },
        read_only: if (read_only_number.value ?? -1) in [
          0,
          1,
        ] {
          src.parse_bool01(src.observed_text(read_only))
        } else {
          null
        },
        model: {
          ...model.observation,
          value: src.observed_text(model),
        },
        firmware: {
          ...fallback_firmware.observation,
          value: src.observed_text(fallback_firmware),
        },
        parent_name: parent_name,
        parent_pci_address: parent_pci_address,
        holder_names: holders,
        slave_names: slaves,
        parent_device_index: null,
        holder_indices: [],
        slave_indices: [],
        active_scheduler: active_scheduler,
        available_schedulers: available_schedulers,
        read_ahead_kb: read_ahead_number.value,
        discard_granularity_bytes: discard_granularity_number.value,
        discard_max_bytes: discard_max_number.value,
        io_counters: io_counters,
      },
    ]
  }

  # Preserve the first enumerated identity while linking layered devices.
  var block_indices_by_name: Map[Int] = {}
  for index in range(candidates.len()) {
    let name = candidates[index].name
    if name not in block_indices_by_name {
      block_indices_by_name = block_indices_by_name.set(name, index)
    }
  }

  var linked_devices: List[BlockDevice] = []
  for candidate in candidates {
    var holders: List[Int] = []
    var slaves: List[Int] = []
    for name in candidate.holder_names {
      let index = block_name_index(block_indices_by_name, name)
      if index != null {
        holders += [index]
      }
    }

    for name in candidate.slave_names {
      let index = block_name_index(block_indices_by_name, name)
      if index != null {
        slaves += [index]
      }
    }

    linked_devices += [
      {
        ...candidate,
        parent_device_index: block_name_index(block_indices_by_name, candidate.parent_name),
        holder_indices: holders,
        slave_indices: slaves,
      },
    ]
  }

  {
    listing_state: listing.state,
    enumeration_succeeded: listing.enumeration_succeeded,
    devices: linked_devices,
    issues: issues,
  }
}
