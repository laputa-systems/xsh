test test_archive_tar_cpio_and_compression { |ctx|
  let root = test.temp_dir(ctx, name: "archive")?
  let src = fp"{root}/src"
  let out = fp"{root}/out"
  fp"{src}/dir".mkdir()
  out.mkdir()

  fp"{src}/dir/a.txt".write(
    """ alpha
""",
  )

  fp"{src}/link".symlink(to: p"dir/a.txt")
  let tarball = fp"{out}/pkg.tar.gz"
  archive.create(tarball, src, [p"."], .Tar, compression: .Gzip)?
  let entries = archive.list(tarball, .Tar)?.collect()
  assert entries.len() >= 3, "tar list should include dir, file, and symlink"
  assert entries |> any .path.display().ends_with("dir/a.txt"), "tar entry missing"
  let sorted_tarball = fp"{out}/sorted.tar"
  var sorted_entries = [p"dir/a.txt"]
  sorted_entries = sorted_entries |> sort-by .display()
  archive.create(sorted_tarball, src, sorted_entries, .Tar)?
  assert archive.list(sorted_tarball, .Tar)?.collect().len() == 1
  let extracted = fp"{out}/extract"
  archive.extract(tarball, extracted, .Tar)?
  assert fp"{extracted}/dir/a.txt".read_text()?.trim() == "alpha"
  let selected = fp"{out}/selected"
  archive.extract(tarball, selected, .Tar, members: [p"dir/a.txt"])?
  assert fp"{selected}/dir/a.txt".read_text()?.trim() == "alpha"
  assert ! fp"{selected}/link".exists()?
  assert archive.extract(tarball, extracted, .Tar) is Err(ArchiveError.Collision {..})
  let cpio = fp"{out}/pkg.cpio"
  archive.create(cpio, src, [p"."], .Cpio)?
  let cpio_entries = archive.list(cpio, .Cpio)?.collect()
  assert cpio_entries.len() >= 3, "cpio list should include source entries"
  let cpio_out = fp"{out}/cpio"
  archive.extract(cpio, cpio_out, .Cpio)?
  assert fp"{cpio_out}/dir/a.txt".read_text()?.trim() == "alpha"
  let payload = fp"{src}/dir/a.txt"
  let gz = fp"{out}/a.txt.gz"
  let bz2 = fp"{out}/a.txt.bz2"
  let xz = fp"{out}/a.txt.xz"
  let lzma = fp"{out}/a.txt.lzma"
  archive.compress(payload, gz, format: .Gzip)?
  archive.compress(payload, bz2, format: .Bzip2)?
  archive.compress(payload, xz, format: .Xz)?
  archive.compress(payload, lzma, format: .Lzma)?
  assert archive.decompress_bytes(gz)?.utf8()?.trim() == "alpha"
  archive.decompress(bz2, fp"{out}/bz2.out")?
  archive.decompress(xz, fp"{out}/xz.out")?
  archive.decompress(lzma, fp"{out}/lzma.out")?
  assert fp"{out}/bz2.out".read_text()?.trim() == "alpha"
  assert fp"{out}/xz.out".read_text()?.trim() == "alpha"
  assert fp"{out}/lzma.out".read_text()?.trim() == "alpha"
}

test test_archive_zip_error_contracts { |ctx|
  let not_zip = test.temp_file(ctx, name: "not.zip", contents: b"not a zip")?
  assert archive.list(not_zip, .Zip) is Err(ArchiveError.Corrupt {..})
  assert archive.extract(not_zip, test.temp_path(ctx, name: "zip-out"), .Zip) is Err(ArchiveError.Corrupt {..})
}

# One member of a hand-written ustar archive: `kind` is the type flag ("0"
# file, "1" hard link, "2" symlink, "g" pax global header).
type TarMember = {name: Str, kind: Str, link: Str, data: Bytes}

type ZipFile = {name: Str, data: Bytes}

# `value` as `digits` octal digits and the NUL that ends a ustar number.
pure tar_octal(value: Int, digits: Int) -> Str {
  var rest = value
  var text = ""
  repeat digits times {
    text = f"{rest % 8}{text}"
    rest = rest / 8
  }

  f"{text}\0"
}

