use support.uu

# origin: busybox tar/tar Empty file is not a tarball
test test_bb_tar_tar_Empty_file_is_not_a_tarball_c16ed8df { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tar", ["xvf", "-"])?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tar: short read\n")
  uu.no_stdout(r)
}

# origin: busybox tar/tar Twenty zeroed blocks is an empty tarball
test test_bb_tar_tar_Twenty_zeroed_blocks_is_an_empty_tarball_75170f9c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tar", ["xvf", "-"], stdin: bytes.concat([b"\0" for _ in range(10240)]))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: busybox tar/tar Two zeroed blocks is a ('truncated') empty tarball
test test_bb_tar_tar_Two_zeroed_blocks_is_a_truncated_empty_tarball_787da9cf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tar", ["xvf", "-"], stdin: bytes.concat([b"\0" for _ in range(1024)]))?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: busybox tar/tar-demands-at-least-one-ctx
test test_bb_tar_tar_demands_at_least_one_ctx_ac192449 { |ctx|
  let s = uu.scene(ctx)?
  uu.fails(uu.invoke(s, "tar", ["v"])?)
}

# origin: busybox tar/tar-demands-at-most-one-ctx
test test_bb_tar_tar_demands_at_most_one_ctx_bd24554d { |ctx|
  let s = uu.scene(ctx)?
  uu.fails(uu.invoke(s, "tar", ["tx"])?)
}

# origin: busybox tar/tar-archives-multiple-files
test test_bb_tar_tar_archives_multiple_files_5fceaeb8 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.touch(s, "bar")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo", "bar"])?)
  uu.remove(s, "foo")?
  uu.remove(s, "bar")?
  uu.succeeds(uu.invoke(s, "tar", ["xf", "foo.tar"])?)
  assert uu.file_exists(s, "foo")?
  assert uu.file_exists(s, "bar")?
}

# origin: busybox tar/tar-extracts-file
test test_bb_tar_tar_extracts_file_06ab1ab8 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo"])?)
  uu.remove(s, "foo")?
  uu.succeeds(uu.invoke(s, "tar", ["xf", "foo.tar"])?)
  assert uu.file_exists(s, "foo")?
}

# origin: busybox tar/tar-extracts-multiple-files
test test_bb_tar_tar_extracts_multiple_files_2d3e594c { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.touch(s, "bar")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo", "bar"])?)
  uu.remove(s, "foo")?
  uu.remove(s, "bar")?
  uu.succeeds(uu.invoke(s, "tar", ["-xf", "foo.tar"])?)
  assert uu.file_exists(s, "foo")?
  assert uu.file_exists(s, "bar")?
}

# origin: busybox tar/tar-complains-about-missing-file
test test_bb_tar_tar_complains_about_missing_file_c9ba03e1 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo"])?)
  uu.fails(uu.invoke(s, "tar", ["xf", "foo.tar", "bar"])?)
}

# origin: busybox tar/tar-extracts-from-standard-input
test test_bb_tar_tar_extracts_from_standard_input_71f3d9b7 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo"])?)
  uu.remove(s, "foo")?
  uu.succeeds(uu.invoke(s, "tar", ["x"], stdin: uu.read(s, "foo.tar")?)?)
  assert uu.file_exists(s, "foo")?
}

# origin: busybox tar/tar-extracts-to-standard-output
test test_bb_tar_tar_extracts_to_standard_output_2dd3ca8c { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo\n")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo"])?)
  let r = uu.invoke(s, "tar", ["Ox"], stdin: uu.read(s, "foo.tar")?)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "foo")?)
}

# origin: busybox tar/tar-handles-cz-options
test test_bb_tar_tar_handles_cz_options_b1520a3b { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "tar", ["czf", "foo.tar.gz", "foo"])?)
  uu.succeeds(uu.invoke(s, "gzip", ["-d", "foo.tar.gz"])?)
}

# origin: busybox tar/tar-handles-empty-include-and-non-empty-exclude-list
test test_bb_tar_tar_handles_empty_include_and_non_empty_exclude_list_3c019d40 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo"])?)
  uu.write(s, "foo.exclude", "foo\n")?
  uu.succeeds(uu.invoke(s, "tar", ["xf", "foo.tar", "-X", "foo.exclude"])?)
}

# origin: busybox tar/tar-handles-exclude-and-extract-lists
test test_bb_tar_tar_handles_exclude_and_extract_lists_3d9a781f { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.touch(s, "bar")?
  uu.touch(s, "baz")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo", "bar", "baz"])?)
  uu.write(s, "foo.exclude", "foo\n")?
  uu.remove(s, "foo")?
  uu.remove(s, "bar")?
  uu.remove(s, "baz")?
  uu.succeeds(uu.invoke(s, "tar", ["xf", "foo.tar", "foo", "bar", "-X", "foo.exclude"])?)
  assert ! uu.exists(s, "foo")?
  assert uu.file_exists(s, "bar")?
  assert ! uu.exists(s, "baz")?
}

# origin: busybox tar/tar-handles-multiple-X-options
test test_bb_tar_tar_handles_multiple_X_options_4f38cc7e { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.touch(s, "bar")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo", "bar"])?)
  uu.write(s, "foo.exclude", "foo\n")?
  uu.write(s, "bar.exclude", "bar\n")?
  uu.remove(s, "foo")?
  uu.remove(s, "bar")?
  uu.succeeds(uu.invoke(s, "tar", ["xf", "foo.tar", "-X", "foo.exclude", "-X", "bar.exclude"])?)
  assert ! uu.exists(s, "foo")?
  assert ! uu.exists(s, "bar")?
}

# origin: busybox tar/tar-handles-nested-exclude
test test_bb_tar_tar_handles_nested_exclude_d3adfc7b { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "foo")?
  uu.touch(s, "foo/bar")?
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "foo"])?)
  uu.remove(s, "foo")?
  uu.write(s, "foobar.exclude", "foo/bar\n")?
  uu.succeeds(uu.invoke(s, "tar", ["xf", "foo.tar", "foo", "-X", "foobar.exclude"])?)
  assert uu.dir_exists(s, "foo")?
  assert ! uu.exists(s, "foo/bar")?
}

# origin: busybox tar/tar-extracts-all-subdirs
test test_bb_tar_tar_extracts_all_subdirs_bdbd050d { |ctx|
  let s = uu.scene(ctx)?
  for name in ["foo/1/10/100", "foo/1/10/101", "foo/1/10/102", "foo/1/11", "foo/2", "foo/3"] { uu.mkdir(s, name)? }
  uu.succeeds(uu.invoke(s, "tar", ["cf", "foo.tar", "-C", "foo", "."])?)
  for name in ["foo/1", "foo/2", "foo/3"] { uu.remove(s, name)? }
  uu.succeeds(uu.invoke(s, "tar", ["xf", "foo.tar", "-C", "foo", "./1/10"])?)
  let names = fs.walk(uu.at(s, "foo"), hidden: true)? |> map { |entry| entry.path.relative_to(s.root).display() } |> sort() |> collect()
  assert names == ["foo", "foo/1", "foo/1/10", "foo/1/10/100", "foo/1/10/101", "foo/1/10/102"]
}
