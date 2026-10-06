#!/bin/xsh
use lib.gnu
use lib.search

type Metadata = {kind: Str, size: Int, uid: Int, gid: Int, ino: Int, nlink: Int, mode: Int, atime_ns: Int, mtime_ns: Int}
type Node = {kind: Str, value: Str, left: Int, right: Int, argv: List[Str]}
type Parsed = {nodes: List[Node], root: Int, at: Int}
type Deferred = {node: Int, target: Path}
type Visit = {quit: Bool, failed: Bool, deferred: List[Deferred]}
type Decision = {matched: Bool, prune: Bool, quit: Bool, deferred: List[Deferred]}
type Options = {follow: Int, minimum: Int, maximum: Int, depth: Bool, device: Bool, print_default: Bool}

# Symbolic permissions are changes applied to an initially empty mode, with
# no umask filtering. Conditional execute is evaluated separately for dirs.
pure permission_mode(spec: Str, directory: Bool) -> Result[Int] {
  var numeric_mode = 0
  var octal = spec != ""
  for digit in spec {
    let number = digit.parse_int() ?? -1
    if number < 0 or number > 7 { octal = false; break }
    numeric_mode = numeric_mode * 8 + number
  }
  if octal { if numeric_mode > 0o7777 { search.reject("permission mode exceeds 07777")? }; return Ok(numeric_mode) }
  var mode = 0
  for clause in spec.split(",") {
    var at = 0
    var who = ""
    while at < clause.byte_len() and clause.byte_slice(at,1) in "ugoa" { who = f"{who}{clause.byte_slice(at,1)}"; at += 1 }
    if who == "" or "a" in who { who = "ugo" }
    if at >= clause.byte_len() { search.reject("invalid symbolic permission mode")? }
    while at < clause.byte_len() {
      let operation = clause.byte_slice(at,1); at += 1
      if operation not in ["+","-","="] { search.reject("invalid symbolic permission operation")? }
      let start = at
      while at < clause.byte_len() and clause.byte_slice(at,1) not in ["+","-","="] { at += 1 }
      let permissions = clause.byte_slice(start,at - start)
      var ordinary = 0
      var bits = 0
      if permissions in ["u","g","o"] {
        ordinary = mode / (if permissions == "u" { 64 } else if permissions == "g" { 8 } else { 1 }) % 8
      } else {
        for permission in permissions {
          match permission {
            "r" => ordinary = ordinary.bit_or(4)
            "w" => ordinary = ordinary.bit_or(2)
            "x" => ordinary = ordinary.bit_or(1)
            "X" => { if directory or mode.bit_and(0o111) != 0 { ordinary = ordinary.bit_or(1) } }
            "s" => { if "u" in who { bits = bits.bit_or(0o4000) }; if "g" in who { bits = bits.bit_or(0o2000) } }
            "t" => { if "o" in who { bits = bits.bit_or(0o1000) } }
            _ => search.reject("invalid symbolic permission character")?
          }
        }
      }
      var mask = 0
      if "u" in who { bits = bits.bit_or(ordinary * 64); mask = mask.bit_or(0o4700) }
      if "g" in who { bits = bits.bit_or(ordinary * 8); mask = mask.bit_or(0o2070) }
      if "o" in who { bits = bits.bit_or(ordinary); mask = mask.bit_or(0o1007) }
      if operation == "+" { mode = mode.bit_or(bits) } else if operation == "-" { mode = mode.clear_bits(bits) } else { mode = mode.clear_bits(mask).bit_or(bits) }
    }
  }
  Ok(mode)
}

pure numeric(actual: Int, expected: Str) -> Bool {
  if expected.starts_with("+") { return actual > (expected.byte_slice(1).parse_int() ?? -1) }
  if expected.starts_with("-") { return actual < (expected.byte_slice(1).parse_int() ?? -1) }
  actual == (expected.parse_int() ?? -1)
}

