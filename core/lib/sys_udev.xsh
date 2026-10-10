##! Typed udev device view over sysfs and the udev runtime database.
##!
##! Every function reads through a caller-owned `FsRoot` whose children are
##! `sys` and `run/udev`, so the live host (rooted at `/`) and a synthetic
##! fixture tree share one code path. `udevadm` selects the root with the
##! `XSH_UDEVADM_ROOT` environment variable (see `root_path`).
##!
##! A device is a sysfs directory that has a `uevent` file. Its properties are
##! the `uevent` lines, `DEVPATH`, `SUBSYSTEM` from the `subsystem` link, and
##! the udev database record (`run/udev/data/<id>`) when one exists: `E:` lines
##! are properties, `S:` lines device symlinks, `G:` tags, `L:` the link
##! priority, and `I:` the initialization time. The database file name is
##! `b<major>:<minor>` for block devices, `c<major>:<minor>` for other devices
##! with a number, `n<ifindex>` for network interfaces, and
##! `+<subsystem>:<sysname>` otherwise.
use sys_source as src

## Names one device property.
export type Property = {name: Str, value: Str}

## Describes one device: identity from sysfs, naming and tags from the database.
## `devnode` is the node name relative to `/dev`; `properties` is sorted by name
## and includes `DEVLINKS` and `TAGS` when the database supplies them.
export type Device = {
  devpath: Str,
  sysname: Str,
  subsystem: Str?,
  driver: Str?,
  devnode: Str?,
  major: Int?,
  minor: Int?,
  symlinks: List[Str],
  link_priority: Int,
  tags: List[Str],
  properties: List[Property],
}

## Names one readable sysfs attribute of a device.
export type Attribute = {name: Str, value: Str}

## The directory below which `sys` and `run/udev` are read; `/` unless
## `XSH_UDEVADM_ROOT` names a synthetic tree.
export proc root_path() [env] -> Path {
  let named = env.get_or("XSH_UDEVADM_ROOT", "/") ?? "/"
  fp"{named}"
}

## Looks up a property value by name.
export pure property_value(device: Device, name: Str) -> Str? {
  for property in device.properties {
    return property.value when property.name == name
  }

  null
}

pure upsert(properties: List[Property], name: Str, value: Str) -> List[Property] {
  var updated: List[Property] = []
  var replaced = false
  for property in properties {
    if property.name == name {
      updated += [{name: name, value: value}]
      replaced = true
    } else {
      updated += [property]
    }
  }

  if ! replaced {
    updated += [{name: name, value: value}]
  }

  updated
}

pure sorted_unique(values: List[Str]) -> List[Str] {
  var seen: Set[Str] = set.empty()
  var kept: List[Str] = []
  for value in values |> sort-by { |item| item } {
    if value not in seen {
      seen = seen.add(value)
      kept += [value]
    }
  }

  kept
}

pure path_parts(text: Str) -> List[Str] {
  var parts: List[Str] = []
  for part in text.split("/") {
    if part != "" {
      parts += [part]
    }
  }

  parts
}

## Whether a command-line argument names a path below the sysfs mount point,
## such as `/sys/class/net/lo`.
export pure is_sysfs_path(text: Str) -> Bool {
  text.starts_with("/sys/") or text == "/sys"
}

## Drops the sysfs mount point from an argument, so `/sys/class/net/lo` and
## `/class/net/lo` both name the same entry.
export pure sysfs_relative(text: Str) -> Str {
  if is_sysfs_path(text) { text.byte_slice(4) } else { text }
}

# A directory has a `uevent` file only when it is a device. Entries that are
# not directories (such as `class/net/bonding_masters`) report ENOTDIR, which
# is the same answer as absence here.
proc has_uevent(root: FsRoot, directory: Str) [fs, error] -> Bool {
  root.stat(fp"{directory}/uevent") is Ok(_)
}

## Resolves symbolic links below the root as a kernel path walk would, never
## leaving the root: an absolute link target restarts at the root. A missing
## component, or a link chain longer than 40, resolves to null.
export proc resolve(root: FsRoot, relative: Str) [fs, error] -> Result[Str?, Error] {
  var pending = path_parts(relative)
  var resolved: List[Str] = []
  var links = 0
  while ! pending.is_empty() {
    let part = pending[0]
    pending = pending |> drop(1)
    continue when part == "."

    if part == ".." {
      if ! resolved.is_empty() {
        resolved = resolved |> take(resolved.len() - 1)
      }

      continue
    }

    var candidate = resolved
    candidate += [part]
    let link = root.readlink_result(fp"{candidate.join("/")}")?
    if link.state == "observed" {
      links += 1
      return Ok(null) when links > 40

      let target = (link.target ?? p"").display()
      if target.starts_with("/") {
        resolved = []
      }

      pending = path_parts(target) + pending
    } else if link.state == "absent" {
      return Ok(null)
    } else {
      resolved = candidate
    }
  }

  Ok(resolved.join("/"))
}

