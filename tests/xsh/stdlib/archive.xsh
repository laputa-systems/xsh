proc test_archive_tar_cpio_and_compression(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "archive")?
  let src = fp"${root}/src"
  let out = fp"${root}/out"
  fs.mkdir(fp"${src}/dir")?
  fs.mkdir(out)?

  fs.write(
    fp"${src}/dir/a.txt",
    """ alpha
""",
  )?

  fs.symlink(p"dir/a.txt", fp"${src}/link")?
  let tarball = fp"${out}/pkg.tar.gz"
  archive.tar_create(tarball, src, [p"."], compression: "gz")?
  let entries = archive.tar_list(tarball)?.collect()
  test.ok(entries.len() >= 3, "tar list should include dir, file, and symlink")?
  test.ok(entries |> any .path.display().ends_with("dir/a.txt"), "tar entry missing")?
  let sorted_tarball = fp"${out}/sorted.tar"
  var sorted_entries = [p"dir/a.txt"]
  sorted_entries = sorted_entries |> sort-by .display()
  archive.tar_create(sorted_tarball, src, sorted_entries)?
  test.eq(archive.tar_list(sorted_tarball)?.collect().len(), 1)?
  let extracted = fp"${out}/extract"
  archive.tar_extract(tarball, extracted)?
  test.eq(fp"${extracted}/dir/a.txt".read_text()?.trim(), "alpha")?
  let selected = fp"${out}/selected"
  archive.tar_extract(tarball, selected, 0, "", false, [p"dir/a.txt"])?
  test.eq(fp"${selected}/dir/a.txt".read_text()?.trim(), "alpha")?
  test.ok(! fp"${selected}/link".exists()?)?
  test.error_kind(archive.tar_extract(tarball, extracted), "archive-extract")?
  let cpio = fp"${out}/pkg.cpio"
  archive.cpio_create(cpio, src, [p"."])?
  let cpio_entries = archive.cpio_list(cpio)?
  test.ok(cpio_entries.len() >= 3, "cpio list should include source entries")?
  let cpio_out = fp"${out}/cpio"
  archive.cpio_extract(cpio, cpio_out)?
  test.eq(fp"${cpio_out}/dir/a.txt".read_text()?.trim(), "alpha")?
  let payload = fp"${src}/dir/a.txt"
  let gz = fp"${out}/a.txt.gz"
  let bz2 = fp"${out}/a.txt.bz2"
  let xz = fp"${out}/a.txt.xz"
  let lzma = fp"${out}/a.txt.lzma"
  archive.compress(payload, gz, format: "gzip")?
  archive.compress(payload, bz2, format: "bzip2")?
  archive.compress(payload, xz, format: "xz")?
  archive.compress(payload, lzma, format: "lzma")?
  test.eq(archive.decompress_bytes(gz)?.utf8()?.trim(), "alpha")?
  archive.decompress(bz2, fp"${out}/bz2.out")?
  archive.decompress(xz, fp"${out}/xz.out")?
  archive.decompress(lzma, fp"${out}/lzma.out")?
  test.eq(fp"${out}/bz2.out".read_text()?.trim(), "alpha")?
  test.eq(fp"${out}/xz.out".read_text()?.trim(), "alpha")?
  test.eq(fp"${out}/lzma.out".read_text()?.trim(), "alpha")?
}

proc test_archive_zip_error_contracts(ctx: TestContext) [fs, error] {
  let not_zip = test.temp_file(ctx, name: "not.zip", contents: b"not a zip")?
  test.error_kind(archive.zip_list(not_zip), "archive-zip-open")?
  test.error_kind(archive.zip_extract(not_zip, test.temp_path(ctx, name: "zip-out")), "archive-zip-open")?
}
