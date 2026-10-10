#!/bin/xsh
use lib.gnu
use lib.perm

proc main(...argv: List[Bytes]) [fs, error, process, env, io] {
  let opts = perm.options(argv)?
  if ! opts.help and ! opts.version {
    if let spec = opts.from {
      if let Ok(text) = spec.utf8() {
        if let Err(failure) = perm.group_filter(text) {
          gnu.error(f"{failure.message}: {gnu.quote(text)}")
          exit 1
        }
      } else {
        gnu.error(f"invalid user: {gnu.quote_bytes(spec)}")
        exit 1
      }
    }
  }
  perm.ownership(argv, group_only: true)
}
