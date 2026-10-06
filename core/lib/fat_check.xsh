##! FAT allocation graph, directory invariants and explicitly requested repairs.
use fat_geometry as geom
use fat_image as image_io

## Problems discovered, successful repair actions and remaining corruption.
export type Report = {issues: List[Str], repaired: UInt, unresolved: UInt}

type State = {layout: geom.Layout, table: Bytes, changes: Map[Int, Int], owners: Map[Int, Int], issues: List[Str], repaired: UInt, unresolved: UInt, repair: Bool, labels: List[geom.Patch]}
type Chain = {state: State, clusters: List[Int]}
type Directory = {cluster: Int, parent: Int, root: Bool}

pure issue(state: State, message: Str, safe: Bool) -> State {
  var updated = state
  updated.issues += [message]
  if state.repair and safe { updated.repaired += 1 } else { updated.unresolved += 1 }
  updated
}

pure value(state: State, cluster: Int) -> Result[Int] {
  if cluster in state.changes { return Ok(state.changes[cluster]) }
  geom.table_get(state.table, state.layout, cluster)
}

pure chain(initial: State, start: Int, owner: Int, safe_empty: Bool) -> Result[Chain] {
  var state = initial
  var clusters: List[Int] = []
  var current = start
  while true {
    if ! geom.valid(state.layout, current) {
      state = issue(state, f"invalid cluster {current} in entry {owner}", ! clusters.is_empty() or safe_empty)
      if state.repair and ! clusters.is_empty() { state.changes[clusters[clusters.len() - 1]] = geom.eoc(state.layout) }
      break
    }
    if current in state.owners {
      let kind = if state.owners[current] == owner { "loop" } else { "cross-link" }
      state = issue(state, f"cluster {current}: {kind} in entry {owner}", ! clusters.is_empty() or safe_empty)
      if state.repair and ! clusters.is_empty() { state.changes[clusters[clusters.len() - 1]] = geom.eoc(state.layout) }
      break
    }
    let next = value(state, current)?
    if next == geom.bad(state.layout) {
      state = issue(state, f"bad cluster {current} in entry {owner}", ! clusters.is_empty() or safe_empty)
      if state.repair and ! clusters.is_empty() { state.changes[clusters[clusters.len() - 1]] = geom.eoc(state.layout) }
      break
    }
    state.owners[current] = owner
    clusters += [current]
    break when next >= geom.eoc(state.layout) - 7
    if next == 0 {
      state = issue(state, f"cluster {current}: chain ends in free cluster", true)
      if state.repair { state.changes[current] = geom.eoc(state.layout) }
      break
    }
    current = next
  }
  Ok({state, clusters})
}

pure valid_lfn(parts: List[geom.Patch], short_name: Bytes) -> Result[Bool] {
  var checksum = 0
  for index in range(11) { checksum = (checksum % 2 * 128 + checksum / 2 + geom.byte(short_name, index)?) % 256 }
  guard parts.len() <= 20 else { return Ok(false) }
  for index in range(parts.len()) {
    let part = parts[index].data
    let ordinal = parts.len() - index + (if index == 0 { 64 } else { 0 })
    if geom.byte(part, 0)? != ordinal or geom.byte(part, 12)? != 0 or geom.byte(part, 13)? != checksum or geom.number(part, 26, 2)? != 0 { return Ok(false) }
  }
  var terminated = false
  var high_surrogate = false
  var length = 0
  var index = parts.len()
  while index > 0 {
    index -= 1
    for offset in [1, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30] {
      let unit = geom.number(parts[index].data, offset, 2)?
      if terminated { if unit != 65535 { return Ok(false) }; continue }
      if unit == 0 { if high_surrogate { return Ok(false) }; terminated = true; continue }
      if unit == 65535 or unit < 32 or unit in [34, 42, 47, 58, 60, 62, 63, 92, 124] { return Ok(false) }
      length += 1
      if high_surrogate {
        if unit < 56320 or unit > 57343 { return Ok(false) }
        high_surrogate = false
      } else if unit >= 55296 and unit <= 56319 { high_surrogate = true } else if unit >= 56320 and unit <= 57343 { return Ok(false) }
    }
  }
  Ok(length > 0 and length <= 255 and ! high_surrogate)
}

