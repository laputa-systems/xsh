test test_tar_create_list_extract { |ctx|
  let root = test.temp_dir(ctx, name: "tar-src")?
  fp"{root}/file.txt".write("tar payload")
  fp"{root}/other.txt".write("other payload")
  let tarball = test.temp_path(ctx, name: "archive.tar")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root .
  let listed = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -tf $tarball
  assert "file.txt" in listed
  let filtered = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -tf $tarball file.txt
  assert "file.txt" in filtered
  assert ! ("other.txt" in filtered)
  let out = test.temp_dir(ctx, name: "tar-out")?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out
  assert "tar payload" in fp"{out}/file.txt".read_text()?
  let selected = test.temp_dir(ctx, name: "tar-selected")?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xf $tarball -C $selected file.txt
  assert "tar payload" in fp"{selected}/file.txt".read_text()?
  assert ! fp"{selected}/other.txt".exists()?
  let err = test.temp_path(ctx, name: "tar.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out 2> $err
  assert ! status.exited_with(0)
  assert "destination exists" in err.read_text()?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- --overwrite -xf $tarball -C $out
}

test test_tar_bundled_letters_without_dash_and_stdin_archive { |ctx|
  let root = test.temp_dir(ctx, name: "tar-bundled")?
  fp"{root}/one.txt".write("one")
  let tarball = test.temp_path(ctx, name: "bundled.tar")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- cf $tarball -C $root one.txt
  let listed_text = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- tf $tarball
  assert listed_text == "one.txt\n"
  let piped_text = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- tf - < $tarball
  assert piped_text == "one.txt\n"
}

test test_tar_extract_to_stdout_writes_regular_file_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "tar-stdout-src")?
  fp"{root}/data.txt".write("payload\n")
  let tarball = test.temp_path(ctx, name: "stdout.tar")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root data.txt
  let listed_text = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xOf $tarball
  assert listed_text == "payload\n"
}

test test_tar_exclude_list_skips_named_members_and_their_subtree { |ctx|
  let root = test.temp_dir(ctx, name: "tar-exclude-src")?
  fp"{root}/a.txt".write("a")
  fp"{root}/b.txt".write("b")
  let sub = fp"{root}/sub"
  sub.mkdir()?
  fp"{sub}/c.txt".write("c")
  let tarball = test.temp_path(ctx, name: "exclude.tar")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root .
  let exclude = test.temp_path(ctx, name: "exclude.list")
  exclude.write("b.txt\nsub\n")
  let out = test.temp_dir(ctx, name: "tar-exclude-out")?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out -X $exclude
  assert fp"{out}/a.txt".exists()?
  assert ! fp"{out}/b.txt".exists()?
  assert ! fp"{out}/sub".exists()?
  let listed = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -tf $tarball -X $exclude
  assert "a.txt" in listed
  assert ! ("b.txt" in listed)
  assert ! ("c.txt" in listed)
}

test test_tar_empty_input_is_short_read_and_zero_blocks_are_empty { |ctx|
  let empty = test.temp_path(ctx, name: "empty.tar")
  empty.write(b"")?
  let err = test.temp_path(ctx, name: "empty.err")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xf $empty 2> $err
  assert status.exited_with(1)
  assert err.read_text()? == "tar: short read\n"
  let zeros = test.temp_path(ctx, name: "zeros.tar")
  zeros.write(bytes.zero(1024)?)?
  let listed_text = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -tf $zeros
  assert listed_text == ""
}

test test_tar_missing_member_and_mode_errors { |ctx|
  let root = test.temp_dir(ctx, name: "tar-errors-src")?
  fp"{root}/one.txt".write("one")
  let tarball = test.temp_path(ctx, name: "errors.tar")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root one.txt
  let err = test.temp_path(ctx, name: "errors.err")
  let missing = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xf $tarball nothere 2> $err
  assert missing.exited_with(1)
  assert err.read_text()? == "tar: nothere: Not found in archive\n"
  let two = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -tx -f $tarball 2> $err
  assert two.exited_with(1)
  assert "You may not specify more than one" in err.read_text()?
  let none = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -f $tarball 2> $err
  assert none.exited_with(1)
  assert "You must specify one of" in err.read_text()?
}

test test_tar_keep_and_strip_components_on_extract { |ctx|
  let root = test.temp_dir(ctx, name: "tar-keep-src")?
  let sub = fp"{root}/top"
  sub.mkdir()?
  fp"{sub}/inner.txt".write("inner")
  let tarball = test.temp_path(ctx, name: "keep.tar")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root top
  let out = test.temp_dir(ctx, name: "tar-keep-out")?
  fp"{out}/inner.txt".write("kept")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xkf $tarball -C $out --strip-components=1
  assert fp"{out}/inner.txt".read_text()? == "kept"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out --strip-components=1 --overwrite
  assert fp"{out}/inner.txt".read_text()? == "inner"
}

test test_tar_gzip_create_list_and_autodetect_extract { |ctx|
  let root = test.temp_dir(ctx, name: "tar-gz-src")?
  fp"{root}/zipped.txt".write("zipped")
  let gz_archive = test.temp_path(ctx, name: "archive.tar.gz")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -czf $gz_archive -C $root zipped.txt
  let magic = gz_archive.read_bytes()?.slice(0, 2)
  assert magic == b"\x1f\x8b"
  let listed_text = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -tzf $gz_archive
  assert listed_text == "zipped.txt\n"
  let out = test.temp_dir(ctx, name: "tar-gz-out")?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xf $gz_archive -C $out
  assert fp"{out}/zipped.txt".read_text()? == "zipped"
}

test test_tar_verbose_listing_shows_mode_size_time_and_link_target { |ctx|
  let root = test.temp_dir(ctx, name: "tar-verbose-src")?
  fp"{root}/data.txt".write("12345")
  fp"{root}/link".symlink(to: p"data.txt")?
  let tarball = test.temp_path(ctx, name: "verbose.tar")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root data.txt link
  let listed = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -tvf $tarball
  let lines = listed.split("\n")
  assert lines[0].starts_with("-rw")
  assert lines[0].ends_with(" data.txt")
  assert "        5 " in lines[0]
  assert lines[1].starts_with("lrwxrwxrwx ")
  assert lines[1].ends_with(" link -> data.txt")
}

test test_tar_long_names_split_into_ustar_prefix { |ctx|
  let root = test.temp_dir(ctx, name: "tar-long-src")?
  let directory = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
  let leaf = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee.txt"
  fp"{root}/{directory}".mkdir()?
  fp"{root}/{directory}/{leaf}".write("long")
  let tarball = test.temp_path(ctx, name: "long.tar")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -cf $tarball -C $root $directory
  let listed_text = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -tf $tarball
  assert listed_text == f"{directory}/\n{directory}/{leaf}\n"
  let out = test.temp_dir(ctx, name: "tar-long-out")?
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/tar.xsh" -- -xf $tarball -C $out
  assert fp"{out}/{directory}/{leaf}".read_text()? == "long"
}
