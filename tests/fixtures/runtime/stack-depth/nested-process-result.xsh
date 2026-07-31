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

proc path_from_string(item: Str) [error] -> Result[Path] {
  return fp"${item}"
}

proc paths_from_strings(items: List[Str]) [error] -> Result[List[Path]] {
  [path_from_string(item)? for item in items]
}

proc archive_owners_from_records(items: List[Record]) [error] -> Result[List[ArchiveOwner]] {
  var owners: List[ArchiveOwner] = []
  for item in items {
    let object: Str = item.get("object")?
    let dir: Str = item.get("dir")?
    owners = owners.push({object: fp"${object}", dir: fp"${dir}"})
  }
  return owners
}

proc composites_from_records(items: List[Record]) [error] -> Result[List[Composite]] {
  var composites: List[Composite] = []
  for item in items {
    let object: Str = item.get("object")?
    let members: List[Str] = item.get("members")?
    composites = composites.push({
      object: fp"${object}",
      members: paths_from_strings(members)?,
    })
  }
  return composites
}

proc materialize(item: Record) [error] -> Result[Scan] {
  let dir_key: Str = item.get("dir")?
  let plan_value: Record = item.get("plan")?
  let dirs: List[Str] = plan_value.get("dirs")?
  let objects: List[Str] = plan_value.get("objects")?
  let lib_objects: List[Str] = plan_value.get("lib_objects")?
  let archive_owners: List[Record] = plan_value.get("archive_owners")?
  let composites: List[Record] = plan_value.get("composites")?
  let unsupported: List[Str] = plan_value.get("unsupported")?
  let child_dirs: List[Str] = item.get("child_dirs")?
  let entries: List[Str] = item.get("entries")?
  return {
    dir: fp"${dir_key}",
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
  let members = [f"member-${member}" for member in range(0, 156)]
  for index in range(0, 631) {
    let value = f"dir-${index}"
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