proc bad_lfn(image: Path, state: State, parts: List[geom.Patch]) -> Result[State] {
  let updated = issue(state, f"invalid long-filename chain at byte {parts[0].offset}", true)
  if state.repair { for part in parts { image_io.write_region(image, part.offset, b"\xe5")? } }
  Ok(updated)
}

proc directories(image: Path, initial: State) -> Result[State] {
  var state = initial
  let layout = state.layout
  var queue: List[Directory] = [{cluster: layout.root_cluster, parent: 0, root: true}]
  var visited: Map[Int, Bool] = {}
  var next_directory = 0
  while next_directory < queue.len() {
    let directory = queue[next_directory]
    next_directory += 1
    if directory.cluster in visited { state = issue(state, f"directory cycle at cluster {directory.cluster}", false); continue }
    visited[directory.cluster] = true
    var offsets: List[Int] = []
    let length = if directory.root and layout.bits != 32 { layout.roots * 32 } else { geom.cluster_bytes(layout) }
    if directory.root and layout.bits != 32 { offsets = [geom.root_offset(layout)] } else {
      let walked = chain(state, directory.cluster, -directory.cluster - 1, false)?
      state = walked.state
      offsets = [geom.cluster_offset(layout, cluster) for cluster in walked.clusters]
    }
    var parts: List[geom.Patch] = []
    var ended = false
    var found_dot = directory.root
    var found_parent = directory.root
    for base in offsets {
      break when ended
      let chunk = image_io.read_region(image, base, length)?
      for index in range(chunk.len() / 32) {
        let at = base + index * 32
        var entry = chunk.slice(index * 32, length: 32)
        let first = geom.byte(entry, 0)?
        if first == 0 { ended = true; break }
        if first == 229 { if ! parts.is_empty() { state = bad_lfn(image, state, parts)?; parts = [] }; continue }
        let attributes = geom.byte(entry, 11)?
        if attributes == 15 { parts += [{offset: at, data: entry}]; continue }
        if ! parts.is_empty() { if ! valid_lfn(parts, entry.slice(0, length: 11))? { state = bad_lfn(image, state, parts)? }; parts = [] }
        let cluster = geom.number(entry, 20, 2)? * 65536 + geom.number(entry, 26, 2)?
        let size = geom.number(entry, 28, 4)?
        if attributes.bit_and(192) != 0 or (layout.bits != 32 and geom.number(entry, 20, 2)? != 0) { state = issue(state, f"invalid directory entry at byte {at}", false) }
        if attributes.bit_and(8) != 0 {
          if directory.root { state.labels += [{offset: at, data: entry.slice(0, length: 11)}] } else { state = issue(state, f"volume label outside root at byte {at}", false) }
          if attributes != 8 or cluster != 0 or size != 0 { state = issue(state, f"invalid volume-label entry at byte {at}", false) }
          continue
        }
        let short_name = entry.slice(0, length: 11)
        if short_name in [b".          ", b"..         "] {
          let dot = short_name == b".          "
          if dot { found_dot = true } else { found_parent = true }
          let expected = if dot { directory.cluster } else { directory.parent }
          if directory.root or attributes.bit_and(16) == 0 or cluster != expected or size != 0 {
            state = issue(state, f"invalid dot directory entry at byte {at}", ! directory.root)
            if state.repair and ! directory.root {
              entry = geom.put(entry, 20, 2, expected / 65536)?
              entry = geom.put(entry, 26, 2, expected % 65536)?
              entry = geom.put(entry, 28, 4, 0)?
              entry = geom.put(entry, 11, 1, 16)?
              image_io.write_region(image, at, entry)?
            }
          }
          continue
        }
        var invalid_name = short_name.slice(0, length: 8) == b"        " or geom.byte(entry, 12)?.clear_bits(24) != 0
        for name_at in range(11) {
          let byte = geom.byte(short_name, name_at)?
          if (byte < 32 and ! (name_at == 0 and byte == 5)) or byte in [34, 42, 43, 44, 46, 47, 58, 59, 60, 61, 62, 63, 91, 92, 93, 124] { invalid_name = true }
        }
        if invalid_name { state = issue(state, f"invalid short filename at byte {at}", false) }
        if attributes.bit_and(16) != 0 {
          if size != 0 { state = issue(state, f"nonzero directory size at byte {at}", true); if state.repair { entry = geom.put(entry, 28, 4, 0)?; image_io.write_region(image, at, entry)? } }
          if ! geom.valid(layout, cluster) { state = issue(state, f"invalid directory cluster {cluster} at byte {at}", false) } else { queue += [{cluster, parent: if directory.root { 0 } else { directory.cluster }, root: false}] }
          continue
        }
        if cluster == 0 {
          if size != 0 { state = issue(state, f"nonempty file without cluster at byte {at}", true); if state.repair { entry = geom.put(entry, 28, 4, 0)?; image_io.write_region(image, at, entry)? } }
          continue
        }
        let walked = chain(state, cluster, at, true)?
        state = walked.state
        if state.repair and walked.clusters.is_empty() {
          entry = geom.put(entry, 20, 2, 0)?
          entry = geom.put(entry, 26, 2, 0)?
          image_io.write_region(image, at, entry)?
        }
        let capacity = walked.clusters.len() * geom.cluster_bytes(layout)
        let required = (size + geom.cluster_bytes(layout) - 1) / geom.cluster_bytes(layout)
        if capacity < size {
          state = issue(state, f"file size exceeds chain at byte {at}", true)
          if state.repair {
            entry = geom.put(entry, 28, 4, capacity)?
            if walked.clusters.is_empty() { entry = geom.put(entry, 20, 2, 0)?; entry = geom.put(entry, 26, 2, 0)? }
            image_io.write_region(image, at, entry)?
          }
        }
        if walked.clusters.len() > required {
          state = issue(state, f"file chain exceeds size at byte {at}", true)
          if state.repair {
            if required == 0 { entry = geom.put(entry, 20, 2, 0)?; entry = geom.put(entry, 26, 2, 0)?; image_io.write_region(image, at, entry)? } else { state.changes[walked.clusters[required - 1]] = geom.eoc(layout) }
            for chain_index in range(walked.clusters.len()) { if chain_index >= required { let discarded = walked.clusters[chain_index]; state.changes[discarded] = 0; state.owners = state.owners.remove(discarded) } }
          }
        }
      }
    }
    if ! parts.is_empty() { state = bad_lfn(image, state, parts)? }
    if ! found_dot or ! found_parent { state = issue(state, f"directory cluster {directory.cluster} missing dot entries", false) }
  }
  Ok(state)
}

