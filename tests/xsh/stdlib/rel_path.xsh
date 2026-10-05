# `RelPath` is a `Path` known not to be absolute and not to climb above where
# it starts. A value gets the type from a literal the checker can judge, from
# one validation at an explicit boundary, or from `parent()` and
# `normalize()`; every other operation reads it as the path it is.

type Entry = {rel: RelPath, mode: Int}

type Defaults = {rel: RelPath = "etc/default", mode: Int}

const CONFIG: RelPath = "etc/app.conf"

pure beneath(root: Path, rel: RelPath) -> Path {
  fp"{root}/{rel}"
}

pure joined(dir: RelPath, name: RelPath) -> RelPath {
  fp"{dir}/{name}"
}

# An unannotated return keeps the type a preserving operation returns.
pure directory(rel: RelPath) {
  rel.parent()
}

pure validated(value: Any) -> Result[Path] {
  let rel: RelPath = value.require()?
  Ok(rel)
}

proc takes_rel(rel: RelPath) [error] -> Result[Str] {
  Ok(rel.display())
}

test test_rel_path_literals_are_judged_where_written {
  let plain: RelPath = "usr/lib"
  let marked: RelPath = p"usr/lib"
  let dotted: RelPath = "a/./b//c/"
  let returning: RelPath = "a/../b"
  let here: RelPath = "."
  let hidden: RelPath = "..data/...x"
  assert plain == marked
  assert dotted.display() == "a/./b//c/"
  assert returning.normalize() == "b"
  assert here.display() == "."
  assert hidden.name() == "...x"
  assert CONFIG.name() == "app.conf"
  assert Defaults(mode: 1).rel == "etc/default"
  assert Entry(rel: "bin/sh", mode: 0o755).rel.name() == "sh"
  assert beneath("/srv", "www/index.html") == "/srv/www/index.html"

  let maybe: RelPath? = "opt"
  assert maybe != null and maybe.name() == "opt"
  let listed: List[RelPath] = ["x", p"y/z"]
  assert listed.len() == 2
  assert "x" in listed
  assert /x not in listed
}

test test_interpolating_literal_joins_rel_paths {
  let dir: RelPath = "usr/share"
  let name: RelPath = "doc/README"
  let direct: RelPath = fp"{dir}/{name}"
  let between: RelPath = fp"{dir}/cache/{name}"
  let leading: RelPath = fp"opt/{dir}"
  let trailing: RelPath = fp"{dir}/"
  let returning: RelPath = fp"{dir}/x/../{name}"
  assert direct == "usr/share/doc/README"
  assert between == "usr/share/cache/doc/README"
  assert leading == "opt/usr/share"
  assert trailing.display() == "usr/share/"
  assert returning.normalize() == "usr/share/doc/README"
  assert joined(".", ".") == "./."
  assert joined(dir, name).parent() == "usr/share/doc"

  # Without the expectation the literal is a plain path.
  let unexpected = fp"{dir}/{name}"
  assert unexpected == direct
  assert ! (fp"/{unexpected}" is RelPath)
}

test test_parent_and_normalize_keep_the_guarantee {
  let rel: RelPath = "a/b/../c/./d"
  assert rel.parent() == "a/b/../c"
  assert rel.normalize() == "a/c/d"
  assert joined(rel.parent(), rel.normalize()) == "a/b/../c/a/c/d"
  assert directory("etc/app.conf") == "etc"

  let maybe: RelPath? = "x/y"
  let none: RelPath? = null
  assert maybe?.parent() == "x"
  assert none?.parent() == null
  let dirs: List[RelPath] = [each.parent() for each in [rel, joined("a", "b")]]
  assert dirs.len() == 2

  var climbing: RelPath = "one/two"
  climbing = climbing.parent()
  assert climbing == "one"
  climbing = climbing.parent()
  assert climbing == "."
  climbing = climbing.parent()
  assert climbing == "."
  let cancelled: RelPath = "a/.."
  assert cancelled.normalize() == "."
}

