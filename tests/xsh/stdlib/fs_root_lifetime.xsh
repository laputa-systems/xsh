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

proc write_root_marker(root: FsRoot, name: Str) [fs, error] {
  root.write(fp"{name}", name)
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
  var retained: FsRoot? = null
  for _ in range(1) {
    let opened = fs.tempdir()?
    retained = opened
  }

  let root = retained.require(FsRoot)?
  defer root.close()
  root.write(p"after-loop", "retained")
  assert root.read_text(p"after-loop")? == "retained"
}