# The bytes `length` occupies in a tar archive, which stores data in 512-byte blocks.
pure tar_blocks(length: Int) -> Int {
  (length + 511) / 512 * 512
}

# Overwrites the bytes of `file` from `offset` with `data`.
proc put(file: Path, offset: Int, data: Bytes) [fs, error] {
  assert bytes.write_at(file, offset, data)? == data.len()
}

# Writes `members` as a ustar archive byte by byte, so an archive can name
# members that `archive.create` refuses to produce.
proc write_tar(tarball: Path, members: List[TarMember]) [fs, error] {
  var size = 1024
  for member in members {
    size += 512 + tar_blocks(member.data.len())
  }

  tarball.write(bytes.zero(size)?)
  var header = 0
  for member in members {
    let fields = [
      {at: 0, text: member.name},
      {at: 100, text: tar_octal(0o644, 7)},
      {at: 108, text: tar_octal(0, 7)},
      {at: 116, text: tar_octal(0, 7)},
      {at: 124, text: tar_octal(member.data.len(), 11)},
      {at: 136, text: tar_octal(0, 11)},
      # The checksum is computed over a header whose checksum field is spaces.
      {at: 148, text: "        "},
      {at: 156, text: member.kind},
      {at: 157, text: member.link},
      {at: 257, text: "ustar\0"},
      {at: 263, text: "00"},
    ]
    for field in fields {
      if field.text != "" {
        put(tarball, header + field.at, bytes.from_text(field.text))
      }
    }

    let written = bytes.read_at(tarball, header, 512)?
    var checksum = 0
    for index in range(512) {
      checksum += written.byte_at(index) ?? 0
    }

    put(tarball, header + 148, bytes.from_text(tar_octal(checksum, 7)))
    if ! member.data.is_empty() {
      put(tarball, header + 512, member.data)
    }

    header += 512 + tar_blocks(member.data.len())
  }
}

# Writes `files` into `zip` with Info-ZIP 3.0, stored or deflated. A name may
# climb out of the staging directory, which is how an escaping member is made.
proc write_zip(ctx: TestContext, zip: Path, files: List[ZipFile], stored: Bool) [fs, process, env, error] {
  let version = run.capture --text "/usr/bin/zip" -v
  let banner = f"{version.stdout}{version.stderr}"
  assert version.status.exited_with(0), banner
  assert "Info-ZIP" in banner and "This is Zip 3.0" in banner, f"/usr/bin/zip must be Info-ZIP 3.0, got: {banner}"

  let base = fp"{test.temp_dir(ctx, name: "zip-src")?}/base"
  base.mkdir()
  for file in files {
    let source = fp"{base}/{file.name}"
    source.parent().mkdir()
    source.write(file.data)
  }

  let level: List[Str] = if stored { ["-0"] } else { [] }
  let names = [file.name for file in files]
  cd base {
    run "/usr/bin/zip" -q @level $zip @names
  }
}

# Recomputes the ustar checksum of the first header of `file` after its fields changed.
proc rewrite_checksum(file: Path) [fs, error] {
  put(file, 148, bytes.from_text("        "))
  let header = bytes.read_at(file, 0, 512)?
  var checksum = 0
  for index in range(512) {
    checksum += header.byte_at(index) ?? 0
  }

  put(file, 148, bytes.from_text(tar_octal(checksum, 7)))
}

# The inode number of `file`, which two hard links to one file share.
proc inode(file: Path) [process, error] -> Result[Str] {
  let listed = run.text ls -i $file
  Ok(listed.trim().split(" ")[0])
}

# `text` written `count` times.
pure repeated(text: Str, count: Int) -> Str {
  var result = ""
  repeat count times {
    result = f"{result}{text}"
  }

  result
}