proc primary(args: List[Str], start: Int, nodes: List[Node]) -> Result[Parsed] {
  if start >= args.len() { search.reject("missing expression")? }
  var out = nodes
  let token = args[start]
  var at = start + 1
  if token in ["!","-not"] {
    let inner = expression(args, at, out, 3)?
    out = inner.nodes; out += [{kind: "not", value: "", left: inner.root, right: -1, argv: []}]
    return Ok({nodes: out, root: out.len() - 1, at: inner.at})
  }
  if token == "(" {
    let inner = expression(args, at, out, 0)?
    if inner.at >= args.len() or args[inner.at] != ")" { search.reject("missing closing parenthesis")? }
    return Ok({nodes: inner.nodes, root: inner.root, at: inner.at + 1})
  }
  var kind = token
  var value = ""
  var argv: List[Str] = []
  if token in ["-name","-iname","-path","-ipath","-type","-size","-mtime","-mmin","-atime","-amin","-newer","-uid","-gid","-user","-group","-inum","-links","-perm","-regex","-iregex"] {
    if at >= args.len() { search.reject(f"missing argument to {token}")? }
    value = args[at]; at += 1
  } else if token in ["-exec","-execdir"] {
    while at < args.len() and args[at] not in [";","+"] { argv += [args[at]]; at += 1 }
    if at >= args.len() or argv.is_empty() { search.reject("missing command terminator")? }
    if args[at] == "+" {
      if argv[argv.len() - 1] != "{}" { search.reject("-exec ... + requires {} as the final argument")? }
      var markers = 0
      for word in argv { if word == "{}" { markers += 1 } }
      if markers != 1 { search.reject("-exec ... + requires exactly one {} argument")? }
      kind = f"{token}+"
    }
    at += 1
  } else if token not in ["-true","-false","-print","-print0","-prune","-quit","-delete","-empty"] { search.reject(f"unsupported predicate {token}")? }
  if token == "-user" { value = f"{if let Ok(account) = user.lookup(value) { account.uid } else { value.parse_int()? }}"; kind = "-uid" }
  if token == "-group" { value = f"{if let Ok(account) = group.lookup(value) { account.gid } else { value.parse_int()? }}"; kind = "-gid" }
  if token == "-type" and value not in ["b","c","d","f","l","p","s"] { search.reject("invalid file type")? }
  if kind in ["-uid","-gid","-inum","-links","-mtime","-mmin","-atime","-amin","-size"] {
    var number = value
    if kind == "-size" {
      if number == "" { search.reject("invalid size")? }
      let suffix = number.byte_slice(number.byte_len() - 1)
      if suffix in ["c","w","k","M","G","b"] { number = number.byte_slice(0,number.byte_len() - 1) }
    }
    if number.starts_with("+") or number.starts_with("-") { number = number.byte_slice(1) }
    if (number.parse_int() ?? -1) < 0 { search.reject(f"invalid numeric argument to {token}")? }
  }
  if kind == "-perm" {
    var spec = value
    if spec.starts_with("-") or spec.starts_with("/") { spec = spec.byte_slice(1) }
    if spec == "" { search.reject("missing permission mode")? }
    argv = [f"{permission_mode(spec,false)?}",f"{permission_mode(spec,true)?}"]
  }
  if kind == "-newer" { value = f"{fs.stat(fp"{value}", follow_symlinks: true)?.mtime_ns}" }

  out += [{kind: kind, value: value, left: -1, right: -1, argv: argv}]
  Ok({nodes: out, root: out.len() - 1, at: at})
}

# Explicit and implicit conjunction share precedence; disjunction short
# circuits before actions in its right subtree are evaluated.
proc expression(args: List[Str], start: Int, nodes: List[Node], precedence: Int) -> Result[Parsed] {
  var parsed = primary(args, start, nodes)?
  while parsed.at < args.len() and args[parsed.at] != ")" {
    let token = args[parsed.at]
    let disjunction = token in ["-o","-or"]
    let rank = if disjunction { 1 } else { 2 }
    if rank <= precedence { break }
    let explicit = disjunction or token in ["-a","-and"]
    let rhs = expression(args, parsed.at + (if explicit { 1 } else { 0 }), parsed.nodes, rank)?
    var out = rhs.nodes
    out += [{kind: if disjunction { "or" } else { "and" }, value: "", left: parsed.root, right: rhs.root, argv: []}]
    parsed = {nodes: out, root: out.len() - 1, at: rhs.at}
  }
  Ok(parsed)
}

