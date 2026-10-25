const APP = r"""
const TOP = "top help"
const REPO = "repository help"
const BUILD = "build help"
cli main(...unknown: List[Str]) with {name: "pm", help: TOP} {
  if unknown.is_empty() { print $TOP } else { fail f"unknown pm command {unknown[0]}" }
}
cli main repo(...unknown: List[Str]) with {name: "pm repo", help: REPO} {
  if unknown.is_empty() { print $REPO } else { fail f"unknown pm repo command {unknown[0]}" }
}
proc default_jobs() -> Int { print "DEFAULT"; 4 }
cli main repo build(input: Path, store: Path, jobs: Int = default_jobs(), roots: List[Str] = []) with {
  name: "pm repo build",
  help: BUILD,
  parameters: {
    input: {form: "PLAN"},
    store: {form: "--store STORE"},
    jobs: {form: "-j --jobs N", min: 1},
    roots: {form: "--root PACKAGE"},
  },
} {
  print f"{input} {store} {jobs} {roots.join(",")}"
}
"""

test metadata_preserves_required_options_aliases_repetition_and_lazy_defaults { |ctx|
  let given = test.expect(ctx, APP, status: 0, args: ["repo", "build", "plan", "--store", "store", "-j2", "--root", "a", "--root=b"])?
  assert given.stdout == "plan store 2 a,b\n"
  let omitted = test.expect(ctx, APP, status: 0, args: ["repo", "build", "plan", "--store=store"])?
  assert omitted.stdout == "DEFAULT\nplan store 4 \n"
  for arguments in [["repo", "build", "plan"], ["repo", "build", "plan", "--store", "store", "-j0"]] {
    let rejected = test.expect(ctx, APP, status: 2, stderr: ["usage: pm repo build PLAN [OPTIONS]"], args: arguments)?
    assert rejected.stdout == ""
  }
}

test metadata_groups_route_raw_unknown_words_and_preserve_help_boundaries { |ctx|
  for {arguments, output} in [
    {arguments: [], output: "top help\n"},
    {arguments: ["repo"], output: "repository help\n"},
    {arguments: ["help"], output: "top help\n"},
    {arguments: ["repo", "help", "ignored"], output: "repository help\n"},
    {arguments: ["repo", "build", "--help"], output: "build help\n"},
    {arguments: ["--", "repo", "build", "help"], output: "build help\n"},
  ] {
    let result = test.expect(ctx, APP, status: 0, args: arguments)?
    assert result.stdout == output
  }
  let unknown = test.expect(ctx, APP, status: 3, stderr: ["unknown pm repo command --bogus"], args: ["repo", "--bogus"])?
  assert unknown.stdout == ""
  let strict = test.expect(ctx, APP, status: 0, args: ["repo", "build", "plan", "--help"])?
  assert "usage: pm repo build PLAN [OPTIONS]" in strict.stdout
  assert "DEFAULT" not in strict.stdout
}

test metadata_rejects_unknown_fields_forms_collisions_and_conflicting_defaults { |ctx|
  for declaration in [
    "cli main(n: Int = 4) with {unknown: 1} {}",
    "cli main(n: Int = 4) with {parameters: {missing: {form: \"--missing N\"}}} {}",
    "cli main(n: Int = 4) with {parameters: {n: {kind: \"Int\"}}} {}",
    "cli main(n: Int = 4) with {parameters: {n: {form: \"-long N\"}}} {}",
    "cli main(n: Int = 4) with {parameters: {n: {min: 5}}} {}",
    "cli main(n: Int = 4) with {parameters: {n: {form: \"--jobs\"}}} {}",
    "cli main(n: Int) with {parameters: {n: {form: \"...N\"}}} {}",
    "cli main(n: Int = 4, jobs: Int = 1) with {parameters: {n: {form: \"--jobs N\"}}} {}",
    "cli main(n: Int = 4, other: Int = 1) with {parameters: {n: {form: \"-j --jobs N\"}, other: {form: \"-j --other N\"}}} {}",
    "cli main(n: Str) {}\ncli main child() {}",
    "proc metadata() { print \"SIDE EFFECT\"; {} }\ncli main() with metadata() {}",
  ] {
    let rejected = test.expect(ctx, declaration, status: 2, stderr: ["check.cli-entry"], args: ["--help"])?
    assert "SIDE EFFECT" not in rejected.stdout
  }
}

