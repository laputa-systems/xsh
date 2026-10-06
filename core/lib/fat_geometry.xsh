##! FAT boot records, geometry and allocation-table encoding.

## Validated filesystem offsets and cluster geometry, measured in bytes and sectors.
export type Layout = {bits: Int, sector: Int, spc: Int, reserved: Int, fats: Int, fat_sectors: Int, roots: Int, total: Int, clusters: Int, root_cluster: Int, fsinfo: Int, backup: Int}

## Public volume identity and usable geometry.
export type Info = {bits: UInt, sector_size: UInt, cluster_size: UInt, clusters: UInt, label: Str, serial: UInt}

## Decode a little-endian unsigned field without host byte-order assumptions.
export pure number(data: Bytes, at: Int, width: Int) -> Result[Int, Error] { bytes.unpack_le(data.slice(at, length: width), width) }

## Read one byte and reject an invalid offset explicitly.
export pure byte(data: Bytes, at: Int) -> Result[Int, Error] {
  guard let value = data.byte_at(at) else { return Err(error.failure("byte offset outside FAT record")) }
  Ok(value)
}

## Replace one bounded byte field without changing the surrounding record.
export pure replace(data: Bytes, at: Int, value: Bytes) -> Bytes { bytes.concat([data.slice(0, length: at), value, data.slice(at + value.len())]) }

## Encode a little-endian unsigned field into an existing bounded record.
export pure put(data: Bytes, at: Int, width: Int, value: Int) -> Result[Bytes, Error] { Ok(replace(data, at, bytes.pack_le(value, width)?)) }

## Number of sectors occupied by the fixed FAT12/16 root directory.
export pure root_sectors(layout: Layout) -> Int { (layout.roots * 32 + layout.sector - 1) / layout.sector }

## First data sector after all FATs and the fixed root directory.
export pure data_sector(layout: Layout) -> Int { layout.reserved + layout.fats * layout.fat_sectors + root_sectors(layout) }

## Byte offset of the fixed FAT12/16 root directory.
export pure root_offset(layout: Layout) -> Int { (layout.reserved + layout.fats * layout.fat_sectors) * layout.sector }

## Byte size of one data cluster.
export pure cluster_bytes(layout: Layout) -> Int { layout.spc * layout.sector }

## Byte offset of a validated data cluster.
export pure cluster_offset(layout: Layout, cluster: Int) -> Int { (data_sector(layout) + (cluster - 2) * layout.spc) * layout.sector }

## Canonical end-of-chain value for this FAT width.
export pure eoc(layout: Layout) -> Int { if layout.bits == 12 { 4095 } else if layout.bits == 16 { 65535 } else { 268435455 } }

## Bad-cluster marker for this FAT width.
export pure bad(layout: Layout) -> Int { eoc(layout) - 8 }

## Reserved FAT markers never name a data cluster, including near type boundaries.
export pure valid(layout: Layout, cluster: Int) -> Bool { cluster >= 2 and cluster < layout.clusters + 2 and cluster < bad(layout) - 7 }

pure power_two(number: Int) -> Bool { number > 0 and number.bit_and(number - 1) == 0 }