proc evaluate(target: Path, meta: Metadata, nodes: List[Node], index: Int) -> Result[Decision] {
  let node = nodes[index]
  if node.kind in ["and","or","not"] {
    let lhs = evaluate(target, meta, nodes, node.left)?
    if node.kind == "not" { return Ok({matched: ! lhs.matched, prune: lhs.prune, quit: lhs.quit, deferred: lhs.deferred}) }
    if lhs.quit or (node.kind == "and" and ! lhs.matched) or (node.kind == "or" and lhs.matched) { return Ok(lhs) }
    let rhs = evaluate(target, meta, nodes, node.right)?
    return Ok({matched: rhs.matched, prune: lhs.prune or rhs.prune, quit: lhs.quit or rhs.quit, deferred: lhs.deferred + rhs.deferred})
  }
  var matched = true
  var prune = false
  var quit = false
  var deferred: List[Deferred] = []
  match node.kind {
    "-true" => matched = true
    "-false" => matched = false
    "-name" | "-iname" => matched = search.glob(node.value, search.basename_bytes(target.bytes()), insensitive: node.kind == "-iname")
    "-path" | "-ipath" => matched = search.glob(node.value, target.bytes(), insensitive: node.kind == "-ipath")
    "-type" => matched = meta.kind == (match node.value { "d" => "dir"; "f" => "file"; "l" => "symlink"; "p" => "fifo"; "s" => "socket"; "b" => "block"; "c" => "char"; _ => "invalid" })
    "-uid" => matched = numeric(meta.uid, node.value)
    "-gid" => matched = numeric(meta.gid, node.value)
    "-inum" => matched = numeric(meta.ino, node.value)
    "-links" => matched = numeric(meta.nlink, node.value)
    "-size" => {
      let last = node.value.byte_slice(node.value.byte_len() - 1)
      let unit = match last { "c" => 1; "w" => 2; "k" => 1024; "M" => 1048576; "G" => 1073741824; _ => 512 }
      let value = if last in ["c","w","k","M","G","b"] { node.value.byte_slice(0,node.value.byte_len() - 1) } else { node.value }
      matched = numeric((meta.size + unit - 1) / unit, value)
    }
    "-mtime" | "-mmin" | "-atime" | "-amin" => {
      let stamp = if node.kind in ["-atime","-amin"] { meta.atime_ns } else { meta.mtime_ns }
      let unit = if node.kind in ["-mmin","-amin"] { 60000000000 } else { 86400000000000 }
      matched = numeric((time.now() * 1000000 - stamp) / unit, node.value)
    }
    "-newer" => matched = meta.mtime_ns > (node.value.parse_int() ?? 0)
    "-perm" => {
      let prefix = node.value.byte_slice(0,1)
      let mode = (if meta.kind == "dir" { node.argv[1] } else { node.argv[0] }).parse_int()?
      let bits = meta.mode.bit_and(0o7777)
      matched = if prefix == "-" { bits.bit_and(mode) == mode } else if prefix == "/" { mode == 0 or bits.bit_and(mode) != 0 } else { bits == mode }
    }
    "-regex" | "-iregex" => {
      let spans = regex.find_bytes(node.value, target.bytes(), extended: node.argv == ["extended"], ignore_case: node.kind == "-iregex")?
      matched = false
      for span in spans { if span.start == 0 and span.end == target.bytes().len() { matched = true } }
    }
    "-empty" => matched = if meta.kind == "dir" { fs.children(target)?.collect().is_empty() } else { meta.kind == "file" and meta.size == 0 }
    "-print" | "-print0" => gnu.write_bytes(bytes.concat([target.bytes(),if node.kind == "-print0" { b"\0" } else { b"\n" }]))
    "-prune" => prune = true
    "-quit" => quit = true
    "-delete" => { if meta.kind == "dir" { target.remove_dir()? } else { target.remove(missing_ok: false)? } }
    "-exec+" | "-execdir+" => deferred = [{node: index,target: target}]
    "-exec" | "-execdir" => {
      var argv: List[Path] = []
      for word in node.argv {
        argv += [if word == "{}" { if node.kind == "-execdir" { search.child(p".", search.basename_bytes(target.bytes()))? } else { target } } else { fp"{word}" }]
      }
      let command = argv[0]; let rest = argv[1..]
      if node.kind == "-execdir" {
        if command.bytes().byte_at(0) != 47 { search.reject("-execdir requires an absolute command path")? }
        let status = process.run(process.command_argv(command, argv, cwd: target.parent()))?
        matched = status.exited_with(0)
      } else { let status = run.status $command @rest; matched = status.exited_with(0) }
    }
    _ => search.reject(f"unsupported predicate {node.kind}")?
  }
  Ok({matched: matched, prune: prune, quit: quit, deferred: deferred})
}