test test_archive_roundtrips_compression_and_rejects_escape_paths { |ctx|
  let root = test.temp_dir(ctx, name: "archive-module")?
  let src = fp"{root}/src"
  let out = fp"{root}/out"
  fp"{src}/dir".mkdir()
  out.mkdir()
  fp"{src}/dir/a.txt".write(" alpha\n")
  fp"{src}/link".symlink(to: p"dir/a.txt")

  let tgz = fp"{out}/pkg.tar.gz"
  let tbz = fp"{out}/pkg.tar.bz2"
  let txz = fp"{out}/pkg.tar.xz"
  let plain = fp"{out}/pkg.tar"
  let inferred = fp"{out}/inferred.tgz"
  archive.create(tgz, src, [p"."], .Tar, compression: .Gzip)?
  archive.create(tbz, src, [p"."], .Tar, compression: .Bzip2)?
  archive.create(txz, src, [p"."], .Tar, compression: .Xz)?
  archive.create(plain, src, [p"."], .Tar)?
  archive.create(inferred, src, [p"."], .Tar)?
  assert archive.list(tgz, .Tar)?.collect().len() == 3
  let dest = fp"{out}/dest"
  archive.extract(tgz, dest, .Tar)?
  assert fp"{dest}/dir/a.txt".read_text()? == " alpha\n"
  let stripped = fp"{out}/stripped"
  archive.extract(tgz, stripped, .Tar, strip_components: 1)?
  assert fp"{stripped}/a.txt".read_text()?.trim() == "alpha"
  assert archive.extract(tgz, dest, .Tar) is Err(ArchiveError.Collision {..})
  assert archive.list(tbz, .Tar)?.collect().len() == 3
  assert archive.list(txz, .Tar)?.collect().len() == 3
  assert archive.list(plain, .Tar)?.collect().len() == 3
  assert archive.list(inferred, .Tar)?.collect().len() == 3

  let cpio = fp"{out}/pkg.cpio"
  archive.create(cpio, src, [p"."], .Cpio)?
  assert archive.list(cpio, .Cpio)?.collect().len() == 3
  let cpio_dest = fp"{out}/cpio"
  archive.extract(cpio, cpio_dest, .Cpio)?
  assert fp"{cpio_dest}/dir/a.txt".read_text()?.trim() == "alpha"

  let payload = fp"{src}/dir/a.txt"
  let gz = fp"{out}/a.txt.gz"
  let bz2 = fp"{out}/a.txt.bz2"
  let xz = fp"{out}/a.txt.xz"
  let lzma = fp"{out}/a.txt.lzma"
  archive.compress(payload, gz, format: .Gzip)?
  archive.compress(payload, bz2, format: .Bzip2)?
  archive.compress(payload, xz, format: .Xz)?
  archive.compress(payload, lzma, format: .Lzma)?
  # Without a format, compression follows the destination's extension.
  let auto_gz = fp"{out}/auto.gz"
  let auto_bz2 = fp"{out}/auto.bz2"
  let auto_xz = fp"{out}/auto.xz"
  let auto_lzma = fp"{out}/auto.lzma"
  archive.compress(payload, auto_gz)?
  archive.compress(payload, auto_bz2)?
  archive.compress(payload, auto_xz)?
  archive.compress(payload, auto_lzma)?
  # Decompression recognizes gzip, bzip2, and xz by content under any name.
  let gz_probe = fp"{out}/gzip.probe"
  let bz2_probe = fp"{out}/bzip2.probe"
  let xz_probe = fp"{out}/xz.probe"
  auto_gz.copy(to: gz_probe)
  auto_bz2.copy(to: bz2_probe)
  auto_xz.copy(to: xz_probe)
  assert archive.decompress_bytes(gz)?.utf8()?.trim() == "alpha"
  archive.decompress(bz2, fp"{out}/a.bz2.out")?
  archive.decompress(xz, fp"{out}/a.xz.out")?
  archive.decompress(lzma, fp"{out}/a.lzma.out")?
  assert fp"{out}/a.bz2.out".read_text()?.trim() == "alpha"
  assert fp"{out}/a.xz.out".read_text()?.trim() == "alpha"
  assert fp"{out}/a.lzma.out".read_text()?.trim() == "alpha"
  assert archive.decompress_bytes(gz_probe)?.utf8()?.trim() == "alpha"
  assert archive.decompress_bytes(bz2_probe)?.utf8()?.trim() == "alpha"
  assert archive.decompress_bytes(xz_probe)?.utf8()?.trim() == "alpha"
  assert archive.decompress_bytes(auto_lzma)?.utf8()?.trim() == "alpha"
  assert archive.compress(payload, fp"{out}/bad.zz") is Err(ArchiveError.Options {..})
  assert archive.compress(payload, fp"{out}/bad.zz", format: .Plain) is Err(ArchiveError.Options {..})
  assert archive.compress(payload, fp"{out}/bad.gz", format: .Auto, level: 10) is Err(ArchiveError.Options {..})
  assert ! fp"{out}/bad.gz".exists()?
  let unknown = fp"{out}/unknown.bin"
  payload.copy(to: unknown)
  assert archive.decompress(unknown, fp"{out}/unknown.out") is Err(ArchiveError.Options {..})

  let good_zip = fp"{root}/good.zip"
  write_zip(ctx, good_zip, [{name: "zip/note.txt", data: b"zip\n"}], true)
  let good_entries = archive.list(good_zip, .Zip)?.collect()
  assert good_entries.len() == 1
  assert good_entries[0].modified != null, "an Info-ZIP timestamp is a UTC time, never zero"
  archive.extract(good_zip, fp"{out}/zip", .Zip)?
  assert fp"{out}/zip/zip/note.txt".read_text()?.trim() == "zip"

  let parent_escape = fp"{root}/parent.tar"
  let absolute_escape = fp"{root}/absolute.tar"
  let symlink_escape = fp"{root}/symlink.tar"
  let bad_zip = fp"{root}/bad.zip"
  write_tar(parent_escape, [{name: "../evil", kind: "0", link: "", data: b"bad"}])
  write_tar(absolute_escape, [{name: "/evil", kind: "0", link: "", data: b"bad"}])
  write_tar(symlink_escape, [{name: "link", kind: "2", link: "../evil", data: b""}])
  write_zip(ctx, bad_zip, [{name: "../escape.txt", data: b"bad\n"}], true)
  assert archive.extract(parent_escape, fp"{out}/bad-parent", .Tar) is Err(ArchiveError.Unsafe {..})
  assert archive.extract(absolute_escape, fp"{out}/bad-absolute", .Tar) is Err(ArchiveError.Unsafe {..})
  assert archive.extract(symlink_escape, fp"{out}/bad-symlink", .Tar) is Err(ArchiveError.Unsafe {..})
  assert archive.extract(bad_zip, fp"{out}/bad-zip", .Zip) is Err(ArchiveError.Unsafe {..})
  assert ! fp"{out}/bad-parent".exists()?
  assert ! fp"{root}/evil".exists()?
}

