use support.uu as uu

# origin: busybox dirname/dirname-handles-absolute-path
test test_bb_dirname_dirname_handles_absolute_path_fb8adf49 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["/foo/bar/baz"])?
  uu.succeeds(r)
  uu.stdout_only(r, "/foo/bar\n")
}

# origin: busybox dirname/dirname-handles-empty-path
test test_bb_dirname_dirname_handles_empty_path_ec301f60 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", [""])?
  uu.succeeds(r)
  uu.stdout_only(r, ".\n")
}

# origin: busybox dirname/dirname-handles-multiple-slashes
test test_bb_dirname_dirname_handles_multiple_slashes_094e2133 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["foo/bar///baz"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo/bar\n")
}

# origin: busybox dirname/dirname-handles-relative-path
test test_bb_dirname_dirname_handles_relative_path_7d6e27b1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["foo/bar/baz"])?
  uu.succeeds(r)
  uu.stdout_only(r, "foo/bar\n")
}

# origin: busybox dirname/dirname-handles-root
test test_bb_dirname_dirname_handles_root_2a128156 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["/"])?
  uu.succeeds(r)
  uu.stdout_only(r, "/\n")
}

# origin: busybox dirname/dirname-handles-single-component
test test_bb_dirname_dirname_handles_single_component_c49d2324 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "dirname", ["foo"])?
  uu.succeeds(r)
  uu.stdout_only(r, ".\n")
}

# origin: busybox dirname/dirname-works
test test_bb_dirname_dirname_works_73442aa2 { |ctx|
  let s = uu.scene(ctx)?
  let cwd = s.root.resolve()?
  let r = uu.invoke(s, "dirname", [cwd.display()])?
  uu.succeeds(r)
  uu.stdout_only(r, f"{cwd.parent()}\n")
}