proc visit(target: Path, depth: Int, ancestors: List[Str], device: Int?, nodes: List[Node], root: Int, opts: Options) -> Result[Visit] {
  var failed = false
  var deferred: List[Deferred] = []
  let meta = fs.stat(target, follow_symlinks: opts.follow == 2 or (opts.follow == 1 and depth == 0))?
  # Predicates after deletion still inspect the entry encountered by traversal.
  let snapshot: Metadata = {kind: meta.kind, size: meta.size, uid: meta.uid, gid: meta.gid, ino: meta.ino, nlink: meta.nlink, mode: meta.mode, atime_ns: meta.atime_ns, mtime_ns: meta.mtime_ns}
  let identity = f"{meta.dev}:{meta.ino}"
  if identity in ancestors { search.reject("filesystem loop")? }
  var decision: Decision = {matched: false, prune: false, quit: false, deferred: []}
  if ! opts.depth and depth >= opts.minimum {
    decision = evaluate(target, snapshot, nodes, root)?
    deferred += decision.deferred
    if decision.matched and opts.print_default { gnu.write_bytes(bytes.concat([target.bytes(),b"\n"])) }
    if decision.quit { return Ok({quit: true, failed: failed, deferred: deferred}) }
  }
  if meta.kind == "dir" and ! decision.prune and (opts.maximum < 0 or depth < opts.maximum) and (! opts.device or device == null or meta.dev == device) {
    for entry in fs.children(target)? {
      let leaf = search.child(target,search.basename_bytes(entry.path.bytes()))?
      let visited = visit(leaf, depth + 1, ancestors + [identity], device ?? meta.dev, nodes, root, opts)
      match visited {
        Ok(result) => { failed = failed or result.failed; deferred += result.deferred; if result.quit { return Ok({quit: true, failed: failed, deferred: deferred}) } }
        Err(failure) => { gnu.error(f"{gnu.quote_bytes(leaf.bytes())}: {failure.message}"); failed = true }
      }
    }
  }
  if opts.depth and depth >= opts.minimum {
    decision = evaluate(target, snapshot, nodes, root)?
    deferred += decision.deferred
    if decision.matched and opts.print_default { gnu.write_bytes(bytes.concat([target.bytes(),b"\n"])) }
  }
  Ok({quit: decision.quit, failed: failed, deferred: deferred})
}

