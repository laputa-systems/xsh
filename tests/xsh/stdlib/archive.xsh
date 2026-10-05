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
  archive.tar_create(tarball, src, [p"."], compression: "gz")
  let entries = archive.tar_list(tarball)?.collect()
  assert entries.len() >= 3, "tar list should include dir, file, and symlink"
  assert entries |> any .path.display().ends_with("dir/a.txt"), "tar entry missing"
  let sorted_tarball = fp"{out}/sorted.tar"
  var sorted_entries = [p"dir/a.txt"]
  sorted_entries = sorted_entries |> sort-by .display()
  archive.tar_create(sorted_tarball, src, sorted_entries)
  assert archive.tar_list(sorted_tarball)?.collect().len() == 1
  let extracted = fp"{out}/extract"
  archive.tar_extract(tarball, extracted)
  assert fp"{extracted}/dir/a.txt".read_text()?.trim() == "alpha"
  let selected = fp"{out}/selected"
  archive.tar_extract(tarball, selected, 0, "", false, [p"dir/a.txt"])
  assert fp"{selected}/dir/a.txt".read_text()?.trim() == "alpha"
  assert ! fp"{selected}/link".exists()?
  test.error_kind(archive.tar_extract(tarball, extracted), "archive-extract")
  let cpio = fp"{out}/pkg.cpio"
  archive.cpio_create(cpio, src, [p"."])
  let cpio_entries = archive.cpio_list(cpio)?.collect()
  assert cpio_entries.len() >= 3, "cpio list should include source entries"
  let cpio_out = fp"{out}/cpio"
  archive.cpio_extract(cpio, cpio_out)
  assert fp"{cpio_out}/dir/a.txt".read_text()?.trim() == "alpha"
  let payload = fp"{src}/dir/a.txt"
  let gz = fp"{out}/a.txt.gz"
  let bz2 = fp"{out}/a.txt.bz2"
  let xz = fp"{out}/a.txt.xz"
  let lzma = fp"{out}/a.txt.lzma"
  archive.compress(payload, gz, format: "gzip")
  archive.compress(payload, bz2, format: "bzip2")
  archive.compress(payload, xz, format: "xz")
  archive.compress(payload, lzma, format: "lzma")
  assert archive.decompress_bytes(gz)?.utf8()?.trim() == "alpha"
  archive.decompress(bz2, fp"{out}/bz2.out")
  archive.decompress(xz, fp"{out}/xz.out")
  archive.decompress(lzma, fp"{out}/lzma.out")
  assert fp"{out}/bz2.out".read_text()?.trim() == "alpha"
  assert fp"{out}/xz.out".read_text()?.trim() == "alpha"
  assert fp"{out}/lzma.out".read_text()?.trim() == "alpha"
}