test test_archive_create_evaluates_named_operands_in_written_order { |ctx|
  let root = test.temp_dir(ctx, name: "archive-tar-create-operand-order")?
  fp"{root}/member".write("payload")
  let target = fp"{root}/must-not-be-created.tar"
  let output = test.expect(
    ctx,
    r"""proc mark_collision(label: Str, value: archive.Collision) [io] -> archive.Collision {
  print $label
  return value
}
proc mark_level(label: Str, value: Int) [io] -> Int {
  print $label
  return value
}
let root = Path(args[0])
let target = Path(args[1])
archive.create(
  target,
  root,
  [p"member"],
  .Tar,
  collision: mark_collision("collision", .Replace),
  level: mark_level("level", 99),
)?
""",
    status: 3,
    stderr: ["level needs a compression"],
    args: [root, target],
  )?
  assert output.stdout == """collision
level
""", output.stdout
  assert ! target.exists()?
}

test test_archive_zip_extracts_many_files_and_overwrites { |ctx|
  let root = test.temp_dir(ctx, name: "archive-zip-many")?
  let out = fp"{root}/out"
  fp"{out}/extract/many".mkdir()
  fp"{out}/extract/many/file-00.txt".write("old\n")
  let zip = fp"{root}/many.zip"
  let files = [
    {name: f"many/file-{index:02}.txt", data: bytes.from_text(f"payload-{index:02}\n")}
    for index in range(12)
  ]
  write_zip(ctx, zip, files, true)

  assert archive.list(zip, .Zip)?.collect().len() == 12
  assert archive.extract(zip, fp"{out}/extract", .Zip) is Err(ArchiveError.Collision {..})
  assert fp"{out}/extract/many/file-00.txt".read_text()? == "old\n", "a refused extraction publishes nothing"
  assert ! fp"{out}/extract/many/file-01.txt".exists()?
  archive.extract(zip, fp"{out}/extract", .Zip, collision: .Replace)?
  for file in files {
    assert fp"{out}/extract/{file.name}".read_bytes()? == file.data
  }

  let not_zip = fp"{root}/not.zip"
  not_zip.write("not a zip")
  assert archive.list(not_zip, .Zip) is Err(ArchiveError.Corrupt {..})
  assert archive.extract(not_zip, fp"{out}/bad-open", .Zip) is Err(ArchiveError.Corrupt {..})
}