## Resolves a sysfs-relative path such as `/class/net/lo` to the canonical
## device path below `sys` (`/devices/virtual/net/lo`), or null when the path
## does not exist.
export proc canonical_devpath(root: FsRoot, sysfs_path: Str) [fs, error] -> Result[Str?, Error] {
  let resolved = resolve(root, f"sys{sysfs_path}")?
  return Ok(null) when resolved == null or (! resolved.starts_with("sys/") and resolved != "sys")

  Ok(resolved.byte_slice(3))
}

pure database_name(
  subsystem: Str?,
  sysname: Str,
  major: Int?,
  minor: Int?,
  ifindex: Int?,
) -> Str? {
  if major != null and minor != null {
    return f"{if subsystem == "block" { "b" } else { "c" }}{major}:{minor}"
  }

  return f"n{ifindex}" when ifindex != null
  return f"+{subsystem}:{sysname}" when subsystem != null

  null
}

## Reads the device at a canonical device path, or null when the directory has
## no `uevent` file and is therefore not a device.
export proc load(root: FsRoot, devpath: Str) [fs, error] -> Result[Device?, Error] {
  let directory = f"sys{devpath}"
  return Ok(null) when ! has_uevent(root, directory)

  let sysname = fp"{devpath}".basename()
  let subsystem_link = root.readlink_result(fp"{directory}/subsystem")?
  let subsystem: Str? = if subsystem_link.state == "observed" { (subsystem_link.target ?? p"").basename() } else { null }
  let driver_link = root.readlink_result(fp"{directory}/driver")?
  let driver: Str? = if driver_link.state == "observed" { (driver_link.target ?? p"").basename() } else { null }

  var properties: List[Property] = [{name: "DEVPATH", value: devpath}]
  if subsystem != null {
    properties = upsert(properties, "SUBSYSTEM", subsystem)
  }

  var devnode: Str? = null
  var major: Int? = null
  var minor: Int? = null
  var ifindex: Int? = null
  for line in root.read_text(fp"{directory}/uevent")?.lines() {
    let pair = line.split("=", maxsplit: 1)
    continue when pair.len() != 2 or pair[0] == ""

    let value = pair[1]
    if pair[0] == "MAJOR" {
      major = src.parse_integer(value)
    } else if pair[0] == "MINOR" {
      minor = src.parse_integer(value)
    } else if pair[0] == "IFINDEX" {
      ifindex = src.parse_integer(value)
    }

    if pair[0] == "DEVNAME" {
      let name = if value.starts_with("/dev/") { value.byte_slice(5) } else { value }
      devnode = name
      properties = upsert(properties, "DEVNAME", f"/dev/{name}")
    } else {
      properties = upsert(properties, pair[0], value)
    }
  }

  var symlinks: List[Str] = []
  var tags: List[Str] = []
  var link_priority = 0
  var initialized: Str? = null
  let record = database_name(subsystem, sysname, major, minor, ifindex)
  if record != null and root.exists(fp"run/udev/data/{record}")? {
    for line in root.read_text(fp"run/udev/data/{record}")?.lines() {
      let text = line.byte_slice(2)
      if line.starts_with("S:") {
        symlinks += [text]
      } else if line.starts_with("G:") {
        tags += [text]
      } else if line.starts_with("L:") {
        link_priority = src.parse_integer(text) ?? 0
      } else if line.starts_with("I:") {
        initialized = text
      } else if line.starts_with("E:") {
        let pair = text.split("=", maxsplit: 1)
        if pair.len() == 2 and pair[0] != "" {
          properties = upsert(properties, pair[0], pair[1])
        }
      }
    }
  }

  let links = sorted_unique(symlinks)
  let tag_names = sorted_unique(tags)
  if ! links.is_empty() {
    properties = upsert(properties, "DEVLINKS", links |> map { |link| f"/dev/{link}" }.join(" "))
  }

  if ! tag_names.is_empty() {
    properties = upsert(properties, "TAGS", f":{tag_names.join(":")}:")
  }

  if initialized != null {
    properties = upsert(properties, "USEC_INITIALIZED", initialized)
  }

  Ok({
    devpath: devpath,
    sysname: sysname,
    subsystem: subsystem,
    driver: driver,
    devnode: devnode,
    major: major,
    minor: minor,
    symlinks: links,
    link_priority: link_priority,
    tags: tag_names,
    properties: properties |> sort-by .name,
  })
}

## The nearest ancestor directory that is itself a device, or null. The walk
## stops below `/devices` and never considers `/devices` itself.
export proc parent_devpath(root: FsRoot, devpath: Str) [fs, error] -> Result[Str?, Error] {
  var parts = path_parts(devpath)
  while parts.len() > 2 {
    parts = parts |> take(parts.len() - 1)
    let candidate = f"/{parts.join("/")}"
    return Ok(candidate) when has_uevent(root, f"sys{candidate}")
  }

  Ok(null)
}

