##! FAT image creation, inspection, allocation checking and conventional applet presentation.

use gnu
use fat_geometry as geom
use fat_image as image_io
use fat_check as verify

## Validated FAT volume identity and geometry.
export type Info = geom.Info

## Discovered problems, repair actions and unresolved corruption.
export type Report = verify.Report

type FormatOptions = {create: Bool, bits: UInt, sector: UInt, cluster: UInt, reserved: UInt, fats: UInt, label: Str, serial: Str?, invariant: Bool, help: Bool, version: Bool, operands: List[Str]}
type CheckOptions = {automatic: Bool, preen: Bool, yes: Bool, readonly: Bool, verbose: Bool, help: Bool, version: Bool, operands: List[Str]}
type LabelOptions = {help: Bool, version: Bool, operands: List[Str]}

pure volume_id(text: Str) -> Result[UInt] {
  guard rx"^[0-9a-fA-F]{1,8}$".matches(text) else { return Err(error.failure("volume ID must contain one to eight hexadecimal digits")) }
  var value: UInt = 0
  var index = 0
  while index < text.byte_len() {
    let char = text.lower().byte_slice(index, length: 1)
    let digit = "0123456789abcdef".find(char) ?? -1
    guard digit >= 0 else { return Err(error.failure("invalid hexadecimal volume ID")) }
    value = value * 16 + digit as UInt
    index += 1
  }
  Ok(value)
}

