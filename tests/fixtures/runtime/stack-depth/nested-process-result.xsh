type ArchiveOwner = {object: Path, dir: Path}
type Composite = {object: Path, members: List[Path]}
type Plan = {
  dirs: List[Path],
  objects: List[Path],
  lib_objects: List[Path],
  archive_owners: List[ArchiveOwner],
  composites: List[Composite],
  unsupported: List[Str],
}
type Scan = {dir: Path, plan: Plan, child_dirs: List[Path], entries: List[Path]}
type RawArchiveOwner = {object: Str, dir: Str}
type RawComposite = {object: Str, members: List[Str]}
type RawPlan = {
  dirs: List[Str],
  objects: List[Str],
  lib_objects: List[Str],
  archive_owners: List[RawArchiveOwner],
  composites: List[RawComposite],
  unsupported: List[Str],
}
type RawScan = {dir: Str, plan: RawPlan, child_dirs: List[Str], entries: List[Str]}

proc path_from_string(item: Str) [error] -> Result[Path] {
  return fp"{item}"
}

proc paths_from_strings(items: List[Str]) [error] -> Result[List[Path]] {
  [path_from_string(item)? for item in items]
}

proc archive_owners_from_records(items: List[RawArchiveOwner]) [error] -> Result[List[ArchiveOwner]] {
  var owners: List[ArchiveOwner] = []
  for item in items {
    let object = item.object
    let dir = item.dir
    owners = owners.push({object: fp"{object}", dir: fp"{dir}"})
  }
  return owners
}

proc composites_from_records(items: List[RawComposite]) [error] -> Result[List[Composite]] {
  var composites: List[Composite] = []
  for item in items {
    let object = item.object
    let members = item.members
    composites = composites.push({
      object: fp"{object}",
      members: paths_from_strings(members)?,
    })
  }
  return composites
}

proc materialize(item: Record) [error] -> Result[Scan] {
  let checked = item.require(RawScan)?
  let dir_key = checked.dir
  let plan_value = checked.plan
  let dirs = plan_value.dirs
  let objects = plan_value.objects
  let lib_objects = plan_value.lib_objects
  let archive_owners = plan_value.archive_owners
  let composites = plan_value.composites
  let unsupported = plan_value.unsupported
  let child_dirs = checked.child_dirs
  let entries = checked.entries
  return {
    dir: fp"{dir_key}",
    plan: {
      dirs: paths_from_strings(dirs)?,
      objects: paths_from_strings(objects)?,
      lib_objects: paths_from_strings(lib_objects)?,
      archive_owners: archive_owners_from_records(archive_owners)?,
      composites: composites_from_records(composites)?,
      unsupported: unsupported,
    },
    child_dirs: paths_from_strings(child_dirs)?,
    entries: paths_from_strings(entries)?,
  }
}

proc materialize_all(records: List[Record]) [error] -> Result[Int] {
  var scans: List[Scan] = []
  for item in records {
    scans = scans.push(materialize(item)?)
  }
  return scans.len()
}

proc main(...argv: List[Str]) [error] -> Result[Unit] {
  var records: List[Record] = []
  let members = [f"member-{member}" for member in range(0, 156)]
  for index in range(0, 631) {
    let value = f"dir-{index}"
    let composite_items = if index == 100 {
      [{object: value, members: members}]
    } else {
      []
    }
    records = records.push({
      dir: value,
      plan: {
        dirs: [value],
        objects: [value],
        lib_objects: [value],
        archive_owners: [{object: value, dir: value}],
        composites: composite_items,
        unsupported: [],
      },
      child_dirs: [value],
      entries: [value],
    })
  }

  print materialize_all(records)?
}

main(@args)?
