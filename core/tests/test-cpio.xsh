type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Archives written by GNU cpio 2.15 (`--reproducible -R 1000:1000`) for the
# tree built by `build_tree`: inode numbers count from 0 and device numbers are 0.
const GNU_NEWC = b"07070100000000000041ED000003E8000003E8000000023A7B837200000000000000000000000000000000000000000000000400000000dir\x00\x00\x000707010000000100008180000003E8000003E8000000013A7B83720000000B000000000000000000000000000000000000000D00000000dir/data.bin\x00\x00\x00\x01\x02\xfe\xff\nabc\x00Z\x0007070100000002000081A4000003E8000003E8000000023A7B837200000000000000000000000000000000000000000000000600000000a.txt\x0007070100000002000081A4000003E8000003E8000000023A7B837200000006000000000000000000000000000000000000000900000000link.txt\x00\x00hello\n\x00\x0007070100000003000081A0000003E8000003E8000000013A7B837200000007000000000000000000000000000000000000000B00000000sp ace.txt\x00\x00\x00\x00spaced\n\x00070701000000040000A1FF000003E8000003E8000000013A7B837200000005000000000000000000000000000000000000000400000000sym\x00\x00\x00a.txt\x00\x00\x0007070100000005000081A4000003E8000003E8000000013A7B837200000000000000000000000000000000000000000000000A00000000empty.txt\x0007070100000006000011A4000003E8000003E8000000013A7B837200000000000000000000000000000000000000000000000500000000pipe\x00\x0007070100000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000B00000000TRAILER!!!\x00\x00\x00\x00"
const GNU_CRC = b"07070200000000000041ED000003E8000003E8000000023A7B837200000000000000000000000000000000000000000000000400000000dir\x00\x00\x000707020000000100008180000003E8000003E8000000013A7B83720000000B000000000000000000000000000000000000000D0000038Adir/data.bin\x00\x00\x00\x01\x02\xfe\xff\nabc\x00Z\x0007070200000002000081A4000003E8000003E8000000023A7B837200000000000000000000000000000000000000000000000600000000a.txt\x0007070200000002000081A4000003E8000003E8000000023A7B83720000000600000000000000000000000000000000000000090000021Elink.txt\x00\x00hello\n\x00\x0007070200000003000081A0000003E8000003E8000000013A7B837200000007000000000000000000000000000000000000000B0000027Asp ace.txt\x00\x00\x00\x00spaced\n\x00070702000000040000A1FF000003E8000003E8000000013A7B837200000005000000000000000000000000000000000000000400000000sym\x00\x00\x00a.txt\x00\x00\x0007070200000005000081A4000003E8000003E8000000013A7B837200000000000000000000000000000000000000000000000A00000000empty.txt\x0007070200000006000011A4000003E8000003E8000000013A7B837200000000000000000000000000000000000000000000000500000000pipe\x00\x0007070200000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000B00000000TRAILER!!!\x00\x00\x00\x00"
const GNU_ODC = b"0707070000000000000407550017500017500000020000000723670156200000400000000000dir\x000707070000000000011006000017500017500000010000000723670156200001500000000013dir/data.bin\x00\x00\x01\x02\xfe\xff\nabc\x00Z0707070000000000021006440017500017500000020000000723670156200000600000000006a.txt\x00hello\n0707070000000000021006440017500017500000020000000723670156200001100000000006link.txt\x00hello\n0707070000000000031006400017500017500000010000000723670156200001300000000007sp ace.txt\x00spaced\n0707070000000000041207770017500017500000010000000723670156200000400000000005sym\x00a.txt0707070000000000051006440017500017500000010000000723670156200001200000000000empty.txt\x000707070000000000060106440017500017500000010000000723670156200000500000000000pipe\x000707070000000000000000000000000000000000010000000000000000000001300000000000TRAILER!!!\x00"
const GNU_BIN = b"\xc7q\x00\x00\x00\x00\xedA\xe8\x03\xe8\x03\x02\x00\x00\x00{:r\x83\x04\x00\x00\x00\x00\x00dir\x00\xc7q\x00\x00\x01\x00\x80\x81\xe8\x03\xe8\x03\x01\x00\x00\x00{:r\x83\x0d\x00\x00\x00\x0b\x00dir/data.bin\x00\x00\x00\x01\x02\xfe\xff\nabc\x00Z\x00\xc7q\x00\x00\x02\x00\xa4\x81\xe8\x03\xe8\x03\x02\x00\x00\x00{:r\x83\x06\x00\x00\x00\x06\x00a.txt\x00hello\n\xc7q\x00\x00\x02\x00\xa4\x81\xe8\x03\xe8\x03\x02\x00\x00\x00{:r\x83\x09\x00\x00\x00\x06\x00link.txt\x00\x00hello\n\xc7q\x00\x00\x03\x00\xa0\x81\xe8\x03\xe8\x03\x01\x00\x00\x00{:r\x83\x0b\x00\x00\x00\x07\x00sp ace.txt\x00\x00spaced\n\x00\xc7q\x00\x00\x04\x00\xff\xa1\xe8\x03\xe8\x03\x01\x00\x00\x00{:r\x83\x04\x00\x00\x00\x05\x00sym\x00a.txt\x00\xc7q\x00\x00\x05\x00\xa4\x81\xe8\x03\xe8\x03\x01\x00\x00\x00{:r\x83\n\x00\x00\x00\x00\x00empty.txt\x00\xc7q\x00\x00\x06\x00\xa4\x11\xe8\x03\xe8\x03\x01\x00\x00\x00{:r\x83\x05\x00\x00\x00\x00\x00pipe\x00\x00\xc7q\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x0b\x00\x00\x00\x00\x00TRAILER!!!\x00\x00"
const NAMES = "dir\ndir/data.bin\na.txt\nlink.txt\nsp ace.txt\nsym\nempty.txt\npipe\n"
const MTIME_NS = 981173106000000000

