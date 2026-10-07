#!/bin/xsh
use lib.gnu
use lib.perm

proc main(...argv: List[Bytes]) [fs, error, process, env, io] {
  let opts = perm.options(argv)?
  if ! opts.help and ! opts.version and opts.reference == null and ! opts.operands.is_empty() {
    if let Ok(spec) = opts.operands[0].utf8() {
      if rx"^[ \t\n\u{b}\u{c}\r]*\+?[0-9]+:$".matches(spec) {
        gnu.error(f"invalid spec: {gnu.quote(spec)}")
        exit 1
      }
    }
  }
  perm.ownership(argv)
}
