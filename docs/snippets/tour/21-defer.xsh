proc rotate(dir: Path) [fs, error] {
  let lock = fp"{dir}/.rotate.lock"
  lock.write("locked\n")?
  defer {
    lock.remove()?
    print "released lock"
  }

  let staging = fp"{dir}/staging"
  staging.mkdir()?
  defer {
    staging.remove_dir()?
    print "removed staging"
  }

  print "rotating"
  let _ = fp"{dir}/missing.log".read_text()?
  print "never reached"
}

let scratch = fs.tempdir()?
defer scratch.close()?
let dir = scratch.host_path()?

match rotate(dir) {
  Ok(_) => print "rotated"
  Err(_) => print "rotate failed"
}

print f"left behind: {fs.children(dir)? |> count()}"