test test_archive_zip_extracts_deflated_files { |ctx|
  let root = test.temp_dir(ctx, name: "archive-zip-deflated")?
  let zip = fp"{root}/deflated.zip"
  let out = fp"{root}/out"
  let payload = bytes.from_text(repeated("deflated payload\n", 128))
  write_zip(ctx, zip, [{name: "nested/note.txt", data: payload}], false)

  assert archive.list(zip, .Zip)?.collect().len() == 1
  archive.extract(zip, out, .Zip)?
  assert fp"{out}/nested/note.txt".read_bytes()? == payload
}

test test_archive_zip_rejects_crc_mismatches { |ctx|
  let root = test.temp_dir(ctx, name: "archive-zip-crc")?
  let zip = fp"{root}/bad-crc.zip"
  write_zip(ctx, zip, [{name: "note.txt", data: b"deflated payload\n"}], false)

  # Flip one bit of the CRC-32 recorded in the last central directory header.
  let data = zip.read_bytes()?
  var central = data.len() - 4
  while data[central..central + 4] != b"PK\x01\x02" {
    central -= 1
  }

  let crc_byte = data.byte_at(central + 16) ?? 0
  let flipped = if crc_byte % 2 == 0 { crc_byte + 1 } else { crc_byte - 1 }
  put(zip, central + 16, bytes.from_ints([flipped])?)

  assert archive.extract(zip, fp"{root}/out", .Zip) is Err(ArchiveError.Corrupt {..})
  assert ! fp"{root}/out".exists()?, "a failed extraction publishes nothing"
}

test test_archive_preserves_tar_metadata_filters_and_overwrites { |ctx|
  let root = test.temp_dir(ctx, name: "archive-metadata")?
  let src = fp"{root}/src"
  let out = fp"{root}/out"
  fp"{src}/dir".mkdir()
  out.mkdir()
  fp"{src}/dir/a.txt".write(" alpha\n", mode: 0o640)
  fp"{src}/dir/other.txt".write(" other\n")
  fp"{src}/link".symlink(to: p"dir/a.txt")

  let tarball = fp"{out}/pkg.tar"
  archive.create(tarball, src, [p"."], .Tar)?
  let entries = archive.list(tarball, .Tar)?.collect()
  assert entries.len() == 4
  let files = entries |> where .path == p"dir/a.txt"
  assert files.len() == 1
  assert files[0].kind == .File
  assert files[0].mode == 0o640
  assert files[0].size == 7
  assert files[0].uid == 0, "a tar archive made here carries no owner of its own"
  let links = entries |> where .path == p"link"
  assert links.len() == 1
  assert links[0].kind == .Symlink
  assert links[0].link_name == p"dir/a.txt"
  assert files[0].link_name == null, "a member without a link has no link name"
  assert archive.list(tarball, .Tar, members: [p"dir"])?.collect().len() == 3

  let dest = fp"{out}/dest"
  archive.extract(tarball, dest, .Tar)?
  assert fp"{dest}/dir/a.txt".read_text()?.trim() == "alpha"
  assert fp"{dest}/dir/a.txt".metadata()?.mode % 512 == 0o640
  assert fp"{dest}/link".is_symlink()?
  assert fp"{dest}/link".readlink()? == p"dir/a.txt"
  fp"{dest}/dir/a.txt".write("stale")
  archive.extract(tarball, dest, .Tar, collision: .Replace, members: [p"dir/a.txt"])?
  assert fp"{dest}/dir/a.txt".read_text()?.trim() == "alpha"

  let selected = fp"{out}/selected"
  archive.extract(tarball, selected, .Tar, members: [p"dir"])?
  assert fp"{selected}/dir/a.txt".exists()?
  assert fp"{selected}/dir/other.txt".exists()?
  assert ! fp"{selected}/link".exists()?

  # Every member has fewer than two leading components left to strip.
  let stripped = fp"{out}/stripped"
  archive.extract(tarball, stripped, .Tar, strip_components: 2)?
  assert ! fp"{stripped}/a.txt".exists()?
}

