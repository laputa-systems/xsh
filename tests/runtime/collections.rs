use super::common::*;

#[test]
fn structured_streams_walk_filter_map_collect_and_count() {
    let root = temp_path("stream-walk-root");
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).expect("create stream fixture dirs");
    std::fs::write(root.join("a.txt"), "a").expect("write a");
    std::fs::write(nested.join("b.txt"), "b").expect("write b");
    let source = format!(
        "\
let root = Path({})
let entries = fs.walk(root)
|> where {{
  .kind == \"file\"
}}
|> collect()
let names = entries
|> map {{ |entry|
  entry.name
}}
|> sort-by {{ . }}
let count = fs.walk(root)
|> where {{
  .kind == \"file\"
}}
|> count()
print ${{names[0]}} ${{names[1]}}
print ${{count}}
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("structured-streams", &source);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "a.txt b.txt\n2\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fs_walk_is_parallel_unordered_and_honors_gitignore() {
    let root = temp_path("fs-walk-parallel-root");
    let sub = root.join("sub");
    let ignored = root.join("ignored");
    std::fs::create_dir_all(&sub).expect("create sub");
    std::fs::create_dir_all(&ignored).expect("create ignored");
    std::fs::write(root.join(".gitignore"), "ignored/\n*.log\n").expect("write gitignore");
    for index in 0..200 {
        std::fs::write(sub.join(format!("f{index:03}.txt")), "x").expect("write file");
        std::fs::write(sub.join(format!("f{index:03}.log")), "x").expect("write log");
    }
    std::fs::write(ignored.join("hidden.txt"), "x").expect("write hidden");
    let source = format!(
        "\
let root = Path({})
let par = fs.walk(root) |> map {{ |e| e.path.display() }} |> sort-by {{ . }}
let par_files = fs.walk(root) |> where .kind == \"file\" |> count()
let has_hidden = (par) |> any {{ .contains(\"hidden\") }}
print f\"count=${{par.len()}} files=${{par_files}} ignored=${{has_hidden}}\"
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("fs-walk-parallel", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // 200 .txt files survive (.gitignore hidden, .log + ignored/ excluded);
    // plus root + sub dirs = 202 entries.
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "count=202 files=200 ignored=false\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fs_walk_streams_lazily_and_short_circuits_take_first_any_and_break() {
    // A flat directory of 50 files. A lazy `fs.walk` must stop pulling entries
    // as soon as the consumer is satisfied, so a `take(3)`/`first`/`any`/`break`
    // touches only a handful of entries rather than the whole tree.
    let root = temp_path("fs-walk-lazy-root");
    std::fs::create_dir_all(&root).expect("create lazy walk root");
    for index in 0..50 {
        std::fs::write(root.join(format!("f{index:02}.txt")), "x").expect("write file");
    }
    let source = format!(
        "\
let root = Path({})

var pulled = 0
let first3 = fs.walk(root)
|> tee {{ |entry| pulled = pulled + 1 }}
|> where .kind == \"file\"
|> take(3)
|> map .name
print f\"take ok=${{pulled < 50 && first3.len() == 3}}\"

var pulled_any = 0
let any_file = fs.walk(root)
|> tee {{ |entry| pulled_any = pulled_any + 1 }}
|> any .kind == \"file\"
print f\"any ok=${{pulled_any < 50 && any_file}}\"

var pulled_break = 0
for entry in fs.walk(root) |> where .kind == \"file\" {{
  pulled_break = pulled_break + 1
  if pulled_break >= 2 {{ break }}
}}
print f\"break seen=${{pulled_break}}\"

let total = fs.walk(root) |> where .kind == \"file\" |> count()
print f\"total=${{total}}\"
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("fs-walk-lazy", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "take ok=true\n\
any ok=true\n\
break seen=2\n\
total=50\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fs_walk_lazy_folding_terminals_match_eager_results() {
    // count/sum/min/max/fold drive the live walk by folding one item at a
    // time (no materialization); the results must equal the eager path.
    let root = temp_path("fs-walk-fold-root");
    std::fs::create_dir_all(&root).expect("create fold root");
    std::fs::write(root.join("a.txt"), "a").expect("write a"); // 1 byte
    std::fs::write(root.join("bb.txt"), "bb").expect("write bb"); // 2 bytes
    std::fs::write(root.join("ccc.txt"), "ccc").expect("write ccc"); // 3 bytes
    let source = format!(
        "\
let root = Path({})
let n = fs.walk(root) |> where .kind == \"file\" |> count()
let s = fs.walk(root) |> where .kind == \"file\" |> map .size |> sum()
let lo = fs.walk(root) |> where .kind == \"file\" |> map .size |> min()
let hi = fs.walk(root) |> where .kind == \"file\" |> map .size |> max()
let f = fs.walk(root) |> where .kind == \"file\" |> map .size |> fold(0) {{ |acc| acc + . }}
print f\"n=${{n}} s=${{s}} lo=${{lo?}} hi=${{hi?}} f=${{f}}\"
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("fs-walk-fold", &source);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "n=3 s=6 lo=1 hi=3 f=6\n"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fs_walk_honors_gitignore_by_default_and_can_disable_it() {
    let root = temp_path("fs-walk-gitignore-root");
    let ignored = root.join("ignored");
    let nested = root.join("nested");
    let build = root.join("build");
    let git = root.join(".git");
    let cache = root.join(".cache");
    std::fs::create_dir_all(&ignored).expect("create ignored dir");
    std::fs::create_dir_all(&nested).expect("create nested dir");
    std::fs::create_dir_all(&build).expect("create build dir");
    std::fs::create_dir_all(&git).expect("create git dir");
    std::fs::create_dir_all(&cache).expect("create hidden cache dir");
    std::fs::write(
        root.join(".gitignore"),
        "ignored/\n*.log\n!keep.log\n/build\n",
    )
    .expect("write gitignore");
    std::fs::write(root.join("visible.txt"), "visible").expect("write visible");
    std::fs::write(root.join("a.log"), "ignored").expect("write ignored log");
    std::fs::write(root.join("keep.log"), "kept").expect("write kept log");
    std::fs::write(ignored.join("hidden.txt"), "ignored").expect("write ignored child");
    std::fs::write(nested.join("a.log"), "ignored").expect("write nested ignored log");
    std::fs::write(build.join("output.txt"), "ignored").expect("write build output");
    std::fs::write(git.join("config"), "ignored").expect("write git config");
    std::fs::write(cache.join("secret.txt"), "hidden").expect("write hidden cache file");
    std::fs::write(root.join(".env"), "hidden").expect("write hidden file");
    let source = format!(
        "\
let root = Path({})
let filtered = fs.walk(root)
|> where .kind == \"file\"
|> sort-by .path
|> map {{ |entry|
  entry.path.strip_prefix(root)?.display()
}}
let raw = fs.walk(root, gitignore: false)
|> where .kind == \"file\"
|> sort-by .path
|> map {{ |entry|
  entry.path.strip_prefix(root)?.display()
}}
let raw_hidden = fs.walk(root, gitignore: false, hidden: true)
|> where .kind == \"file\"
|> sort-by .path
|> map {{ |entry|
  entry.path.strip_prefix(root)?.display()
}}
print ${{filtered.contains(\"visible.txt\")}} ${{filtered.contains(\"keep.log\")}} ${{filtered.contains(\"a.log\")}} ${{filtered.contains(\"ignored/hidden.txt\")}} ${{filtered.contains(\"nested/a.log\")}} ${{filtered.contains(\"build/output.txt\")}} ${{filtered.contains(\".git/config\")}} ${{filtered.contains(\".cache/secret.txt\")}} ${{filtered.contains(\".env\")}}
print ${{raw.contains(\"a.log\")}} ${{raw.contains(\"ignored/hidden.txt\")}} ${{raw.contains(\"nested/a.log\")}} ${{raw.contains(\"build/output.txt\")}} ${{raw.contains(\".git/config\")}} ${{raw.contains(\".cache/secret.txt\")}} ${{raw.contains(\".env\")}}
print ${{raw_hidden.contains(\".gitignore\")}} ${{raw_hidden.contains(\".git/config\")}} ${{raw_hidden.contains(\".cache/secret.txt\")}} ${{raw_hidden.contains(\".env\")}}
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("fs-walk-gitignore", &source);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true true false false false false false false false\ntrue true true true false false false\ntrue true true true\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn fs_files_recurses_with_raw_walk_and_preserves_entry_ext() {
    let root = temp_path("fs-files-recursive-root");
    let include = root.join("include");
    let bits = include.join("bits");
    let sys = include.join("sys");
    let src = root.join("src");
    let obj = root.join("obj");
    std::fs::create_dir_all(&bits).expect("create bits dir");
    std::fs::create_dir_all(&sys).expect("create sys dir");
    std::fs::create_dir_all(&src).expect("create src dir");
    std::fs::create_dir_all(&obj).expect("create obj dir");
    std::fs::write(root.join(".gitignore"), "*.lo\n*.so\n*.a\n/obj/\n").expect("write gitignore");
    std::fs::write(include.join("top.h"), "top").expect("write top header");
    std::fs::write(bits.join("alltypes.h"), "bits").expect("write bits header");
    std::fs::write(sys.join("stat.h"), "sys").expect("write sys header");
    std::fs::write(src.join("main.c"), "main").expect("write c source");
    std::fs::write(src.join("Makefile"), "all:").expect("write extensionless source");
    std::fs::write(src.join("skip.lo"), "obj").expect("write ignored object");
    std::fs::write(obj.join("hidden.h"), "hidden").expect("write ignored obj header");
    let source = format!(
        "\
let root = Path({})
let raw_headers = fs.files(fp\"${{root}}/include\", gitignore: false)
|> sort-by .path
|> map {{ |entry|
  entry.path.strip_prefix(root)?.display()
}}
let filtered = fs.files(root)
|> sort-by .path
|> map {{ |entry|
  entry.path.strip_prefix(root)?.display()
}}
let c_files = fs.files(fp\"${{root}}/src\", gitignore: false)
|> where .ext == \"c\"
let dot_c_files = fs.files(fp\"${{root}}/src\", gitignore: false)
|> where .ext == \".c\"
let source_headers = fs.files(root, exts: [\"h\", \"c\"])
|> sort-by .path
|> map {{ |entry|
  entry.path.strip_prefix(root)?.display()
}}
let dotted_filter = fs.files(root, exts: [\".c\"]) |> count()
let extensionless = fs.files(root, exts: [\"\"])
|> sort-by .path
|> map {{ |entry|
  entry.path.strip_prefix(root)?.display()
}}
let cheap_c = fs.files(root, gitignore: false, stat: false, exts: [\"c\"]) |> first()?
print ${{raw_headers.len()}} ${{raw_headers.contains(\"include/top.h\")}} ${{raw_headers.contains(\"include/bits/alltypes.h\")}} ${{raw_headers.contains(\"include/sys/stat.h\")}}
print ${{filtered.contains(\"include/top.h\")}} ${{filtered.contains(\"include/bits/alltypes.h\")}} ${{filtered.contains(\"include/sys/stat.h\")}} ${{filtered.contains(\"src/main.c\")}} ${{filtered.contains(\"src/skip.lo\")}} ${{filtered.contains(\"obj/hidden.h\")}}
print ${{c_files.len()}} ${{c_files[0].name}} ${{c_files[0].ext}} ${{dot_c_files.len()}}
print ${{source_headers.len()}} ${{source_headers.contains(\"include/top.h\")}} ${{source_headers.contains(\"src/main.c\")}} ${{source_headers.contains(\"src/skip.lo\")}} ${{dotted_filter}}
print ${{extensionless.len()}} ${{extensionless.contains(\"src/Makefile\")}}
print ${{cheap_c.name}} ${{cheap_c.ext}} ${{cheap_c.kind}} ${{cheap_c.size}} ${{cheap_c.mode}} ${{cheap_c.executable}} ${{cheap_c.path.strip_prefix(root)?.display()}}
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("fs-files-recursive", &source);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "3 true true true\ntrue true true true false false\n1 main.c c 0\n4 true true false 0\n1 true\nmain.c c file 0 0 false src/main.c\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn filesystem_path_and_install_apis() {
    let root = temp_path("fs-path-install-root");
    let source = format!(
        "\
let root = Path({})
let _clean = fs.remove(root, missing_ok: true)?
let _made = fs.mkdir(root, parents: true)?
let note = fp\"${{root}}/note.txt\"
let _old = fs.write_atomic(note, \"old\")?
let _wrote = fs.write_atomic(note, \"hello\\n\")?
let note_text = fs.read_text(note)?
let _mode = fs.chmod(note, 384)?
let link = fp\"${{root}}/note.link\"
let _linked = fs.symlink(note, link)?
let entries = fs.ls(root) |> sort-by .name
let files = fs.children(root) |> where {{ .kind == \"file\" }}
let usage = root.du()?
let renamed = note.with_ext(\"log\")
let stripped = note.strip_prefix(root)?
let resolved = root.resolve()?
let cwd = fs.cwd()?
let scratch = fs.tempdir()?
let temp = fs.tempfile()?
print ${{entries[0].name}} ${{entries[1].name}}
print ${{files[0].mode % 512}} ${{files[0].uid >= 0}} ${{files[0].modified > 0}}
print ${{renamed.name}} ${{renamed.ext}} ${{note.parent.name == root.name}} ${{stripped.display()}}
print ${{usage >= 6}} ${{resolved.name == root.name}} ${{note_text == \"hello\\n\"}}
print ${{fs.root_exists(scratch, p\".\")?}} ${{fs.root_exists(temp.root, temp.path)?}}
let copy = fp\"${{root}}/copy.txt\"
let moved = fp\"${{root}}/moved.txt\"
let hard = fp\"${{root}}/hard.txt\"
let empty = fp\"${{root}}/empty\"
let _copy = fs.copy(note, copy)?
let refused = fs.copy(note, copy)
let _rename = copy.rename(moved)?
let _truncated = moved.truncate(4)?
let moved_text = fs.read_text(moved)?
let moved_meta = moved.metadata()?
let installed = fp\"${{root}}/bin/tool\"
let _installed = fs.install(moved, installed, 0o755, parents: true)?
let install_refused = fs.install(moved, installed, 0o755)
let _overwritten = fs.install(moved, installed, 0o700, parents: false, overwrite: true)?
let installed_meta = installed.metadata()?
let _synced = fs.fsync(installed)?
let fifo = fp\"${{root}}/control\"
let _fifo = fs.mkfifo(fifo, 0o600)?
let fifo_meta = fifo.metadata()?
let install_link = fp\"${{root}}/installed.link\"
let _install_link = fs.symlink(installed, install_link)?
let symlink_refused = fs.install(moved, install_link, 0o755)
let _touch = fp\"${{root}}/stamp\".touch()?
let _empty = empty.mkdir()?
let _rmdir = empty.remove_dir()?
let _hard = moved.hardlink(hard)?
let _hard_unlink = hard.unlink()?
let link_target = link.readlink()?
print ${{moved_text == \"hell\"}} ${{moved_meta.size == 4}} ${{link_target.display() == note.display()}} ${{cwd.name != \"\"}}
print ${{installed_meta.mode % 512 == 448}} ${{installed.read_text()? == moved_text}} ${{fifo_meta.kind == \"other\"}}
match refused {{
  Err(e) => print ${{e.kind}}
}}
match install_refused {{
  Err(e) => print ${{e.kind}}
}}
match symlink_refused {{
  Err(e) => print ${{e.kind}}
}}
let _temp_clean = fs.close_root(temp.root)
let _scratch_clean = fs.close_root(scratch)
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("filesystem-path-install", &source);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "note.link note.txt\n384 true true\nnote.log log true note.txt\ntrue true true\ntrue true\ntrue true true true\ntrue true true\nfs-copy\nfs-install\nfs-install\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn filesystem_package_policy_apis_cover_copy_tree_locks_ownership_and_manifest_remove() {
    let root = temp_path("package-policy-fs-root");
    let source = format!(
        "\
let root = Path({})
let _clean = fs.remove(root, missing_ok: true)?
let src = fp\"${{root}}/src\"
let _made = fs.mkdir(fp\"${{src}}/dir\", parents: true)?
let tool = fp\"${{src}}/dir/tool\"
let _tool = fs.write(tool, \"tool\\n\")?
let _mode = fs.chmod(tool, 0o755)?
let _link = fs.symlink(Path(\"dir/tool\"), fp\"${{src}}/tool.link\")?
let copied = fs.copy_tree(src, fp\"${{root}}/copy\", parents: true)?
let me = user.current()?
let grp = group.current()?
let copied_tool = fp\"${{root}}/copy/dir/tool\"
let _owner = fs.chown(copied_tool, me)?
let _group = fs.chgrp(copied_tool, grp)?
let lock = fs.lock(fp\"${{root}}/pm.lock\")?
print ${{lock.id > 0}} ${{lock.shared}}
let _unlock = fs.unlock(lock)?
let installed = fp\"${{root}}/image/usr/bin/tool\"
let _installed = fs.install_as(copied_tool, installed, 0o755, me, grp, parents: true)?
let installed_meta = installed.metadata()?
let removed = fs.remove_manifest(fp\"${{root}}/image\", [Path(\"usr/bin/tool\")])?
print ${{copied.files}} ${{copied.dirs}} ${{copied.symlinks}} ${{installed_meta.mode % 512 == 493}} ${{removed.removed}} ${{removed.pruned_dirs}}
print ${{fs.exists(installed)? == false}}
match fs.remove_manifest(fp\"${{root}}/image\", [Path(\"../escape\")], missing_ok: true) {{
  Err(e) => print ${{e.kind}}
}}
match fs.copy_tree(src, fp\"${{root}}/copy\") {{
  Err(e) => print ${{e.kind}}
}}
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("package-policy-fs", &source);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "true false\n1 2 1 true 1 2\ntrue\nfs-remove-manifest\nfs-copy-tree\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn stable_tables_sort_files_and_process_records_without_text_parsing() {
    let root = temp_path("table-sort-process-root");
    let source = format!(
        "\
let root = Path({})
let _clean = fs.remove(root, missing_ok: true)?
let _made = fs.mkdir(root, parents: true)?
let _small = fs.write(fp\"${{root}}/small\", \"a\")?
let _large = fs.write(fp\"${{root}}/large\", \"abcd\")?
fs.ls(root)
|> sort-by {{ .size }}
|> table.print(columns: [\"name\", \"size\"])
let process_count = process.list() |> count()
print ${{process_count > 0}}
",
        xsh_string_literal(root.to_str().unwrap())
    );

    let output = run_temp_script("table-sort-process", &source);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "┌───────┬──────┐\n│ name  │ size │\n├───────┼──────┤\n│ small │    1 │\n├───────┼──────┤\n│ large │    4 │\n└───────┴──────┘\ntrue\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn table_print_wraps_cells_to_terminal_width_without_ellipsis() {
    let path = write_temp_script(
        "table-wrap",
        "\
let rows = [{name: \"very-long-command-name-that-keeps-going\", size: 123}]
(rows) |> table.print(columns: [\"name\", \"size\"])
",
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xsh"))
        .env("COLUMNS", "40")
        .arg(&path)
        .output()
        .expect("run xsh");
    let _ = std::fs::remove_file(path);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains('…'), "{stdout}");
    assert!(stdout.contains("very-long-command-name-that-k"), "{stdout}");
    assert!(stdout.contains("eeps-going"), "{stdout}");
    assert!(
        stdout.lines().all(|line| line.chars().count() <= 40),
        "{stdout}"
    );
}

#[test]
fn list_comprehension_basic_transform() {
    let source = r#"
let nums: List[Int] = [1, 2, 3]
let doubled = [x * 2 for x in nums]
print ${doubled[0]} ${doubled[1]} ${doubled[2]}
"#;
    let output = run_temp_script("list-comp-basic", source);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "2 4 6\n");
}

#[test]
fn list_comprehension_with_guard_filters_elements() {
    let source = r#"
let nums: List[Int] = [1, 2, 3, 4, 5]
let evens = [x for x in nums if x % 2 == 0]
print ${evens[0]} ${evens[1]}
"#;
    let output = run_temp_script("list-comp-guard", source);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "2 4\n");
}

#[test]
fn list_comprehension_guard_can_produce_empty_list() {
    let source = r#"
let nums: List[Int] = [1, 3, 5]
let evens: List[Int] = [x for x in nums if x % 2 == 0]
let count = evens |> count()
print ${count}
"#;
    let output = run_temp_script("list-comp-empty-guard", source);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "0\n");
}

#[test]
fn list_comprehension_with_record_destructuring() {
    let source = r#"
type Entry = {name: Str, score: Int}
let entries: List[Entry] = [
  {name: "alice", score: 90},
  {name: "bob", score: 55},
  {name: "carol", score: 80},
]
let passing = [name for {name, score} in entries if score >= 60]
print ${passing[0]} ${passing[1]}
"#;
    let output = run_temp_script("list-comp-destructure", source);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "alice carol\n");
}
