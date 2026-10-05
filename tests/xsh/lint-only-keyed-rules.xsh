# Every rule that a `[lint]` key of `xsht-config.ini` turns on or off runs
# when `--only` names it, whatever the key says: naming a rule asks for it.

# A rule with a `[lint]` key: its code, the configuration under which a plain
# `xsht lint` does not run it, and a program it reports.
type KeyedRule = {code: Str, off: Str, source: Str}

const keyed_rules: List[KeyedRule] = [
  {
    code: "lint.prefer-inferred-pure-return",
    off: "",
    source: "pure label(name: Str) -> Str { name.trim() }\nprint label(\"ready\")\n",
  },
  {
    code: "lint.prefer-inferred-proc-return",
    off: "",
    source: "proc parsed(text: Str) -> Result[Int] {\n  text.parse_int()\n}\n\nlet total = parsed(\"1\")?\nassert total == 1\n",
  },
  {
    code: "lint.prefer-set",
    off: "",
    source: "let seen: Map[Str, Bool] = {\"a\": false}\nprint seen.len()\n",
  },
  {
    code: "lint.prefer-text-pattern",
    off: "",
    source: "pure probe(line: Str) -> Str {\n  let parts = line.split(\"=\")\n  let key = parts[0]\n  let value = parts[1]\n  key + value\n}\n\nprint probe(\"a=b\")\n",
  },
  {
    code: "lint.prefer-rel-path",
    off: "",
    source: "proc stage(root: FsRoot, file: Path) [fs, error] {\n  root.write(file, \"x\")\n}\n",
  },
  {
    code: "lint.prefer-with-scope",
    off: "",
    source: "let root = fs.open_root(p\".\")?\ndefer root.close()\n",
  },
  {
    code: "lint.prefer-inferred-private-effects",
    off: "[lint]\nprefer-inferred-private-effects = false\n",
    source: "proc parsed(text: Str) [error] -> Result[Int] {\n  text.parse_int()? + 1\n}\n\nlet total = parsed(\"1\")?\nassert total == 2\n",
  },
  {
    code: "lint.prefer-env-string",
    off: "[lint]\nprefer-env-string = false\n",
    source: "let home = env.get(\"HOME\") ?? \"\"\nassert home.byte_len() >= 0\n",
  },
  {
    code: "lint.prefer-item-shorthand",
    off: "[lint]\nprefer-item-shorthand = false\n",
    source: "type Hit = {url: Str, size: Int}\nlet hits = [Hit(url: \"/a\", size: 1), Hit(url: \"/b\", size: 2)]\nlet urls = hits |> count { |hit| hit.url }\nassert urls.len() == 2\n",
  },
  {
    code: "lint.prefer-tempdir-scope",
    off: "[lint]\nprefer-tempdir-scope = false\n",
    source: "let scratch = fs.tempdir()?\ndefer scratch.close()?\nlet log = fp\"{scratch.host_path()?}/app.log\"\nlog.write(\"ok\\n\")?\nprint log.name()\n",
  },
]

# How many findings with `code` `xsht lint FLAGS main.xsh` reports in `root`.
# The file must parse and check, so that an absent finding is a statement
# about the rule.
proc findings(root: Path, flags: List[Str], code: Str) [process, env, error] -> Result[Int] {
  let linted = cd (root) {
    run.capture --text "xsht" lint @flags main.xsh
  }?

  assert "err[" not in linted.stderr, linted.stderr
  Ok(linted.stderr.split(f"[{code}]").len() - 1)
}

test test_only_runs_every_rule_that_a_lint_key_governs { |ctx|
  for rule in keyed_rules {
    let root = test.temp_dir(ctx, name: "project")?
    fp"{root}/xsht-config.ini".write(rule.off)
    fp"{root}/main.xsh".write(rule.source)
    assert findings(root, [], rule.code)? == 0, f"{rule.code} ran without being asked"
    assert findings(root, ["--only", rule.code], rule.code)? > 0, f"--only {rule.code} did not run the rule"
  }
}

# A project whose `check --annotate` writes return annotations keeps them: the
# two rules that remove a return stay off there however they are asked for.
test test_only_does_not_remove_returns_that_annotate_writes { |ctx|
  for rule in keyed_rules {
    continue unless rule.code in ["lint.prefer-inferred-pure-return", "lint.prefer-inferred-proc-return"]
    let root = test.temp_dir(ctx, name: "project")?
    fp"{root}/xsht-config.ini".write("[check]\nannotate = returns\n")
    fp"{root}/main.xsh".write(rule.source)
    assert findings(root, ["--only", rule.code], rule.code)? == 0, rule.code
  }
}
