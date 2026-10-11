use support.uu as uu

# origin: uutils test_realpath::test_realpath_current_directory
test test_uu_realpath_realpath_current_directory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", ["."])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{s.root}\n")
}

# origin: uutils test_realpath::test_realpath_long_redirection_to_current_dir
test test_uu_realpath_realpath_long_redirection_to_current_dir { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", [["." for _ in range(128)].join("/")])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{s.root}\n")
}

# origin: uutils test_realpath::test_realpath_long_redirection_to_root
test test_uu_realpath_realpath_long_redirection_to_root { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", [[".." for _ in range(85)].join("/")])?
  uu.succeeds(r)
  uu.stdout_is(r, "/\n")
}

# origin: uutils test_realpath::test_realpath_file_and_links
test test_uu_realpath_realpath_file_and_links { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, f"{s.root}/foo", "bar")?
  for option in [""] {
    for name in ["foo", "bar"] {
      let args = [name]
      let r = uu.invoke(s, "realpath", args)?
      uu.succeeds(r)
      let expected = "foo"
      uu.stdout_contains(r, f"{expected}\n")
    }
  }
}

# origin: uutils test_realpath::test_realpath_file_and_links_zero
test test_uu_realpath_realpath_file_and_links_zero { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, f"{s.root}/foo", "bar")?
  for option in [""] {
    for name in ["foo", "bar"] {
      let args = [name].extend(["-z"])
      let r = uu.invoke(s, "realpath", args)?
      uu.succeeds(r)
      let expected = "foo"
      uu.stdout_contains(r, f"{expected}\u{0}")
    }
  }
}

# origin: uutils test_realpath::test_realpath_file_and_links_strip
test test_uu_realpath_realpath_file_and_links_strip { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, f"{s.root}/foo", "bar")?
  for option in ["-s", "--strip", "--no-symlinks"] {
    for name in ["foo", "bar"] {
      let args = [name].extend([option])
      let r = uu.invoke(s, "realpath", args)?
      uu.succeeds(r)
      let expected = name
      uu.stdout_contains(r, f"{expected}\n")
    }
  }
}

# origin: uutils test_realpath::test_realpath_file_and_links_strip_zero
test test_uu_realpath_realpath_file_and_links_strip_zero { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "foo")?
  uu.symlink(s, f"{s.root}/foo", "bar")?
  for option in ["-s", "--strip", "--no-symlinks"] {
    for name in ["foo", "bar"] {
      let args = [name].extend([option]).extend(["-z"])
      let r = uu.invoke(s, "realpath", args)?
      uu.succeeds(r)
      let expected = name
      uu.stdout_contains(r, f"{expected}\u{0}")
    }
  }
}

# origin: uutils test_realpath::test_realpath_physical_mode
test test_uu_realpath_realpath_physical_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir2/bar")?
  uu.symlink(s, f"{s.root}/dir2/bar", "dir1/foo")?
  let r = uu.invoke(s, "realpath", ["dir1/foo/.."])?
  uu.succeeds(r)
  uu.stdout_contains(r, "dir2\n")
}

# origin: uutils test_realpath::test_realpath_logical_mode
test test_uu_realpath_realpath_logical_mode { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.mkdir(s, "dir2")?
  uu.symlink(s, f"{s.root}/dir2", "dir1/foo")?
  let r = uu.invoke(s, "realpath", ["-L", "dir1/foo/.."])?
  uu.succeeds(r)
  uu.stdout_contains(r, "dir1\n")
}

# origin: uutils test_realpath::test_realpath_dangling
test test_uu_realpath_realpath_dangling { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, f"{s.root}/nonexistent-file", "link")?
  let r = uu.invoke(s, "realpath", ["link"])?
  uu.succeeds(r)
  uu.stdout_is(r, f"{s.root}/nonexistent-file\n")
}

# origin: uutils test_realpath::test_realpath_loop
test test_uu_realpath_realpath_loop { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, f"{s.root}/2", "1")?
  uu.symlink(s, f"{s.root}/3", "2")?
  uu.symlink(s, f"{s.root}/1", "3")?
  let r = uu.invoke(s, "realpath", ["1"])?
  uu.fails(r)
  uu.stderr_contains(r, "Too many levels of symbolic links")
}



# origin: uutils test_realpath::test_realpath_default_allows_final_non_existent
test test_uu_realpath_realpath_default_allows_final_non_existent { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", ["supercalifragilisticexpialidocious"])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root}/supercalifragilisticexpialidocious\n")
}