# A ustar header holds 100 bytes of name and of link target; longer ones need
# an extension record on both the write and the read side.
test test_archive_roundtrips_long_tar_paths_and_link_targets { |ctx|
  let root = test.temp_dir(ctx, name: "archive-long-paths")?
  let src = fp"{root}/src"
  let out = fp"{root}/out"
  let long_path = f"{repeated("d", 101)}/note.txt"
  let long_target = f"target{repeated("/part", 30)}"
  fp"{src}/{long_path}".parent().mkdir()
  out.mkdir()
  fp"{src}/{long_path}".write("long path\n")
  fp"{src}/long-link".symlink(to: fp"{long_target}")

  let tarball = fp"{out}/pkg.tar"
  archive.create(tarball, src, [p"."], .Tar)?
  let path_entries = archive.list(tarball, .Tar, members: [fp"{long_path}"])?.collect()
  let link_entries = archive.list(tarball, .Tar, members: [p"long-link"])?.collect()
  assert path_entries[0].path.display() == long_path
  assert (link_entries[0].link_name ?? p"").display() == long_target
  let dest = fp"{out}/dest"
  archive.extract(tarball, dest, .Tar)?
  assert fp"{dest}/long-link".readlink()?.display() == long_target
}

# A pax global header, as `git archive` writes first, describes the archive
# and is not a member: listing shows only the payload, in archive order.
test test_archive_omits_pax_global_headers_from_tar_listing { |ctx|
  let tarball = test.temp_path(ctx, name: "global.tar")
  write_tar(
    tarball,
    [
      {name: "pax_global_header", kind: "g", link: "", data: b"17 comment=hello\n"},
      {name: "payload.txt", kind: "0", link: "", data: b"payload\n"},
    ],
  )

  let entries = archive.list(tarball, .Tar)?.collect()
  assert entries.len() == 1
  assert entries[0].path == p"payload.txt"

  # Iterating the listing reads the same members without collecting it.
  let paths = [entry.path.display() for entry in archive.list(tarball, .Tar)?]
  assert paths.join(",") == "payload.txt"
}

test test_archive_extracts_tar_hardlinks_as_hardlinks { |ctx|
  let root = test.temp_dir(ctx, name: "archive-hardlink")?
  let tarball = fp"{root}/hardlink.tar"
  let out = fp"{root}/out"
  write_tar(
    tarball,
    [
      {name: "dir/source.txt", kind: "0", link: "", data: b"shared\n"},
      {name: "dir/copy.txt", kind: "1", link: "dir/source.txt", data: b""},
    ],
  )

  archive.extract(tarball, out, .Tar)?
  assert fp"{out}/dir/source.txt".read_text()? == "shared\n"
  assert fp"{out}/dir/copy.txt".read_text()? == "shared\n"
  assert inode(fp"{out}/dir/source.txt")? == inode(fp"{out}/dir/copy.txt")?
}

# A release tarball's hard links name their targets under the same top-level
# directory the extraction strips, as in strace's tarball.
test test_archive_strips_tar_hardlink_targets_like_their_names { |ctx|
  let root = test.temp_dir(ctx, name: "archive-hardlink-strip")?
  let tarball = fp"{root}/hardlink.tar"
  let out = fp"{root}/out"
  write_tar(
    tarball,
    [
      {name: "pkg-1.0/source.txt", kind: "0", link: "", data: b"shared\n"},
      {name: "pkg-1.0/copy.txt", kind: "1", link: "pkg-1.0/source.txt", data: b""},
    ],
  )

  archive.extract(tarball, out, .Tar, strip_components: 1)?
  assert fp"{out}/source.txt".read_text()? == "shared\n"
  assert fp"{out}/copy.txt".read_text()? == "shared\n"
  assert inode(fp"{out}/source.txt")? == inode(fp"{out}/copy.txt")?
}

