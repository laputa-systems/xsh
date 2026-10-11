##! Permission and ownership argument handling shared by core applets.
use gnu

## Invalid ownership argument.
export error PermError = Invalid : Usage
## Null IDs leave the corresponding ownership field unchanged.
export type Owner = {uid: Int?, gid: Int?}
## Parsed permission options with independent link policies.
export type Options = {
  recursive: Bool, quiet: Bool, verbosity: Str, traversal: Str,
  dereference: Bool, preserve_root: Bool, reference: Bytes?, from: Bytes?,
  operands: List[Bytes], option_like_mode: Bool, help: Bool, version: Bool,
}

pure find_byte(data: Bytes, wanted: Int) -> Int? {
  for at in range(data.len()) { if data.byte_at(at) == wanted { return at } }
  null
}

# Traversal and dereferencing are independent: -H/-L select directory descent,
# while -h selects whether the ownership syscall acts on the link itself.
## Parse ordered GNU permission options.
export proc options(argv: List[Bytes], chmod = false) [process, env] -> Result[Options, Error] {
  var recursive = false
  var quiet = false
  var verbosity = ""
  var traversal = if chmod { "H" } else { "P" }
  var dereference = true
  var explicit_dereference = false
  var preserve_root = false
  var reference: Bytes? = null
  var from: Bytes? = null
  var operands: List[Bytes] = []
  var option_mode: Str? = null
  var stopped = false
  var at = 0
  while at < argv.len() {
    let raw = argv[at]
    var word = raw.utf8() ?? ""
    if ! stopped and raw.utf8() is Err(_) {
      if raw.starts_with(b"--") {
        if let equal = find_byte(raw, 61) {
          let name = raw[0..equal].utf8() ?? ""
          if name == "--reference" {
            reference = raw[equal + 1..]
            at += 1
            continue
          }
        }
        gnu.usage_error(f"unrecognized option {gnu.quote_bytes(raw)}")
      } else if raw.starts_with(b"-") {
        gnu.usage_error(f"invalid option {gnu.quote_bytes(raw)}")
      }
    }
    if ! stopped and word.starts_with("--") and word != "--" {
      let equal = word.find("=")
      let name = if let split = equal { word.byte_slice(0, split) } else { word }
      let known = ["--recursive", "--silent", "--quiet", "--verbose", "--changes", "--dereference", "--no-dereference", "--preserve-root", "--no-preserve-root", "--reference", "--help", "--version"]
      let choices = if chmod { known } else { known + ["--from"] }
      let matches = collect { for option in choices { yield option when option.starts_with(name) } }
      if ! (name in choices) and matches.len() == 1 {
        word = f"{matches[0]}{if let split = equal { word.byte_slice(split) } else { "" }}"
      } else if ! (name in choices) and matches.len() > 1 {
        gnu.usage_error(f"option {gnu.quote(word)} is ambiguous")
      }
    }
    at += 1
    if stopped or word == "-" or ! word.starts_with("-") {
      operands += [raw]
      if env.get("POSIXLY_CORRECT") is Ok(_) { stopped = true }
    } else if word == "--" {
      stopped = true
    } else if chmod and rx"^-[rwxXstugoa0-7,+-=]+$".matches(word) {
      option_mode = if let previous = option_mode { f"{previous},{word}" } else { word }
    } else if word == "--help" {
      return {recursive: recursive, quiet: quiet, verbosity: verbosity, traversal: traversal, dereference: dereference, preserve_root: preserve_root, reference: reference, from: from, operands: operands, option_like_mode: option_mode != null, help: true, version: false}
    } else if word == "--version" {
      return {recursive: recursive, quiet: quiet, verbosity: verbosity, traversal: traversal, dereference: dereference, preserve_root: preserve_root, reference: reference, from: from, operands: operands, option_like_mode: option_mode != null, help: false, version: true}
    } else if word == "--recursive" {
      recursive = true
    } else if word == "--silent" or word == "--quiet" {
      quiet = true
    } else if word == "--verbose" {
      verbosity = "verbose"
    } else if word == "--changes" {
      verbosity = "changes"
    } else if word == "--dereference" {
      dereference = true
      explicit_dereference = true
    } else if word == "--no-dereference" {
      dereference = false
      explicit_dereference = true
    } else if word == "--preserve-root" {
      preserve_root = true
    } else if word == "--no-preserve-root" {
      preserve_root = false
    } else if word == "--reference" or word.starts_with("--reference=") or (! chmod and (word == "--from" or word.starts_with("--from="))) {
      var value: Bytes = b""
      if let equal = word.find("=") {
        value = bytes.from_text(word.byte_slice(equal + 1))
      } else {
        if at >= argv.len() {
          gnu.usage_error(f"option {gnu.quote(word)} requires an argument")
        }
        value = argv[at]
        at += 1
      }
      if word.starts_with("--reference") { reference = value } else { from = value }
    } else if word.starts_with("--") {
      gnu.usage_error(f"unrecognized option {gnu.quote(word)}")
    } else {
      for ch in word.byte_slice(1) {
        match ch {
          "R" => recursive = true
          "f" => quiet = true
          "v" => verbosity = "verbose"
          "c" => verbosity = "changes"
          "H" => traversal = "H"
          "L" => traversal = "L"
          "P" => traversal = "P"
          "h" => { dereference = false; explicit_dereference = true }
          else => gnu.usage_error(f"invalid option -- '{ch}'")
        }
      }
    }
  }
  if let mode = option_mode { operands = [bytes.from_text(mode)] + operands }
  if recursive and traversal == "P" {
    if explicit_dereference and dereference {
      gnu.usage_error("-R --dereference requires -H or -L")
    }
    dereference = false
  }
  {recursive: recursive, quiet: quiet, verbosity: verbosity, traversal: traversal, dereference: dereference, preserve_root: preserve_root, reference: reference, from: from, operands: operands, option_like_mode: option_mode != null, help: false, version: false}
}