## Lists the attributes `udevadm info --attribute-walk` prints: regular files
## the owner can read, other than `uevent` and `dev`, whose value (one trailing
## run of newlines removed) is at most 4095 printable ASCII characters. Names
## are sorted so the order does not depend on the directory.
export proc attributes(root: FsRoot, devpath: Str) [fs, error] -> Result[List[Attribute], Error] {
  let printable = regex.compile("^[ -~]*$")?
  let listing = root.children(fp"sys{devpath}", max_entries: 8192)?
  var names: List[Str] = []
  for entry in listing.children {
    names += [entry.name()]
  }

  var found: List[Attribute] = []
  for name in names |> sort-by { |item| item } {
    continue when name == "uevent" or name == "dev"

    let file = fp"sys{devpath}/{name}"
    let info = root.stat(file, follow_symlinks: false)
    continue when info is Err(_)

    let facts = info?
    continue when facts.kind != "file" or facts.mode / 256 % 2 == 0

    let raw = root.read_result(file, max_bytes: 4096)?
    continue when raw.state != "observed" or raw.truncated or raw.data == null

    let data = raw.data ?? b""
    continue when data.len() >= 4096

    let decoded = data.utf8()
    continue when decoded is Err(_)

    var value = decoded?
    while value.ends_with("\n") {
      value = value.byte_slice(0, value.byte_len() - 1)
    }

    continue when ! printable.matches(value)

    found += [{name: name, value: value}]
  }

  Ok(found)
}

## Lists the canonical device paths of every device that sysfs links from
## `bus/*/devices` or `class/*`, sorted, which is the set a trigger scans.
## Class and bus entries are links whose target is a plain relative path, so
## the target is joined lexically instead of walked component by component.
export proc enumerate(root: FsRoot) [fs, error] -> Result[List[Str], Error] {
  var seen: Set[Str] = set.empty()
  var devpaths: List[Str] = []
  for area in ["bus", "class"] {
    let kinds = root.children(fp"sys/{area}", max_entries: 4096)?
    for kind in kinds.children {
      let directory = if area == "bus" { fp"{kind}/devices" } else { kind }
      let entries = root.children(directory, max_entries: 65536)?
      for entry in entries.children {
        let link = root.readlink_result(entry)?
        var real = entry.display()
        if link.state == "observed" {
          real = fp"{directory}/{(link.target ?? p"").display()}".normalize().display()
        }

        continue when ! real.starts_with("sys/")

        let devpath = real.byte_slice(3)
        continue when devpath in seen

        if has_uevent(root, real) {
          seen = seen.add(devpath)
          devpaths += [devpath]
        }
      }
    }
  }

  Ok(devpaths |> sort-by { |item| item })
}

## One bus or driver directory that a subsystem-type trigger addresses.
export type Subsystem = {syspath: Str, kind: Str}

## Lists `bus/*` directories (kind `subsystem`) and their `drivers/*`
## directories (kind `drivers`) as sorted sysfs-relative paths.
export proc subsystems(root: FsRoot) [fs, error] -> Result[List[Subsystem], Error] {
  var found: List[Subsystem] = []
  let buses = root.children(p"sys/bus", max_entries: 4096)?
  for bus in buses.children {
    found += [{syspath: f"/{bus.display().byte_slice(4)}", kind: "subsystem"}]
    let drivers = root.children(fp"{bus}/drivers", max_entries: 65536)?
    for driver in drivers.children {
      found += [{syspath: f"/{driver.display().byte_slice(4)}", kind: "drivers"}]
    }
  }

  Ok(found |> sort-by .syspath)
}

## Finds the device whose node is `name` (relative to `/dev`) or whose database
## record lists `name` as a symlink, scanning the block and character device
## number links. Returns the canonical device path, or null.
export proc find_by_name(root: FsRoot, name: Str) [fs, error] -> Result[Str?, Error] {
  return Ok(null) when name == ""

  for kind in ["block", "char"] {
    let numbers = root.children(fp"sys/dev/{kind}", max_entries: 65536)?
    for entry in numbers.children |> sort-by { |item| item.display() } {
      let uevent = root.read_result(fp"{entry}/uevent", max_bytes: 65536)?
      var matched = false
      if uevent.state == "observed" and uevent.data != null {
        for line in (uevent.data ?? b"").utf8() ?? "" |> lines() {
          if line == f"DEVNAME={name}" or line == f"DEVNAME=/dev/{name}" {
            matched = true
          }
        }
      }

      if ! matched {
        let record = root.read_result(fp"run/udev/data/{kind_prefix(kind)}{entry.name()}", max_bytes: 1048576)?
        if record.state == "observed" and record.data != null {
          for line in (record.data ?? b"").utf8() ?? "" |> lines() {
            if line == f"S:{name}" {
              matched = true
            }
          }
        }
      }

      if matched {
        return canonical_devpath(root, f"/dev/{kind}/{entry.name()}")
      }
    }
  }

  Ok(null)
}

pure kind_prefix(kind: Str) -> Str {
  if kind == "block" { "b" } else { "c" }
}

## Whether the udev daemon has events queued: the `run/udev/queue` file exists.
export proc queue_pending(root: FsRoot) [fs, error] -> Result[Bool, Error] {
  root.exists(p"run/udev/queue")
}

## Whether a daemon control socket exists, which is how a daemon is told to be running.
export proc daemon_present(root: FsRoot) [fs, error] -> Result[Bool, Error] {
  root.exists(p"run/udev/control")
}