## Select a representable FAT width and cluster size from explicit constraints.
export pure geometry(size: Int, bits: Int, sector: Int, cluster: Int, reserved: Int, fats: Int) -> Result[Layout, Error] {
  guard size > 0 and sector in [512, 1024, 2048, 4096] and size % sector == 0 and size / sector <= 4294967295 and bits in [0, 12, 16, 32] and fats >= 1 and fats <= 4 and reserved >= 0 and reserved <= 65535 else { return Err(error.failure("invalid FAT format geometry")) }
  guard cluster == 0 or (power_two(cluster) and cluster <= 128 and cluster * sector <= 65536) else { return Err(error.failure("cluster must be a power of two sectors, at most 64 KiB")) }
  let kinds = if size < 16777216 { [12, 16, 32] } else if size < 536870912 { [16, 12, 32] } else { [32, 16, 12] }
  let total = size / sector
  for kind in kinds {
    continue when bits != 0 and kind != bits
    let preferred_bytes = if kind == 32 and size >= 536870912 { if size < 8589934592 { 4096 } else if size < 17179869184 { 8192 } else if size < 34359738368 { 16384 } else { 32768 } } else if kind == 16 { 2048 } else { sector }
    let preferred = if preferred_bytes / sector > 0 { preferred_bytes / sector } else { 1 }
    let candidates = [spc for spc in [1, 2, 4, 8, 16, 32, 64, 128] if spc >= preferred] + [spc for spc in [1, 2, 4, 8, 16, 32, 64, 128] if spc < preferred]
    for spc in candidates {
      continue when (cluster != 0 and spc != cluster) or spc * sector > 65536
      let reserved_count = if reserved == 0 { if kind == 32 { 32 } else { 1 } } else { reserved }
      continue when kind == 32 and reserved_count < 8
      let roots = if kind == 32 { 0 } else { 512 }
      let root_count = (roots * 32 + sector - 1) / sector
      var fat_sectors = 1
      for _ in range(64) {
        let overhead = reserved_count + fats * fat_sectors + root_count
        break when overhead >= total
        let count = (total - overhead) / spc
        let next = ((count + 2) * kind + 8 * sector - 1) / (8 * sector)
        break when next <= fat_sectors
        fat_sectors = next
      }
      let overhead = reserved_count + fats * fat_sectors + root_count
      continue when overhead >= total
      let count = (total - overhead) / spc
      continue when count > 16777216 or fat_sectors * sector > 268435456
      let fits = if kind == 12 { count >= 1 and count <= 4078 } else if kind == 16 { count >= 4085 and count <= 65518 } else { count >= 65525 and count < 268435437 }
      if fits { return Ok({bits: kind, sector, spc, reserved: reserved_count, fats, fat_sectors, roots, total, clusters: count, root_cluster: if kind == 32 { 2 } else { 0 }, fsinfo: if kind == 32 { 1 } else { 0 }, backup: if kind == 32 { 6 } else { 0 }}) }
    }
  }
  Err(error.failure("image size cannot represent the requested FAT type and geometry"))
}

## Validate the primary BPB before trusting any derived disk offset.
export pure parse(boot: Bytes, size: Int) -> Result[Layout, Error] {
  guard boot.len() >= 512 and boot.slice(510, length: 2) == b"\x55\xaa" else { return Err(error.failure("missing FAT boot signature")) }
  let sector = number(boot, 11, 2)?
  let spc = byte(boot, 13)?
  guard sector in [512, 1024, 2048, 4096] and power_two(spc) and spc <= 128 and spc * sector <= 65536 else { return Err(error.failure("invalid FAT sector or cluster size")) }
  let reserved = number(boot, 14, 2)?
  let fats = byte(boot, 16)?
  let roots = number(boot, 17, 2)?
  let short_total = number(boot, 19, 2)?
  let total = if short_total != 0 { short_total } else { number(boot, 32, 4)? }
  let short_fat = number(boot, 22, 2)?
  let fat_sectors = if short_fat != 0 { short_fat } else { number(boot, 36, 4)? }
  guard reserved > 0 and fats >= 1 and fats <= 4 and fat_sectors > 0 and total > 0 and total * sector <= size else { return Err(error.failure("invalid or truncated FAT volume geometry")) }
  let overhead = reserved + fats * fat_sectors + (roots * 32 + sector - 1) / sector
  guard overhead < total else { return Err(error.failure("FAT geometry has no data area")) }
  let clusters = (total - overhead) / spc
  let bits = if clusters < 4085 { 12 } else if clusters < 65525 { 16 } else { 32 }
  guard clusters <= 16777216 and fat_sectors * sector <= 268435456 and (clusters + 2) * bits <= fat_sectors * sector * 8 else { return Err(error.failure("FAT metadata exceeds its supported bounds or cluster capacity")) }
  if bits == 32 {
    guard roots == 0 and short_fat == 0 and number(boot, 42, 2)? == 0 and number(boot, 40, 2)?.bit_and(128) == 0 else { return Err(error.failure("unsupported FAT32 version or active-FAT layout")) }
  } else { guard roots > 0 and short_fat > 0 else { return Err(error.failure("invalid FAT12/16 root directory")) } }
  let layout: Layout = {bits, sector, spc, reserved, fats, fat_sectors, roots, total, clusters, root_cluster: if bits == 32 { number(boot, 44, 4)? } else { 0 }, fsinfo: if bits == 32 { number(boot, 48, 2)? } else { 0 }, backup: if bits == 32 { number(boot, 50, 2)? } else { 0 }}
  if bits == 32 { guard valid(layout, layout.root_cluster) and layout.fsinfo > 0 and layout.fsinfo < reserved and layout.backup > 0 and layout.backup + 1 < reserved else { return Err(error.failure("invalid FAT32 root or reserved-sector references")) } }
  Ok(layout)
}