proc flush_changes(image: Path, state: State) -> Result[Unit] {
  let layout = state.layout
  for {key: cluster, value: replacement} in state.changes {
    var offset = 0
    var encoded = b""
    if layout.bits == 12 {
      offset = cluster + cluster / 2
      let packed = if cluster % 2 == 0 { replacement % 4096 + value(state, cluster + 1)? % 16 * 4096 } else { value(state, cluster - 1)? / 256 % 16 + replacement % 4096 * 16 }
      encoded = bytes.pack_le(packed, 2)?
    } else { let change = geom.table_patch(state.table, layout, cluster, replacement)?; offset = change.offset; encoded = change.data }
    for copy in range(layout.fats) { image_io.write_region(image, (layout.reserved + copy * layout.fat_sectors) * layout.sector + offset, encoded)? }
  }
  Ok()
}

## Check the full allocation graph and repair only explicitly selected safe actions.
export proc check(image: Path, repair = false) -> Result[Report, Error] {
  let loaded = image_io.load(image)?
  let layout = loaded.layout
  var boot = loaded.boot
  var state: State = {layout, table: image_io.table(image, layout)?, changes: {}, owners: {}, issues: [], repaired: 0, unresolved: 0, repair, labels: []}
  if value(state, 0)? % 256 != geom.byte(boot, 21)? { state = issue(state, "FAT media descriptor differs from BPB", true); if repair { state.changes[0] = geom.eoc(layout) - 255 + geom.byte(boot, 21)? } }
  let flags = value(state, 1)?
  let mask = if layout.bits == 16 { 49152 } else if layout.bits == 32 { 201326592 } else { 0 }
  if flags.bit_and(mask) != mask { state = issue(state, "volume has dirty or I/O error flags", true); if repair { state.changes[1] = flags.bit_or(mask) } }
  if flags.bit_or(mask) != geom.eoc(layout) { state = issue(state, "invalid reserved FAT entry", true); if repair { state.changes[1] = geom.eoc(layout) } }
  for copy in range(layout.fats) {
    continue when copy == 0
    let offset = (layout.reserved + copy * layout.fat_sectors) * layout.sector
    if image_io.read_region(image, offset, state.table.len())? != state.table { state = issue(state, f"FAT copy {copy} differs from first FAT", true); if repair { image_io.write_region(image, offset, state.table)? } }
  }
  state = directories(image, state)?
  let label_at = if layout.bits == 32 { 71 } else { 43 }
  let root_label = if state.labels.is_empty() { b"NO NAME    " } else { state.labels[0].data }
  if boot.slice(label_at, length: 11) != root_label { state = issue(state, "boot and root directory volume labels differ", true); if repair { boot = geom.replace(boot, label_at, root_label); image_io.write_region(image, 0, boot)? } }
  for index in range(state.labels.len()) { if index > 0 { let offset = state.labels[index].offset; state = issue(state, f"duplicate root volume label at byte {offset}", true); if repair { image_io.write_region(image, offset, b"\xe5")? } } }
  var lost = 0
  var first_lost = 0
  var free = 0
  var next_free = 4294967295
  for index in range(layout.clusters) {
    let cluster = index + 2
    let allocated = value(state, cluster)?
    if allocated != 0 and allocated != geom.bad(layout) and cluster not in state.owners { if lost == 0 { first_lost = cluster }; lost += 1; if repair { state.changes[cluster] = 0 } }
    if value(state, cluster)? == 0 { free += 1; if next_free == 4294967295 { next_free = cluster } }
  }
  if lost > 0 { state = issue(state, f"{lost} lost clusters, first cluster {first_lost}", true) }
  if layout.bits == 32 {
    if image_io.read_region(image, layout.backup * layout.sector, layout.sector)? != boot { state = issue(state, "backup boot sector differs", true); if repair { image_io.write_region(image, layout.backup * layout.sector, boot)? } }
    for sector in [layout.fsinfo, layout.backup + layout.fsinfo] {
      continue when sector >= layout.reserved
      let info = image_io.read_region(image, sector * layout.sector, layout.sector)?
      let known = geom.number(info, 488, 4)?
      let next = geom.number(info, 492, 4)?
      let invalid_count = known != 4294967295 and (known > layout.clusters or (sector == layout.fsinfo and known != free))
      if geom.number(info, 0, 4)? != 1096897106 or geom.number(info, 484, 4)? != 1631679090 or geom.number(info, 508, 4)? != 2857697280 or invalid_count or (next != 4294967295 and ! geom.valid(layout, next)) {
        state = issue(state, f"invalid FSInfo sector {sector}", true)
        if repair { image_io.write_region(image, sector * layout.sector, geom.fsinfo(layout, free, next_free)?)? }
      }
    }
  }
  if repair and state.repaired > 0 { flush_changes(image, state)?; fs.fsync(image)? }
  Ok({issues: state.issues, repaired: state.repaired, unresolved: state.unresolved})
}