# What is not listed to keep the guarantee gives a plain path back.
test test_other_operations_read_the_value_as_its_path {
  let rel: RelPath = "usr/lib/libc.so"
  let plain: Path = rel
  let renamed = rel.with_ext("a")
  let stripped = rel.strip_prefix("usr")?
  let any: Any = rel
  assert plain == rel
  assert renamed == "usr/lib/libc.a"
  assert stripped == "lib/libc.so"
  assert any == rel
  assert rel.name() == "libc.so"
  assert rel.ext() == "so"
  assert rel.components().len() == 3
  assert rel.starts_with("usr") and ! rel.starts_with("us")
  assert rel.ends_with("libc.so")
  assert "lib" in rel
  assert f"{rel}" == "usr/lib/libc.so"
  assert rel.display() == "usr/lib/libc.so"
  assert rel.bytes().len() == 15
  let printed = run.text printf "%s" $rel ?
  assert printed == "usr/lib/libc.so"

  let kind = match rel {
    "usr/lib/libc.so" => "libc",
    else => "other",
  }
  assert kind == "libc"

  var by_path: Map[Path, Int] = {}
  by_path[rel] = 1
  assert by_path[p"usr/lib/libc.so"] == 1

  let sorted = [rel, joined("a", "b")] |> sort |> collect()
  assert sorted.len() == 2
}

# The one runtime check runs at the conversion: `.require`, a type test, or a
# type pattern. A failed validation is ordinary data.
test test_validation_runs_once_at_the_explicit_boundary {
  let computed = p"share/doc"
  let checked = computed.require(RelPath)?
  assert checked.parent() == "share"

  for rejected in [p"", /, /etc, p"..", p"a/../..", p"a/b/../../../c"] {
    match rejected.require(RelPath) {
      Ok(_) => assert false, f"{rejected} was validated"
      Err(error) => assert "expected RelPath, found a path that is empty, absolute, or climbs above where it starts" in error.message, error.message
    }

    assert ! (rejected is RelPath)
  }

  for accepted in [p".", p"a", p"a/..", ./a//b/, p"..."] {
    assert accepted is RelPath, accepted.display()
  }

  if computed is RelPath {
    assert computed.parent() == "share"
  }

  let described = if let known is RelPath = computed { known.normalize().display() } else { "outside" }
  assert described == "share/doc"

  assert validated(p"a/b")? == "a/b"
  assert validated(/a) is Err(_)
  assert validated("a/b") is Err(_)

  let word: Union[RelPath, Int] = checked
  assert word is RelPath
}

# `strip_prefix` returns what is left beneath the prefix as a RelPath, and
# fails wherever that remainder would not be confined.
test test_strip_prefix_returns_a_rel_path_or_fails {
  let file = /srv/tree/share/doc
  let rel: RelPath = file.strip_prefix("/srv/tree")?
  assert rel == "share/doc"
  let whole: RelPath = p"/srv/tree".strip_prefix("/srv/tree")?
  assert whole == "."
  let beneath_root = file.strip_prefix(/)?
  assert beneath_root == "srv/tree/share/doc"

  # A `..` that stays beneath the prefix is kept as written.
  let dotted: RelPath = p"/srv/tree/a/../b".strip_prefix("/srv/tree")?
  assert dotted == "a/../b"
  assert dotted.normalize() == "b"

  # An empty prefix would return an absolute path whole.
  assert prefix_failure(file, p"") == "prefix is empty"
  assert prefix_failure(p"share/doc", p"") == "prefix is empty"
  # A `..` after the prefix would leave it.
  assert prefix_failure(/srv/tree/../../etc, "/srv") == "remainder climbs above the prefix"
  assert prefix_failure(/srv/.., "/srv") == "remainder climbs above the prefix"
  assert prefix_failure(file, "/srv/tr") == "path does not start with prefix"

  # The result is a plain path wherever one is expected.
  let plain: Path = file.strip_prefix("/srv")?
  assert plain == "tree/share/doc"
}

pure prefix_failure(whole: Path, prefix: Path) -> Str {
  match whole.strip_prefix(prefix) {
    Ok(rest) => f"ok: {rest}"
    Err(error) => error.message
  }
}

test test_fs_root_takes_a_rel_path { |ctx|
  let dir = test.temp_dir(ctx, name: "rel-path-root")?
  let root = fs.open_root(dir)?
  defer root.close()?
  let entry = Entry(rel: "etc/app/config", mode: 0o600)
  root.mkdir(entry.rel.parent(), parents: true)
  root.write(entry.rel, "key = 1\n")
  root.chmod(entry.rel, entry.mode)
  assert root.read_text(entry.rel)? == "key = 1\n"
  assert root.exists(joined("etc", "app"))?
  assert beneath(dir, entry.rel).read_text()? == "key = 1\n"

  let found = beneath(dir, entry.rel).strip_prefix(dir)?
  assert found == entry.rel
}