## Resolve a name or decimal user ID without requiring an account for numeric IDs.
export proc uid(raw: Str) [fs, error] -> Result[Int, Error] {
  if ! raw.starts_with("+") { if let Ok(account) = user.lookup(raw) { return account.uid } }
  if rx"^[ \t\n\u{b}\u{c}\r]*\+?[0-9]+$".matches(raw) {
    let value = raw.trim().parse_int()?
    return Err(PermError.Invalid("invalid user")) when value < 0 or value >= 4294967295
    return value
  }
  user.lookup(raw)?.uid
}

## Resolve a name or decimal group ID.
export proc gid(raw: Str) [fs, error] -> Result[Int, Error] {
  if ! raw.starts_with("+") { if let Ok(account) = group.lookup(raw) { return account.gid } }
  if rx"^[ \t\n\u{b}\u{c}\r]*\+?[0-9]+$".matches(raw) {
    let value = raw.trim().parse_int()?
    return Err(PermError.Invalid("invalid group")) when value < 0 or value >= 4294967295
    return value
  }
  group.lookup(raw)?.gid
}

## Parse owner and optional group; a trailing colon selects the login group.
export proc owner(raw: Str, group_only = false) [fs, error, process] -> Result[Owner, Error] {
  if group_only {
    if raw == "" { return {uid: null, gid: null} }
    return {uid: null, gid: gid(raw)?}
  }
  let spec = raw
  if ! (":" in raw) and "." in raw and user.lookup(raw) is Err(_) {
    let separator = raw.split(".")[0].byte_len()
    let legacy = f"{raw.byte_slice(0, separator)}:{raw.byte_slice(separator + 1)}"
    if let Ok(parsed) = owner(legacy) {
      gnu.error("warning: '.' should be ':'")
      return parsed
    }
    return Err(PermError.Invalid("invalid user"))
  }
  let parts = spec.split(":")
  return Err(PermError.Invalid("invalid group")) when parts.len() > 2
  let username = parts[0]
  var user_id: Int? = null
  var group_id: Int? = null
  if username != "" {
    match uid(username) {
      Ok(id) => user_id = id
      Err(_) => return Err(PermError.Invalid("invalid user"))
    }
  }
  if parts.len() == 2 {
    if parts[1] != "" {
      match gid(parts[1]) {
        Ok(id) => group_id = id
        Err(_) => return Err(PermError.Invalid("invalid group"))
      }
    } else if username != "" {
      group_id = if rx"^[ \t\n\u{b}\u{c}\r]*\+?[0-9]+$".matches(username) { user.by_uid(uid(username)?)?.gid } else { user.lookup(username)?.gid }
    }
  }
  {uid: user_id, gid: group_id}
}

proc owner_label(ids: Owner, group_only: Bool) [fs, error] -> Str {
  let user_name = if let id = ids.uid { if let Ok(rec) = user.by_uid(id) { rec.name } else { f"{id}" } } else { "" }
  let group_name = if let id = ids.gid { if let Ok(rec) = group.by_gid(id) { rec.name } else { f"{id}" } } else { "" }
  if group_only { group_name } else if ids.gid != null { f"{user_name}:{group_name}" } else { user_name }
}

type OwnerNames = {user: Str?, group: Str?}

