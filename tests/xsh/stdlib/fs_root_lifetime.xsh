proc root_from_result_match() [fs, io, process, error] -> FsRoot {
  let opened = fs.tempdir()
  match opened {
    Ok(root) => root
    Err(problem) => {
      eprint f"could not create temporary root: {problem.message}"
      exit 1
    }
  }
}

proc write_root_marker(root: FsRoot, name: RelPath) [fs, error] {
  root.write(name, name.display())
}

test test_fs_root_returned_from_match_stays_live_through_borrowed_calls {
  let root = root_from_result_match()
  defer root.close()

  write_root_marker(root, "first")
  write_root_marker(root, "second")
  assert root.read_text(p"first")? == "first"
  assert root.read_text(p"second")? == "second"
}

test test_fs_root_assigned_to_outer_optional_survives_loop_scope {
  var retained = null
  repeat 1 times {
    let opened = fs.tempdir()?
    retained = opened
  }

  let root = retained.require(FsRoot)?
  defer root.close()
  root.write(p"after-loop", "retained")
  assert root.read_text(p"after-loop")? == "retained"
}

proc replace_temp_root_path(owned: FsRoot, base: Path) [fs, error] -> Result[Path] {
  let cached = owned.host_path()?
  owned.write(p"owned-marker", "original")?
  fs.rename_noreplace(cached, fp"{base}/moved")?
  cached.mkdir()?
  fp"{cached}/sentinel".write("replacement")?
  cached
}

test test_temp_root_close_spares_a_replacement_at_its_old_path { |ctx|
  let base = test.temp_dir(ctx, name: "temp-root-replaced")?
  let owned = fs.tempdir_in(base)?
  let cached = replace_temp_root_path(owned, base)?
  test.error_kind(owned.close(), "fs-root")
  assert fp"{cached}/sentinel".read_text()? == "replacement"
  assert fp"{base}/moved/owned-marker".read_text()? == "original"
}

proc release_replaced_temp_root(base: Path) [fs, error] -> Result[Path] {
  let owned = fs.tempdir_in(base)?
  replace_temp_root_path(owned, base)?
}

test test_temp_root_scope_exit_spares_a_replacement_at_its_old_path { |ctx|
  let base = test.temp_dir(ctx, name: "temp-root-scope-replaced")?
  let cached = release_replaced_temp_root(base)?
  assert fp"{cached}/sentinel".read_text()? == "replacement"
  assert fp"{base}/moved/owned-marker".read_text()? == "original"
}

test test_temp_root_close_removes_its_directory { |ctx|
  let base = test.temp_dir(ctx, name: "temp-root-removed")?
  let owned = fs.tempdir_in(base)?
  let location = owned.host_path()?
  owned.write(p"nested-marker", "owned")?
  owned.close()?
  assert ! location.exists()?

  let emptied = fs.tempdir_in(base)?
  emptied.host_path()?.remove()?
  emptied.close()?
}