# origin: uutils test_realpath::test_realpath_default_forbids_non_final_non_existent
test test_uu_realpath_realpath_default_forbids_non_final_non_existent { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", ["supercalifragilisticexpialidocious/supercalifragilisticexpialidocious"])?
  uu.fails(r)
}

# origin: uutils test_realpath::test_realpath_existing
test test_uu_realpath_realpath_existing { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", ["-e", "."])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root}\n")
}

# origin: uutils test_realpath::test_realpath_existing_error
test test_uu_realpath_realpath_existing_error { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", ["-e", "supercalifragilisticexpialidocious"])?
  uu.fails(r)
}

# origin: uutils test_realpath::test_realpath_existing_error_quiet
test test_uu_realpath_realpath_existing_error_quiet { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", ["-q", "-e", "supercalifragilisticexpialidocious"])?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: uutils test_realpath::test_realpath_missing
test test_uu_realpath_realpath_missing { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", ["-m", "supercalifragilisticexpialidocious/supercalifragilisticexpialidocious"])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{s.root}/supercalifragilisticexpialidocious/supercalifragilisticexpialidocious\n")
}

# origin: uutils test_realpath::test_realpath_empty
test test_uu_realpath_realpath_empty { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", [])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_realpath::test_realpath_when_symlink_is_absolute_and_enoent
test test_uu_realpath_realpath_when_symlink_is_absolute_and_enoent { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir2/bar")?
  uu.mkdir(s, "dir1")?
  uu.symlink(s, f"{s.root}/dir2/bar", "dir1/foo1")?
  uu.symlink(s, "/dir2/bar", "dir1/foo2")?
  uu.symlink(s, "../dir2/baz", "dir1/foo3")?
  let r = uu.invoke(s, "realpath", ["dir1/foo1", "dir1/foo2", "dir1/foo3"])?
  uu.fails(r)
  uu.stdout_contains(r, "/dir2/bar\n")
  uu.stdout_contains(r, "/dir2/baz\n")
  uu.stderr_is(r, "realpath: dir1/foo2: No such file or directory\n")
}

# origin: uutils test_realpath::test_realpath_when_symlink_part_is_missing
test test_uu_realpath_realpath_when_symlink_part_is_missing { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir2")?
  uu.touch(s, "dir2/bar")?
  uu.mkdir(s, "dir1")?
  uu.symlink(s, "../dir2/bar", "dir1/foo1")?
  uu.symlink(s, "dir2/bar", "dir1/foo2")?
  uu.symlink(s, "../dir2/baz", "dir1/foo3")?
  uu.symlink(s, f"{s.root}/dir3/bar", "dir1/foo4")?
  let r = uu.invoke(s, "realpath", ["dir1/foo1", "dir1/foo2", "dir1/foo3", "dir1/foo4"])?
  uu.fails(r)
  uu.stdout_contains(r, "dir2/bar\n")
  uu.stdout_contains(r, "dir2/baz\n")
  uu.stderr_contains(r, "realpath: dir1/foo2: No such file or directory\n")
  uu.stderr_contains(r, "realpath: dir1/foo4: No such file or directory\n")
}

# origin: uutils test_realpath::test_relative_existing_require_directories
test test_uu_realpath_relative_existing_require_directories { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.touch(s, "dir1/f")?
  let r = uu.invoke(s, "realpath", ["-e", "--relative-base=.", "--relative-to=dir1/f", "."])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "directory")
}

# origin: uutils test_realpath::test_relative_existing_require_directories_2
test test_uu_realpath_relative_existing_require_directories_2 { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir1")?
  uu.touch(s, "dir1/f")?
  let r = uu.invoke(s, "realpath", ["-e", "--relative-base=.", "--relative-to=dir1", "."])?
  uu.succeeds(r)
  uu.stdout_is(r, "..\n")
}

# origin: uutils test_realpath::test_relative_base_not_prefix_of_relative_to
test test_uu_realpath_relative_base_not_prefix_of_relative_to { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "realpath", ["-sm", "--relative-base=/usr/local", "--relative-to=/usr", "/usr", "/usr/local"])?
  uu.succeeds(r)
  uu.stdout_is(r, "/usr\n/usr/local\n")
}

# origin: uutils test_realpath::test_relative_string_handling
test test_uu_realpath_relative_string_handling { |ctx|
  let s = uu.scene(ctx)?
  for item in [{to: "prefix", path: "prefixed/1", expected: "../prefixed/1\n"}, {to: "prefixed", path: "prefix/1", expected: "../prefix/1\n"}, {to: "prefixed", path: "prefixed/1", expected: "1\n"}] {
    let r = uu.invoke(s, "realpath", ["-m", f"--relative-to={item.to}", item.path])?
    uu.succeeds(r)
    uu.stdout_is(r, item.expected)
  }
}

