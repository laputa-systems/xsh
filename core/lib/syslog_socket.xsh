##! Publishes and retires a datagram log socket without ever touching an entry the
##! caller did not create.
##!
##! `publish` validates the parent directory once and then works only through that
##! directory's descriptor, so a path swapped afterwards cannot redirect it. It
##! binds under a private temporary name, makes the socket world writable and
##! credential-passing, and only then renames it into place with no-replace
##! semantics, so a client either finds a bound socket or nothing. A stale socket
##! left by a dead receiver is exchanged out; a live receiver, a symlink, a
##! directory or any other file stops the call untouched.

## A socket this process created and published. `dir` is the validated parent
## directory's descriptor, kept for `unpublish`.
export type Published = {fd: Int, dir: Int, name: Str, path: Path, dev: Int, ino: Int}

## Kernel receive buffer requested for the socket, in bytes.
export const RECEIVE_BUFFER = 262144
const WORLD_WRITABLE = 438
const GROUP_OR_WORLD_WRITABLE = 18
const ADDRESS_LIMIT = 107
const EEXIST = 17
const ENOENT = 2
const ECONNREFUSED = 111

pure refuse(message: Str) -> Error {
  error.failure(message)
}

pure anchored(dir: Int, name: Str) -> Path {
  Path(f"/proc/self/fd/{dir}/{name}")
}

proc inspect_parent(dir: Int, parent: Path) [fs, process, error] -> Result[Bool, Error] {
  let info = fs.stat(Path(f"/proc/self/fd/{dir}"), follow_symlinks: true)?
  let owner = unix.id()?.euid
  if info.kind != "dir" or (info.uid != 0 and info.uid != owner) or info.mode.bit_and(GROUP_OR_WORLD_WRITABLE) != 0 {
    return Err(refuse(f"{parent} must be a directory owned by root or the service user and writable by no one else"))
  }
  Ok(true)
}

proc open_parent(parent: Path) [fs, process, error] -> Result[Int, Error] {
  let opened = unix.open_fd(parent, flags: ["directory", "nofollow"])
  if opened is Err(_) {
    return Err(refuse(f"{parent} is not a directory reachable without a final symlink"))
  }
  let dir = opened ?? -1
  match inspect_parent(dir, parent) {
    Ok(_) => Ok(dir)
    Err(failure) => {
      unix.close_fd(dir)?
      Err(failure)
    }
  }
}

proc discard(entry: Path) [fs, io] {
  match entry.unlink() {
    Ok(_) => {}
    Err(failure) => if failure.errno != ENOENT { { eprint f"syslogd: cannot remove {entry}: {failure.message}"; let _ = io.flush_stderr() } }
  }
}

proc open_receiver(temporary: Path) [fs, process, net, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_UNIX, c.SOCK_DGRAM.bit_or(c.SOCK_NONBLOCK))?
  linux.bind(fd, {family: "unix", address: temporary.display()})
  temporary.chmod(WORLD_WRITABLE)
  linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_PASSCRED, 1)
  linux.setsockopt_int(fd, c.SOL_SOCKET, c.SO_RCVBUF, RECEIVE_BUFFER)
  Ok(fd)
}

proc is_live(target: Path) [process, net, error] -> Result[Bool, Error] {
  let c = linux.net_constants()
  let probe = linux.socket(c.AF_UNIX, c.SOCK_DGRAM.bit_or(c.SOCK_NONBLOCK))?
  defer unix.close_fd(probe)
  match linux.connect(probe, {family: "unix", address: target.display()}) {
    Ok(_) => Ok(true)
    Err(failure) => if failure.errno == ECONNREFUSED { Ok(false) } else { Err(failure) }
  }
}

proc replace_stale(temporary: Path, target: Path, shown: Path) [fs, process, net, io, error] -> Result[Unit, Error] {
  let seen = fs.stat(target)?
  if seen.kind != "socket" {
    return Err(refuse(f"{shown} exists and is a {seen.kind}, not a socket"))
  }
  if is_live(target)? { return Err(refuse(f"another receiver is listening on {shown}")) }
  fs.rename_exchange(temporary, target)?
  let displaced = fs.stat(temporary)?
  if displaced.dev != seen.dev or displaced.ino != seen.ino {
    fs.rename_exchange(temporary, target)?
    return Err(refuse(f"{shown} changed while it was being replaced"))
  }
  temporary.unlink()?
}

## Binds a nonblocking, credential-passing datagram socket and publishes it at
## `target`. The parent directory must be a real directory owned by root or the
## service user and writable by no one else. Returns the socket and the identity
## of the entry it created.
export proc publish(target: Path) [fs, process, net, io, time, error] -> Result[Published, Error] {
  let name = target.name()
  let dir = open_parent(target.parent())?
  var published = false
  defer { if ! published { unix.close_fd(dir) } }
  let boot = time.boottime()?
  let temporary = anchored(dir, f".{name}.{process.current_pid()?}.{boot}.tmp")
  let final = anchored(dir, name)
  if temporary.display().byte_len() > ADDRESS_LIMIT {
    return Err(refuse(f"the socket name {name} is too long for a socket address"))
  }
  let fd = open_receiver(temporary)?
  defer {
    if ! published {
      unix.close_fd(fd)
      discard(temporary)
    }
  }
  match fs.rename_noreplace(temporary, final) {
    Ok(_) => {}
    Err(failure) => {
      if failure.errno != EEXIST { return Err(failure) }
      replace_stale(temporary, final, target)?
    }
  }
  let info = fs.stat(final)?
  published = true
  Ok({fd: fd, dir: dir, name: name, path: target, dev: info.dev, ino: info.ino})
}

## Removes the published entry only if it is still the one `publish` created and
## releases the directory descriptor. Returns whether it removed the entry.
export proc unpublish(published: Published) [fs, process, error] -> Result[Bool, Error] {
  defer unix.close_fd(published.dir)
  let final = anchored(published.dir, published.name)
  let seen = fs.stat(final)
  if seen is Err(_) { return Ok(false) }
  let info = seen?
  if info.kind != "socket" or info.dev != published.dev or info.ino != published.ino { return Ok(false) }
  final.unlink()?
  Ok(true)
}
