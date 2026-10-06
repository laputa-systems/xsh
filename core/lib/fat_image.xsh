##! Bounded image IO and FAT root-directory traversal.
use fat_geometry as geom

## Primary boot record and its validated geometry.
export type Loaded = {layout: geom.Layout, boot: Bytes}

## Check image kind before interpreting any bytes.
export proc regular(image: Path) -> Result[Int, Error] {
  let metadata = fs.stat(image)?
  guard metadata.kind == "file" else { return Err(error.failure("FAT operations currently require a regular image file")) }
  Ok(metadata.size)
}

## Read a bounded interval from an opened regular image.
export proc read_region(image: Path, offset: Int, length: Int) -> Result[Bytes, Error] {
  guard length >= 0 and length <= 268435456 else { return Err(error.failure("FAT read exceeds its metadata bound")) }
  bytes.read_at(image, offset, length, regular: true)
}

## Write a bounded interval after verifying the opened descriptor is regular.
export proc write_region(image: Path, offset: Int, data: Bytes) -> Result[Unit, Error] {
  let _ = bytes.write_at(image, offset, data, regular: true)?
  Ok()
}

## Validate the BPB before loading its full logical sector.
export proc load(image: Path) -> Result[Loaded, Error] {
  let size = regular(image)?
  let layout = geom.parse(read_region(image, 0, 512)?, size)?
  Ok({layout, boot: read_region(image, 0, layout.sector)?})
}

## Read exactly one bounded allocation table, independent of image size.
export proc table(image: Path, layout: geom.Layout) -> Result[Bytes, Error] { read_region(image, layout.reserved * layout.sector, layout.fat_sectors * layout.sector) }

## Enumerate FAT32 root clusters, rejecting loops and invalid references.
export pure root_offsets(layout: geom.Layout, table: Bytes) -> Result[List[Int], Error] {
  if layout.bits != 32 { return Ok([geom.root_offset(layout)]) }
  var offsets: List[Int] = []
  var seen: Map[Int, Bool] = {}
  var cluster = layout.root_cluster
  while true {
    guard geom.valid(layout, cluster) and cluster not in seen else { return Err(error.failure("invalid FAT32 root directory chain")) }
    seen[cluster] = true
    offsets += [geom.cluster_offset(layout, cluster)]
    let next = geom.table_get(table, layout, cluster)?
    break when next >= geom.eoc(layout) - 7
    cluster = next
  }
  Ok(offsets)
}

## Root entry buffer length for fixed roots or one FAT32 cluster.
export pure root_length(layout: geom.Layout) -> Int { if layout.bits == 32 { geom.cluster_bytes(layout) } else { layout.roots * 32 } }