test test_archive_zip_error_contracts { |ctx|
  let not_zip = test.temp_file(ctx, name: "not.zip", contents: b"not a zip")?
  test.error_kind(archive.zip_list(not_zip), "archive-zip-open")
  test.error_kind(archive.zip_extract(not_zip, test.temp_path(ctx, name: "zip-out")), "archive-zip-open")
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
# members that `archive.tar_create` refuses to produce.
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
  archive.tar_create(tgz, src, [p"."], compression: "gz")
  archive.tar_create(tbz, src, [p"."], compression: "bz2")
  archive.tar_create(txz, src, [p"."], compression: "xz")
  archive.tar_create(plain, src, [p"."])
  archive.tar_create(inferred, src, [p"."])
  assert archive.tar_list(tgz)?.collect().len() == 3
  let dest = fp"{out}/dest"
  archive.tar_extract(tgz, dest)
  assert fp"{dest}/dir/a.txt".read_text()? == " alpha\n"
  let stripped = fp"{out}/stripped"
  archive.tar_extract(tgz, stripped, strip_components: 1)
  assert fp"{stripped}/a.txt".read_text()?.trim() == "alpha"
  test.error_kind(archive.tar_extract(tgz, dest), "archive-extract")
  assert archive.tar_list(tbz)?.collect().len() == 3
  assert archive.tar_list(txz)?.collect().len() == 3
  assert archive.tar_list(plain)?.collect().len() == 3
  assert archive.tar_list(inferred)?.collect().len() == 3

  let cpio = fp"{out}/pkg.cpio"
  archive.cpio_create(cpio, src, [p"."])
  assert archive.cpio_list(cpio)?.collect().len() == 3
  let cpio_dest = fp"{out}/cpio"
  archive.cpio_extract(cpio, cpio_dest)
  assert fp"{cpio_dest}/dir/a.txt".read_text()?.trim() == "alpha"

  let payload = fp"{src}/dir/a.txt"
  let gz = fp"{out}/a.txt.gz"
  let bz2 = fp"{out}/a.txt.bz2"
  let xz = fp"{out}/a.txt.xz"
  let lzma = fp"{out}/a.txt.lzma"
  archive.compress(payload, gz, format: "gzip")
  archive.compress(payload, bz2, format: "bzip2")
  archive.compress(payload, xz, format: "xz")
  archive.compress(payload, lzma, format: "lzma")
  # Without a format, compression follows the destination's extension.
  let auto_gz = fp"{out}/auto.gz"
  let auto_bz2 = fp"{out}/auto.bz2"
  let auto_xz = fp"{out}/auto.xz"
  let auto_lzma = fp"{out}/auto.lzma"
  archive.compress(payload, auto_gz)
  archive.compress(payload, auto_bz2)
  archive.compress(payload, auto_xz)
  archive.compress(payload, auto_lzma)
  # Decompression recognizes gzip, bzip2, and xz by content under any name.
  let gz_probe = fp"{out}/gzip.probe"
  let bz2_probe = fp"{out}/bzip2.probe"
  let xz_probe = fp"{out}/xz.probe"
  auto_gz.copy(to: gz_probe)
  auto_bz2.copy(to: bz2_probe)
  auto_xz.copy(to: xz_probe)
  assert archive.decompress_bytes(gz)?.utf8()?.trim() == "alpha"
  archive.decompress(bz2, fp"{out}/a.bz2.out")
  archive.decompress(xz, fp"{out}/a.xz.out")
  archive.decompress(lzma, fp"{out}/a.lzma.out")
  assert fp"{out}/a.bz2.out".read_text()?.trim() == "alpha"
  assert fp"{out}/a.xz.out".read_text()?.trim() == "alpha"
  assert fp"{out}/a.lzma.out".read_text()?.trim() == "alpha"
  assert archive.decompress_bytes(gz_probe)?.utf8()?.trim() == "alpha"
  assert archive.decompress_bytes(bz2_probe)?.utf8()?.trim() == "alpha"
  assert archive.decompress_bytes(xz_probe)?.utf8()?.trim() == "alpha"
  assert archive.decompress_bytes(auto_lzma)?.utf8()?.trim() == "alpha"
  test.error_kind(archive.compress(payload, fp"{out}/bad.zz", format: "zip"), "archive-compression")
  test.error_kind(archive.compress(payload, fp"{out}/bad.gz", format: "auto", level: 10), "archive-compression")
  let unknown = fp"{out}/unknown.bin"
  payload.copy(to: unknown)
  test.error_kind(archive.decompress(unknown, fp"{out}/unknown.out"), "archive-compression")

  let good_zip = fp"{root}/good.zip"
  write_zip(ctx, good_zip, [{name: "zip/note.txt", data: b"zip\n"}], true)
  assert archive.zip_list(good_zip)?.collect().len() == 1
  archive.zip_extract(good_zip, fp"{out}/zip")
  assert fp"{out}/zip/zip/note.txt".read_text()?.trim() == "zip"

  let parent_escape = fp"{root}/parent.tar"
  let absolute_escape = fp"{root}/absolute.tar"
  let symlink_escape = fp"{root}/symlink.tar"
  let bad_zip = fp"{root}/bad.zip"
  write_tar(parent_escape, [{name: "../evil", kind: "0", link: "", data: b"bad"}])
  write_tar(absolute_escape, [{name: "/evil", kind: "0", link: "", data: b"bad"}])
  write_tar(symlink_escape, [{name: "link", kind: "2", link: "../evil", data: b""}])
  write_zip(ctx, bad_zip, [{name: "../escape.txt", data: b"bad\n"}], true)
  test.error_kind(archive.tar_extract(parent_escape, fp"{out}/bad-parent"), "archive-path")
  test.error_kind(archive.tar_extract(absolute_escape, fp"{out}/bad-absolute"), "archive-path")
  test.error_kind(archive.tar_extract(symlink_escape, fp"{out}/bad-symlink"), "archive-escape")
  test.error_kind(archive.zip_extract(bad_zip, fp"{out}/bad-zip"), "archive-path")
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

  assert archive.zip_list(zip)?.collect().len() == 12
  test.error_kind(archive.zip_extract(zip, fp"{out}/extract"), "archive-zip-extract")
  archive.zip_extract(zip, fp"{out}/extract", overwrite: true)
  for file in files {
    assert fp"{out}/extract/{file.name}".read_bytes()? == file.data
  }

  let not_zip = fp"{root}/not.zip"
  not_zip.write("not a zip")
  test.error_kind(archive.zip_list(not_zip), "archive-zip-open")
  test.error_kind(archive.zip_extract(not_zip, fp"{out}/bad-open"), "archive-zip-open")
}