# A dynamic call passes an unchecked value to a checked parameter, so the
# parameter's type is tested where the call arrives.
test test_dynamic_call_tests_the_validation_at_the_parameter { |ctx|
  let callee = takes_rel
  assert callee.call(p"a/b")? == "a/b"

  let output = test.run_script(
    ctx,
    r"""proc takes_rel(rel: RelPath) [error] -> Result[Str] {
  Ok(rel.display())
}

let callee: Proc = takes_rel
let result = callee.call(p"/etc/passwd")
print ${result is Err(_)}
""",
  )?
  assert output.status == 3, f"{output.stdout}{output.stderr}"
  assert "expected RelPath, found Path" in output.stderr, output.stderr
}

proc check_errors(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert output.stdout == "", output.stdout
  output.stderr
}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

test test_literal_that_is_not_a_rel_path_is_rejected { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Entry = {rel: RelPath, mode: Int}

pure fallback(rel: RelPath = "/etc") -> Path {
  rel
}

pure up() -> RelPath {
  ".."
}

let absolute: RelPath = "/usr"
let empty: RelPath = ""
let escaping: RelPath = p"a/../../b"
let entry = Entry(rel: "../x", mode: 1)
let literal: Entry = {rel: "/x", mode: 1}
var kept: RelPath = "a"
kept = "a/b/../../.."
print ${fallback("/")} ${up()} $absolute $empty $escaping ${entry.rel} ${literal.rel} $kept
""",
  )?
  assert count(stderr, "err[check.validated-literal]") == 9, stderr
  assert count(stderr, "err[") == 9, stderr
  assert "this path is absolute, so it is not a RelPath" in stderr, stderr
  assert "this path is empty, so it is not a RelPath" in stderr, stderr
  assert "this path climbs above where it starts, so it is not a RelPath" in stderr, stderr
}

test test_interpolating_literal_that_may_escape_is_rejected { |ctx|
  let stderr = check_errors(
    ctx,
    r"""let dir: RelPath = "srv"
let name = "notes"
let plain = Path(name)
let text: RelPath = fp"{dir}/{name}"
let pathed: RelPath = fp"{dir}/{plain}"
let glued: RelPath = fp"{dir}{dir}"
let suffixed: RelPath = fp"{dir}-old"
let prefixed: RelPath = fp"old-{dir}"
let above: RelPath = fp"{dir}/.."
let rooted: RelPath = fp"/{dir}"
print $text $pathed $glued $suffixed $prefixed $above $rooted
""",
  )?
  assert count(stderr, "err[check.validated-literal]") == 7, stderr
  assert count(stderr, "err[") == 7, stderr
  assert "this path interpolates a Str, so it is not a RelPath" in stderr, stderr
  assert "this path interpolates a Path, so it is not a RelPath" in stderr, stderr
  assert count(stderr, "this interpolation is not set off by `/`, so the path is not a RelPath") == 3, stderr
  assert "this path climbs above where it starts, so it is not a RelPath" in stderr, stderr
  assert "this path is absolute, so it is not a RelPath" in stderr, stderr
}

test test_constant_that_fails_the_validation_is_rejected { |ctx|
  let stderr = check_errors(
    ctx,
    r"""const OUTSIDE: RelPath = "../etc"
print $OUTSIDE
""",
  )?
  assert "err[check.validated-literal]" in stderr, stderr
  assert "this path climbs above where it starts, so it is not a RelPath" in stderr, stderr
}

# A path is never a `RelPath` without the validation, wherever it comes from.
test test_unvalidated_path_never_fits_rel_path { |ctx|
  let stderr = check_errors(
    ctx,
    r"""pure beneath(root: Path, rel: RelPath) -> Path {
  fp"{root}/{rel}"
}

let computed = Path("a")
let rel: RelPath = "a/b"
let direct: RelPath = computed
let dynamic: Any = p"a"
let unchecked: RelPath = dynamic
let paths: List[Path] = [rel]
let rels: List[RelPath] = paths
var kept: RelPath = "a"
kept = computed
print ${beneath("/", computed)} $direct $unchecked ${rels.len()} $kept
""",
  )?
  assert count(stderr, "err[check.type-mismatch]") == 4, stderr
  assert count(stderr, "err[check.dynamic-boundary]") == 1, stderr
  assert count(stderr, "err[") == 5, stderr
  assert "expected RelPath, found Path" in stderr, stderr
  assert "validate it with `.require(RelPath)?`" in stderr, stderr
  assert "unchecked Any cannot establish RelPath" in stderr, stderr
}

test test_rel_path_type_is_well_formed_only_where_it_can_be_checked { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Both = Union[RelPath, Path]

type Keyed = Map[RelPath, Int]

let both: Both? = null
let keyed: Keyed? = null
print ${both == null} ${keyed == null}
""",
  )?
  assert "every `RelPath` already fits the member `Path`" in stderr, stderr
  assert "err[check.map-key-type]" in stderr, stderr
}