## Create or format a named regular image, with exclusive creation for -C.
export proc mkfs(argv: List[Str]) {
  let opts: FormatOptions = cli.applet(argv, {
    gnu: {status: 1},
    create: {form: "-C", default: false},
    bits: {form: "-F BITS", default: 0},
    sector: {form: "-S SIZE", default: 512},
    cluster: {form: "-s SECTORS", default: 0},
    reserved: {form: "-R SECTORS", default: 0},
    fats: {form: "-f COUNT", default: 2},
    label: {form: "-n LABEL", default: ""},
    serial: {form: "-i HEX"},
    invariant: {form: "--invariant", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...DEVICE"},
  })?
  if opts.help { gnu.help("Usage: mkfs.fat [OPTIONS] DEVICE [BLOCKS]\n  -C create image (BLOCKS are 1024 bytes)\n  -F 12|16|32  -S sector-size  -s sectors-per-cluster\n  -R reserved-sectors  -f FAT-count  -n label  -i volume-id\n  --invariant reproducible metadata\nCurrently supports regular image files.\n"); return }
  if opts.version { gnu.version("mkfs.fat"); return }
  if opts.operands.len() < 1 or opts.operands.len() > 2 { gnu.usage_error("expected DEVICE and optional BLOCKS") }
  let image = opts.operands[0] as Path
  if opts.create and opts.operands.len() != 2 { gnu.usage_error("-C requires a block count") }
  if opts.create and image.exists()? { gnu.error("image already exists"); exit 1 }
  var size: UInt = 0
  if opts.operands.len() == 2 {
    let blocks = match opts.operands[1].parse_int() { Ok(value) => value, Err(failure) => { gnu.error(failure.message); exit 1 } }
    if blocks <= 0 or blocks > 9007199254740991 { gnu.usage_error("invalid block count") }
    size = blocks as UInt * 1024
  }
  if ! opts.create {
    let meta = match image.metadata() { Ok(value) => value, Err(failure) => { gnu.error(failure.message); exit 1 } }
    if meta.kind != "file" { gnu.error("currently requires a regular image file"); exit 1 }
    if size == 0 { size = meta.size as UInt }
    if size > meta.size as UInt { gnu.error("requested volume exceeds image size"); exit 1 }
  }
  var serial: UInt = 0
  if let text = opts.serial { serial = match volume_id(text) { Ok(value) => value, Err(failure) => { gnu.error(failure.message); exit 1 } } } else if ! opts.invariant { serial = time.now() / 1000 as UInt % 4294967296 }
  match format(image, size, bits: opts.bits, sector: opts.sector, cluster: opts.cluster, reserved: opts.reserved, fats: opts.fats, label: opts.label, serial:, create: opts.create) {
    Ok(_) => { print "mkfs.fat (XSH)" }, Err(failure) => { gnu.error(failure.message); exit 1 },
  }
}

## Check without writes by default; repair requires an explicit noninteractive mode.
export proc fsck(argv: List[Str]) {
  let opts: CheckOptions = cli.applet(argv, {
    gnu: {status: 2},
    automatic: {form: "-a", default: false}, preen: {form: "-p", default: false},
    yes: {form: "-y", default: false}, readonly: {form: "-n", default: false},
    verbose: {form: "-v", default: false},
    help: {form: "--help", default: false, stop: true}, version: {form: "--version", default: false, stop: true},
    operands: {form: "...DEVICE"},
  })?
  if opts.help { gnu.help("Usage: fsck.fat [-a|-p|-y|-n] [-v] DEVICE\nCheck a regular FAT image. Repairs require -a, -p, or -y.\n"); return }
  if opts.version { gnu.version("fsck.fat"); return }
  if opts.operands.len() != 1 { gnu.usage_error("expected one DEVICE", status: 2) }
  let repair = opts.automatic or opts.preen or opts.yes
  if opts.readonly and repair { gnu.usage_error("-n conflicts with repair mode", status: 2) }
  let report = match check(opts.operands[0] as Path, repair:) { Ok(value) => value, Err(failure) => { gnu.error(failure.message); exit 8 } }
  for issue in report.issues { print $issue }
  if opts.verbose { print f"{report.repaired} repairs, {report.unresolved} unresolved problems" }
  if report.unresolved > 0 { exit 1 }
  if report.repaired > 0 { exit 1 }
}

## Read or update both the root volume label and boot-sector label.
export proc fatlabel(argv: List[Str]) {
  let opts: LabelOptions = cli.applet(argv, {
    gnu: {status: 1}, help: {form: "--help", default: false, stop: true}, version: {form: "--version", default: false, stop: true}, operands: {form: "...DEVICE"},
  })?
  if opts.help { gnu.help("Usage: fatlabel DEVICE [LABEL]\nRead or change the volume label of a regular FAT image.\n"); return }
  if opts.version { gnu.version("fatlabel"); return }
  if opts.operands.len() < 1 or opts.operands.len() > 2 { gnu.usage_error("expected DEVICE and optional LABEL") }
  let name: Str? = if opts.operands.len() == 2 { opts.operands[1] } else { null }
  let result = match label(opts.operands[0] as Path, label: name) { Ok(value) => value, Err(failure) => { gnu.error(failure.message); exit 1 } }
  if name == null { print $result }
}

## Format a FAT image with all geometry and boot-record policy owned by XSH.
export proc format(image: Path, size: UInt, bits: UInt = 0, sector: UInt = 512, cluster: UInt = 0, reserved: UInt = 0, fats: UInt = 2, label: Str = "", serial: UInt = 0, create = false) -> Result[Info, Error] {
  let layout = geom.geometry(size, bits, sector, cluster, reserved, fats)?
  let name = geom.label_bytes(label)?
  guard serial <= 4294967295 else { return Err(error.failure("volume ID must fit 32 bits")) }
  if create or ! image.exists()? {
    bytes.resize(image, size, create: true, exclusive: true, regular: true)?
  } else { guard image_io.regular(image)? >= size else { return Err(error.failure("requested FAT volume exceeds image size")) } }
  let clear_length = (geom.data_sector(layout) + (if layout.bits == 32 { layout.spc } else { 0 })) * layout.sector
  let zeroes = bytes.zero(65536)?
  var offset = 0
  while offset < clear_length {
    let length = if clear_length - offset < 65536 { clear_length - offset } else { 65536 }
    image_io.write_region(image, offset, zeroes.slice(0, length: length))?
    offset += length
  }
  var boot = bytes.zero(layout.sector)?
  boot = geom.replace(boot, 0, b"\xeb\x58\x90XSH FAT ")
  boot = geom.put(boot, 11, 2, layout.sector)?
  boot = geom.put(boot, 13, 1, layout.spc)?
  boot = geom.put(boot, 14, 2, layout.reserved)?
  boot = geom.put(boot, 16, 1, layout.fats)?
  boot = geom.put(boot, 17, 2, layout.roots)?
  boot = geom.put(boot, 21, 1, 248)?
  boot = geom.put(boot, 24, 2, 32)?
  boot = geom.put(boot, 26, 2, 64)?
  if layout.total <= 65535 and layout.bits != 32 { boot = geom.put(boot, 19, 2, layout.total)? } else { boot = geom.put(boot, 32, 4, layout.total)? }
  let identity = if layout.bits == 32 { 64 } else { 36 }
  if layout.bits == 32 {
    boot = geom.put(boot, 36, 4, layout.fat_sectors)?
    boot = geom.put(boot, 44, 4, 2)?
    boot = geom.put(boot, 48, 2, 1)?
    boot = geom.put(boot, 50, 2, 6)?
  } else { boot = geom.put(boot, 22, 2, layout.fat_sectors)? }
  boot = geom.put(boot, identity, 1, 128)?
  boot = geom.put(boot, identity + 2, 1, 41)?
  boot = geom.put(boot, identity + 3, 4, serial)?
  boot = geom.replace(boot, identity + 7, if label == "" { b"NO NAME    " } else { name })
  boot = geom.replace(boot, identity + 18, if layout.bits == 12 { b"FAT12   " } else if layout.bits == 16 { b"FAT16   " } else { b"FAT32   " })
  boot = geom.replace(boot, 510, b"\x55\xaa")
  var table = bytes.zero(layout.fat_sectors * layout.sector)?
  for entry in [{cluster: 0, value: geom.eoc(layout) - 255 + 248}, {cluster: 1, value: geom.eoc(layout)}] {
    let change = geom.table_patch(table, layout, entry.cluster, entry.value)?
    table = geom.replace(table, change.offset, change.data)
  }
  if layout.bits == 32 { let change = geom.table_patch(table, layout, 2, geom.eoc(layout))?; table = geom.replace(table, change.offset, change.data) }
  for copy in range(layout.fats) { image_io.write_region(image, (layout.reserved + copy * layout.fat_sectors) * layout.sector, table)? }
  if label != "" {
    let entry = bytes.concat([name, b"\x08", bytes.zero(20)?])
    image_io.write_region(image, if layout.bits == 32 { geom.cluster_offset(layout, 2) } else { geom.root_offset(layout) }, entry)?
  }
  if layout.bits == 32 {
    let info = geom.fsinfo(layout, layout.clusters - 1, 3)?
    image_io.write_region(image, layout.sector, info)?
    image_io.write_region(image, 7 * layout.sector, info)?
    image_io.write_region(image, 6 * layout.sector, boot)?
  }
  image_io.write_region(image, 0, boot)?
  fs.fsync(image)?
  geom.information(layout, boot, label.upper())
}

## Inspect geometry and the root volume label without changing image bytes.
export proc inspect(image: Path) -> Result[Info, Error] {
  let loaded = image_io.load(image)?
  let table = image_io.table(image, loaded.layout)?
  var name = ""
  var found_label = false
  var ended = false
  for offset in image_io.root_offsets(loaded.layout, table)? {
    break when ended
    let chunk = image_io.read_region(image, offset, image_io.root_length(loaded.layout))?
    for index in range(chunk.len() / 32) {
      let entry = chunk.slice(index * 32, length: 32)
      let first = geom.byte(entry, 0)?
      if first == 0 { ended = true; break }
      if first != 229 and geom.byte(entry, 11)? == 8 {
        guard ! found_label else { return Err(error.failure("duplicate root volume labels")) }
        found_label = true
        name = geom.label_text(entry.slice(0, length: 11))?
      }
    }
  }
  geom.information(loaded.layout, loaded.boot, name)
}

## Read or change the root entry and primary/backup boot label together.
export proc label(image: Path, label: Str? = null) -> Result[Str, Error] {
  if label == null { return Ok(inspect(image)?.label) }
  let name = geom.label_bytes(label)?
  let loaded = image_io.load(image)?
  let layout = loaded.layout
  let table = image_io.table(image, layout)?
  var slot: Int? = null
  var labels: List[Int] = []
  var ended = false
  for offset in image_io.root_offsets(layout, table)? {
    let chunk = image_io.read_region(image, offset, image_io.root_length(layout))?
    for index in range(chunk.len() / 32) {
      let entry = chunk.slice(index * 32, length: 32)
      let first = geom.byte(entry, 0)?
      if ended or first in [0, 229] { if slot == null { slot = offset + index * 32 }; if first == 0 { ended = true } } else if geom.byte(entry, 11)? == 8 { labels += [offset + index * 32] }
    }
  }
  var chosen = slot
  if ! labels.is_empty() { chosen = labels[0] }
  guard let offset = chosen else { return Err(error.failure("root directory has no free volume-label entry")) }
  let entry = bytes.concat([if label == "" { bytes.concat([b"\xe5", name.slice(1)]) } else { name }, b"\x08", bytes.zero(20)?])
  image_io.write_region(image, offset, entry)?
  for index in range(labels.len()) { if index > 0 { image_io.write_region(image, labels[index], b"\xe5")? } }
  let boot = geom.replace(loaded.boot, if layout.bits == 32 { 71 } else { 43 }, if label == "" { b"NO NAME    " } else { name })
  image_io.write_region(image, 0, boot)?
  if layout.bits == 32 { image_io.write_region(image, layout.backup * layout.sector, boot)? }
  fs.fsync(image)?
  Ok(label.upper())
}


## Check image allocation and directory invariants, with explicit repair authorization.
export proc check(image: Path, repair = false) -> Result[Report, Error] { verify.check(image, repair:) }