# GNU retains account operand spelling, but resolves numeric operands separately
# from account names when choosing ownership versus group verbose messages.
proc requested_names(raw: Str, ids: Owner, group_only: Bool) [fs, error] -> OwnerNames {
  if group_only {
    if let id = ids.gid { return {user: null, group: if ! raw.starts_with("+") and group.lookup(raw) is Ok(_) { raw } else { f"{id}" }} }
    return {user: null, group: null}
  }
  let spec = if ! (":" in raw) and "." in raw and user.lookup(raw) is Err(_) { raw.replace(".", with: ":") } else { raw }
  let parts = spec.split(":")
  var user_name: Str? = null
  var group_name: Str? = null
  if parts[0] != "" and ! parts[0].starts_with("+") and user.lookup(parts[0]) is Ok(_) { user_name = parts[0] }
  if parts.len() == 2 and ids.gid != null {
    if parts[1] == "" { group_name = owner_label({uid: null, gid: ids.gid}, true) } else if ! parts[1].starts_with("+") and group.lookup(parts[1]) is Ok(_) { group_name = parts[1] }
  }
  if user_name == null and group_name != null { user_name = "" }
  if user_name == null { if let id = ids.uid { user_name = f"{id}" } }
  if group_name == null { if let id = ids.gid { group_name = f"{id}" } }
  {user: user_name, group: group_name}
}

pure names_label(names: OwnerNames) -> Str {
  if let username = names.user { if let groupname = names.group { return f"{username}:{groupname}" }; return username }
  names.group ?? ""
}

proc change_owner(target: Path, ids: Owner, filter: Owner?, opts: Options, names: OwnerNames) [fs, error, process, env, io] -> Bool {
  let name = f"{target}"
  let before = fs.stat(target, follow_symlinks: opts.dereference)
  if let Err(failure) = before {
    # A link that exists but whose referent cannot be followed is reported as a
    # dereference failure; a name that does not exist is an access failure.
    if ! opts.quiet {
      if opts.dereference and fs.stat(target, follow_symlinks: false) is Ok(_) { gnu.cannot("dereference", name, failure) } else { gnu.cannot_access(name, failure) }
    }
    if opts.verbosity == "verbose" {
      let noun = if names.user != null { "ownership" } else if names.group != null { "group" } else { "ownership" }
      gnu.write_text(f"failed to change {noun} of {gnu.quote(name)} to {names_label(names)}\n")
    }
    return false
  }
  let meta = before?
  if let required = filter {
    if (required.uid != null and required.uid != meta.uid) or (required.gid != null and required.gid != meta.gid) {
      if opts.verbosity == "verbose" {
        let noun = if names.user != null { "ownership" } else if names.group != null { "group" } else { "ownership" }
        let retained: Owner = {uid: if names.user == null { null } else { meta.uid }, gid: if ids.gid != null { meta.gid } else { null }}
        gnu.write_text(f"{noun} of {gnu.quote(name)} retained as {owner_label(retained, names.user == null)}\n")
      }
      return true
    }
  }
  let old: Owner = {uid: if names.user == null { null } else { meta.uid }, gid: if ids.gid != null { meta.gid } else { null }}
  let changed = (ids.uid != null and ids.uid != meta.uid) or (ids.gid != null and ids.gid != meta.gid)
  let noun = if names.user != null { "ownership" } else if names.group != null { "group" } else { "ownership" }
  let result = fs.set_owner(target, uid: ids.uid, gid: ids.gid, follow_symlinks: opts.dereference)
  if let Err(failure) = result {
    if ! opts.quiet { gnu.error(f"changing {if ids.uid != null { "ownership" } else { "group" }} of {gnu.quote(name)}: {gnu.strerror(failure)}") }
    if opts.verbosity == "verbose" { gnu.write_text(f"failed to change {noun} of {gnu.quote(name)} from {owner_label(old, names.user == null)} to {names_label(names)}\n") }
    return false
  }
  if opts.verbosity == "verbose" and ids.uid == null and ids.gid == null {
    gnu.write_text(f"ownership of {gnu.quote(name)} retained\n")
    return true
  }
  if opts.verbosity == "verbose" or (opts.verbosity == "changes" and changed) {
    let text = if changed { f"changed {noun} of {gnu.quote(name)} from {owner_label(old, names.user == null)} to {names_label(names)}" } else { f"{noun} of {gnu.quote(name)} retained as {owner_label(old, names.user == null)}" }
    gnu.write_text(f"{text}\n")
  }
  true
}

# Track ancestors by inode, so following directory links cannot recurse forever.
proc owner_tree(target: Path, ids: Owner, filter: Owner?, opts: Options, names: OwnerNames, top: Bool, ancestors: List[Str]) [fs, error, process, env, io] -> Bool {
  var success = true
  let follow = opts.traversal == "L" or (top and opts.traversal == "H")
  if let Ok(meta) = fs.stat(target, follow_symlinks: follow) {
    if opts.recursive and meta.kind == "dir" {
      let key = f"{meta.dev}:{meta.ino}"
      if key in ancestors { return change_owner(target, ids, filter, opts, names) }
      if opts.preserve_root {
        let root = fs.stat(p"/", follow_symlinks: true)?
        if root.dev == meta.dev and root.ino == meta.ino { refuse_root(target); return false }
      }
      match fs.children(target) {
        Ok(children) => for child in children { if ! owner_tree(child_path(target, child.path)?, ids, filter, opts, names, false, ancestors + [key]) { success = false } }
        Err(failure) => { if ! opts.quiet { gnu.cannot("read directory", f"{target}", failure) }; success = false }
      }
    }
  }
  if ! change_owner(target, ids, filter, opts, names) { success = false }
  success
}