test test_archive_reads_tar_and_cpio_from_byte_streams_and_rejects_zip_streams { |ctx|
  let root = test.temp_dir(ctx, name: "archive-streams")?
  let src = fp"{root}/src"
  fp"{src}/dir".mkdir()
  fp"{src}/dir/a.txt".write("streamed\n")
  let tarball = fp"{root}/pkg.tar.gz"
  let cpio = fp"{root}/pkg.cpio"
  archive.create(tarball, src, [p"."], .Tar)?
  archive.create(cpio, src, [p"."], .Cpio)?

  let listing_stream = run.stream --bytes cat $tarball
  let from_process = archive.list(listing_stream, .Tar)?.collect()
  assert from_process |> any .path == p"dir/a.txt"
  let tar_stream = run.stream --bytes cat $tarball
  archive.extract(tar_stream, fp"{root}/from-stream", .Tar)?
  assert fp"{root}/from-stream/dir/a.txt".read_text()? == "streamed\n"
  let cpio_stream = run.stream --bytes cat $cpio
  archive.extract(cpio_stream, fp"{root}/from-cpio-stream", .Cpio)?
  assert fp"{root}/from-cpio-stream/dir/a.txt".read_text()? == "streamed\n"
  archive.extract(tarball.read_bytes()?, fp"{root}/from-bytes", .Tar)?
  assert fp"{root}/from-bytes/dir/a.txt".read_text()? == "streamed\n"

  let zip = fp"{root}/pkg.zip"
  write_zip(ctx, zip, [{name: "note.txt", data: b"zip\n"}], false)
  let zip_stream = run.stream --bytes cat $zip
  assert archive.list(zip_stream, .Zip) is Err(ArchiveError.NeedsSeek {..})
  let second_zip_stream = run.stream --bytes cat $zip
  assert archive.extract(second_zip_stream, fp"{root}/zip-stream", .Zip) is Err(ArchiveError.NeedsSeek {..})
  assert ! fp"{root}/zip-stream".exists()?
  assert archive.list(zip.read_bytes()?, .Zip)?.collect().len() == 1
  assert archive.list(zip, .Zip, compression: .Gzip) is Err(ArchiveError.Unsupported {..})
}

test test_archive_reports_missing_trailers_and_damaged_compression { |ctx|
  let root = test.temp_dir(ctx, name: "archive-integrity")?
  let src = fp"{root}/src"
  src.mkdir()
  fp"{src}/a.txt".write("payload payload payload\n")
  let plain = fp"{root}/pkg.tar"
  let gz = fp"{root}/pkg.tar.gz"
  archive.create(plain, src, [p"."], .Tar)?
  archive.create(gz, src, [p"."], .Tar)?

  # One header block and one data block, with the end-of-archive blocks cut off.
  let no_trailer = fp"{root}/no-trailer.tar"
  no_trailer.write(plain.read_bytes()?.slice(0, 1024))
  test.expect(
    ctx,
    r"""let listing = archive.list(Path(args[0]), .Tar)?
print listing.collect().len()
""",
    status: 3,
    stderr: ["ArchiveError.Truncated"],
    args: [no_trailer],
  )?
  assert archive.extract(no_trailer, fp"{root}/out-no-trailer", .Tar) is Err(ArchiveError.Truncated {..})
  assert ! fp"{root}/out-no-trailer".exists()?

  let packed = gz.read_bytes()?
  let cut = fp"{root}/cut.tar.gz"
  cut.write(packed.slice(0, packed.len() - 6))
  assert archive.extract(cut, fp"{root}/out-cut", .Tar) is Err(_)
  assert ! fp"{root}/out-cut".exists()?

  let flipped = fp"{root}/flipped.tar.gz"
  flipped.write(packed)
  let at = packed.len() - 8
  put(flipped, at, bytes.from_ints([(packed.byte_at(at) ?? 0) + 1 - 2 * ((packed.byte_at(at) ?? 0) % 2)])?)
  assert archive.extract(flipped, fp"{root}/out-flipped", .Tar) is Err(ArchiveError.Corrupt {..})
  assert ! fp"{root}/out-flipped".exists()?

  let garbage = fp"{root}/garbage.tar"
  garbage.write(bytes.concat([plain.read_bytes()?, b"appended payload that no reader would show"]))
  test.expect(
    ctx,
    r"""let listing = archive.list(Path(args[0]), .Tar)?
print listing.collect().len()
""",
    status: 3,
    stderr: ["ArchiveError.Corrupt"],
    args: [garbage],
  )?
  assert archive.extract(garbage, fp"{root}/out-garbage", .Tar) is Err(ArchiveError.Corrupt {..})
  assert ! fp"{root}/out-garbage".exists()?
}