## Decode one FAT entry, including FAT12's shared nibble and FAT32's reserved bits.
export pure table_get(table: Bytes, layout: Layout, cluster: Int) -> Result[Int, Error] {
  if layout.bits == 12 { let value = number(table, cluster + cluster / 2, 2)?; return Ok(if cluster % 2 == 0 { value % 4096 } else { value / 16 }) }
  if layout.bits == 16 { return number(table, cluster * 2, 2) }
  Ok(number(table, cluster * 4, 4)? % 268435456)
}

## One bounded byte replacement in an allocation table.
export type Patch = {offset: Int, data: Bytes}

## Encode one FAT entry while preserving its neighbor and reserved high bits.
export pure table_patch(table: Bytes, layout: Layout, cluster: Int, value: Int) -> Result[Patch, Error] {
  if layout.bits == 12 {
    let offset = cluster + cluster / 2
    let old = number(table, offset, 2)?
    let encoded = if cluster % 2 == 0 { old / 4096 * 4096 + value % 4096 } else { old % 16 + value % 4096 * 16 }
    return Ok({offset, data: bytes.pack_le(encoded, 2)?})
  }
  if layout.bits == 16 { return Ok({offset: cluster * 2, data: bytes.pack_le(value, 2)?}) }
  let offset = cluster * 4
  Ok({offset, data: bytes.pack_le(number(table, offset, 4)? / 268435456 * 268435456 + value % 268435456, 4)?})
}

## Validate the portable ASCII volume-label subset and encode its fixed width.
export pure label_bytes(label: Str) -> Result[Bytes, Error] {
  let raw = bytes.from_text(label)
  guard raw.len() <= 11 else { return Err(error.failure("volume label must contain at most 11 printable ASCII characters without FAT punctuation")) }
  for index in range(raw.len()) {
    let value = byte(raw, index)?
    guard value >= 32 and value <= 126 and value not in [34, 42, 43, 44, 46, 47, 58, 59, 60, 61, 62, 63, 91, 92, 93, 124] else { return Err(error.failure("invalid volume label character")) }
  }
  Ok(bytes.concat([bytes.from_text(label.upper()), bytes.from_text("           ".byte_slice(0, length: 11 - raw.len()))]))
}

## Construct a complete FAT32 FSInfo sector with bounded free-space hints.
export pure fsinfo(layout: Layout, free: Int, next: Int) -> Result[Bytes, Error] {
  var info = bytes.zero(layout.sector)?
  info = put(info, 0, 4, 1096897106)?
  info = put(info, 484, 4, 1631679090)?
  info = put(info, 488, 4, free)?
  info = put(info, 492, 4, next)?
  info = put(info, 508, 4, 2857697280)?
  Ok(info)
}

## Project a validated layout and root label into the public volume record.
export pure information(layout: Layout, boot: Bytes, label: Str) -> Result[Info, Error] {
  Ok({bits: layout.bits as UInt, sector_size: layout.sector as UInt, cluster_size: cluster_bytes(layout) as UInt, clusters: layout.clusters as UInt, label, serial: number(boot, if layout.bits == 32 { 67 } else { 39 }, 4)? as UInt})
}

## Decode the root label while preserving meaningful leading spaces.
export pure label_text(data: Bytes) -> Result[Str, Error] {
  var end = data.len()
  while end > 0 and byte(data, end - 1)? == 32 { end -= 1 }
  data.slice(0, length: end).utf8()
}