test metadata_required_repeated_options_require_a_supplied_occurrence { |ctx|
  let source = r"""
cli main(roots: List[Str]) with {parameters: {roots: {form: "--root PACKAGE"}}} {
  print roots.join(",")
}
"""
  let given = test.expect(ctx, source, status: 0, args: ["--root", "a", "--root=b"])?
  assert given.stdout == "a,b\n"
  let missing = test.expect(ctx, source, status: 2, stderr: ["missing required argument --root"])?
  assert missing.stdout == ""
}

test metadata_group_fallbacks_keep_generated_help_for_known_prefixes { |ctx|
  let source = r"""
cli main(...unknown: List[Str]) {
  fail f"fallback {unknown.join(",")}"
}
cli main repo check() {}
cli main store() with {name: "pm store"} {}
cli main store verify() {}
"""
  let implicit = test.expect(ctx, source, status: 0, stdout: [" repo COMMAND [ARGS]..."], args: ["repo", "--help"])?
  assert "fallback" not in implicit.stderr
  let declared = test.expect(ctx, source, status: 0, stdout: ["usage: pm store COMMAND [ARGS]..."], args: ["store", "--help"])?
  assert declared.stderr == ""
}

test metadata_explicit_aliases_do_not_infer_additional_long_flags { |ctx|
  let source = r"""
cli main(workers: Int = 1) with {parameters: {workers: {form: "-j N"}}} {
  print $workers
}
"""
  let given = test.expect(ctx, source, status: 0, args: ["-j2"])?
  assert given.stdout == "2\n"
  let rejected = test.expect(ctx, source, status: 2, stderr: ["unknown argument at argv[0]: --workers"], args: ["--workers=2"])?
  assert rejected.stdout == ""
  let help = test.expect(ctx, source, status: 0, stdout: ["-j N"], args: ["--help"])?
  assert "--workers" not in help.stdout
}

test metadata_minimum_constrains_computed_defaults_before_the_body { |ctx|
  for {annotation, result} in [
    {annotation: "Int", result: "0"},
    {annotation: "UInt", result: "0"},
    {annotation: "List[Int]", result: "[2, 0]"},
    {annotation: "List[UInt]", result: "[2, 0]"},
  ] {
    let source = f"""
proc computed() -> {annotation} {{ print "DEFAULT"; {result} }}
cli main(workers: {annotation} = computed()) with {{parameters: {{workers: {{form: "--workers N", min: 1}}}}}} {{
  print "BODY"
}}
"""
    let invalid = test.expect(ctx, source, status: 2, stderr: ["option --workers expects value >= 1"])?
    assert invalid.stdout == "DEFAULT\n"
    let help = test.expect(ctx, source, status: 0, args: ["--help"])?
    assert "DEFAULT" not in help.stdout
    assert "BODY" not in help.stdout
    let rejected = test.expect(ctx, source, status: 2, args: ["--workers", "0"])?
    assert rejected.stdout == ""
    let supplied = test.expect(ctx, source, status: 0, args: ["--workers", "3"])?
    assert supplied.stdout == "BODY\n"
  }
}

test metadata_invalid_path_defaults_are_check_errors { |ctx|
  let source = r"""cli main(root: Path = p"bad\0path") {}"""
  let invalid = test.expect(ctx, source, status: 2, stderr: ["check.cli-entry"], args: ["--help"])?
  assert "panicked" not in invalid.stderr
}