proc missing_operand_after_bytes(operand: Bytes) [process, env] {
  gnu.usage_error(f"missing operand after {gnu.quote_bytes(operand)}")
}

## Apply ownership changes and retain failure status across operands.
export proc ownership(argv: List[Bytes], group_only = false) [fs, error, process, env, io] {
  let opts = options(argv)?
  let command = if group_only { "chgrp" } else { "chown" }
  if opts.help { gnu.help(f"Usage: {command} [OPTION]... {if group_only { "GROUP" } else { "OWNER[:GROUP]" }} FILE...\nChange ownership of each FILE.\n  -R, --recursive\n  -c, --changes\n  -f, --silent\n  -v, --verbose\n  -h, --no-dereference\n  -H -L -P\n      --reference=RFILE\n      --from=OWNER[:GROUP]\n      --preserve-root\n      --no-preserve-root\n      --help\n      --version"); return }
  if opts.version { gnu.version(command); return }
  if opts.operands.is_empty() { gnu.missing_operand() }
  if opts.reference == null and opts.operands.len() < 2 { missing_operand_after_bytes(opts.operands[0]) }
  var ids: Owner = {uid: null, gid: null}
  var targets = opts.operands
  var names: OwnerNames = {user: null, group: null}
  if let reference = opts.reference {
    let reference_path = Path.parse_bytes(reference)?
    match fs.stat(reference_path, follow_symlinks: true) {
      Ok(meta) => {
        ids = {uid: if group_only { null } else { meta.uid }, gid: meta.gid}
        names = {user: if group_only { null } else { owner_label({uid: meta.uid, gid: null}, false) }, group: owner_label({uid: null, gid: meta.gid}, true)}
      }
      Err(failure) => { gnu.error(f"failed to get attributes of {gnu.quote_bytes(reference)}: {gnu.strerror(failure)}"); exit 1 }
    }
  } else {
    let raw_owner = opts.operands[0]
    let owner_spec = if let Ok(text) = raw_owner.utf8() {
      text
    } else {
      gnu.error(f"invalid {if group_only { "group" } else { "user" }}: {gnu.quote_bytes(raw_owner)}")
      exit 1
    }
    match owner(owner_spec, group_only) {
      Ok(parsed) => ids = parsed
      Err(failure) => { gnu.error(f"{if group_only { "invalid group" } else { failure.message }}: {gnu.quote(owner_spec)}"); exit 1 }
    }
    names = requested_names(owner_spec, ids, group_only)
    targets = opts.operands[1..]
    if targets.is_empty() { missing_operand_after_bytes(raw_owner) }
  }
  var filter: Owner? = null
  if let spec = opts.from {
    let owner_filter = if let Ok(text) = spec.utf8() {
      text
    } else {
      gnu.error(f"invalid owner filter: {gnu.quote_bytes(spec)}")
      exit 1
    }
    let parsed_filter = owner(owner_filter)
    match parsed_filter {
      Ok(parsed) => filter = parsed
      Err(failure) => { gnu.error(f"{failure.message}: {gnu.quote(owner_filter)}"); exit 1 }
    }
  }
  var success = true
  for target in targets { if ! owner_tree(Path.parse_bytes(target)?, ids, filter, opts, names, true, []) { success = false } }
  if let Err(failure) = io.flush_stdout() { gnu.write_failed(failure) }
  if ! success { exit 1 }
}

## Refuse recursive mutation of the filesystem root with the conventional hint.
export proc refuse_root(target: Path) [process, env] {
  let name = f"{target}"
  let alias = if name == "/" { "" } else { " (same as '/')" }
  gnu.error(f"it is dangerous to operate recursively on {gnu.quote(name)}{alias}")
  gnu.error("use --no-preserve-root to override this failsafe")
}

## Preserve the operand spelling while retaining a directory child's native bytes.
export pure child_path(parent: Path, child: Path) -> Result[Path, Error] {
  let prefix = parent.bytes()
  let separator = if prefix.byte_at(prefix.len() - 1) == 47 { b"" } else { b"/" }
  let component = child.relative_to(child.parent()).bytes()
  Path.parse_bytes(bytes.concat([prefix, separator, component]))
}