# origin: uutils test_realpath::test_relative
test test_uu_realpath_relative { |ctx|
  let s = uu.scene(ctx)?
  for args in [["-sm", "--relative-base=/usr", "--relative-to=/usr", "/tmp", "/usr"], ["-sm", "--relative-base=/usr", "/tmp", "/usr"]] {
    let r = uu.invoke(s, "realpath", args)?
    uu.succeeds(r)
    uu.stdout_is(r, "/tmp\n.\n")
  }
  for args in [["-sm", "--relative-base=/", "--relative-to=/", "/", "/usr"], ["-sm", "--relative-base=/", "/", "/usr"]] {
    let r = uu.invoke(s, "realpath", args)?
    uu.succeeds(r)
    uu.stdout_is(r, ".\nusr\n")
  }
}

# origin: uutils test_realpath::test_realpath_trailing_slash
test test_uu_realpath_realpath_trailing_slash { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")?
  uu.mkdir(s, "dir")?
  uu.symlink(s, "file", "link_file")?
  uu.symlink(s, "dir", "link_dir")?
  uu.symlink(s, "no_dir", "link_no_dir")?
  for mode in [[], ["-e"], ["-m"]] {
    for name in ["link_file", "link_file/", "link_dir", "link_dir/", "link_no_dir", "link_no_dir/"] {
      let r = uu.invoke(s, "realpath", mode.extend([name]))?
      if (name == "link_file/" and mode != ["-m"]) or (name.starts_with("link_no_dir") and mode == ["-e"]) {
        uu.fails_with_code(r, 1)
      } else {
        uu.succeeds(r)
        let target = if name.starts_with("link_file") { "file" } else if name.starts_with("link_no_dir") { "no_dir" } else { "dir" }
        uu.stdout_contains(r, f"/{target}\n")
      }
    }
  }
  for name in ["nonexistent/.", "nonexistent/./"] {
    let r = uu.invoke(s, "realpath", [name])?
    uu.fails(r)
    uu.stderr_contains(r, "No such file or directory\n")
  }
}

# origin: uutils test_realpath::test_realpath_trailing_slash_unreadable_directory
test test_uu_realpath_realpath_trailing_slash_unreadable_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.set_mode(s, "dir", 0o100)?
  for args in [["dir/"], ["-e", "dir/"]] {
    let r = uu.invoke(s, "realpath", args)?
    uu.succeeds(r)
    uu.stdout_contains(r, "/dir\n")
  }
  uu.set_mode(s, "dir", 0o700)?
}

# origin: uutils test_realpath::test_realpath_non_utf8_paths
test test_uu_realpath_realpath_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let name = b"test_\xff\xfe.txt"
  let file = uu.at_bytes(s, name)?
  file.write(b"")?
  let r = uu.invoke_paths(s, "realpath", [Path.parse_bytes(name)?])?
  uu.succeeds(r)
  assert bytes.from_text("test_") in r.stdout
  assert bytes.from_text(".txt") in r.stdout
}

# origin: uutils test_realpath::test_realpath_empty_string
test test_uu_realpath_realpath_empty_string { |ctx|
  let s = uu.scene(ctx)?
  for args in [[""], ["--relative-base=", "--relative-to=.", "."], ["--relative-to=", "."]] {
    let r = uu.invoke(s, "realpath", args)?
    uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_realpath::test_realpath_canonicalize_options
test test_uu_realpath_realpath_canonicalize_options { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "existing_dir")?
  for args in [[], ["-E"], ["--canonicalize"]] {
    let r = uu.invoke(s, "realpath", args.extend(["existing_dir/nonexistent"]))?
    uu.succeeds(r)
    uu.stdout_contains(r, "existing_dir/nonexistent")
  }
}

# origin: uutils test_realpath::test_realpath_canonicalize_vs_existing
test test_uu_realpath_realpath_canonicalize_vs_existing { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "existing_dir")?
  for args in [["-E"], ["-e"], ["-e", "-E"]] {
    let r = uu.invoke(s, "realpath", args.extend(["existing_dir/nonexistent"]))?
    if args == ["-e"] {
      uu.fails(r)
    } else {
      uu.succeeds(r)
      uu.stdout_contains(r, "existing_dir/nonexistent")
    }
  }
}