proc main(...args: List[Str]) {
  var opts: Options = {follow: 0, minimum: 0, maximum: -1, depth: false, device: false, print_default: true}
  var regex_type = "emacs"
  var roots: List[Path] = []
  var expression_args: List[Str] = []
  var at = 0
  while at < args.len() and args[at] in ["-P","-H","-L"] {
    opts.follow = if args[at] == "-L" { 2 } else if args[at] == "-H" { 1 } else { 0 }; at += 1
  }
  while at < args.len() and ! args[at].starts_with("-") and args[at] not in ["!","("] { roots += [fp"{args[at]}"]; at += 1 }
  if roots.is_empty() { roots = [p"."] }
  while at < args.len() {
    let token = args[at]; at += 1
    if token in ["-maxdepth","-mindepth"] {
      if at >= args.len() { gnu.usage_error("missing depth argument") }
      let number = args[at].parse_int() ?? -1; at += 1
      if number < 0 { gnu.usage_error("invalid depth") }
      if token == "-maxdepth" { opts.maximum = number } else { opts.minimum = number }
      continue
    }
    if token == "-regextype" {
      if at >= args.len() { gnu.usage_error("missing regex type") }
      regex_type = args[at]; at += 1
      if regex_type not in ["posix-basic","posix-extended"] { gnu.usage_error("supported regex types are posix-basic and posix-extended") }
      continue
    }
    if token == "-depth" { opts.depth = true; continue }
    if token in ["-xdev","-mount"] { opts.device = true; continue }
    if token == "--help" { gnu.help("Usage: find [PATH]... [EXPRESSION]\nEvaluate expressions while traversing directory trees."); return }
    if token == "--version" { gnu.version("find"); return }
    expression_args += [token]
    if token in ["-exec","-execdir"] {
      while at < args.len() {
        let word = args[at]; expression_args += [word]; at += 1
        if word in [";","+"] { break }
      }
    }
  }
  if expression_args.is_empty() { expression_args = ["-true"] }
  var failed = false
  var deferred: List[Deferred] = []
  let outcome = try {
    var parsed = expression(expression_args,0,[],0)?
    for index in range(parsed.nodes.len()) {
      if parsed.nodes[index].kind in ["-regex","-iregex"] {
        if regex_type == "emacs" { search.reject("the default Emacs regex dialect is not supported; select -regextype posix-basic or posix-extended")? }
        parsed.nodes[index].argv = [if regex_type == "posix-extended" { "extended" } else { "basic" }]
        let _ = regex.find_bytes(parsed.nodes[index].value,b"",extended: regex_type == "posix-extended")?
      }
    }
    if parsed.at != expression_args.len() { search.reject("unexpected closing parenthesis")? }
    var deleting = false
    var pruning = false
    for node in parsed.nodes { if node.kind == "-delete" { deleting = true }; if node.kind == "-prune" { pruning = true } }
    if deleting and pruning and ! opts.depth { search.reject("-delete turns on -depth, but -prune does nothing with -depth; use -depth explicitly if that is intended")? }
    for node in parsed.nodes {
      if node.kind in ["-print","-print0","-exec","-execdir","-exec+","-execdir+","-delete","-quit"] { opts.print_default = false }
      if node.kind == "-delete" { opts.depth = true }
    }
    for target in roots {
      let visited = visit(target,0,[],null,parsed.nodes,parsed.root,opts)
      match visited { Ok(result) => { failed = failed or result.failed; deferred += result.deferred; if result.quit { break } }; Err(failure) => { gnu.error(f"{gnu.quote_bytes(target.bytes())}: {failure.message}"); failed = true } }
    }
    for index in range(parsed.nodes.len()) {
      let node = parsed.nodes[index]
      if node.kind not in ["-exec+","-execdir+"] { continue }
      var batch: List[Path] = []
      var directory: Path? = null
      var size = 0
      var initial: List[Path] = []
      for word in node.argv[..node.argv.len() - 1] { initial += [fp"{word}"] }
      if initial.is_empty() { search.reject("missing command")? }
      var initial_size = 0
      for word in initial { initial_size += word.bytes().len() + 1 }
      if initial_size >= 65536 { search.reject("initial command arguments exceed the size limit")? }
      if node.kind == "-execdir+" and initial[0].bytes().byte_at(0) != 47 { search.reject("-execdir requires an absolute command path")? }
      for item in deferred {
        if item.node != index { continue }
        let cwd: Path? = if node.kind == "-execdir+" { item.target.parent() } else { null }
        let name = if node.kind == "-execdir+" { search.child(p".",search.basename_bytes(item.target.bytes()))? } else { item.target }
        if ! batch.is_empty() and (cwd != directory or initial_size + size + name.bytes().len() + 1 > 65536) {
          let status = process.run(process.command_argv(initial[0], initial + batch, cwd: directory ?? p"."))?
          if ! status.exited_with(0) { failed = true }
          batch = []; size = 0
        }
        if initial_size + name.bytes().len() + 1 > 65536 { search.reject("file argument exceeds the size limit")? }
        directory = cwd; batch += [name]; size += name.bytes().len() + 1
      }
      if ! batch.is_empty() {
        let status = process.run(process.command_argv(initial[0], initial + batch, cwd: directory ?? p"."))?
        if ! status.exited_with(0) { failed = true }
      }
    }
  }
  if let Err(failure) = outcome { gnu.error(failure.message); exit 1 }
  if let Err(failure) = io.flush_stdout() { gnu.error(failure.message); exit 1 }
  exit if failed { 1 } else { 0 }
}