test test_archive_extraction_policy_and_budgets { |ctx|
  let root = test.temp_dir(ctx, name: "archive-policy")?
  let tarball = fp"{root}/policy.tar"
  write_tar(
    tarball,
    [
      {name: "dir/file.txt", kind: "0", link: "", data: b"data\n"},
      {name: "dir/up", kind: "2", link: "..", data: b""},
      {name: "dir/absolute", kind: "2", link: "/etc/hostname", data: b""},
    ],
  )

  assert archive.extract(tarball, fp"{root}/default", .Tar) is Err(ArchiveError.Unsafe {..})
  assert archive.extract(tarball, fp"{root}/refused", .Tar, links: .Refuse) is Err(ArchiveError.Unsafe {..})
  archive.extract(tarball, fp"{root}/verbatim", .Tar, links: .Verbatim)?
  assert fp"{root}/verbatim/dir/absolute".readlink()? == p"/etc/hostname"
  assert fp"{root}/verbatim/dir/up".readlink()? == p".."

  let many = fp"{root}/many.tar"
  write_tar(many, [{name: f"f{index}", kind: "0", link: "", data: b"x"} for index in range(6)])
  assert archive.extract(many, fp"{root}/too-many", .Tar, max_entries: 3) is Err(ArchiveError.Limit {..})
  assert ! fp"{root}/too-many".exists()?
  assert archive.extract(many, fp"{root}/too-big", .Tar, max_entry: 0) is Err(ArchiveError.Limit {..})
  assert archive.list(many, .Tar)?.collect().len() == 6

  let rootfs = fp"{root}/modes.tar"
  write_tar(rootfs, [{name: "setuid", kind: "0", link: "", data: b"x"}])
  put(rootfs, 100, bytes.from_text(tar_octal(0o4755, 7)))
  rewrite_checksum(rootfs)
  archive.extract(rootfs, fp"{root}/plain-modes", .Tar)?
  assert fp"{root}/plain-modes/setuid".metadata()?.mode % 4096 == 0o755
  archive.extract(rootfs, fp"{root}/exact-modes", .Tar, modes: .Exact)?
  assert fp"{root}/exact-modes/setuid".metadata()?.mode % 4096 == 0o4755

  let bomb = fp"{root}/bomb.gz"
  let zeros = fp"{root}/zeros"
  zeros.write(bytes.zero(2 * 1024 * 1024)?)
  archive.compress(zeros, bomb)?
  assert archive.decompress_bytes(bomb, max_bytes: 1024) is Err(ArchiveError.Limit {..})
  assert archive.decompress_bytes(bomb)?.len() == 2 * 1024 * 1024
}

test test_archive_error_variants_carry_the_entry_and_facets { |ctx|
  let root = test.temp_dir(ctx, name: "archive-error-fields")?
  let tarball = fp"{root}/escape.tar"
  write_tar(tarball, [{name: "../evil", kind: "0", link: "", data: b"bad"}])
  match archive.extract(tarball, fp"{root}/out", .Tar) {
    Err(ArchiveError.Unsafe {entry, format, ..}) => {
      assert entry == p"../evil"
      assert format == .Tar
    }
    other => assert false, f"expected Unsafe, got {other is Ok(_)}"
  }

  assert archive.list(fp"{root}/missing.tar", .Tar) is Err(is NotFound)
  assert archive.list(fp"{root}/missing.tar", .Tar) is Err(ArchiveError.Open {..})
  assert archive.extract(tarball, fp"{root}/out2", .Tar) is Err(is InvalidData)
}