test test_archive_zip_extracts_deflated_files { |ctx|
  let root = test.temp_dir(ctx, name: "archive-zip-deflated")?
  let zip = fp"{root}/deflated.zip"
  let out = fp"{root}/out"
  let payload = bytes.from_text(repeated("deflated payload\n", 128))
  write_zip(ctx, zip, [{name: "nested/note.txt", data: payload}], false)

  assert archive.zip_list(zip)?.collect().len() == 1
  archive.zip_extract(zip, out)
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

  test.error_kind(archive.zip_extract(zip, fp"{root}/out"), "archive-zip-extract")
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
  archive.tar_create(tarball, src, [p"."])
  let entries = archive.tar_list(tarball)?.collect()
  assert entries.len() == 4
  let files = entries |> where .path == p"dir/a.txt"
  assert files.len() == 1
  assert files[0].kind == "file"
  assert files[0].mode % 512 == 0o640
  assert files[0].size == 7
  let links = entries |> where .path == p"link"
  assert links.len() == 1
  assert links[0].kind == "symlink"
  assert links[0].link_name == "dir/a.txt"
  assert archive.tar_list(tarball, "", [p"dir"])?.collect().len() == 3

  let dest = fp"{out}/dest"
  archive.tar_extract(tarball, dest)
  assert fp"{dest}/dir/a.txt".read_text()?.trim() == "alpha"
  assert fp"{dest}/dir/a.txt".metadata()?.mode % 512 == 0o640
  assert fp"{dest}/link".is_symlink()?
  assert fp"{dest}/link".readlink()? == p"dir/a.txt"
  fp"{dest}/dir/a.txt".write("stale")
  archive.tar_extract(tarball, dest, 0, "", true, [p"dir/a.txt"])
  assert fp"{dest}/dir/a.txt".read_text()?.trim() == "alpha"

  let selected = fp"{out}/selected"
  archive.tar_extract(tarball, selected, 0, "", false, [p"dir"])
  assert fp"{selected}/dir/a.txt".exists()?
  assert fp"{selected}/dir/other.txt".exists()?
  assert ! fp"{selected}/link".exists()?

  # Every member has fewer than two leading components left to strip.
  let stripped = fp"{out}/stripped"
  archive.tar_extract(tarball, stripped, 2)
  assert ! fp"{stripped}/a.txt".exists()?
  test.error_kind(archive.tar_extract(tarball, fp"{out}/negative", -1), "archive-extract")
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
  archive.tar_create(tarball, src, [p"."])
  let path_entries = archive.tar_list(tarball, "", [fp"{long_path}"])?.collect()
  let link_entries = archive.tar_list(tarball, "", [p"long-link"])?.collect()
  assert path_entries[0].path.display() == long_path
  assert link_entries[0].link_name == long_target
  let dest = fp"{out}/dest"
  archive.tar_extract(tarball, dest)
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

  let entries = archive.tar_list(tarball)?.collect()
  assert entries.len() == 1
  assert entries[0].path == p"payload.txt"

  # Iterating the listing reads the same members without collecting it.
  let paths = [entry.path.display() for entry in archive.tar_list(tarball)?]
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

  archive.tar_extract(tarball, out)
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

  archive.tar_extract(tarball, out, 1)
  assert fp"{out}/source.txt".read_text()? == "shared\n"
  assert fp"{out}/copy.txt".read_text()? == "shared\n"
  assert inode(fp"{out}/source.txt")? == inode(fp"{out}/copy.txt")?
}