# Runs core/cpio.xsh by its real path inside `root`, capturing both streams.
proc cpio(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/cpio.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C", TZ: "UTC"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

pure archive_of(format: Str) -> Bytes {
  match format {
    "newc" => GNU_NEWC
    "crc" => GNU_CRC
    "odc" => GNU_ODC
    _ => GNU_BIN
  }
}

# The tree the GNU fixtures were made from: a hard link, a symbolic link, an
# empty file, a FIFO, an odd-sized binary file and a name with a space, all with
# one modification time.
proc build_tree(root: Path) [fs, error] -> Result[Unit] {
  fp"{root}/dir".mkdir()?
  fp"{root}/dir/data.bin".write(b"\x00\x01\x02\xfe\xff\nabc\x00Z")?
  fp"{root}/a.txt".write(b"hello\n")?
  fp"{root}/a.txt".hardlink(at: fp"{root}/link.txt")?
  fp"{root}/sp ace.txt".write(b"spaced\n")?
  fp"{root}/sym".symlink(to: p"a.txt")?
  fp"{root}/empty.txt".write(b"")?
  fs.mkfifo(fp"{root}/pipe", 0o644)?
  fp"{root}/dir".chmod(0o755)?
  fp"{root}/dir/data.bin".chmod(0o600)?
  fp"{root}/a.txt".chmod(0o644)?
  fp"{root}/sp ace.txt".chmod(0o640)?
  fp"{root}/empty.txt".chmod(0o644)?
  fp"{root}/pipe".chmod(0o644)?
  for name in ["dir/data.bin", "a.txt", "sp ace.txt", "empty.txt", "pipe", "dir"] {
    fs.set_times(fp"{root}/{name}", mtime_ns: MTIME_NS)?
  }
  fs.set_times(fp"{root}/sym", mtime_ns: MTIME_NS, follow_symlinks: false)?
  Ok()
}

test test_cpio_lists_gnu_archives_of_every_format { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-list")?
  for format in ["newc", "crc", "odc", "bin"] {
    let listed = cpio(ctx, root, ["-it", "--quiet"], archive_of(format))?
    assert listed.status == 0, format
    assert listed.stdout == bytes.from_text(NAMES), format
    assert listed.stderr == "", format
  }
  let verbose = cpio(ctx, root, ["-itvn", "--quiet"], GNU_ODC)?
  let lines = verbose.stdout.utf8()?.split("\n")
  assert lines[0] == "drwxr-xr-x   2 1000     1000            0 Feb  3  2001 dir"
  assert lines[1] == "-rw-------   1 1000     1000           11 Feb  3  2001 dir/data.bin"
  assert lines[2] == "-rw-r--r--   2 1000     1000            6 Feb  3  2001 a.txt"
  assert lines[5] == "lrwxrwxrwx   1 1000     1000            5 Feb  3  2001 sym -> a.txt"
  assert lines[7] == "prw-r--r--   1 1000     1000            0 Feb  3  2001 pipe"
}

test test_cpio_extracts_gnu_archives_of_every_format { |ctx|
  for format in ["newc", "crc", "odc", "bin"] {
    let root = test.temp_dir(ctx, name: f"cpio-extract-{format}")?
    let ran = cpio(ctx, root, ["-idm", "--quiet"], archive_of(format))?
    assert ran.status == 0, format
    assert ran.stderr == "", format
    assert fp"{root}/dir/data.bin".read_bytes()? == b"\x00\x01\x02\xfe\xff\nabc\x00Z", format
    assert fp"{root}/sp ace.txt".read_text()? == "spaced\n", format
    assert fp"{root}/empty.txt".read_bytes()? == b"", format
    assert fp"{root}/sym".readlink()? == p"a.txt", format
    let first = fs.stat(fp"{root}/a.txt")?
    let second = fs.stat(fp"{root}/link.txt")?
    assert first.ino == second.ino and first.nlink == 2, format
    assert fp"{root}/a.txt".read_text()? == "hello\n", format
    assert fs.stat(fp"{root}/pipe")?.kind == "fifo", format
    assert fs.stat(fp"{root}/dir/data.bin")?.mode.bit_and(0o777) == 0o600, format
    assert fs.stat(fp"{root}/a.txt")?.mtime_ns == MTIME_NS, format
    assert fs.stat(fp"{root}/dir")?.mtime_ns == MTIME_NS, format
  }
}

test test_cpio_creates_archives_byte_identical_to_gnu { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-create")?
  build_tree(root)?
  for format in ["newc", "crc", "odc", "bin"] {
    let ran = cpio(ctx, root, ["-o", "-H", format, "--reproducible", "-R", "1000:1000", "--quiet"], bytes.from_text(NAMES))?
    assert ran.status == 0, format
    assert ran.stderr == "", format
    # GNU pads to 512-byte blocks; the fixture keeps only the part up to the trailer.
    let expected = archive_of(format)
    assert ran.stdout.slice(0, expected.len()) == expected, format
    assert ran.stdout.len() % 512 == 0, format
    assert is_zero_tail(ran.stdout, expected.len()), format
  }
}

pure is_zero_tail(data: Bytes, from: Int) -> Bool {
  for index in range(from, data.len()) {
    if data.byte_at(index) != 0 { return false }
  }
  true
}

test test_cpio_round_trips_through_every_format { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-roundtrip")?
  let source = test.temp_dir(ctx, name: "cpio-roundtrip-src")?
  var every_byte: List[Int] = []
  for value in range(256) { every_byte += [value] }
  fp"{source}/all bytes".write(bytes.from_ints(every_byte)?)?
  fp"{source}/line\nbreak".write(b"newline in the name")?
  fp"{source}/odd".write(b"123")?
  fp"{source}/dir".mkdir()?
  fp"{source}/dir/-dash".write(b"x")?
  let names = b"all bytes\0line\nbreak\0odd\0dir\0dir/-dash\0"
  for format in ["newc", "crc", "odc", "bin"] {
    let packed_archive = cpio(ctx, source, ["-o0", "-H", format, "--quiet"], names)?
    assert packed_archive.status == 0, format
    let out = test.temp_dir(ctx, name: f"cpio-roundtrip-{format}")?
    let ran = cpio(ctx, out, ["-id", "--quiet"], packed_archive.stdout)?
    assert ran.status == 0, format
    assert fp"{out}/all bytes".read_bytes()? == bytes.from_ints(every_byte)?, format
    assert fp"{out}/line\nbreak".read_bytes()? == b"newline in the name", format
    assert fp"{out}/odd".read_bytes()? == b"123", format
    assert fp"{out}/dir/-dash".read_bytes()? == b"x", format
    let listed = cpio(ctx, out, ["-it0", "--quiet"], packed_archive.stdout)?
    assert listed.stdout == names, format
  }
}

test test_cpio_creates_block_padded_archives_and_counts_blocks { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-blocks")?
  fp"{root}/f".write(b"data")?
  let plain = cpio(ctx, root, ["-o", "-H", "newc"], b"f\n")?
  assert plain.stdout.len() == 512
  assert plain.stderr == "1 block\n"
  let big = cpio(ctx, root, ["-oB", "-H", "newc"], b"f\n")?
  assert big.stdout.len() == 5120
  assert big.stderr == "1 block\n"
  let sized = cpio(ctx, root, ["-o", "-H", "newc", "--block-size=3"], b"f\n")?
  assert sized.stdout.len() == 1536
  let bytes_io = cpio(ctx, root, ["-o", "-H", "newc", "-C", "100"], b"f\n")?
  assert bytes_io.stdout.len() == 300
  assert bytes_io.stderr == "3 blocks\n"
  let quiet = cpio(ctx, root, ["-o", "-H", "newc", "--quiet"], b"f\n")?
  assert quiet.stderr == ""
  let listed = cpio(ctx, root, ["-it"], plain.stdout)?
  assert listed.stderr == "1 block\n"
  let listed_big = cpio(ctx, root, ["-it", "-B"], big.stdout)?
  assert listed_big.stderr == "1 block\n"
  # The trailing padding of an packed_archive counts as input only up to the block
  # holding the trailer.
  let many = cpio(ctx, root, ["-it", "-C", "64"], plain.stdout)?
  assert many.stderr == "8 blocks\n"
}

test test_cpio_default_format_is_binary_and_c_means_odc { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-defaults")?
  fp"{root}/f".write(b"data")?
  let default_format = cpio(ctx, root, ["-o", "--quiet"], b"f\n")?
  assert default_format.stdout.slice(0, 2) == b"\xc7\x71"
  let portable = cpio(ctx, root, ["-oc", "--quiet"], b"f\n")?
  assert portable.stdout.slice(0, 6) == b"070707"
  let crc = cpio(ctx, root, ["-o", "-H", "CRC", "--quiet"], b"f\n")?
  assert crc.stdout.slice(0, 6) == b"070702"
  let reversed = cpio(ctx, root, ["-o", "-H", "newc", "-c"], b"f\n")?
  assert reversed.status == 2
  assert "Archive format multiply defined" in reversed.stderr
}

test test_cpio_deferred_hard_links_in_newc { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-links")?
  fp"{root}/one".write(b"shared")?
  fp"{root}/one".hardlink(at: fp"{root}/two")?
  fp"{root}/one".hardlink(at: fp"{root}/three")?
  # Only two of three links are named: the first listed carries the data.
  let partial = cpio(ctx, root, ["-o", "-H", "newc", "--quiet"], b"two\none\n")?
  let listed = cpio(ctx, root, ["-itvn", "--quiet"], partial.stdout)?
  let lines = listed.stdout.utf8()?.split("\n")
  assert lines[0].ends_with(" one") and "          0 " in lines[0]
  assert lines[1].ends_with(" two") and "          6 " in lines[1]
  # All three: the data goes with the last link, earlier names have no data and
  # are written newest first.
  let all = cpio(ctx, root, ["-o", "-H", "newc", "--quiet"], b"one\ntwo\nthree\n")?
  let all_listed = cpio(ctx, root, ["-itv", "-n", "--quiet"], all.stdout)?
  let all_lines = all_listed.stdout.utf8()?.split("\n")
  assert "          0 " in all_lines[0] and all_lines[0].ends_with(" two")
  assert "          0 " in all_lines[1] and all_lines[1].ends_with(" one")
  assert "          6 " in all_lines[2] and all_lines[2].ends_with(" three")
  let out = test.temp_dir(ctx, name: "cpio-links-out")?
  let ran = cpio(ctx, out, ["-idv"], all.stdout)?
  assert ran.status == 0
  assert ran.stderr == "two\none\ncpio: three linked to one\ncpio: three linked to two\nthree\n1 block\n"
  let a = fs.stat(fp"{out}/one")?
  let b = fs.stat(fp"{out}/two")?
  let c = fs.stat(fp"{out}/three")?
  assert a.ino == b.ino and b.ino == c.ino and a.nlink == 3
  assert fp"{out}/one".read_text()? == "shared"
}

test test_cpio_pass_through_copies_tree_and_keeps_links { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-pass")?
  let source = fp"{root}/src"
  source.mkdir()?
  build_tree(source)?
  let ran = cpio(ctx, source, ["-pdm", "--quiet", "../dest"], bytes.from_text(NAMES))?
  assert ran.status == 0
  assert ran.stderr == ""
  let dest = fp"{root}/dest"
  assert fp"{dest}/sp ace.txt".read_text()? == "spaced\n"
  assert fp"{dest}/sym".readlink()? == p"a.txt"
  assert fs.stat(fp"{dest}/a.txt")?.ino == fs.stat(fp"{dest}/link.txt")?.ino
  assert fs.stat(fp"{dest}/pipe")?.kind == "fifo"
  assert fs.stat(fp"{dest}/a.txt")?.mtime_ns == MTIME_NS
  let counted = cpio(ctx, source, ["-pd", "../dest2"], bytes.from_text(NAMES))?
  assert counted.stderr == "1 block\n"
  let linked = cpio(ctx, source, ["-pdl", "../dest3"], bytes.from_text(NAMES))?
  assert linked.stderr == "0 blocks\n"
  assert fs.stat(fp"{root}/dest3/a.txt")?.ino == fs.stat(fp"{source}/a.txt")?.ino
}

test test_cpio_extraction_policies { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-policy")?
  let packed_archive = cpio(ctx, root, ["-o", "-H", "newc", "--quiet"], b"")?
  assert packed_archive.status == 0
  # A name needs its directory: without -d the extraction fails and says so.
  let source = test.temp_dir(ctx, name: "cpio-policy-src")?
  fp"{source}/deep".mkdir()?
  fp"{source}/deep/file".write(b"content")?
  let packed = cpio(ctx, source, ["-o", "-H", "newc", "--quiet"], b"deep/file\n")?
  let out = test.temp_dir(ctx, name: "cpio-policy-out")?
  let refused = cpio(ctx, out, ["-i"], packed.stdout)?
  assert refused.status == 2
  assert "deep/file: Cannot open: No such file or directory" in refused.stderr
  let made = cpio(ctx, out, ["-id", "--quiet"], packed.stdout)?
  assert made.status == 0
  assert fp"{out}/deep/file".read_text()? == "content"
  # An existing file that is not older stays, with GNU's message and status 0.
  fp"{out}/deep/file".write(b"local")?
  let kept = cpio(ctx, out, ["-id", "--quiet"], packed.stdout)?
  assert kept.status == 0
  assert "deep/file not created: newer or same age version exists" in kept.stderr
  assert fp"{out}/deep/file".read_text()? == "local"
  let forced = cpio(ctx, out, ["-idu", "--quiet"], packed.stdout)?
  assert fp"{out}/deep/file".read_text()? == "content"
  assert forced.stderr == ""
}

test test_cpio_patterns_select_and_exclude_names { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-patterns")?
  for format in ["newc", "odc"] {
    let packed_archive = archive_of(format)
    let star = cpio(ctx, root, ["-it", "--quiet", "dir/*"], packed_archive)?
    assert star.stdout == b"dir/data.bin\n"
    let question = cpio(ctx, root, ["-it", "--quiet", "?.txt", "s*"], packed_archive)?
    assert question.stdout == b"a.txt\nsp ace.txt\nsym\n"
    let bracket = cpio(ctx, root, ["-it", "--quiet", "[a-b].txt"], packed_archive)?
    assert bracket.stdout == b"a.txt\n"
    let excluded = cpio(ctx, root, ["-itf", "--quiet", "*.txt", "dir*", "s*"], packed_archive)?
    assert excluded.stdout == b"pipe\n"
    fp"{root}/patterns".write("a.txt\nlink.txt\n")?
    let from_file = cpio(ctx, root, ["-it", "--quiet", "-E", "patterns", "pipe"], packed_archive)?
    assert from_file.stdout == b"a.txt\nlink.txt\npipe\n"
  }
  let out = test.temp_dir(ctx, name: "cpio-patterns-out")?
  let one = cpio(ctx, out, ["-idv", "link.txt"], GNU_NEWC)?
  # Skipping the first link must not lose the data carried by the second.
  assert fp"{out}/link.txt".read_text()? == "hello\n"
  assert ! fp"{out}/a.txt".exists()?
}

test test_cpio_absolute_names_and_no_absolute_filenames { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-absolute")?
  fp"{root}/inner".mkdir()?
  fp"{root}/inner/file".write(b"in")?
  let names = f"{root}/inner/file\n"
  let packed = cpio(ctx, root, ["-o", "-H", "newc", "--quiet"], bytes.from_text(names))?
  let listed = cpio(ctx, root, ["-it", "--quiet"], packed.stdout)?
  assert listed.stdout == bytes.from_text(names)
  let out = test.temp_dir(ctx, name: "cpio-absolute-out")?
  let ran = cpio(ctx, out, ["-id", "--no-absolute-filenames"], packed.stdout)?
  assert ran.status == 0
  assert "Removing leading `/' from member names" in ran.stderr
  let relative = f"{root}/inner/file".byte_slice(1)
  assert fp"{out}/{relative}".read_text()? == "in"
  # Without the option the archived absolute name is used as it is.
  fp"{root}/inner/file".remove()?
  let restored = cpio(ctx, out, ["-id", "--quiet", "--absolute-filenames"], packed.stdout)?
  assert fp"{root}/inner/file".read_text()? == "in"
  # ".." components are cut together with everything before them.
  let dots = cpio(ctx, root, ["-o", "-H", "newc", "--quiet"], b"inner/../inner/file\n")?
  let cut = cpio(ctx, out, ["-it", "--no-absolute-filenames", "--quiet"], dots.stdout)?
  assert cut.stdout == b"inner/file\n"
}

test test_cpio_verbose_names_directory_and_archive_files { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-files")?
  let source = test.temp_dir(ctx, name: "cpio-files-src")?
  fp"{source}/a".write(b"A")?
  fp"{source}/b".write(b"B")?
  let made = cpio(ctx, root, ["-ov", "-D", f"{source}", "-H", "odc", "-O", "out.cpio"], b"a\nb\n")?
  assert made.status == 0
  assert made.stdout == b""
  assert made.stderr == "a\nb\n1 block\n"
  assert fp"{root}/out.cpio".read_bytes()?.len() == 512
  let listed = cpio(ctx, root, ["-it", "-I", "out.cpio", "--quiet"])?
  assert listed.stdout == b"a\nb\n"
  let out = fp"{root}/target"
  let extracted = cpio(ctx, root, ["-id", "-D", f"{out}", "-F", "out.cpio", "--quiet"])?
  assert extracted.status == 0
  assert fp"{out}/b".read_text()? == "B"
  let dots = cpio(ctx, root, ["-iV", "-D", f"{out}", "-F", "out.cpio", "-u", "--quiet"])?
  assert dots.stderr == "..\n"
}

test test_cpio_append_adds_to_an_archive { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-append")?
  fp"{root}/one".write(b"1")?
  fp"{root}/two".write(b"22")?
  fp"{root}/three".write(b"333")?
  for format in ["newc", "odc", "bin", "crc"] {
    let name = f"{format}.cpio"
    let first = cpio(ctx, root, ["-o", "-H", format, "-O", name, "--quiet"], b"one\n")?
    assert first.status == 0, format
    let second = cpio(ctx, root, ["-oA", "-H", format, "-F", name, "--quiet"], b"two\nthree\n")?
    assert second.status == 0, format
    let listed = cpio(ctx, root, ["-it", "-F", name, "--quiet"])?
    assert listed.stdout == b"one\ntwo\nthree\n", format
    assert fp"{root}/{name}".read_bytes()?.len() % 512 == 0, format
  }
  let missing = cpio(ctx, root, ["-oA", "-O", "absent.cpio"], b"one\n")?
  assert missing.status == 2
  assert "Cannot open" in missing.stderr
  let unnamed = cpio(ctx, root, ["-oA"], b"one\n")?
  assert unnamed.status == 2
  assert "--append is used but no archive file name is given" in unnamed.stderr
}

test test_cpio_to_stdout_and_swapped_data { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-swap")?
  fp"{root}/word".write(b"abcdefgh")?
  let packed_archive = cpio(ctx, root, ["-o", "-H", "newc", "--quiet"], b"word\n")?
  let plain = cpio(ctx, root, ["-i", "--to-stdout", "--quiet"], packed_archive.stdout)?
  assert plain.stdout == b"abcdefgh"
  assert cpio(ctx, root, ["-i", "--to-stdout", "-s", "--quiet"], packed_archive.stdout)?.stdout == b"badcfehg"
  assert cpio(ctx, root, ["-i", "--to-stdout", "-S", "--quiet"], packed_archive.stdout)?.stdout == b"cdabghef"
  assert cpio(ctx, root, ["-i", "--to-stdout", "-b", "--quiet"], packed_archive.stdout)?.stdout == b"dcbahgfe"
  let named = cpio(ctx, root, ["-i", "--to-stdout", "--quiet", "nomatch"], packed_archive.stdout)?
  assert named.stdout == b""
  fp"{root}/seven".write(b"abcdefg")?
  let odd = cpio(ctx, root, ["-o", "-H", "newc", "--quiet"], b"seven\n")?
  let refused = cpio(ctx, root, ["-i", "--to-stdout", "-s", "--quiet"], odd.stdout)?
  assert refused.stdout == b"abcdefg"
  assert "cannot swap bytes of seven: odd number of bytes" in refused.stderr
  let out = test.temp_dir(ctx, name: "cpio-swap-out")?
  let _ = cpio(ctx, out, ["-id", "-b", "--quiet"], packed_archive.stdout)?
  assert fp"{out}/word".read_bytes()? == b"dcbahgfe"
}

test test_cpio_checks_crc_checksums { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-crc")?
  fp"{root}/data".write(b"checksummed")?
  let packed_archive = cpio(ctx, root, ["-o", "-H", "crc", "--quiet"], b"data\n")?
  let good = cpio(ctx, root, ["-i", "--only-verify-crc", "--quiet"], packed_archive.stdout)?
  assert good.status == 0
  assert good.stderr == ""
  let listed = cpio(ctx, root, ["-iv", "--only-verify-crc", "--quiet"], packed_archive.stdout)?
  assert listed.stderr == "data\n"
  var damaged = packed_archive.stdout
  # The data starts after the 110-byte header and the 5-byte name, padded to 4.
  let at = 116
  damaged = bytes.concat([damaged.slice(0, at), b"X", damaged.slice(at + 1, damaged.len() - at - 1)])
  let bad = cpio(ctx, root, ["-i", "--only-verify-crc", "--quiet"], damaged)?
  assert bad.status == 0
  assert "data: checksum error (0x" in bad.stderr
  assert "should be 0x" in bad.stderr
}

test test_cpio_usage_errors_and_exit_statuses { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-usage")?
  let none = cpio(ctx, root, [])?
  assert none.status == 2
  assert none.stderr == "cpio: You must specify one of -oipt options.\nTry 'cpio --help' or 'cpio --usage' for more information.\n"
  let twice = cpio(ctx, root, ["-io"])?
  assert twice.stderr.starts_with("cpio: Mode already defined\n")
  let unknown = cpio(ctx, root, ["-x"])?
  assert unknown.status == 64
  assert unknown.stderr.starts_with("cpio: invalid option -- 'x'\nTry ")
  let long_unknown = cpio(ctx, root, ["--bogus"])?
  assert long_unknown.status == 64
  assert long_unknown.stderr.starts_with("cpio: unrecognized option '--bogus'\n")
  let ambiguous = cpio(ctx, root, ["--ver"])?
  assert ambiguous.stderr.starts_with("cpio: option '--ver' is ambiguous; possibilities: '--verbose' '--version'\n")
  let missing_value = cpio(ctx, root, ["-i", "-F"])?
  assert missing_value.stderr.starts_with("cpio: option requires an argument -- 'F'\n")
  let bad_format = cpio(ctx, root, ["-o", "-H", "zip"])?
  assert bad_format.status == 2
  assert bad_format.stderr.starts_with("cpio: invalid archive format `zip'; valid formats are:\ncrc newc odc bin ustar tar (all-caps also recognized)\n")
  let meaningless = cpio(ctx, root, ["-o", "-d"])?
  assert meaningless.stderr.starts_with("cpio: --make-directories is meaningless with --create\n")
  let bad_block = cpio(ctx, root, ["-i", "--block-size=0"])?
  assert bad_block.stderr.starts_with("cpio: invalid block size\n")
  let help = cpio(ctx, root, ["--help"])?
  assert help.status == 0
  assert help.stdout.utf8()?.starts_with("Usage: cpio [OPTION...] [destination-directory]\n")
  let version = cpio(ctx, root, ["--version"])?
  assert version.stdout.utf8()?.starts_with("cpio (")
}

test test_cpio_reports_bad_archives { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-bad")?
  let garbage = cpio(ctx, root, ["-t"], b"not an packed_archive at all\n")?
  assert garbage.status == 2
  assert garbage.stderr == "cpio: premature end of archive\n"
  let empty = cpio(ctx, root, ["-t"], b"")?
  assert empty.stderr == "cpio: premature end of archive\n"
  let cut = cpio(ctx, root, ["-t"], GNU_NEWC.slice(0, 200))?
  assert cut.status == 2
  assert cut.stdout == b"dir\n"
  assert cut.stderr == "cpio: premature end of file\n"
  let junk = cpio(ctx, root, ["-t", "--quiet"], bytes.concat([b"JUNK", GNU_ODC]))?
  assert junk.status == 0
  assert junk.stderr == "cpio: warning: skipped 4 bytes of junk\n"
  assert junk.stdout == bytes.from_text(NAMES)
  let missing = cpio(ctx, root, ["-it", "-I", "nowhere.cpio"])?
  assert missing.status == 2
  assert missing.stderr == "cpio: Cannot open nowhere.cpio: No such file or directory\n"
  let absent = cpio(ctx, root, ["-o", "--quiet"], b"nonexistent\n\nf\n")?
  assert absent.status == 2
  assert "cpio: nonexistent: Cannot stat: No such file or directory\n" in absent.stderr
  assert "cpio: blank line ignored\n" in absent.stderr
}

# Swaps the bytes of each 16-bit word in data[from, from + length).
pure swap_words(data: Bytes, from: Int, length: Int) -> Bytes {
  var out: List[Int] = []
  for index in range(data.len()) {
    out += [data.byte_at(index) ?? 0]
  }
  var word = from
  while word < from + length {
    out[word] = data.byte_at(word + 1) ?? 0
    out[word + 1] = data.byte_at(word) ?? 0
    word += 2
  }
  bytes.from_ints(out) ?? data
}

test test_cpio_reads_reversed_byte_order_binary_archives { |ctx|
  let root = test.temp_dir(ctx, name: "cpio-reverse")?
  fp"{root}/ab".write(b"wxyz")?
  let native = cpio(ctx, root, ["-o", "-H", "bin", "--quiet"], b"ab\n")?
  # Two 26-byte headers (file and trailer) with a 4-byte name and 4 bytes of
  # data between them, as written by a machine of the other byte order.
  let first = swap_words(native.stdout, 0, 26)
  let reversed = swap_words(first, 34, 26)
  let listed = cpio(ctx, root, ["-it", "--quiet"], reversed)?
  assert listed.status == 0
  assert listed.stdout == b"ab\n"
  assert listed.stderr == "cpio: warning: archive header has reverse byte-order\n"
  let out = test.temp_dir(ctx, name: "cpio-reverse-out")?
  let ran = cpio(ctx, out, ["-id", "--quiet"], reversed)?
  assert ran.status == 0
  assert fp"{out}/ab".read_bytes()? == b"wxyz"
}
