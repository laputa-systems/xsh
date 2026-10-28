use docker
use fixtures
use gates
use gate_execution
use internal as container_internal
use test_workflows as tests
use unix

proc init_gate_repository(root: Path, repository: Path) [fs, process, env, error] -> Result[Unit, Error] {
  let dev = fp"{root}/dev"
  dev.mkdir(parents: true)?
  fp"{dev}/gate_source.py".write(fp"{repository}/dev/gate_source.py".read_bytes()?)
  fp"{dev}/performance".mkdir()
  fp"{dev}/performance/results.py".write(fp"{repository}/dev/performance/results.py".read_bytes()?)
  fp"{root}/.gitignore".write(".work/\ntarget/\ntools/\nstdout\nstderr\nready\nchild\ncargo-calls\nharness-calls\n")
  let _ = run.capture --text git -C $root init --quiet
  let _ = run.capture --text git -C $root add .
  let _ = run.capture --text git -C $root -c "user.name=Fixture" -c "user.email=fixture@example.invalid" commit --quiet -m fixture
}

test test_changed_path_discovery_preserves_git_filename_boundaries_and_staged_changes { |ctx|
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-git-paths")?
  fp"{root}/dev".mkdir()
  fp"{root}/dev/gate_source.py".write(fp"{repository}/dev/gate_source.py".read_bytes()?)
  fp"{root}/docs".mkdir()
  fp"{root}/docs/staged.md".write("original\n")
  let _ = run.capture --text git -C $root init --quiet
  let _ = run.capture --text git -C $root add docs/staged.md
  let _ = run.capture --text git -C $root -c "user.name=Fixture" -c "user.email=fixture@example.invalid" commit --quiet -m fixture
  fp"{root}/docs/staged.md".write("staged\n")
  let _ = run.capture --text git -C $root add docs/staged.md
  fp"{root}/docs/staged.md".write("original\n")
  fp"{root}/docs/line\nbreak.md".write("untracked\n")
  let source = f"""
use gates
use fixtures

proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--plan", "--json"])?
}}
"""
  let result = test.expect(ctx, source, status: 0, args: [],
    env: {XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", GIT_DIR: f"{root}/missing"})?
  assert "docs/staged.md" in result.stdout, result.stdout
  assert "docs/line\\nbreak.md" in result.stdout, result.stdout
  let _ = run.capture --text git -C $root update-index --assume-unchanged docs/staged.md
  let flagged = test.expect(ctx, source, status: 2, args: [],
    env: {XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", GIT_DIR: f"{root}/missing"})?
  assert "assume-unchanged" in flagged.stderr, flagged.stderr
  let explicit = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--plan", "--json", "--case", "verification/source"])?
}}
"""
  let explicit_plan = test.expect(ctx, explicit, status: 0, args: [],
    env: {XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", GIT_DIR: f"{root}/missing"})?
  assert "verification/source" in explicit_plan.stdout
  let _ = run.capture --text git -C $root update-index --no-assume-unchanged docs/staged.md
  let _ = run.capture --text python3 -c "import os,sys; open(os.fsencode(sys.argv[1]) + b'/docs/\\xff.md', 'wb').close()" $root
  let invalid = test.expect(ctx, source, status: 2, args: [],
    env: {XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev"})?
  assert "UTF-8" in invalid.stderr or "UTF-8" in invalid.stdout, f"stdout={invalid.stdout} stderr={invalid.stderr}"
  let _ = run.capture --text python3 -c "import os,sys; os.unlink(os.fsencode(sys.argv[1]) + b'/docs/\\xff.md')" $root
  let _ = run.capture --text git -C $root update-ref -d HEAD
  let unborn = test.expect(ctx, source, status: 0, args: [],
    env: {XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev"})?
  assert "docs/staged.md" in unborn.stdout, unborn.stdout
}

test test_gate_rejects_source_changed_after_host_selection { |ctx|
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-selection-source")?
  fp"{root}/Cargo.toml".write("original\n")
  init_gate_repository(root, repository)?
  let snapshot = cd (root) { run.text python3 -B dev/gate_source.py }?
  fp"{root}/Cargo.toml".write("changed\n")
  fp"{root}/tools".mkdir()
  let marker = fp"{root}/started"
  fp"{root}/tools/cargo".write(f"#!/bin/sh\ntouch '{marker}'\nexit 0\n", mode: 0o755)
  let inherited = env.get_or("PATH", "")?
  let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--source-digest", "{snapshot.trim()}", "--case", "showcase/tokei/correctness"])?
}}
main()?
""", status: 2, stdout: ["source changed"], args: [], env: {{
    PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    XSH_DEV_BUILD_PRELOAD: "",
  }})?
  assert ! marker.exists()?
}

test test_source_observation_ignores_user_git_excludes { |ctx|
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-source-excludes")?
  init_gate_repository(root, repository)?
  fp"{root}/tools".mkdir()
  fp"{root}/tools/excludes".write("local-input\n")
  let config = fp"{root}/tools/gitconfig"
  config.write(f"[core]\n  excludesFile = {root}/tools/excludes\n")
  fp"{root}/local-input".write("input\n")
  let with_excludes = cd (root) { run.text env GIT_CONFIG_GLOBAL=$config python3 -B dev/gate_source.py }?
  let without_excludes = cd (root) { run.text env GIT_CONFIG_GLOBAL=/dev/null python3 -B dev/gate_source.py }?
  assert with_excludes == without_excludes
  fp"{root}/local-input".write("changed\n")
  let changed = cd (root) { run.text env GIT_CONFIG_GLOBAL=$config python3 -B dev/gate_source.py }?
  assert changed != with_excludes
}

test test_result_reconciler_changes_select_its_tests {
  let ctx = fixtures.linux_context(/repo)?
  let result_owner = gates.plan_paths(ctx, tests.Verification, ["dev/performance/results.py"], [], gates.Change)?
  assert [entry.id for entry in result_owner.cases] == ["verification/results", "verification/core-workloads", "verification/performance-check", "verification/laputa-adapter"]
  assert result_owner.products.is_empty()
  let result_tests = gates.plan_paths(ctx, tests.Verification, ["dev/performance/test_results.py"], [], gates.Change)?
  assert [entry.id for entry in result_tests.cases] == ["verification/results"]
  assert result_tests.products.is_empty()
  for changed_path in ["dev/gate_source.py", "dev/performance/test_gate_source.py"] {
    let plan = gates.plan_paths(ctx, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in plan.cases] == ["verification/source"]
    assert plan.products.is_empty()
  }
}

test test_scenario_only_scope_does_not_report_a_focused_pass { |ctx|
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-scenario-only")?
  init_gate_repository(root, repository)?
  let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["crates/xsh-fuzz/src/driver.rs"])?
}}
main()?
""", status: 2, stdout: ["incomplete"], args: [], env: {{
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: "",
  }})?
}

test test_pipeline_gate_preserves_incomplete_and_cancelled_status { |ctx|
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "pipeline-status")?
  fp"{root}/dev".mkdir()
  fp"{root}/dev/gate_product.xsh".write(fp"{repository}/dev/gate_product.xsh".read_bytes()?)
  fp"{root}/tools".mkdir()
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let python = (run.text which python3)?.trim()
  fp"{root}/tools/python3".write(f"""#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = "dev.performance.core_trace" ]; then exit "$PIPELINE_STATUS"; fi
done
exec '{python}' "$@"
""", mode: 0o755)
  let products = fp"{root}/target/x86_64-unknown-linux-musl/verification"
  products.mkdir(parents: true)?
  fp"{products}/xsht".write("fixture\n", mode: 0o755)
  init_gate_repository(root, repository)?
  let inherited = env.get_or("PATH", "")?
  let source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--case", "performance/core-pipeline-small/correctness"])?
}}
main()?
"""
  for status in [{raw: 1, expected: 1}, {raw: 2, expected: 2}, {raw: 130, expected: 130}, {raw: 143, expected: 130}] {
    let _ = test.expect(ctx, source, status: status.expected, stdout: [f"raw_status={status.raw}"], args: [], env: {
      PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      PIPELINE_STATUS: f"{status.raw}", XSH_DEV_BUILD_PRELOAD: "",
    })?
  }
}

test test_native_gate_reports_required_skips_as_incomplete { |ctx|
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-required-skip")?
  fp"{root}/dev".mkdir()
  fp"{root}/dev/gate_product.xsh".write(fp"{repository}/dev/gate_product.xsh".read_bytes()?)
  fp"{root}/xsht-config.ini".write("test_roots = tests\n")
  fp"{root}/tests".mkdir()
  fp"{root}/tests/xsh".mkdir()
  fp"{root}/tests/xsh/required.xsh".write("test required { test.skip(\"missing fixture\") }\n")
  fp"{root}/tools".mkdir()
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl/verification".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl/verification/xsh".write("fixture\n", mode: 0o755)
  fp"{root}/target/x86_64-unknown-linux-musl/verification/xshi".write("fixture\n", mode: 0o755)
  let xsht = fp"{ctx.xsh_bin.parent()}/xsht"
  fp"{root}/target/x86_64-unknown-linux-musl/verification/xsht".write(
    f"#!/bin/sh\nexec '{xsht}' \"$@\"\n", mode: 0o755)
  init_gate_repository(root, repository)?
  let source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--case", "native-tests/xsh/required.xsh"])?
}}
"""
  let inherited = env.get_or("PATH", "")?
  let _ = run.capture --text git -C $root update-ref -d HEAD
  fp"{root}/.git/index".unlink()?
  fp"{root}/tests/xsh/required.xsh".write("test required { assert true }\n")
  let _ = test.expect(ctx, source, status: 0, stdout: ["1 passed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/line\nbreak.xsh".write("test required { assert true }\n")
  let newline_id = json.encode("native-tests/xsh/line\nbreak.xsh")?
  let newline_source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--case", {newline_id}])?
}}
"""
  let _ = test.expect(ctx, newline_source, status: 0, stdout: ["1 passed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write("test required { test.skip(\"missing fixture\") }\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["incomplete", "missing fixture"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write("test required { assert true }\n")
  let _ = test.expect(ctx, source, status: 0, stdout: ["1 passed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write("test required { assert false }\n")
  let _ = test.expect(ctx, source, status: 1, stdout: ["failed", "Results: failed=1"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write(r"""test required { |ctx|
  run sh -c "rmdir \"$1\"; : > \"$1\"" -- ${ctx.temp_root.display()}
  assert false
}
""")
  let _ = test.expect(ctx, source, status: 1, stdout: ["failed", "Results: failed=1", "cleanup_failure"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write("pure nothing() { return }\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["incomplete"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  let marker = fp"{root}/started"
  let runner = fp"{root}/target/x86_64-unknown-linux-musl/verification/xsht"
  runner.write(f"#!/bin/sh\nif [ \"$2\" = --list-json ]; then exit 7; fi\nprintf run > '{marker}'\n", mode: 0o755)
  let _ = test.expect(ctx, source, status: 1, stdout: ["raw_status=7"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert ! marker.exists()?
  runner.write(f"#!/bin/sh\nif [ \"$2\" = --list-json ]; then exit 130; fi\nprintf run > '{marker}'\n", mode: 0o755)
  let _ = test.expect(ctx, source, status: 130, stdout: ["raw_status=130"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert ! marker.exists()?
  runner.write(f"#!/bin/sh\nif [ \"$2\" = --list-json ]; then printf '\\377\\n'; exit 0; fi\nprintf run > '{marker}'\n", mode: 0o755)
  let _ = test.expect(ctx, source, status: 2, stdout: ["incomplete", "UTF-8"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert ! marker.exists()?
  runner.write(f"#!/bin/sh\nif [ \"$2\" = --list-json ]; then echo '[\"tests/xsh/unrelated.xsh::other\"]'; exit 0; fi\nprintf run > '{marker}'\n", mode: 0o755)
  let _ = test.expect(ctx, source, status: 2, stdout: ["incomplete"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert ! marker.exists()?
  runner.write("#!/bin/sh\nif [ \"$2\" = --list-json ]; then echo '[\"tests/xsh/required.xsh::required\"]'; fi\nexit 0\n", mode: 0o755)
  let _ = test.expect(ctx, source, status: 2, stdout: ["incomplete"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  runner.write("#!/bin/sh\nif [ \"$2\" = --list-json ]; then echo '[\"tests/xsh/required.xsh::required\"]'; exit 0; fi\nexit 1\n", mode: 0o755)
  let _ = test.expect(ctx, source, status: 1, stdout: ["failed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  let many_ids = [f"tests/xsh/required.xsh::case_{index}_abcdefghijklmnopqrstuvwxyz0123456789" for index in range(2000)]
  let encoded_ids = json.encode(many_ids)?
  assert encoded_ids.byte_len() > 131072
  let listing = fp"{root}/tools/large-ids.json"
  listing.write(encoded_ids)
  runner.write(f"#!/bin/sh\nif [ \"$2\" = --list-json ]; then cat '{listing}'; exit 0; fi\nprintf run > '{marker}'\n", mode: 0o755)
  let _ = test.expect(ctx, source, status: 2, stdout: ["incomplete"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert marker.exists()?
}

test test_native_gate_rejects_product_changes_during_execution { |ctx|
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-product-change")?
  fp"{root}/dev".mkdir()
  fp"{root}/dev/gate_product.xsh".write(fp"{repository}/dev/gate_product.xsh".read_bytes()?)
  fp"{root}/xsht-config.ini".write("test_roots = tests\n")
  fp"{root}/tests".mkdir()
  fp"{root}/tests/xsh".mkdir()
  fp"{root}/tools".mkdir()
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  let products = fp"{root}/target/x86_64-unknown-linux-musl/verification"
  products.mkdir()
  let product = fp"{products}/xsh"
  product.write("original\n", mode: 0o755)
  fp"{products}/xshi".write("original\n", mode: 0o755)
  let xsht = fp"{ctx.xsh_bin.parent()}/xsht"
  fp"{products}/xsht".write(f"#!/bin/sh\nexec '{xsht}' \"$@\"\n", mode: 0o755)
  fp"{root}/tests/xsh/required.xsh".write(
    f"test required {{ run sh -c \"printf changed > '{product}'\" }}\n")
  init_gate_repository(root, repository)?
  let source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--case", "native-tests/xsh/required.xsh"])?
}}
"""
  let inherited = env.get_or("PATH", "")?
  let _ = test.expect(ctx, source, status: 2, stdout: ["incomplete", "xsh"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert product.read_text()? == "changed"
  product.write("original\n")
  fp"{root}/tests/xsh/required.xsh".write("test required { assert true }\n")
  let _ = test.expect(ctx, source, status: 0, stdout: ["1 passed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write(
    f"test required {{ run sh -c \"printf changed > '{product}'\"; assert false }}\n")
  let _ = test.expect(ctx, source, status: 1, stdout: ["failed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  product.write("original\n")
  fp"{root}/tests/xsh/required.xsh".write(f"test required {{ run rm \"{product}\" }}\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["unavailable", "xsh"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  product.write("original\n", mode: 0o600)
  fp"{root}/tests/xsh/required.xsh".write("test required { assert true }\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["unavailable", "xsh"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
}

test test_native_gate_rejects_source_changes_during_execution { |ctx|
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-source-change")?
  fp"{root}/.gitignore".write(".work/\ntarget/\ntools/\n")
  fp"{root}/xsht-config.ini".write("test_roots = tests\n")
  fp"{root}/dev".mkdir()
  fp"{root}/dev/gate_product.xsh".write(fp"{repository}/dev/gate_product.xsh".read_bytes()?)
  fp"{root}/dev/gate_source.py".write(fp"{repository}/dev/gate_source.py".read_bytes()?)
  fp"{root}/dev/performance".mkdir()
  fp"{root}/dev/performance/results.py".write(fp"{repository}/dev/performance/results.py".read_bytes()?)
  fp"{root}/tests".mkdir()
  fp"{root}/tests/xsh".mkdir()
  let input = fp"{root}/input.txt"
  input.write("original\n")
  fp"{root}/tests/xsh/required.xsh".write(f"test required {{ p\"{input}\".write(\"changed\\n\")? }}\n")
  let _ = run.capture --text git -C $root init --quiet
  let _ = run.capture --text git -C $root add .
  let _ = run.capture --text git -C $root -c "user.name=Fixture" -c "user.email=fixture@example.invalid" commit --quiet -m fixture
  fp"{root}/tools".mkdir()
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  let products = fp"{root}/target/x86_64-unknown-linux-musl/verification"
  products.mkdir()
  for product in ["xsh", "xshi"] { fp"{products}/{product}".write("fixture\n", mode: 0o755) }
  let xsht = fp"{ctx.xsh_bin.parent()}/xsht"
  fp"{products}/xsht".write(f"#!/bin/sh\nexec '{xsht}' \"$@\"\n", mode: 0o755)
  let source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--case", "native-tests/xsh/required.xsh"])?
}}
"""
  let inherited = env.get_or("PATH", "")?
  let _ = test.expect(ctx, source, status: 2, stdout: ["incomplete", "source"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert input.read_text()? == "changed\n"
  input.write("original\n")
  fp"{root}/tests/xsh/required.xsh".write("test required { assert true }\n")
  let _ = test.expect(ctx, source, status: 0, stdout: ["1 passed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write(f"test required {{ run git -C \"{root}\" update-index --assume-unchanged input.txt }}\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["source changed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  let _ = run.capture --text git -C $root update-index --no-assume-unchanged input.txt
  let untracked = fp"{root}/untracked.txt"
  untracked.write("before\n")
  fp"{root}/tests/xsh/required.xsh".write(f"test required {{ p\"{untracked}\".write(\"after\\n\")? }}\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["source changed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write(f"test required {{ p\"{input}\".write(\"staged\\n\")?; run git -C \"{root}\" add input.txt }}\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["source changed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write(f"test required {{ p\"{input}\".write(\"changed\\n\")?; assert false }}\n")
  let _ = test.expect(ctx, source, status: 1, stdout: ["failed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  input.write("original\n")
  fp"{root}/tests/xsh/required.xsh".write("test required { assert true }\n")
  fp"{root}/tools/cargo".write(f"#!/bin/sh\nprintf changed > '{input}'\n", mode: 0o755)
  let _ = test.expect(ctx, source, status: 2, stdout: ["source changed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let link = fp"{root}/source-link"
  link.symlink(to: p"input.txt")?
  fp"{root}/tests/xsh/required.xsh".write(
    f"test required {{ p\"{link}\".unlink()?; p\"{link}\".symlink(to: p\"missing\")? }}\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["source changed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  fp"{root}/tests/xsh/required.xsh".write(f"test required {{ p\"{input}\".unlink()? }}\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["source changed"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  link.unlink()?
  link.symlink(to: p"/")?
  let marker = fp"{root}/leaf-started"
  fp"{root}/tests/xsh/required.xsh".write(f"test required {{ p\"{marker}\".write(\"started\")? }}\n")
  let _ = test.expect(ctx, source, status: 2, stdout: ["source unavailable", "escapes"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert ! marker.exists()?
  link.unlink()?
  let _ = run.capture --text python3 -c "import os,sys; open(os.fsencode(sys.argv[1]) + b'/opaque-\\xff', 'wb').close()" $root
  let _ = run.capture --text git -C $root add -A
  let _ = run.capture --text git -C $root -c "user.name=Fixture" -c "user.email=fixture@example.invalid" commit --quiet -m opaque
  let _ = test.expect(ctx, source, status: 2, stdout: ["source unavailable", "utf-8"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert ! marker.exists()?
  let _ = test.expect(ctx, source, status: 2, stdout: ["source unavailable"], args: [],
    env: {PATH: fp"{root}/tools".display(), XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
  assert ! marker.exists()?
}

# Combining owners preserves their required gates without scheduling a second
# copy for every changed file in the same source family.
test test_changed_paths_select_and_deduplicate_their_verification_owners {
  let selected = gates.select(["src/sema/check/records.rs", "src/sema/check/results.rs", "src/runtime/eval/indexed/full.rs", "tests/xsh/retry.xsh", "tests/xsh/retry.xsh"])?
  assert selected.areas == [gates.Checker, gates.Lowering]
  assert selected.native_files == ["tests/xsh/retry.xsh"]
  assert gates.areas_for("src/syntax/grammar.rs")? == [gates.Syntax, gates.Grammar, gates.Docs]
  assert gates.areas_for("crates/xsht/tests/syntax.rs")? == [gates.Syntax]
  assert gates.areas_for("tests/grammar.rs")? == [gates.Syntax]
  assert gates.areas_for("docs/snippets/api/fs.xsh")? == [gates.Docs, gates.Api]
  let source_policy = gates.select(["dev/consolidation/current_source_policy.xsh"])?
  assert source_policy.areas.is_empty()
  assert source_policy.native_files == ["dev/tests/test-consolidation-ratchets-current.xsh"]
  match gates.select(["unowned-family/behavior.rs"]) {
    Ok(_) => test.fail("a changed path without a gate owner was accepted")
    Err(error) => assert error.message == "GateError.UnmappedPath"
  }
}

test test_cargo_callers_select_their_exact_native_tests_and_products {
  let ctx = fixtures.linux_context(/repo)?
  let cases = [
    {
      path: "dev/cargo_steps.xsh",
      files: ["dev/tests/test-cargo-steps.xsh", "dev/tests/test-lifecycle.xsh"],
    },
    {
      path: "dev/cargo_test_runner.sh",
      files: ["dev/tests/test-cargo-steps.xsh"],
    },
    {
      path: "dev/test_workflows.xsh",
      files: [
        "dev/tests/test-cargo-steps.xsh",
        "dev/tests/test-lifecycle.xsh",
        "dev/tests/test-gates.xsh",
        "dev/tests/test-native-workflows.xsh",
      ],
    },
  ]

  for case in cases {
    let selection = gates.select([case.path])?
    assert selection.areas.is_empty()
    assert selection.native_files == case.files

    let plan = gates.lane_plan(ctx, tests.Verification, selection)?
    var expected_stages = ["test-products"]
    for file in case.files {
      expected_stages += [f"native-{file}"]
    }
    assert [spec.stage for spec in plan] == expected_stages
    let product_build = plan[0]
    assert product_build.argv[1..] == [
      "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
      "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht",
    ]
    let native = [spec for spec in plan |> where .stage.starts_with("native-")]
    assert [spec.argv[2] for spec in native] == case.files
  }

  let combined = gates.select([
    "dev/test_workflows.xsh",
    "dev/cargo_steps.xsh",
    "dev/cargo_test_runner.sh",
  ])?
  assert combined.areas.is_empty()
  assert combined.native_files == [
    "dev/tests/test-cargo-steps.xsh",
    "dev/tests/test-lifecycle.xsh",
    "dev/tests/test-gates.xsh",
    "dev/tests/test-native-workflows.xsh",
  ]

  let broad = gates.select(["dev/release.xsh"])?
  assert broad.areas == [gates.Automation]
  assert broad.native_files.is_empty()
  let broad_plan = gates.lane_plan(ctx, tests.Verification, broad)?
  assert [spec.stage for spec in broad_plan] == ["test-products", "native-dev/tests"]
  assert "xshi" in broad_plan[0].argv
}

test test_current_metric_attestation_changes_select_both_native_contracts {
  let files = [
    "dev/tests/test-consolidation-metrics-current.xsh",
    "dev/tests/test-consolidation-ratchets-current.xsh",
  ]
  let paths = [
    "dev/consolidation_metrics_current.xsh",
    "dev/consolidation/metric-observer-successor.json",
    @files,
  ]
  let ctx = fixtures.linux_context(/repo)?
  for changed_path in paths {
    let selected = gates.select([changed_path])?
    assert selected.areas.is_empty()
    assert selected.native_files == files
    let plan = gates.lane_plan(ctx, tests.Verification, selected)?
    assert [spec.stage for spec in plan] == [
      "test-products",
      "native-dev/tests/test-consolidation-metrics-current.xsh",
      "native-dev/tests/test-consolidation-ratchets-current.xsh",
    ]
  }
  let combined = gates.select([@paths, @paths])?
  assert combined.areas.is_empty()
  assert combined.native_files == files
}

test test_route_witness_leaf_selects_its_direct_unit_cli_and_native_cases {
  let ctx = fixtures.linux_context(/repo)?
  let selection = gates.select(["src/runtime/eval/route_witness.rs"])?
  assert selection.areas == [gates.RouteWitness]
  assert selection.native_files.is_empty()

  let plan = gates.lane_plan(ctx, tests.Verification, selection)?
  let stages = [spec.stage for spec in plan]
  assert stages == [
    "test-products",
    "test-route-witness-unit",
    "test-route-witness-inherited-env",
    "test-route-witness-cli",
    "native-route-witness-env",
  ]
  assert "test-runtime" not in stages

  let product_build = [spec for spec in plan |> where .stage == "test-products"][0]
  assert product_build.argv[1..] == [
    "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
    "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht", "-p", "xshi", "--bin", "xshi",
  ]

  let unit = [spec for spec in plan |> where .stage == "test-route-witness-unit"][0]
  assert "--lib" in unit.argv
  assert "runtime::eval::route_witness::tests" in unit.argv
  assert "--profile" not in unit.argv

  let inherited = [spec for spec in plan |> where .stage == "test-route-witness-inherited-env"][0]
  assert "--lib" in inherited.argv
  assert "runtime_env_route_witness_tests" in inherited.argv
  assert "--profile" not in inherited.argv

  let cli_gate = [spec for spec in plan |> where .stage == "test-route-witness-cli"][0]
  assert cli_gate.argv[1..] == [
    "test", "--locked", "--target", ctx.target.triple, "--profile", "verification",
    "-p", "xsht", "--test", "integration", "route_witness",
  ]

  let native = [spec for spec in plan |> where .stage.starts_with("native-route-witness-")]
  assert native.len() == 1
  assert native[0].argv[1..] == [
    "test", "--exact",
    "tests/xsh/stdlib/env.xsh::test_route_witness_prefix_names_are_ordinary_environment_entries",
  ]
}

test test_route_witness_selector_preserves_broad_shared_runtime_gates {
  let ctx = fixtures.linux_context(/repo)?
  let selected = gates.select(["src/runtime/eval.rs"])?
  assert selected.areas == [gates.Runtime]
  let plan = gates.lane_plan(ctx, tests.Verification, selected)?
  let runtime = [spec for spec in plan |> where .stage == "test-runtime"]
  assert runtime.len() == 1
  assert "runtime::" in runtime[0].argv
  assert "--skip" in runtime[0].argv
}

test test_compile_and_soundness_leaf_gates_skip_product_builds {
  let ctx = fixtures.linux_context(/repo)?

  let compile = gates.lane_plan(ctx, tests.Verification, gates.select(["src/frontend/source.rs"])?)?
  assert [spec.stage for spec in compile] == ["check-compile"]

  let soundness = gates.lane_plan(ctx, tests.Verification, gates.select(["crates/xsh-fuzz/src/driver.rs"])?)?
  assert [spec.stage for spec in soundness] == ["test-soundness"]

  let no_products = tests.product_builds(ctx, tests.Verification, [])?
  assert no_products.is_empty()
}

test test_memory_and_documentation_gates_build_only_their_cli_products {
  let ctx = fixtures.linux_context(/repo)?

  let memory = gates.lane_plan(ctx, tests.Verification, gates.select(["src/frontend_stats.rs"])?)?
  assert [spec.stage for spec in memory] == ["test-products", "test-frontend-memory", "frontend-memory-report"]
  assert memory[0].argv[1..] == [
    "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
    "-p", "xsht", "--bin", "xsht",
  ]

  let documentation = gates.lane_plan(ctx, tests.Verification, gates.select(["docs/TESTING.md"])?)?
  assert [spec.stage for spec in documentation] == ["test-products"]
  assert documentation[0].argv[1..] == [
    "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
    "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht",
  ]
}

test test_route_witness_selector_avoids_duplicate_cli_and_native_runs {
  let ctx = fixtures.linux_context(/repo)?
  let selected = gates.select([
    "src/runtime/eval/route_witness.rs",
    "crates/xsht/tests/cli.rs",
    "tests/xsh/stdlib/env.xsh",
  ])?
  let plan = gates.lane_plan(ctx, tests.Verification, selected)?
  let stages = [spec.stage for spec in plan]
  assert "test-tooling" in stages
  assert "test-route-witness-cli" not in stages
  assert "native-route-witness-env" not in stages
  assert "native-tests/xsh/stdlib/env.xsh" in stages
}

test test_process_plan_uses_the_selected_target_profile_and_required_oracles {
  for ctx in [fixtures.linux_context(/repo)?, fixtures.linux_aarch64_context(/repo)?] {
    for profile in [tests.Verification, tests.Release] {
      let plan = tests.rust_plan(ctx, profile, privileged: true)?
      let stages = [spec.stage for spec in plan]
      assert "test-xsht" in stages
      assert "test-soundness" in stages
      assert "test-linux-priv" in stages
      assert "test-debug-assertions" in stages
      let privileged = [spec for spec in plan |> where .stage == "test-linux-priv"]
      assert [spec.argv[12] for spec in privileged] == ["core_compat_extra", "linux_priv"]
      for spec in privileged {
        assert spec.argv[11] == "--test"
        assert spec.argv[spec.argv.len() - 4..] == [
          "--skip", "split::", "--skip", "cp::privileged::test_cp_recursive_block_device_",
        ]
      }
      let ordinary = [spec for spec in plan |> where .stage == "test-rust"]
      assert [spec.argv[10] for spec in ordinary] == [
        "integration", "ambient_fs_policy", "symbol_plateau", "core_compat_boundaries", "core_compat_extra",
      ]
      for spec in ordinary {
        assert spec.argv[9] == "--test"
        assert spec.argv[spec.argv.len() - 4..] == ["--skip", "split::", "--skip", "runtime::coverage::"]
      }
      for spec in plan {
        assert "--target" in spec.argv
        assert ctx.target.triple in spec.argv
        assert ! spec.environment.keys().is_empty()
        if spec.stage == "test-debug-assertions" {
          assert "--lib" in spec.argv
          assert "--profile" not in spec.argv
          assert "--release" not in spec.argv
        } else {
          assert tests.profile_name(profile) in spec.argv
          assert "--lib" not in spec.argv
        }
      }
      let product_build = [spec for spec in plan |> where .stage == "test-products"][0]
      assert product_build.argv[1..] == [
        "build", "--locked", "--profile", tests.profile_name(profile), "--target", ctx.target.triple,
        "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht", "-p", "xshi", "--bin", "xshi",
      ]
      let xsht_integration = [spec for spec in plan |> where .stage == "test-xsht"][0]
      assert xsht_integration.argv[xsht_integration.argv.len() - 6..] == [
        "--skip", "lint_format_invariance::formatting_preserves_lints_on_laputa_corpus",
        "--skip", "examples::example_corpus_is_formatted",
        "--skip", "lint_performance::repository_lint_is_clean_within_wall_budget",
      ]
      assert tests.xsht_integration_exclusions == xsht_integration.argv[xsht_integration.argv.len() - 6..]
      assert "examples::example_corpus_lints_without_warnings" not in tests.xsht_integration_exclusions
      assert "lint_performance::lint_gate_terminates_and_reaps_process_at_wall_deadline" not in tests.xsht_integration_exclusions
      let product = tests.product_path(ctx, profile, tests.Xsht)
      assert product == fp"/repo/target/{ctx.target.triple}/{tests.profile_name(profile)}/xsht"
    }
  }
}

test test_lane_native_selection_never_requests_a_full_native_suite {
  let ctx = fixtures.linux_context(/repo)?
  let selection = gates.select(["src/sema/check/records.rs", "tests/xsh/retry.xsh"])?
  let plan = gates.lane_plan(ctx, tests.Verification, selection)?
  let native = [spec for spec in plan |> where .executable.ends_with("/xsht")]
  assert native.len() == 2
  for spec in native {
    assert spec.argv.len() == 3
    assert spec.argv[1] == "test"
    assert spec.argv[2] != ""
  }
  assert native[1].argv[2] == "tests/xsh/retry.xsh"

  let native_file = gates.lane_plan(ctx, tests.Verification, gates.select(["tests/xsh/retry.xsh"])?)?
  assert [spec.stage for spec in native_file] == ["test-products", "native-tests/xsh/retry.xsh"]
  assert native_file[0].argv[1..] == [
    "build", "--locked", "--profile", "verification", "--target", ctx.target.triple,
    "-p", "xsh", "--bins", "-p", "xsht", "--bin", "xsht", "-p", "xshi", "--bin", "xshi",
  ]
}

test test_core_test_files_select_one_file_and_shared_paths_keep_core_coverage {
  let ctx = fixtures.linux_context(/repo)?
  for test_file in ["core/tests/test-taskset.xsh", "core/tests/test-lsmem.xsh"] {
    let leaf = gates.select([test_file])?
    assert leaf.areas.is_empty()
    assert leaf.native_files == [test_file]

    let leaf_plan = gates.lane_plan(ctx, tests.Verification, leaf)?
    assert [spec.stage for spec in leaf_plan] == ["test-products", f"native-{test_file}"]
    let native = leaf_plan[1]
    assert native.argv[1..] == ["test", test_file]
  }

  for changed_path in ["core/lsmem.xsh", "core/tests/support/uu.xsh"] {
    let shared = gates.select([changed_path])?
    assert shared.areas == [gates.Core]
    assert shared.native_files.is_empty()

    let shared_plan = gates.lane_plan(ctx, tests.Verification, shared)?
    assert [spec.stage for spec in shared_plan] == ["test-products", "native-core/tests"]
    assert shared_plan[1].argv[1..] == ["test", "core/tests"]
  }
}

test test_explicit_unsupported_test_profiles_fail_instead_of_using_a_default {
  for requested in ["dist", "dev", "", "custom"] {
    env DIST_PROFILE=$requested {
      match tests.execution_profile() {
        Ok(_) => test.fail(f"unsupported profile {requested} was accepted")
        Err(error) => assert error.message == "TestProfileError.Unsupported"
      }
    }
  }
  env DIST_PROFILE=verification { assert tests.execution_profile()? == .Verification }
  env DIST_PROFILE=release { assert tests.execution_profile()? == .Release }
}


test test_fixtures_select_their_owner_instead_of_running_as_native_modules {
  let selected = gates.select(["tests/fixtures/syntax/valid/literal.xsh", "tests/fixtures/sema/invalid/type.xsh", "tests/fixtures/runtime/loop.xsh", "tests/fixtures/frontend-indexed/stages.xsh"])?
  assert selected.areas == [gates.Syntax, gates.Checker, gates.Runtime, gates.Memory]
  assert selected.native_files.is_empty()
  assert gates.areas_for("src/runtime/eval/lower/calls.rs")? == [gates.Lowering]
  assert gates.areas_for("src/syntax/grammar/productions.rs")? == [gates.Syntax, gates.Grammar, gates.Docs]
}

test test_catalog_and_xsht_exclusions_preserve_the_selected_verification_scope {
  let ctx = fixtures.linux_context(/repo)?
  let selected = gates.select(["src/stdlib.rs", "crates/xsht/src/lint.rs", "crates/xsht/tests/syntax.rs"])?
  let plan = gates.lane_plan(ctx, tests.Verification, selected)?
  let catalog = [spec for spec in plan |> where .stage == "test-stdlib-catalog"]
  assert catalog.len() == 1
  assert "--lib" in catalog[0].argv
  assert "--profile" not in catalog[0].argv
  assert "stdlib::tests::every_catalog_module_parses_checks_and_lowers" in catalog[0].argv
  for spec in plan {
    if spec.stage == "test-tooling" {
      assert "lint_format_invariance::formatting_preserves_lints_on_laputa_corpus" in spec.argv
      assert "syntax::formatter_is_idempotent_on_laputa_corpus" in spec.argv
      assert "lint_performance::repository_lint_is_clean_within_wall_budget" in spec.argv
      assert "--skip" in spec.argv
    }
    if spec.stage == "test-syntax" {
      assert "xsht" in spec.argv
      assert "integration" in spec.argv
      assert "syntax::formatter_is_idempotent_on_laputa_corpus" in spec.argv
      assert "--skip" in spec.argv
    }
    if spec.stage == "test-grammar-rust" {
      assert "xsh" in spec.argv
      assert "grammar::" in spec.argv
    }
  }
}

test test_batch_automatic_checks_use_retained_repository_gates {
  let ctx = fixtures.linux_context(/repo)?
  let checks = gates.batch_checks(ctx, tests.Verification)
  assert [spec.stage for spec in checks] == ["batch-check", "batch-lint", "batch-diff", "batch-ratchets"]
  let ratchets = checks[3]
  assert ratchets.argv[1] == "dev/consolidation/metrics.xsh"
  assert ratchets.argv[2] == "check-current"
  assert ["--root", ctx.root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src"] == ratchets.argv[3..9]
  assert ["--baseline", "dev/consolidation/baseline-blanket-scope.json", "--baseline-provenance", "dev/consolidation/blanket-metric-provenance-source-advisory.json", "--envelope", "dev/consolidation/envelope-upstream.json", "--output", ".work/consolidation/current.json", "--audit", ".work/consolidation/rises.json"] == ratchets.argv[9..]
  for spec in checks {
    assert spec.executable != "python3"
    assert [arg for arg in spec.argv |> where .starts_with("dev/compat/")].is_empty()
  }
  let retained = gates.select(["dev/checks/check_inventory.py"])?
  assert retained.areas == [gates.Automation]
  assert retained.native_files.is_empty()
}

test test_batch_preserves_rust_plan_and_bounds_native_case_admission { |ctx|
  let fixture_root = test.temp_dir(ctx, name: "batch-native-admission")?
  fixture_root.chmod(0o777)
  let work_root = fp"{fixture_root}/.work"
  work_root.mkdir()
  work_root.chmod(0o2777)
  let target_dir = fp"{fixture_root}/target"
  target_dir.mkdir()
  let product_dir = fp"{target_dir}/x86_64-unknown-linux-musl/verification"
  product_dir.parent().mkdir()
  product_dir.mkdir()
  let observed = fp"{fixture_root}/native-argv"
  fp"{product_dir}/xsht".write(
    f"""#!{ctx.xsh_bin}
p"{observed}".write(args.join("|"))?
""",
    mode: 0o755,
  )
  let workflow_context = fixtures.linux_context(fixture_root, "verification")?
  let rust_plan = tests.rust_plan(workflow_context, tests.Verification, privileged: true)?
  assert [spec.stage for spec in rust_plan] == [
    "test-products",
    "test-rust", "test-rust", "test-rust", "test-rust", "test-rust",
    "test-soundness", "test-debug-assertions", "test-xsht",
    "test-linux-priv", "test-linux-priv",
  ]

  if unix.id()?.euid == 0 {
    env HOST_UID="1000" HOST_GID="1000" {
      tests.native(workflow_context, tests.Verification, ["--jobs", "4"])
    }
  } else {
    tests.native(workflow_context, tests.Verification, ["--jobs", "4"])
  }
  assert observed.read_text()? == "test|--jobs|4"
}

test test_docker_gate_evaluates_context_root_path_expression { |ctx|
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "docker-gate-tools")?
  let cargo_home = fp"{tools}/cargo"
  let cargo_bin = fp"{cargo_home}/bin"
  cargo_home.mkdir()
  cargo_bin.mkdir()
  fp"{cargo_bin}/kache".write(
    f"""#!{ctx.xsh_bin}
exit 0
""",
    mode: 0o755,
  )
  let log = fp"{tools}/docker.log"
  fp"{tools}/docker".write(
    f"""#!{ctx.xsh_bin}
let log = p"{log}"
let previous = if log.exists()? {{ log.read_text()? }} else {{ "" }}
log.write(previous + args.join(" ") + "\\n")?
if "--format" in args {{ print "sha256:0000000000000000000000000000000000000000000000000000000000000000" }}
""",
    mode: 0o755,
  )
  let inherited_path = env.get_or("PATH", "")?
  let result = test.expect(
    ctx,
    f"""
use docker
use fixtures

proc main() [fs, process, env, error, io] -> Result[Unit] {{
  let ctx = fixtures.linux_context(p"{repository}")?
  docker.run_internal(ctx, "gate-lane", false, ["tests/xsh/float-equality.xsh"])?
}}

main()?
""",
    status: 0,
    stdout: [
      "docker-gate-lane target=x86_64-unknown-linux-musl",
    ],
    args: [],
    env: {
      PATH: f"{tools}:{inherited_path}",
      CARGO_HOME: cargo_home,
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TEST_IMAGE_BUILD: "0",
      XSH_TEST_IMAGE: "xsh-test:cwd-regression",
    },
  )?
  assert "docker-gate-lane" in result.stdout
  assert log.read_lines()?.len() == 2
  assert "sha256:0000000000000000000000000000000000000000000000000000000000000000" in log.read_text()?
  assert f"{repository}/dev/musl-linker.sh:/usr/local/bin/xsh-mold-linker:ro" in log.read_text()?
}

test test_focused_catalogue_rejects_manual_closure_cycles_and_missing_cases {
  let ctx = fixtures.linux_context(/repo)?
  let command = gates.CatalogCase(
    id: "safe", tier: gates.Change, limit: gates.RuntimeSeconds(30),
    products: [tests.Xsht], prerequisite_ids: [], reasons: ["fixture"],
    execution: gates.RunCommand({stage: "safe", target: ctx.target.triple,
      executable: "never-start", argv: ["never-start"], cwd: ctx.root, environment: {}}),
  )
  let manual = {...command, id: "full", limit: gates.ManualOwner, execution: gates.ManualFull}
  let alias = {...command, id: "alias", prerequisite_ids: ["full"], execution: gates.Alias}
  let transit = {...command, id: "transit", prerequisite_ids: ["alias"]}
  let cycle = {...command, id: "cycle", prerequisite_ids: ["cycle"]}
  let scenario = {...command, id: "scenario", tier: gates.Scenario}
  let implicit_scenario = {...command, id: "implicit-scenario", prerequisite_ids: ["scenario"]}
  let catalogue = [command, manual, alias, transit, cycle, scenario, implicit_scenario]
  for id in ["full", "alias", "transit", "cycle", "missing", "implicit-scenario"] {
    match gates.resolve_plan(catalogue, ["safe", id], gates.Change) {
      Ok(_) => test.fail(f"invalid closure {id} was accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  let plan = gates.resolve_plan(catalogue, ["safe", "safe"], gates.Change)?
  assert [entry.id for entry in plan.cases] == ["safe"]
  assert plan.products == [tests.Xsht]
  assert gates.resolve_plan(catalogue, ["scenario"], gates.Change)?.cases[0].tier == gates.Scenario
}

test test_gate_request_rejects_all_nonliteral_full_routes {
  for arguments in [
    ["--batch"], ["--full", "src/sema.rs"], ["--plan", "--full"],
    ["--tier", "full"], ["--tier", "scenario"], ["--tier"],
    ["--case"], ["--case", "--full"], ["--json"], ["--from-plan", "saved.json"],
    ["--status", "--plan"], ["--status", "--json"], ["--full", "--status"],
  ] {
    match gates.parse_request(arguments) {
      Ok(_) => test.fail(f"invalid gate request {arguments.join(" ")} was accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  let _ = gates.parse_request(["--full"])?
  let _ = gates.parse_request(["--plan", "--json", "--tier", "edit", "--case", "safe"])?
  let _ = gates.parse_request(["--status", "--case", "safe", "src/sema.rs"])?
}

test test_catalogue_plans_tokei_without_showcase_fallback_and_marks_unknown_scope {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification,
    ["showcase/tokei.xsh", "unknown/import.xsh"], [], gates.Change)?
  assert [entry.id for entry in plan.cases] == ["showcase/tokei/correctness"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved == ["unknown verification owner: unknown/import.xsh"]
  assert plan.cases[0].reasons == ["showcase/tokei.xsh output and classification contract"]
  match gates.plan_paths(ctx, tests.Verification, [], ["full"], gates.Change) {
    Ok(_) => test.fail("explicit case ID conferred manual authority")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_invalid_gate_dispatch_never_starts_docker_or_cargo { |ctx|
  test.timeout(ctx, 120s)
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "gate-forbidden-processes")?
  let marker = fp"{tools}/started"
  for name in ["cargo", "docker"] {
    fp"{tools}/{name}".write(f"""#!{ctx.xsh_bin}
p"{marker}".write("started")?
exit 99
""", mode: 0o755)
  }
  let inherited = env.get_or("PATH", "")?
  for arguments in [
    ["--batch"], ["--full", "src/sema.rs"], ["--case", "full"], ["--tier", "full"], ["--from-plan", "plan.json"],
    ["--case", "showcase/tokei/uutils-rust-correctness"],
    ["--case", "showcase/tokei/uutils-rust-correctness", "--case", "core/cat/byte-stream"],
    ["--case", "showcase/tokei/UUTILS-RUST-CORRECTNESS"],
  ] {
    let encoded = json.encode(arguments)?
    let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  let ctx = fixtures.linux_context(p"{repository}")?
  let arguments = json.decode({json.encode(encoded)?})?.require(List[Str])?
  gates.execute(ctx, arguments)?
}}
main()?
""", status: 2, args: [], env: {{
      PATH: f"{tools}:{inherited}",
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TOKEI_REFERENCE: "",
      XSH_TOKEI_CORPUS_ROOT: "",
    }})?
    assert ! marker.exists()?
  }
}

test test_tokei_gate_requires_each_external_input_before_launch { |ctx|
  test.timeout(ctx, 20s)
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "gate-missing-tokei-input")?
  let marker = fp"{tools}/started"
  for name in ["cargo", "docker"] {
    fp"{tools}/{name}".write(f"""#!{ctx.xsh_bin}
p"{marker}".write("started")?
exit 99
""", mode: 0o755)
  }
  let inherited = env.get_or("PATH", "")?
  for inputs in [["", "/corpus"], ["/reference", ""]] {
    let encoded = json.encode(inputs)?
    let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  let ctx = fixtures.linux_context(p"{repository}")?
  let inputs = json.decode({json.encode(encoded)?})?.require(List[Str])?
  env XSH_TOKEI_REFERENCE=inputs[0] XSH_TOKEI_CORPUS_ROOT=inputs[1] {{
    gates.execute(ctx, ["--case", "showcase/tokei/uutils-rust-correctness"])?
  }}
}}
main()?
""", status: 2, args: [], env: {{
      PATH: f"{tools}:{inherited}",
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    }})?
    assert ! marker.exists()?
  }
}

test test_catalogue_core_case_keeps_product_dependencies_separate_from_ownership {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/lsmem.xsh"], [], gates.Edit)?
  assert plan.tier == gates.Edit
  assert [entry.id for entry in plan.cases] == ["core/lsmem/correctness"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == ["test", "core/tests/test-lsmem.xsh"]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("core correctness was not a native command")
  }
}

test test_lsmem_change_adds_the_exact_native_tests_outside_its_own_module {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/lsmem.xsh"], [], gates.Change)?
  assert [entry.id for entry in plan.cases] == ["core/lsmem/correctness", "core/lsmem/consumers"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[1].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "core/tests/test-util-linux-contexts.xsh::test_util_linux_context_terminal_rooted_memory",
      "--exact-id", "core/tests/test-util-linux-io-boundaries.xsh::test_util_linux_lsmem_preserves_raw_sysroot_argv",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("lsmem consumers were not one exact native command")
  }
  match gates.plan_paths(ctx, tests.Verification, ["core/lsmem.xsh"], ["core/lsmem/consumers"], gates.Edit) {
    Ok(_) => test.fail("Edit accepted a Change-only case")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_catalogue_preserves_documentation_verification_and_products {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["docs/TESTING.md"], [], gates.Change)?
  assert [entry.id for entry in plan.cases] == ["check-docs"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.cases[0].execution == gates.DocumentationCheck
}

test test_catalogue_preserves_combined_route_witness_exclusions {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification,
    ["src/runtime/eval/route_witness.rs", "crates/xsht/src/commands.rs", "tests/xsh/stdlib/env.xsh"], [], gates.Change)?
  let ids = [entry.id for entry in plan.cases]
  assert "test-route-witness-unit" in ids
  assert "test-route-witness-cli" not in ids
  assert "native-route-witness-env" not in ids
  assert "native-tests/xsh/stdlib/env.xsh" in ids
  let exact = gates.plan_paths(ctx, tests.Verification, [], ["test-route-witness-cli"], gates.Change)?
  assert [entry.id for entry in exact.cases] == ["test-route-witness-cli"]
}

test test_edit_rejects_change_only_cases_before_execution {
  let ctx = fixtures.linux_context(/repo)?
  for changed_path in ["src/sema/check/records.rs", "docs/TESTING.md"] {
    match gates.plan_paths(ctx, tests.Verification, [changed_path], [], gates.Edit) {
      Ok(_) => test.fail("Edit admitted a Change-only case")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  assert gate_execution.limit_ms(30, 9000000)? == 9
  assert gate_execution.limit_ms(1, 30000000000)? == 1000
  match gate_execution.limit_ms(1, 0) {
    Ok(_) => test.fail("an exhausted runtime budget admitted a process")
    Err(error) => assert error.message == "LimitError.Expired"
  }
}

test test_soundness_fuzzing_is_an_explicit_scenario_offered_for_semantic_changes {
  let ctx = fixtures.linux_context(/repo)?
  let semantic = gates.plan_paths(ctx, tests.Verification, ["src/sema/check/records.rs"], [], gates.Change)?
  assert "test-soundness" not in [entry.id for entry in semantic.cases]
  assert semantic.available_scenarios == ["test-soundness"]
  let owner = gates.plan_paths(ctx, tests.Verification, ["crates/xsh-fuzz/src/driver.rs"], [], gates.Change)?
  assert owner.cases.is_empty()
  assert owner.available_scenarios == ["test-soundness"]
  let unrelated = gates.plan_paths(ctx, tests.Verification, ["docs/TESTING.md"], [], gates.Change)?
  assert unrelated.available_scenarios.is_empty()
  let requested = gates.plan_paths(ctx, tests.Verification, ["src/sema/check/records.rs"], ["test-soundness"], gates.Change)?
  let soundness = [entry for entry in requested.cases |> where .id == "test-soundness"]
  assert soundness.len() == 1
  assert soundness[0].tier == gates.Scenario
  assert requested.available_scenarios.is_empty()
}

test test_bounded_gate_execution_preserves_finished_failed_and_cancelled_statuses { |ctx|
  test.timeout(ctx, 10s)
  let root = test.temp_dir(ctx, name: "gate-child-status")?
  for raw in [0, 7, 130] {
    let argv = ["sh", "-c", f"exit {raw}"]
    let spec = {stage: "child", target: "fixture", executable: "sh", argv: argv, cwd: root, environment: {}}
    match gate_execution.execute(spec, 1000)? {
      gate_execution.Finished(status) => assert status.shell_code()? == raw
      gate_execution.TimedOut(_) | gate_execution.Cancelled(_) | gate_execution.CouldNotExecute(_) => test.fail("completed child lost its raw status")
    }
    let output = fp"{root}/output-{raw}"
    let captured = {...spec, argv: ["sh", "-c", f"printf captured; exit {raw}"]}
    match gate_execution.execute(captured, 1000, output: output)? {
      gate_execution.Finished(status) => assert status.shell_code()? == raw
      gate_execution.TimedOut(_) | gate_execution.Cancelled(_) | gate_execution.CouldNotExecute(_) => test.fail("captured child lost its raw status")
    }
    assert output.read_text()? == "captured"
  }
}

test test_bounded_gate_timeout_consumes_leader_and_descendant_cleanup { |ctx|
  test.timeout(ctx, 10s)
  let root = test.temp_dir(ctx, name: "gate-timeout-cleanup")?
  let leader_file = fp"{root}/leader"
  let child_file = fp"{root}/child"
  let source = f"echo $$ > '{leader_file}'; sleep 30 & echo $! > '{child_file}'; wait"
  let spec = {stage: "deadline", target: "fixture", executable: "sh", argv: ["sh", "-c", source], cwd: root, environment: {}}
  match gate_execution.execute(spec, 100)? {
    gate_execution.TimedOut(cause) => assert cause is ProcessError.Timeout {..}
    gate_execution.Finished(_) | gate_execution.Cancelled(_) | gate_execution.CouldNotExecute(_) => test.fail("deadline did not stop owned work")
  }
  for file in [leader_file, child_file] {
    let pid = file.read_text()?.trim() as Int
    let started = time.monotonic()?
    while process.kill(pid, signal: "0") is Ok {
      test.fail(f"owned process {pid} survived its consumed deadline") when time.monotonic()? - started > 2000000000
      time.sleep(10ms)
    }
  }
}

test test_gate_prepares_each_cargo_harness_once_and_runs_filters_without_cargo { |ctx|
  test.timeout(ctx, 120s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-prepared-harness")?
  for name in ["Cargo.toml", "rust-toolchain.toml", "xsht-config.ini"] { fp"{root}/{name}".write("") }
  fp"{root}/dev".mkdir()
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl/verification".mkdir()
  for product in ["xsh", "xsht", "xshi"] {
    fp"{root}/target/x86_64-unknown-linux-musl/verification/{product}".write("fixture\n", mode: 0o755)
  }
  fp"{root}/tools".mkdir()
  fp"{root}/dev/gate_case.xsh".write(fp"{repository}/dev/gate_case.xsh".read_bytes()?)
  fp"{root}/dev/gate_product.xsh".write(fp"{repository}/dev/gate_product.xsh".read_bytes()?)
  let cargo_calls = fp"{root}/cargo-calls"
  let harness_calls = fp"{root}/harness-calls"
  let harness = fp"{root}/target/prepared-test"
  harness.write(f"""#!{ctx.xsh_bin}
let name = args[0] + "::case"
if "--list" in args {{
  print json.encode({{type: "suite", event: "discovery"}})?
  print json.encode({{type: "test", event: "discovered", name: name, ignore: false}})?
  print json.encode({{type: "suite", event: "completed", tests: 1, benchmarks: 0, total: 1, ignored: 0}})?
}} else {{
  let output = p"{harness_calls}"
  let previous = if output.exists()? {{ output.read_text()? }} else {{ "" }}
  output.write(previous + args[0] + "\\n")?
  print json.encode({{type: "suite", event: "started", test_count: 1}})?
  print json.encode({{type: "test", event: "started", name: name}})?
  print json.encode({{type: "test", event: "ok", name: name}})?
  print json.encode({{type: "suite", event: "ok", passed: 1, failed: 0, ignored: 0}})?
}}
""", mode: 0o755)
  fp"{root}/tools/cargo".write(f"""#!{ctx.xsh_bin}
let output = p"{cargo_calls}"
let previous = if output.exists()? {{ output.read_text()? }} else {{ "" }}
output.write(previous + args.join(" ") + "\\n")?
if args[0] == "test" {{
  if "--no-run" not in args {{ exit 99 }}
  print json.encode({{reason: "compiler-artifact", profile: {{test: true}}, executable: "{harness}"}})?
  print json.encode({{reason: "build-finished", success: true}})?
}}
""", mode: 0o755)
  init_gate_repository(root, repository)?
  let inherited = env.get_or("PATH", "")?
  let request_source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  let ctx = fixtures.linux_context(p"{root}")?
  gates.lane(ctx, ["--case", "test-route-witness-unit", "--case", "test-route-witness-inherited-env"])?
}}
main()?
"""
  let _ = test.expect(ctx, request_source, status: 0, args: [], env: {{PATH: f"{root}/tools:{inherited}",
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: ""}})?
  let compile_calls = cargo_calls.read_lines()? |> where .starts_with("test ")
  assert compile_calls.len() == 1
  assert "--no-run" in compile_calls[0]
  assert "--message-format=json-render-diagnostics" in compile_calls[0]
  assert harness_calls.read_lines()? == ["runtime::eval::route_witness::tests", "runtime_env_route_witness_tests"]
  harness.write(f"""#!{ctx.xsh_bin}
p"{harness}".write("changed")?
""", mode: 0o755)
  let _ = test.expect(ctx, request_source, status: 2, stdout: ["product changed", "prepared-test"], args: [], env: {{
    PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    XSH_DEV_BUILD_PRELOAD: "",
  }})?
  fp"{root}/tools/cargo".write(f"""#!{ctx.xsh_bin}
if args[0] == "test" {{
  for index in range(17) {{
    print json.encode({{reason: "compiler-message", message: {{rendered: f"error: distinct compiler diagnostic {{index}}"}}}})?
  }}
  exit 7
}}
""", mode: 0o755)
  let failed = test.expect(ctx, request_source, status: 1, args: [], env: {{
    PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    XSH_DEV_BUILD_PRELOAD: "",
  }})?
  assert "error: distinct compiler diagnostic 0" in failed.stdout, failed.stdout
  assert "Cargo diagnostics limited to 16 messages" in failed.stdout, failed.stdout
  assert "error: distinct compiler diagnostic 16" not in failed.stdout, failed.stdout
  assert "raw_status=3" in failed.stdout, failed.stdout
  fp"{root}/dev/performance/results.py".write("import time\ntime.sleep(30)\n")
  let slow = test.expect(ctx, request_source, status: 1, args: [], env: {{
    PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    XSH_DEV_BUILD_PRELOAD: "",
  }})?
  assert "Cargo diagnostics could not be read" in slow.stdout, slow.stdout
  assert "raw_status=3" in slow.stdout, slow.stdout
  let ready = fp"{root}/ready"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  fp"{root}/dev/performance/results.py".write(f"import pathlib,time\npathlib.Path({json.encode(ready.display())?}).write_text('ready')\ntime.sleep(30)\n")
  let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), fp"{repository}/dev/main.xsh".display(), "--", "internal", "gate-lane", "--case", "test-route-witness-unit"],
    cwd: root, env: {{PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: "", CARGO_TARGET_DIR: fp"{root}/target".display()}}, stdout: stdout, stderr: stderr, timeout: 10s)
  let handle = spawn command?
  defer handle.cancel(signal: "KILL", kill_after: 0ms)
  while ! ready.exists()? {{
    if let _ = process.wait_timeout([handle], 10ms)? {{ test.fail(stdout.read_text()? + stderr.read_text()?) }}
  }}
  process.kill(handle.pid, signal: "TERM")
  if let finished = process.wait_timeout([handle], 3s)? {{
    assert finished.status.shell_code()? == 130, stdout.read_text()? + stderr.read_text()?
    assert "cargo preparation failed: test-route-witness-unit raw_status=3" in stdout.read_text()?, stdout.read_text()?
  }} else {{ test.fail("cancelled diagnostic reader did not finish") }}
  fp"{root}/dev/performance/results.py".write(fp"{repository}/dev/performance/results.py".read_bytes()?)
  fp"{root}/dev/gate_case.xsh".write("exit 130\n")
  let _ = test.expect(ctx, request_source, status: 130, stdout: ["raw_status=130"], args: [], env: {{
    PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    XSH_DEV_BUILD_PRELOAD: "",
  }})?
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 7\n", mode: 0o755)
  let _ = test.expect(ctx, request_source, status: 1, stdout: ["execution_error=exit 7"], args: [], env: {{
    PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    XSH_DEV_BUILD_PRELOAD: "",
  }})?
  assert harness_calls.read_lines()?.len() == 2
}

test test_cargo_harness_reports_required_results { |ctx|
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-cargo-results")?
  for name in ["Cargo.toml", "rust-toolchain.toml", "xsht-config.ini"] { fp"{root}/{name}".write("") }
  fp"{root}/dev".mkdir()
  fp"{root}/dev/gate_case.xsh".write(fp"{repository}/dev/gate_case.xsh".read_bytes()?)
  fp"{root}/dev/gate_product.xsh".write(fp"{repository}/dev/gate_product.xsh".read_bytes()?)
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  let products = fp"{root}/target/x86_64-unknown-linux-musl/verification"
  products.mkdir()
  for product in ["xsh", "xsht", "xshi"] { fp"{products}/{product}".write("fixture\n", mode: 0o755) }
  fp"{root}/tools".mkdir()
  let mode_file = fp"{root}/tools/mode"
  let harness = fp"{root}/target/prepared-test"
  harness.write(f"""#!{ctx.xsh_bin}
let mode = p"{mode_file}".read_text()?.trim()
let name = if mode == "outside" {{ "unrelated" }} else if mode == "excluded" {{ "runtime::coverage::selected" }} else {{ "runtime::eval::route_witness::tests::selected" }}
if "--list" in args {{
  print json.encode({{type: "suite", event: "discovery"}})?
  print json.encode({{type: "test", event: "discovered", name: name, ignore: false}})?
  print json.encode({{type: "suite", event: "completed", tests: 1, benchmarks: 0, total: 1, ignored: 0}})?
}} else if mode == "missing" {{
  exit 0
}} else {{
  print json.encode({{type: "suite", event: "started", test_count: 1}})?
  print json.encode({{type: "test", event: "started", name: name}})?
  let outcome = if mode == "skip" {{ "ignored" }} else if mode == "fail" {{ "failed" }} else {{ "ok" }}
  print json.encode({{type: "test", event: outcome, name: name}})?
  print json.encode({{type: "suite", event: if mode == "fail" {{ "failed" }} else {{ "ok" }},
    passed: if mode in ["pass", "outside", "excluded"] {{ 1 }} else {{ 0 }}, failed: if mode == "fail" {{ 1 }} else {{ 0 }},
    ignored: if mode == "skip" {{ 1 }} else {{ 0 }}}})?
  exit 101 when mode == "fail"
}}
""", mode: 0o755)
  fp"{root}/tools/cargo".write(f"""#!{ctx.xsh_bin}
if args[0] == "test" {{
  print json.encode({{reason: "compiler-artifact", profile: {{test: true}}, executable: "{harness}"}})?
  print json.encode({{reason: "build-finished", success: true}})?
}}
""", mode: 0o755)
  init_gate_repository(root, repository)?
  let inherited = env.get_or("PATH", "")?
  let request = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--case", "test-route-witness-unit"])?
}}
main()?
"""
  for case in [{mode: "pass", status: 0}, {mode: "skip", status: 2}, {mode: "missing", status: 2}, {mode: "fail", status: 1}, {mode: "outside", status: 2}] {
    mode_file.write(case.mode)
    let _ = test.expect(ctx, request, status: case.status, args: [], env: {{
      PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: "",
    }})?
  }
  mode_file.write("excluded")
  let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--case", "test-runtime"])?
}}
main()?
""", status: 2, args: [], env: {{
    PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: "",
  }})?
}

test test_gate_cancel_consumes_owned_descendants_before_exit_130 { |ctx|
  test.timeout(ctx, 90s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-cancel-cleanup")?
  fp"{root}/dev".mkdir()
  fp"{root}/dev/gate_product.xsh".write(fp"{repository}/dev/gate_product.xsh".read_bytes()?)
  let child_file = fp"{root}/child"
  let ready = fp"{root}/ready"
  for name in ["Cargo.toml", "rust-toolchain.toml", "xsht-config.ini"] { fp"{root}/{name}".write("") }
  let script = fp"{repository}/dev/main.xsh"
  fp"{root}/tools".mkdir()
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  let products = fp"{root}/target/x86_64-unknown-linux-musl/verification"
  products.mkdir()
  fp"{products}/xsh".write("fixture\n", mode: 0o755)
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  fp"{products}/xsht".write(f"#!/bin/sh\nsleep 30 & child=$!; trap 'wait $child; exit 143' TERM INT; echo $child > '{child_file}'; touch '{ready}'; wait $child\n", mode: 0o755)
  init_gate_repository(root, repository)?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--", "internal", "gate-lane", "--case", "showcase/tokei/correctness"],
    cwd: root, env: {XSH_MODULE_PATH: f"{repository}/dev:{repository}/dev/tests",
      PATH: f"{root}/tools:{env.get_or("PATH", "")?}", XSH_DEV_BUILD_PRELOAD: "", CARGO_TARGET_DIR: fp"{root}/target".display()}, stdout: stdout, stderr: stderr, timeout: 60s)
  let handle = spawn command?
  defer handle.cancel(signal: "KILL", kill_after: 0ms)
  while ! ready.exists()? {
    if let _ = process.wait_timeout([handle], 10ms)? {
      test.fail(stdout.read_text()? + stderr.read_text()?)
    }
  }
  process.kill(handle.pid, signal: "TERM")
  let finished = process.wait_timeout([handle], 2s)?
  if let observed = finished {
    assert observed.status.shell_code()? == 130, stdout.read_text()? + stderr.read_text()?
  } else {
    handle.cancel(signal: "KILL", kill_after: 0ms)
    test.fail("cancelled gate driver did not finish")
  }
  let pid = child_file.read_text()?.trim() as Int
  match process.kill(pid, signal: "0") {
    Err(_) => {}
    Ok(_) => test.fail(f"cancelled gate left descendant {pid} alive")
  }
}

test test_gate_cancel_consumes_product_inspection_before_exit_130 { |ctx|
  test.timeout(ctx, 90s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-product-cancel")?
  let child_file = fp"{root}/child"
  let ready = fp"{root}/ready"
  for name in ["Cargo.toml", "rust-toolchain.toml", "xsht-config.ini"] { fp"{root}/{name}".write("") }
  fp"{root}/dev".mkdir()
  fp"{root}/dev/gate_product.xsh".write(
    f"p\"{child_file}\".write(f\"{{process.current_pid()?}}\")?\np\"{ready}\".write(\"ready\")?\ntime.sleep(30s)?\n")
  fp"{root}/tools".mkdir()
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  let products = fp"{root}/target/x86_64-unknown-linux-musl/verification"
  products.mkdir()
  for product in ["xsh", "xsht"] { fp"{products}/{product}".write("fixture\n", mode: 0o755) }
  init_gate_repository(root, repository)?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{repository}/dev/main.xsh"
  let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--", "internal", "gate-lane", "--case", "showcase/tokei/correctness"],
    cwd: root, env: {XSH_MODULE_PATH: f"{repository}/dev:{repository}/dev/tests",
      PATH: f"{root}/tools:{env.get_or("PATH", "")?}", XSH_DEV_BUILD_PRELOAD: "", CARGO_TARGET_DIR: fp"{root}/target".display()}, stdout: stdout, stderr: stderr, timeout: 60s)
  let handle = spawn command?
  defer handle.cancel(signal: "KILL", kill_after: 0ms)
  while ! ready.exists()? {
    if let _ = process.wait_timeout([handle], 10ms)? {
      test.fail(stdout.read_text()? + stderr.read_text()?)
    }
  }
  process.kill(handle.pid, signal: "TERM")
  let finished = process.wait_timeout([handle], 2s)?
  if let observed = finished {
    assert observed.status.shell_code()? == 130, stdout.read_text()? + stderr.read_text()?
  } else {
    handle.cancel(signal: "KILL", kill_after: 0ms)
    test.fail("cancelled product inspection did not finish")
  }
  let pid = child_file.read_text()?.trim() as Int
  match process.kill(pid, signal: "0") {
    Err(_) => {}
    Ok(_) => test.fail(f"cancelled product inspection left helper {pid} alive")
  }
}

test test_product_inspection_respects_the_selected_case_deadline { |ctx|
  test.timeout(ctx, 45s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-product-deadline")?
  for name in ["Cargo.toml", "rust-toolchain.toml", "xsht-config.ini"] { fp"{root}/{name}".write("") }
  fp"{root}/dev".mkdir()
  fp"{root}/dev/gate_product.xsh".write("time.sleep(60s)?\n")
  fp"{root}/tools".mkdir()
  fp"{root}/tools/cargo".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  let products = fp"{root}/target/x86_64-unknown-linux-musl/verification"
  products.mkdir()
  for product in ["xsh", "xsht"] { fp"{products}/{product}".write("fixture\n", mode: 0o755) }
  init_gate_repository(root, repository)?
  let source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--case", "showcase/tokei/correctness"])?
}}
"""
  let inherited = env.get_or("PATH", "")?
  let _ = test.expect(ctx, source, status: 2, stdout: ["timed out"], args: [],
    env: {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: ""})?
}

test test_gate_container_preserves_selected_failure_incomplete_and_cancel_statuses { |ctx|
  test.timeout(ctx, 120s)
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "gate-container-status")?
  let cargo_home = fp"{tools}/cargo"
  cargo_home.mkdir()
  fp"{cargo_home}/bin".mkdir()
  fp"{cargo_home}/bin/kache".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let requested_status = fp"{tools}/requested-status"
  let invocation = fp"{tools}/invocation"
  fp"{tools}/docker".write(f"""#!{ctx.xsh_bin}
if args[0] == "image" {{
  let inspect_status = env.get_or("INSPECT_STATUS", "0")? as Int
  let identity_status = env.get_or("IDENTITY_STATUS", "0")? as Int
  exit inspect_status when inspect_status != 0
  if "--format" in args {{
    exit identity_status when identity_status != 0
    let image_id = if env.get_or("BAD_IMAGE_ID", "")? == "1" {{ "invalid" }} else {{ "sha256:0000000000000000000000000000000000000000000000000000000000000000" }}
    print $image_id
  }}
  exit 0
}}
p"{invocation}".write(args.join("\\n"))?
let requested = p"{requested_status}".read_text()?.trim() as Int
exit requested
""", mode: 0o755)
  let inherited = env.get_or("PATH", "")?
  for status in [1, 2, 130] {
    requested_status.write(f"{status}")
    let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.execute(fixtures.linux_context(p"{repository}")?, ["--case", "showcase/tokei/correctness"])?
}}
main()?
""", status: status, stdout: [f"raw_status={status}"], args: [], env: {{
      PATH: f"{tools}:{inherited}", CARGO_HOME: cargo_home,
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TEST_IMAGE_BUILD: "0", XSH_TEST_IMAGE: "xsh-test:status-fixture",
    }})?
    assert "--source-digest" in invocation.read_lines()?
  }
  invocation.unlink()?
  for scenario in [{inspect: "1", identity: "0", expected: 2}, {inspect: "0", identity: "1", expected: 2}, {inspect: "130", identity: "0", expected: 130}, {inspect: "0", identity: "130", expected: 130}] {
    let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.execute(fixtures.linux_context(p"{repository}")?, ["--case", "showcase/tokei/correctness"])?
}}
main()?
""", status: scenario.expected, args: [], env: {{
    PATH: f"{tools}:{inherited}", CARGO_HOME: cargo_home,
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", INSPECT_STATUS: scenario.inspect, IDENTITY_STATUS: scenario.identity,
    XSH_TEST_IMAGE_BUILD: "0", XSH_TEST_IMAGE: "xsh-test:status-fixture",
  }})?
    assert ! invocation.exists()?
  }
  let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.execute(fixtures.linux_context(p"{repository}")?, ["--case", "showcase/tokei/correctness"])?
}}
main()?
""", status: 2, stdout: ["image unavailable"], args: [], env: {{
    PATH: f"{tools}:{inherited}", CARGO_HOME: cargo_home,
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", BAD_IMAGE_ID: "1",
    XSH_TEST_IMAGE_BUILD: "0", XSH_TEST_IMAGE: "xsh-test:status-fixture",
  }})?
  assert ! invocation.exists()?
  let _ = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.execute(fixtures.linux_context(p"{repository}")?, ["--case", "showcase/tokei/correctness"])?
}}
main()?
""", status: 2, stdout: ["different image ID"], args: [], env: {{
    PATH: f"{tools}:{inherited}", CARGO_HOME: cargo_home,
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_TEST_IMAGE_BUILD: "0",
    XSH_TEST_IMAGE: "sha256:1111111111111111111111111111111111111111111111111111111111111111",
  }})?
  assert ! invocation.exists()?
}

test test_native_file_case_ids_replay_the_same_owned_case_and_products {
  let repository = fs.cwd()?
  let ctx = fixtures.linux_context(repository)?
  let path_plan = gates.plan_paths(ctx, tests.Verification, ["tests/xsh/stdlib/env.xsh"], [], gates.Change)?
  assert path_plan.cases.len() == 1
  let id = path_plan.cases[0].id
  let replay = gates.plan_paths(ctx, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in replay.cases] == [id]
  assert replay.cases[0].execution == path_plan.cases[0].execution
  assert replay.cases[0].tier == path_plan.cases[0].tier
  assert replay.products == path_plan.products
  for ids in [
    ["native-tests/xsh/not-a-real-test.xsh"], ["native-src/runner.xsh"], ["native-unknown/test.xsh"],
    ["native-/etc/passwd.xsh"], ["native-tests/xsh/../../outside.xsh"], [id, id],
  ] {
    match gates.plan_paths(ctx, tests.Verification, [], ids, gates.Change) {
      Ok(_) => test.fail("invalid native file selection was accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  match gates.parse_request(["--case", id, "--case", id]) {
    Ok(_) => test.fail("duplicate explicit case IDs were accepted")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_native_file_case_discovery_rejects_symlink_escapes { |ctx|
  let root = test.temp_dir(ctx, name: "native-case-owned-root")?
  let outside = test.temp_dir(ctx, name: "native-case-outside-root")?
  fp"{root}/tests".mkdir()
  fp"{root}/tests/xsh".mkdir()
  let target = fp"{outside}/external.xsh"
  target.write("test test_external {}")
  fs.symlink(target, fp"{root}/tests/xsh/escape.xsh")
  match gates.plan_paths(fixtures.linux_context(root)?, tests.Verification, [], ["native-tests/xsh/escape.xsh"], gates.Change) {
    Ok(_) => test.fail("native case discovery escaped its owned root")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_full_catalogue_validates_alias_dependency_and_product_closure {
  let ctx = fixtures.linux_context(/repo)?
  let command = {stage: "substitute", target: ctx.target.triple, executable: "never-start",
    argv: ["never-start"], cwd: ctx.root, environment: {}}
  let base = gates.CatalogCase(id: "prepare", tier: gates.Change, limit: gates.ManualOwner,
    products: [], prerequisite_ids: [], reasons: ["fixture"],
    execution: gates.ManualPreparation({products: [tests.Xsht], command: command}))
  let check = {...base, id: "check", products: [tests.Xsht], prerequisite_ids: ["prepare"], execution: gates.ManualCommand(command)}
  let alias = {...base, id: "alias", prerequisite_ids: ["check"], execution: gates.Alias}
  let full = {...base, id: "full", prerequisite_ids: ["alias"], execution: gates.ManualFull}
  let valid = gates.resolve_full([base, check, alias, full])?
  assert [entry.id for entry in valid.cases] == ["prepare", "check", "alias", "full"]
  assert valid.products == [tests.Xsht]
  let missing = {...check, prerequisite_ids: ["missing"]}
  let cycle = {...check, prerequisite_ids: ["alias"]}
  let unprepared = {...check, products: [tests.Xshi]}
  for catalogue in [
    [base, missing, alias, full], [base, cycle, alias, full],
    [base, unprepared, alias, full], [base, check, check, alias, full],
    [base, check, alias], [base, check, alias, full, full],
  ] {
    match gates.resolve_full(catalogue) {
      Ok(_) => test.fail("invalid manual closure was accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  match gates.resolve_plan([base, check, alias, full], ["alias"], gates.Change) {
    Ok(_) => test.fail("a manual child acquired focused authority through its alias")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_full_catalogue_preserves_the_existing_batch_owner_sequence {
  let ctx = fixtures.linux_context(/repo)?
  let rust = tests.rust_plan(ctx, tests.Verification, privileged: true)?
  let plan = gates.full_plan(ctx, tests.Verification)?
  let commands = collect {
    for entry in plan.cases {
      match entry.execution {
        gates.ManualPreparation(preparation) => yield preparation.command
        gates.ManualCommand(spec) => yield spec
        gates.RunCommand(_) | gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualNative | gates.ManualDocumentation => {}
      }
    }
  }
  assert commands == [@rust, @gates.batch_checks(ctx, tests.Verification)]
  assert [entry.id for entry in plan.cases |> where .execution == gates.ManualNative] == ["full/native"]
  assert [entry.id for entry in plan.cases |> where .execution == gates.ManualDocumentation] == ["full/docs"]
  assert plan.products == tests.all_products()
}

test test_manual_dispatch_validates_every_closure_before_any_substitute_process { |ctx|
  test.timeout(ctx, 15s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "full-dispatch-contract")?
  let marker = fp"{root}/started"
  let runner = fp"{root}/substitute.sh"
  runner.write("printf '%s\\n' \"$1\" >> \"$2\"\n")
  let source = f"""
use gates
use fixtures
use test_workflows as tests
proc main(...arguments: List[Str]) [fs, process, env, time, error, io] {{
  let ctx = fixtures.linux_context(p"{root}")?
  let prepare_command = {{stage: "fixture-prepare", target: ctx.target.triple, executable: "sh",
    argv: ["sh", "{runner}", "prepare", "{marker}"], cwd: ctx.root, environment: {{}}}}
  let check_command = {{...prepare_command, stage: "fixture-check", argv: ["sh", "{runner}", "check", "{marker}"]}}
  let preparation = gates.CatalogCase(id: "prepare", tier: gates.Change, limit: gates.ManualOwner,
    products: [], prerequisite_ids: [], reasons: ["fixture"],
    execution: gates.ManualPreparation({{products: [tests.Xsht], command: prepare_command}}))
  let check = {{...preparation, id: "check", products: [tests.Xsht], prerequisite_ids: ["prepare"], execution: gates.ManualCommand(check_command)}}
  let alias = {{...preparation, id: "alias", prerequisite_ids: ["check"], execution: gates.Alias}}
  let full = {{...preparation, id: "full", prerequisite_ids: ["alias"], execution: gates.ManualFull}}
  let variant = arguments[0]
  let entries = match variant {{
    "missing" => [preparation, {{...check, prerequisite_ids: ["missing"]}}, alias, full]
    "cycle" => [preparation, {{...check, prerequisite_ids: ["alias"]}}, alias, full]
    "duplicate" => [preparation, check, check, alias, full]
    "missing-root" => [preparation, check, alias]
    "wrong-root" => [preparation, check, alias, {{...full, execution: gates.RunCommand(check_command)}}]
    "empty-root" => [preparation, check, alias, {{...full, prerequisite_ids: []}}]
    "unprovided" => [{{...preparation, execution: gates.ManualPreparation({{products: [], command: prepare_command}})}}, check, alias, full]
    "manual-runtime-limit" => [{{...preparation, limit: gates.RuntimeSeconds(1)}}, check, alias, full]
    "automatic-manual-owner" => [preparation, {{...check, execution: gates.RunCommand(check_command)}}, alias, full]
    "zero-runtime" => [preparation, {{...check, limit: gates.RuntimeSeconds(0), execution: gates.RunCommand(check_command)}}, alias, full]
    "product-order" => [preparation, {{...check, prerequisite_ids: []}}, alias, {{...full, prerequisite_ids: ["check", "prepare"]}}]
    else => [preparation, check, alias, full]
  }}
  let request = if variant == "focused" {{ gates.parse_request(["--case", "alias"])? }} else {{ gates.parse_request(["--full"])? }}
  gates.dispatch_catalogue(ctx, tests.Verification, request, entries)?
}}
"""
  for variant in ["missing", "cycle", "duplicate", "missing-root", "wrong-root", "empty-root", "unprovided", "product-order", "manual-runtime-limit", "automatic-manual-owner", "zero-runtime", "focused"] {
    let _ = test.expect(ctx, source, status: 2, args: [variant],
      env: {XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev"})?
    assert ! marker.exists()?
  }
  let _ = test.expect(ctx, source, status: 0, args: ["valid"],
    env: {XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev"})?
  assert marker.read_lines()? == ["prepare", "check"]
}

test test_internal_full_rejects_automatic_operation_identity_before_effects {
  let ctx = fixtures.linux_context(/path-that-must-not-be-created)?
  for operation in ["", "gate-lane", "test-linux", "plan.json"] {
    match container_internal.gate_full(ctx, operation) {
      Ok(_) => test.fail("an automatic or serialized operation acquired Full authority")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
}

test test_internal_full_validation_never_starts_verification_commands { |ctx|
  test.timeout(ctx, 60s)
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "full-validation-forbidden-processes")?
  let marker = fp"{tools}/started"
  for name in ["cargo", "docker"] {
    fp"{tools}/{name}".write(f"#!/bin/sh\ntouch '{marker}'\nexit 99\n", mode: 0o755)
  }
  let source = f"""
proc main(...arguments: List[Str]) [process, error] {{
  let command = process.command_argv(p"{ctx.xsh_bin}",
    ["{ctx.xsh_bin}", "{repository}/dev/main.xsh", "--", "internal", "gate-validate-full", @arguments], cwd: p"{repository}")
  let status = process.run(command)?
  exit status.shell_code()?
}}
"""
  let environment = {PATH: f"{tools}:{env.get_or("PATH", "")?}",
    XSH_MODULE_PATH: f"{repository}/dev:{repository}/dev/tests", TARGET: "x86_64-unknown-linux-musl", DIST_PROFILE: "verification"}
  let _ = test.expect(ctx, source, status: 0, args: [], env: environment)?
  assert ! marker.exists()?
  let _ = test.expect(ctx, source, status: 3, args: ["--full"], env: environment)?
  assert ! marker.exists()?
}

test test_laputa_root_is_explicit_and_scoped {
  for arguments in [
    ["--case", "laputa/pm.make/plan"],
    ["--laputa-root", "/laputa"],
    ["--case", "laputa/pm.make/plan", "--laputa-root", "relative"],
    ["--case", "laputa/pm.make/plan", "--laputa-root", "/laputa/../other"],
    ["--case", "laputa/pm.make/plan", "--laputa-root", "/laputa", "--laputa-root", "/other"],
  ] {
    match gates.parse_request(arguments) {
      Ok(_) => test.fail("invalid Laputa request accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  let _ = gates.parse_request(["--plan", "--case", "laputa/pm.make/plan", "--laputa-root", "/laputa"])?
  let context = fixtures.linux_context(/repo)?
  let selected = gates.plan_paths(context, tests.Verification, [], ["laputa/pm.make/plan"], gates.Change)?
  assert [entry.id for entry in selected.cases] == ["laputa/pm.make/plan"]
  assert selected.cases[0].tier == gates.Scenario
}

test test_laputa_kbuild_discovery_is_an_explicit_scenario {
  let id = "laputa/kbuild/discovery"
  for arguments in [
    ["--case", id],
    ["--laputa-root", "/laputa"],
    ["--case", id, "--case", "laputa/pm.make/plan", "--laputa-root", "/laputa"],
    ["--case", id, "--case", "performance/json-n/correctness", "--laputa-root", "/laputa"],
  ] {
    match gates.parse_request(arguments) {
      Ok(_) => test.fail("invalid Laputa Kbuild request accepted")
      Err(error) => assert error.message == "PlanError.Invalid"
    }
  }
  let _ = gates.parse_request(["--plan", "--case", id, "--laputa-root", "/laputa"])?
  let selected = gates.plan_paths(fixtures.linux_context(/repo)?, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in selected.cases] == [id]
  assert selected.cases[0].tier == gates.Scenario
  assert selected.cases[0].products == [tests.Xsh, tests.Xsht]
}

test test_laputa_root_composition_is_an_explicit_scenario {
  let id = "laputa/pm.root/compose"
  match gates.parse_request(["--case", id]) {
    Ok(_) => test.fail("Laputa composition was accepted without a root")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
  let _ = gates.parse_request(["--plan", "--case", id, "--laputa-root", "/laputa"])?
  let selected = gates.plan_paths(fixtures.linux_context(/repo)?, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in selected.cases] == [id]
  assert selected.cases[0].tier == gates.Scenario
  assert selected.cases[0].products == [tests.Xsh, tests.Xsht]
}

test test_laputa_package_inventory_is_an_explicit_scenario {
  let id = "laputa/pm.plan/inventory"
  match gates.parse_request(["--case", id]) {
    Ok(_) => test.fail("Laputa package inventory was accepted without a root")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
  let _ = gates.parse_request(["--plan", "--case", id, "--laputa-root", "/laputa"])?
  let selected = gates.plan_paths(fixtures.linux_context(/repo)?, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in selected.cases] == [id]
  assert selected.cases[0].tier == gates.Scenario
  assert selected.cases[0].products == [tests.Xsh, tests.Xsht]
}

test test_laputa_kbuild_graph_cache_is_an_explicit_scenario {
  let id = "laputa/kbuild/graph-cache"
  match gates.parse_request(["--case", id]) {
    Ok(_) => test.fail("Laputa Kbuild graph cache was accepted without a root")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
  let _ = gates.parse_request(["--plan", "--case", id, "--laputa-root", "/laputa"])?
  let selected = gates.plan_paths(fixtures.linux_context(/repo)?, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in selected.cases] == [id]
  assert selected.cases[0].tier == gates.Scenario
  assert selected.cases[0].products == [tests.Xsh, tests.Xsht]
}

test test_laputa_world_host_plan_is_an_explicit_scenario {
  let id = "laputa/world/host-plan"
  match gates.parse_request(["--case", id]) {
    Ok(_) => test.fail("Laputa world host plan was accepted without a root")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
  let _ = gates.parse_request(["--plan", "--case", id, "--laputa-root", "/laputa"])?
  let selected = gates.plan_paths(fixtures.linux_context(/repo)?, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in selected.cases] == [id]
  assert selected.cases[0].tier == gates.Scenario
  assert selected.cases[0].products == [tests.Xsh, tests.Xsht]
}

test test_laputa_kbuild_make_semantics_is_an_explicit_scenario {
  let id = "laputa/kbuild/make-semantics"
  match gates.parse_request(["--case", id]) {
    Ok(_) => test.fail("Laputa Kbuild Make semantics was accepted without a root")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
  let _ = gates.parse_request(["--plan", "--case", id, "--laputa-root", "/laputa"])?
  let selected = gates.plan_paths(fixtures.linux_context(/repo)?, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in selected.cases] == [id]
  assert selected.cases[0].tier == gates.Scenario
  assert selected.cases[0].products == [tests.Xsh, tests.Xsht]
}

test test_laputa_preparation_selects_and_validates_its_requested_case { |ctx|
  let repository = fs.cwd()?
  for pair in [
    {id: "laputa/pm.make/plan", other: "laputa/kbuild/discovery"},
    {id: "laputa/kbuild/discovery", other: "laputa/pm.make/plan"},
    {id: "laputa/kbuild/graph-cache", other: "laputa/pm.make/plan"},
    {id: "laputa/kbuild/make-semantics", other: "laputa/pm.make/plan"},
    {id: "laputa/pm.execute/reuse-count", other: "laputa/pm.make/plan"},
    {id: "laputa/pm.root/compose", other: "laputa/pm.make/plan"},
    {id: "laputa/pm.plan/inventory", other: "laputa/pm.make/plan"},
    {id: "laputa/world/host-plan", other: "laputa/pm.make/plan"},
  ] {
    let source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{repository}")?, ["--laputa-preparation", "{pair.id}", "--plan", "--tier", "change"])?
}}
main()?
"""
    let result = test.expect(ctx, source, status: 0, args: [], env: {{
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: "",
    }})?
    assert pair.id in result.stdout, result.stdout
    assert pair.other not in result.stdout, result.stdout
  }
  let invalid = test.expect(ctx, f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{repository}")?, ["--laputa-preparation", "laputa/full", "--plan", "--tier", "change"])?
}}
main()?
""", status: 2, stdout: ["known case ID"], args: [], env: {{
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: "",
  }})?
  assert "laputa/pm.make/plan" not in invalid.stdout
}

test test_laputa_pm_reuse_is_an_explicit_separate_scenario {
  let id = "laputa/pm.execute/reuse-count"
  let _ = gates.parse_request(["--plan", "--case", id, "--laputa-root", "/laputa"])?
  let selected = gates.plan_paths(fixtures.linux_context(/repo)?, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in selected.cases] == [id]
  assert selected.cases[0].tier == gates.Scenario
  assert selected.cases[0].products == [tests.Xsh, tests.Xsht, tests.Xshi]
  let pm = gates.plan_paths(fixtures.linux_context(/repo)?, tests.Verification, [], ["laputa/pm.make/plan"], gates.Change)?
  let kbuild = gates.plan_paths(fixtures.linux_context(/repo)?, tests.Verification, [], ["laputa/kbuild/discovery"], gates.Change)?
  assert pm.cases[0].products == [tests.Xsh, tests.Xsht]
  assert kbuild.cases[0].products == [tests.Xsh, tests.Xsht]
  match gates.parse_request(["--case", id]) {
    Ok(_) => test.fail("Laputa reuse request without root was accepted")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_laputa_and_json_n_must_be_selected_separately {
  match gates.parse_request([
    "--case", "laputa/pm.make/plan",
    "--case", "performance/json-n/correctness",
    "--laputa-root", "/laputa",
  ]) {
    Ok(_) => test.fail("Laputa and json-n mixed selection was accepted")
    Err(error) => assert error.message == "PlanError.Invalid"
  }
}

test test_core_workload_scenarios_require_explicit_selection {
  let context = fixtures.linux_context(/repo)?
  for id in ["performance/core-json-small/correctness", "performance/core-json-medium/correctness", "performance/core-json-large/correctness"] {
    let selected = gates.plan_paths(context, tests.Verification, [], [id], gates.Change)?
    assert [entry.id for entry in selected.cases] == [id]
    assert selected.products == [tests.Xsh]
  }
  let json_trace = gates.plan_paths(context, tests.Verification, [], ["performance/core-json-small/trace"], gates.Change)?
  assert [entry.id for entry in json_trace.cases] == ["performance/core-json-small/trace"]
  assert json_trace.products == [tests.Xsht]
  for id in ["performance/core-wc-small/correctness", "performance/core-wc-medium/correctness", "performance/core-wc-large/correctness", "performance/core-sort-small/correctness", "performance/core-sort-medium/correctness", "performance/core-sort-large/correctness", "performance/core-traversal-small/correctness", "performance/core-traversal-medium/correctness", "performance/core-traversal-large/correctness", "performance/core-awk-small/correctness", "performance/core-awk-medium/correctness", "performance/core-awk-large/correctness"] {
    let selected = gates.plan_paths(context, tests.Verification, [], [id], gates.Change)?
    assert [entry.id for entry in selected.cases] == [id]
    assert selected.products == [tests.Xsh]
    assert selected.cases[0].tier == gates.Scenario
  }
  assert gates.plan_paths(context, tests.Verification, [], [], gates.Change)?.cases.is_empty()
  for changed_path in ["dev/performance/core_cases.py", "dev/performance/core_pipeline.py", "dev/performance/core_trace.py", "dev/performance/test_core_cases.py", "dev/performance/fixtures/core-cases/traversal.xsh"] {
    let changed = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in changed.cases] == ["verification/core-workloads"]
  }
  for id in ["performance/core-wc-small/trace", "performance/core-wc-medium/trace", "performance/core-sort-small/trace", "performance/core-awk-small/trace", "performance/core-traversal-small/trace", "performance/core-traversal-medium/trace", "performance/core-traversal-large/trace", "performance/core-spawn-small/trace", "performance/core-spawn-medium/trace", "performance/core-spawn-large/trace"] {
    let trace = gates.plan_paths(context, tests.Verification, [], [id], gates.Change)?
    assert [entry.id for entry in trace.cases] == [id]
    assert trace.products == [tests.Xsht]
    assert trace.cases[0].tier == gates.Scenario
  }
  for id in ["performance/core-pipeline-small/correctness", "performance/core-pipeline-medium/correctness", "performance/core-pipeline-large/correctness"] {
    let pipeline = gates.plan_paths(context, tests.Verification, [], [id], gates.Change)?
    assert [entry.id for entry in pipeline.cases] == [id]
    assert pipeline.products == [tests.Xsht]
    assert pipeline.cases[0].tier == gates.Scenario
  }
}

test test_inference_corpus_changes_do_not_select_unrelated_native_tests {
  let context = fixtures.linux_context(/repo)?
  for changed_path in ["dev/performance/corpus/check.sh", "dev/performance/corpus/check_views.sh", "dev/performance/corpus/witnesses/source-read.erased.xsh"] {
    let selected = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert selected.cases.is_empty()
    assert selected.unresolved == [f"inference corpus verification requires explicit external inputs: {changed_path}"]
  }
}

test test_frozen_witness_tool_changes_select_their_contract_tests {
  let context = fixtures.linux_context(/repo)?
  for changed_path in ["dev/performance/corpus/frozen_witnesses.py", "dev/performance/corpus/test_frozen_witnesses.py"] {
    let selected = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in selected.cases] == ["verification/frozen-witnesses"]
    assert selected.products.is_empty()
    assert selected.unresolved.is_empty()
  }
}

test test_frontend_disposal_scenarios_select_exact_runtime_controls {
  let context = fixtures.linux_context(/repo)?
  for id in ["compiler/frontend-disposal/schemes", "compiler/frontend-disposal/compact-entry"] {
    let selected = gates.plan_paths(context, tests.Verification, [], [id], gates.Change)?
    assert [entry.id for entry in selected.cases] == [id]
    assert selected.products.is_empty()
    assert selected.cases[0].tier == gates.Scenario
    match selected.cases[0].execution {
      gates.RunCommand(spec) => assert "--exact" in spec.argv
      else => test.fail("frontend disposal case must run its exact Rust control")
    }
  }
}

test test_frontend_scenario_and_owner_are_explicit {
  let context = fixtures.linux_context(/repo)?
  let id = "performance/frontend-edit/correctness"
  let scenario = gates.plan_paths(context, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in scenario.cases] == [id]
  assert scenario.products == [tests.Xsh, tests.Xsht]
  assert scenario.cases[0].tier == gates.Scenario
  for changed_path in ["dev/performance/frontend_cases.py", "dev/performance/frontend_scenario.py", "dev/performance/test_frontend_cases.py"] {
    let selected = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in selected.cases] == ["verification/inference-workloads"]
  }
  let shared = gates.plan_paths(context, tests.Verification, ["dev/performance/owned_case.py"], [], gates.Change)?
  assert [entry.id for entry in shared.cases] == ["verification/core-workloads", "verification/inference-workloads"]
}

test test_inference_workload_changes_select_their_python_contracts {
  let context = fixtures.linux_context(/repo)?
  for changed_path in ["dev/performance/inference_measurement.py", "dev/performance/test_inference_measurement.py", "dev/performance/inference_workloads.py", "dev/performance/test_inference_workloads.py"] {
    let selected = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in selected.cases] == ["verification/inference-workloads"]
    assert selected.products.is_empty()
  }
}

test test_json_n_case_has_correctness_only_scope_and_requires_explicit_selection {
  let context = fixtures.linux_context(/repo)?
  let id = "performance/json-n/correctness"
  let selected = gates.plan_paths(context, tests.Verification, [], [id], gates.Change)?
  assert [entry.id for entry in selected.cases] == [id]
  assert selected.products == [tests.Xsh]
  assert selected.cases[0].tier == gates.Scenario
  assert selected.cases[0].reasons == [
    "json-n output/status/state correctness only; product and source bound to gate preparation; performance not measured",
  ]
  assert gates.plan_paths(context, tests.Verification, [], [], gates.Change)?.cases.is_empty()
}

test test_performance_python_owners_select_contract_tests {
  let context = fixtures.linux_context(/repo)?
  let harness = gates.plan_paths(context, tests.Verification, ["bench/consolidation/run.py"], [], gates.Change)?
  assert [entry.id for entry in harness.cases] == ["verification/consolidation-harness", "verification/core-workloads"]
  for changed_path in ["bench/consolidation/test_harness.py", "bench/consolidation/fixtures.py", "bench/consolidation/prepare.py"] {
    let plan = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in plan.cases] == ["verification/consolidation-harness"]
  }
  for changed_path in ["dev/performance/tokei_scenario.py", "dev/performance/test_tokei_scenario.py"] {
    let plan = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in plan.cases] == ["verification/tokei-scenario"]
    match plan.cases[0].execution {
      gates.RunCommand(spec) => assert spec.argv[1..] == ["-B", "-m", "unittest", "dev.performance.test_tokei_scenario"]
      else => test.fail("Tokei scenario owner did not select its contract test")
    }
  }
  for changed_path in ["dev/performance/tokei_corpus.py", "dev/performance/test_tokei_corpus.py"] {
    let plan = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in plan.cases] == ["verification/tokei-corpus"]
    match plan.cases[0].execution {
      gates.RunCommand(spec) => assert spec.argv[1..] == ["-B", "-m", "unittest", "dev.performance.test_tokei_corpus"]
      else => test.fail("Tokei corpus owner did not select its contract test")
    }
  }
  for changed_path in ["dev/performance/focused.py", "dev/performance/test_focused.py"] {
    let plan = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in plan.cases] == ["verification/focused-adapter"]
    assert plan.products.is_empty()
    match plan.cases[0].execution {
      gates.RunCommand(spec) => assert spec.argv[1..] == [
        "-B", "-m", "unittest", "dev.performance.test_focused",
        "dev.performance.test_check.AdapterDeadlineTests",
      ]
      else => test.fail("focused Python owner did not select its contract test")
    }
  }
  for changed_path in ["dev/performance/check.py", "dev/performance/test_check.py"] {
    let plan = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in plan.cases] == ["verification/performance-check"]
    match plan.cases[0].execution {
      gates.RunCommand(spec) => assert spec.argv[1..] == ["-B", "-m", "unittest", "dev.performance.test_check"]
      else => test.fail("performance check owner did not select its full contract suite")
    }
  }
  let workloads = gates.plan_paths(context, tests.Verification, ["dev/performance/workloads.py"], [], gates.Change)?
  assert [entry.id for entry in workloads.cases] == [
    "verification/focused-adapter", "verification/performance-check", "verification/core-workloads",
  ]
  for changed_path in ["dev/performance/resources.py", "dev/performance/test_resources.py"] {
    let plan = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in plan.cases] == ["verification/resource-diagnostics"]
    match plan.cases[0].execution {
      gates.RunCommand(spec) => assert spec.argv[1..] == ["-B", "-m", "unittest", "dev.performance.test_resources"]
      else => test.fail("resource Python owner did not select its contract test")
    }
  }
  for changed_path in ["dev/performance/laputa_plan.py", "dev/performance/test_laputa_plan.py"] {
    let plan = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in plan.cases] == ["verification/laputa-adapter"]
    match plan.cases[0].execution {
      gates.RunCommand(spec) => assert spec.argv[1..] == ["-B", "-m", "unittest", "dev.performance.test_laputa_plan"]
      else => test.fail("Laputa Python owner did not select its contract test")
    }
  }
  for changed_path in ["dev/util-linux/leases.py", "dev/util-linux/test_leases.py"] {
    let plan = gates.plan_paths(context, tests.Verification, [changed_path], [], gates.Change)?
    assert [entry.id for entry in plan.cases] == ["verification/native-leases"]
    match plan.cases[0].execution {
      gates.RunCommand(spec) => assert spec.argv[1..] == ["-B", "dev/util-linux/test_leases.py"]
      else => test.fail("native lease Python owner did not select its contract test")
    }
  }
}

test test_focused_json_n_binds_prepared_product_and_rejects_incomplete_report { |ctx|
  test.timeout(ctx, 60s)
  let repository = fs.cwd()?
  let owned = fs.tempdir_in(fp"{repository}/.work/tmp")?
  let tools = owned.host_path()?
  let cargo_home = fp"{tools}/cargo"
  fp"{cargo_home}/bin".mkdir(parents: true)?
  fp"{cargo_home}/bin/kache".write("#!/bin/sh\nexit 0\n", mode: 0o755)?
  let target_dir = fp"{tools}/target"
  let product = fp"{target_dir}/x86_64-unknown-linux-musl/verification/xsh"
  let preparation = fp"{tools}/preparation"
  let image_inspections = fp"{tools}/image-inspections"
  let adapter_config = fp"{tools}/adapter-config.json"
  let image = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  let python = (run.text which python3)?.trim()
  fp"{tools}/docker".write(f"""#!{ctx.xsh_bin}
if args[0] == "image" {{
  let calls = if p"{image_inspections}".exists()? {{ p"{image_inspections}".read_text()? }} else {{ "" }}
  p"{image_inspections}".write(calls + args.join(" ") + "\\n")?
  if "--format" in args {{ print "{image}" }}
  exit 0
}}
if "gate-lane" in args {{
  p"{product.parent()}".mkdir(parents: true)?
  p"{product}".write("prepared XSH product\\n", mode: 0o755)?
  let pinned = env.get_or("XSH_TEST_IMAGE", "")?
  let build = env.get_or("XSH_TEST_IMAGE_BUILD", "")?
  p"{preparation}".write(args.join("\\n") + "\\nPIN=" + pinned + "\\nBUILD=" + build)?
  exit 0
}}
exit 2
""", mode: 0o755)
  fp"{tools}/python3".write(f"""#!/usr/bin/python3
import json,os,sys
from pathlib import Path
real = "{python}"
arguments = sys.argv[1:]
if arguments[:3] == ["-B", "-m", "dev.performance.focused"]:
    config_path = arguments[4]
    with open(config_path) as stream:
        config = json.load(stream)
    fixture = Path(config["measurement"]["fixtures"]) / "2048/expected/json"
    if not fixture.is_file():
        raise SystemExit(2)
    with open("{adapter_config}", "w") as stream:
        json.dump(config, stream)
    binding = config["gate_binding"]
    report = {{
        "case": "json-n",
        "correctness_complete": True,
        "cleanup_complete": True,
        "full_cohort_run": False,
        "performance_complete": False,
        "timing_complete": False,
        "performance_status": "not-measured",
        "product_source_binding": "gate-prepared-product-bound-to-source-snapshot",
        "image_id": config["image_id"],
        "gate_binding": binding,
        "product": {{"sha256": binding["product_digest"], "mode": binding["product_mode"]}},
        "resource_diagnostics": {{
            "schema": 1,
            "authority": "diagnostic_only",
            "scope": "fresh_case_container",
            "status": "incomplete",
            "reason": "memory PSI unavailable",
        }},
    }}
    if os.environ.get("REPORT_MODE") == "missing":
        del report["performance_status"]
    with open(config["measurement"]["output"], "w") as stream:
        json.dump(report, stream)
    raise SystemExit(int(os.environ.get("ADAPTER_STATUS", "0")))
os.execv(real, [real, *arguments])
""", mode: 0o755)
  let inherited = env.get_or("PATH", "")?
  let source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  let base = fixtures.linux_context(p"{repository}", "verification")?
  gates.execute({{...base, target_dir: p"{target_dir}"}}, ["--case", "performance/json-n/correctness"])?
}}
main()?
"""
  let result = test.expect(ctx, source, status: 0,
    stdout: ["Selected scope: json-n correctness passed", "Resource diagnostics (diagnostic only):",
      "status", "incomplete", "memory PSI unavailable", "Performance: not measured", "Full cohort: not run"],
    args: [], env: {{
      PATH: f"{tools}:{inherited}",
      CARGO_HOME: cargo_home,
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TEST_IMAGE_BUILD: "0", XSH_TEST_IMAGE: "fixture", DIST_PROFILE: "verification",
    }})?
  assert result.status == 0
  let prepared = preparation.read_text()?
  assert "performance/json-n/correctness" in prepared
  assert "--source-digest" in prepared
  assert f"PIN={image}" in prepared
  assert "BUILD=0" in prepared
  assert image in image_inspections.read_text()?
  let config = json.decode(adapter_config.read_text()?)?.require(Record)?
  assert config.get("image_id")?.require(Str)? == image
  let binding = config.get("gate_binding")?.require(Record)?
  let source_digest = cd (repository) {{ run.text python3 -B dev/gate_source.py }}?
  assert binding.get("source_digest")?.require(Str)? == source_digest.trim()
  assert binding.get("product_digest")?.require(Str)? == hash.sha256(product)?.hex()
  assert binding.get("product_mode")?.require(Int)? == 0o755
  let unavailable = test.expect(ctx, source, status: 2, stdout: ["incomplete", "results unavailable"],
    args: [], env: {{
      PATH: f"{tools}:{inherited}",
      CARGO_HOME: cargo_home,
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TEST_IMAGE_BUILD: "0", XSH_TEST_IMAGE: "fixture", DIST_PROFILE: "verification",
      REPORT_MODE: "missing",
    }})?
  assert unavailable.status == 2
  assert "Selected scope: json-n correctness passed" not in unavailable.stdout
}

test test_laputa_mixed_case_preserves_regular_child_status { |ctx|
  test.timeout(ctx, 180s)
  let repository = fs.cwd()?
  let tools = test.temp_dir(ctx, name: "laputa-mixed")?
  let laputa = fp"{tools}/laputa"
  laputa.mkdir()
  let _ = run.capture --text git -C $laputa init --quiet
  let cargo_home = fp"{tools}/cargo"
  cargo_home.mkdir()
  fp"{cargo_home}/bin".mkdir()
  fp"{cargo_home}/bin/kache".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let desired = fp"{tools}/status"
  let adapter_status = fp"{tools}/adapter-status"
  let adapter_called = fp"{tools}/adapter-called"
  let python = (run.text which python3)?.trim()
  fp"{tools}/python3".write(f"""#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = "--xsh" ]; then
    printf '%s\\n' "$@" > '{adapter_called}'
    exit "$(cat '{adapter_status}')"
  fi
done
exec '{python}' "$@"
""", mode: 0o755)
  let invocation = fp"{tools}/invocation"
  let preparation_invocation = fp"{tools}/preparation"
  let tag_seen = fp"{tools}/tag-seen"
  let build_calls = fp"{tools}/image-builds"
  fp"{tools}/docker".write(f"""#!{ctx.xsh_bin}
if args[0] == "build" {{
  let log = p"{build_calls}"
  let existing = if log.exists()? {{ log.read_text()? }} else {{ "" }}
  log.write(existing + "build\\n")?
  exit env.get_or("IMAGE_SETUP_STATUS", "0")? as Int
}}
if args[0] == "image" {{
  let setup_status = env.get_or("IMAGE_SETUP_STATUS", "0")? as Int
  exit setup_status when setup_status != 0
  if "--format" in args {{
    if "fixture" in args and p"{tag_seen}".exists()? {{
      print "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    }} else {{
      if "fixture" in args {{ p"{tag_seen}".write("seen")? }}
      print "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }}
  }}
  exit 0
}}
if "--laputa-preparation" in args {{ p"{preparation_invocation}".write(args.join("\\n"))?; exit 0 }}
p"{invocation}".write(args.join("\\n"))?
exit p"{desired}".read_text()?.trim() as Int
""", mode: 0o755)
  let inherited = env.get_or("PATH", "")?
  let source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.execute(fixtures.linux_context(p"{repository}")?, ["--case", "showcase/tokei/correctness", "--case", "laputa/pm.make/plan", "--laputa-root", "{laputa}"])?
}}
main()?
"""
  for pair in [[0, 0], [2, 1], [1, 0], [130, 0]] {
    let status = pair[0]
    let external = pair[1]
    desired.write(f"{status}")
    if tag_seen.exists()? { tag_seen.remove()? }
    adapter_status.write(f"{external}")
    let expected = if status == 130 { 130 } else if status == 1 or external == 1 { 1 } else if status == 2 { 2 } else { 0 }
    let _ = test.expect(ctx, source, status: expected, args: [], env: {{
      PATH: f"{tools}:{inherited}", CARGO_HOME: cargo_home,
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TEST_IMAGE_BUILD: "0", XSH_TEST_IMAGE: "fixture",
    }})?
    assert "laputa/pm.make/plan" not in invocation.read_lines()?
    assert "showcase/tokei/correctness" in invocation.read_lines()?
    let pinned = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    assert pinned in preparation_invocation.read_lines()?
    assert pinned in invocation.read_lines()?
    if status in [0, 2] { assert pinned in adapter_called.read_lines()? }
  }
  invocation.remove()?
  preparation_invocation.remove()?
  let _ = test.expect(ctx, source, status: 2, stdout: ["incomplete"], args: [], env: {{
    PATH: f"{tools}:{inherited}", CARGO_HOME: cargo_home,
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_TEST_IMAGE_BUILD: "0",
    XSH_TEST_IMAGE: "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  }})?
  assert ! invocation.exists()?
  assert ! preparation_invocation.exists()?
  if tag_seen.exists()? { tag_seen.remove()? }
  desired.write("0")
  adapter_status.write("0")
  let _ = test.expect(ctx, source, status: 0, args: [], env: {{
    PATH: f"{tools}:{inherited}", CARGO_HOME: cargo_home,
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
    XSH_TEST_IMAGE_BUILD: "1", XSH_TEST_IMAGE: "fixture",
  }})?
  assert build_calls.read_lines()? == ["build"]
  assert "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" in preparation_invocation.read_lines()?
  invocation.remove()?
  preparation_invocation.remove()?
  for setup in [["0", "1", "2"], ["1", "1", "2"], ["0", "130", "130"], ["1", "130", "130"]] {
    let _ = test.expect(ctx, source, status: setup[2] as Int, args: [], env: {{
      PATH: f"{tools}:{inherited}", CARGO_HOME: cargo_home,
      XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev",
      XSH_TEST_IMAGE_BUILD: setup[0], IMAGE_SETUP_STATUS: setup[1], XSH_TEST_IMAGE: "fixture",
    }})?
    assert ! invocation.exists()?
    assert ! preparation_invocation.exists()?
  }
}

test test_local_inference_case_owns_only_its_compiler_source_and_exact_tests {
  let ctx = fixtures.linux_context(/repo)?
  let id = "compiler/local-inference/correctness"
  let plan = gates.plan_paths(ctx, tests.Verification, ["src/sema/check/local_inference.rs"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == [id]
  let matching = [entry for entry in plan.cases if entry.id == id]
  assert matching.len() == 1
  assert matching[0].tier == gates.Edit
  assert matching[0].limit == gates.RuntimeSeconds(120)
  assert matching[0].products == [tests.Xsh, tests.Xsht]
  match matching[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "tests/xsh/local-inference.xsh::nullable_local_has_one_fixed_optional_type",
      "--exact-id", "tests/xsh/local-inference.xsh::empty_list_collects_loop_contributions_before_earlier_reads",
      "--exact-id", "tests/xsh/local-inference.xsh::nullable_local_rejects_incompatible_branch_contributions",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("local inference correctness was not an exact native command")
  }
  let explicit = gates.plan_paths(ctx, tests.Verification, [], [id], gates.Edit)?
  assert [entry.id for entry in explicit.cases] == [id]
  let changed = gates.plan_paths(ctx, tests.Verification, ["src/sema/check/local_inference.rs"], [], gates.Change)?
  let changed_ids = [entry.id for entry in changed.cases]
  assert id in changed_ids
  assert "test-sema" in changed_ids
  assert "native-tests/xsh/checker-" in changed_ids
  let unrelated = gates.plan_paths(ctx, tests.Verification, ["src/sema/check/host_obligations.rs"], [], gates.Change)?
  assert id not in [entry.id for entry in unrelated.cases]
}


test test_imported_inference_case_owns_scheme_source_and_exact_tests {
  let ctx = fixtures.linux_context(/repo)?
  let id = "compiler/imported-inference/correctness"
  let plan = gates.plan_paths(ctx, tests.Verification, ["src/sema/schemes.rs"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == [id]
  assert plan.cases[0].limit == gates.RuntimeSeconds(120)
  assert plan.products == [tests.Xsh, tests.Xsht]
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "tests/xsh/full-inference-callables.xsh::test_full_inference_imported_generic_alias_instantiates_int_str_bool",
      "--exact-id", "tests/xsh/full-inference-callables.xsh::test_full_inference_imported_alias_flows_through_proc_result_and_producer",
      "--exact-id", "tests/xsh/full-inference-callables.xsh::test_full_inference_private_module_callables_remain_inaccessible",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("imported inference correctness was not an exact native command")
  }
  let explicit = gates.plan_paths(ctx, tests.Verification, [], [id], gates.Edit)?
  assert [entry.id for entry in explicit.cases] == [id]
  let changed = gates.plan_paths(ctx, tests.Verification, ["src/sema/schemes.rs"], [], gates.Change)?
  let changed_ids = [entry.id for entry in changed.cases]
  assert id in changed_ids
  assert "test-sema" in changed_ids
  assert "native-tests/xsh/checker-" in changed_ids
  let unrelated = gates.plan_paths(ctx, tests.Verification, ["src/sema/check/local_inference.rs"], [], gates.Edit)?
  assert [entry.id for entry in unrelated.cases] == ["compiler/local-inference/correctness"]
}


test test_inference_work_case_owns_collection_source_and_exact_unit_test {
  let ctx = fixtures.linux_context(/repo)?
  let id = "compiler/inference-work/correctness"
  let plan = gates.plan_paths(ctx, tests.Verification, ["src/sema/inference_work.rs"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == [id]
  assert plan.cases[0].limit == gates.RuntimeSeconds(120)
  assert plan.products == []
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[5..] == [
      "-p", "xsh", "--lib",
      "frontend_stats::tests::inference_work_invocations_are_isolated_and_generic_calls_count_instantiations",
      "--", "--exact",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("inference work correctness was not an exact Rust unit test")
  }
  let explicit = gates.plan_paths(ctx, tests.Verification, [], [id], gates.Edit)?
  assert [entry.id for entry in explicit.cases] == [id]
  let changed = gates.plan_paths(ctx, tests.Verification, ["src/sema/inference_work.rs"], [], gates.Change)?
  let changed_ids = [entry.id for entry in changed.cases]
  assert id in changed_ids
  assert "test-sema" in changed_ids
  assert "native-tests/xsh/checker-" in changed_ids
  let unrelated = gates.plan_paths(ctx, tests.Verification, ["src/sema/check/local_inference.rs"], [], gates.Edit)?
  assert [entry.id for entry in unrelated.cases] == ["compiler/local-inference/correctness"]
}

test test_cat_edit_selects_bounded_byte_stream_and_error_contracts {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/cat.xsh"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == ["core/cat/byte-stream"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "core/tests/test-cat.xsh::test_cat_file_and_stdin",
      "--exact-id", "core/tests/test-cat.xsh::test_cat_preserves_non_utf8_bytes_across_chunks",
      "--exact-id", "core/tests/test-cat.xsh::test_cat_reports_each_failure_and_exits_one",
      "--exact-id", "core/tests/test-cat.xsh::test_cat_accepts_non_utf8_file_path_arguments",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("cat byte-stream case was not an exact native command")
  }
}

test test_cat_change_selects_its_module_and_exact_external_consumers {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/cat.xsh"], [], gates.Change)?
  assert [entry.id for entry in plan.cases] == ["core/cat/full-module", "core/cat/external-consumers"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == ["test", "core/tests/test-cat.xsh"]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("cat module was not a native file selection")
  }
  match plan.cases[1].execution {
    gates.RunCommand(spec) => {
      assert spec.argv[1] == "test"
      assert spec.argv.len() == 134
      var ids: List[Str] = []
      for index in range(spec.argv.len()) {
        continue when index < 2 or index % 2 != 0
        assert spec.argv[index] == "--exact-id"
        ids += [spec.argv[index + 1]]
      }
      assert ids.len() == 66
      for id in [
        "core/tests/test-bb-cat.xsh::test_bb_cat_cat_n_17793a2a",
        "core/tests/test-gnu-cat.xsh::test_gnu_cat_cat_self_log",
        "core/tests/test-gnu-cksum.xsh::test_gnu_cksum_md5sum_parallel_log",
        "core/tests/test-gnu-cp.xsh::test_gnu_cp_sparse_to_pipe_log",
        "core/tests/test-gnu-misc.xsh::test_gnu_misc_invalid_opt_log",
        "core/tests/test-gnu-wc.xsh::test_gnu_wc_wc_parallel_log",
        "core/tests/test-uu-cat.xsh::test_uu_cat_fifo_symlink",
        "core/tests/test-uu-numfmt-1.xsh::test_uu_numfmt_diagnostics_plain_message_when_stderr_is_a_pipe",
        "core/tests/test-uu-stat.xsh::test_uu_stat_diagnostics_plain_message_when_stderr_is_a_pipe",
        "core/tests/test-uu-support.xsh::test_uu_support_non_utf8_argv",
      ] {
        assert id in ids, f"cat external consumer missing: {id}"
      }
      assert "native-core/tests" not in ids
    }
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("cat external consumers were not exact native tests")
  }
}

test test_uniq_edit_selects_record_grouping_and_stream_contracts {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/uniq.xsh"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == ["core/uniq/records"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "core/tests/test-uniq.xsh::test_uniq_merges_adjacent_lines_and_counts",
      "--exact-id", "core/tests/test-uniq.xsh::test_uniq_count_spans_stdin_chunks",
      "--exact-id", "core/tests/test-uniq.xsh::test_uniq_skips_fields_chars_and_limits_width",
      "--exact-id", "core/tests/test-uniq.xsh::test_uniq_zero_terminated",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("uniq records case was not an exact native command")
  }
  let change = gates.plan_paths(ctx, tests.Verification, ["core/uniq.xsh"], [], gates.Change)?
  assert "native-core/tests" in [entry.id for entry in change.cases]
}

test test_find_edit_selects_traversal_and_path_contracts {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/find.xsh"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == ["core/find/traversal"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "core/tests/test-find.xsh::test_find_boolean_expression_prune_and_print0",
      "--exact-id", "core/tests/test-find.xsh::test_find_parentheses_negation_depth_and_predicates",
      "--exact-id", "core/tests/test-find.xsh::test_find_metadata_survives_delete_for_later_actions",
      "--exact-id", "core/tests/test-find.xsh::test_find_accepts_non_utf8_operands",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("find traversal case was not an exact native command")
  }
  let change = gates.plan_paths(ctx, tests.Verification, ["core/find.xsh"], [], gates.Change)?
  assert "native-core/tests" in [entry.id for entry in change.cases]
}

test test_sort_edit_selects_ordering_and_spill_contracts {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/sort.xsh"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == ["core/sort/ordering"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "core/tests/test-sort.xsh::test_sort_unique_reverse",
      "--exact-id", "core/tests/test-sort.xsh::test_sort_numeric_preserves_decimal_precision_and_uniqueness",
      "--exact-id", "core/tests/test-sort.xsh::test_sort_separator_null_and_invalid_character_counts",
      "--exact-id", "core/tests/test-sort.xsh::test_sort_buffer_size_spills_sorted_runs",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("sort ordering case was not an exact native command")
  }
  let change = gates.plan_paths(ctx, tests.Verification, ["core/sort.xsh"], [], gates.Change)?
  assert "native-core/tests" in [entry.id for entry in change.cases]
}

test test_xargs_edit_selects_process_and_stream_contracts {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/xargs.xsh"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == ["core/xargs/processes"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "core/tests/test-xargs.xsh::test_xargs_quotes_and_nul_preserve_argument_boundaries",
      "--exact-id", "core/tests/test-xargs.xsh::test_xargs_batch_replacement_empty_and_status",
      "--exact-id", "core/tests/test-xargs.xsh::test_xargs_parallel_and_fatal_child_status",
      "--exact-id", "core/tests/test-xargs.xsh::test_xargs_argfile_preserves_child_standard_input",
      "--exact-id", "core/tests/test-xargs.xsh::test_xargs_token_spans_reader_chunk_boundary",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("xargs processes case was not an exact native command")
  }
  let change = gates.plan_paths(ctx, tests.Verification, ["core/xargs.xsh"], [], gates.Change)?
  assert "native-core/tests" in [entry.id for entry in change.cases]
}

test test_awk_edit_selects_record_and_program_contracts {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["core/awk.xsh"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == ["core/awk/records"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "core/tests/test-awk.xsh::test_awk_fields_begin_end_and_precedence",
      "--exact-id", "core/tests/test-awk.xsh::test_awk_arrays_functions_and_loops",
      "--exact-id", "core/tests/test-awk.xsh::test_awk_record_and_field_mutation",
      "--exact-id", "core/tests/test-awk.xsh::test_awk_gawk_regex_record_separators_and_rt",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("awk records case was not an exact native command")
  }
  let change = gates.plan_paths(ctx, tests.Verification, ["core/awk.xsh"], [], gates.Change)?
  assert "native-core/tests" in [entry.id for entry in change.cases]
}

test test_json_edit_selects_structured_data_contracts {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["src/modules/json.rs"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == ["runtime/json/structured-data"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "tests/xsh/stdlib/json.xsh::test_json_codec_rejects_duplicates_and_preserves_numeric_boundaries",
      "--exact-id", "tests/xsh/stdlib/json.xsh::test_json_get_overloads_and_encoded_lines",
      "--exact-id", "tests/xsh/stdlib/json.xsh::test_json_set_updates_the_named_position",
      "--exact-id", "tests/xsh/stdlib/json.xsh::test_json_remove_drops_the_named_position",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("JSON structured-data case was not an exact native command")
  }
  let change = gates.plan_paths(ctx, tests.Verification, ["src/modules/json.rs"], [], gates.Change)?
  assert "test-stdlib-catalog" in [entry.id for entry in change.cases]
  assert "runtime/json/structured-data" not in [entry.id for entry in change.cases]
}

test test_effect_inference_edit_selects_capability_contracts {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification, ["src/sema/check/infer_effects.rs"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == ["compiler/effect-inference/correctness"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved.is_empty()
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[1..] == [
      "test",
      "--exact-id", "tests/xsh/full-inference-effect-union.xsh::test_imported_effect_union_preserves_promised_bounds_and_independent_types",
      "--exact-id", "tests/xsh/full-inference-effect-union.xsh::test_imported_effect_union_reports_a_missing_capability",
      "--exact-id", "tests/xsh/full-inference-effect-union.xsh::test_imported_effect_union_keeps_unknown_dynamic_effects_rejected",
      "--exact-id", "tests/xsh/full-inference-stream-effects.xsh::test_full_inference_inferred_stream_effect_alias_rejects_empty_bound",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("effect inference case was not an exact native command")
  }
  let change = gates.plan_paths(ctx, tests.Verification, ["src/sema/check/infer_effects.rs"], [], gates.Change)?
  assert "test-sema" in [entry.id for entry in change.cases]
}

test test_frontend_retention_edit_selects_exact_measurement_contract {
  let ctx = fixtures.linux_context(/repo)?
  let id = "compiler/frontend-retention/correctness"
  let plan = gates.plan_paths(ctx, tests.Verification, ["src/frontend_stats.rs"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == [id]
  assert plan.products == []
  match plan.cases[0].execution {
    gates.RunCommand(spec) => assert spec.argv[5..] == [
      "-p", "xsh", "--lib", "frontend_stats::tests::indexed_execution_retained_columns_are_stable", "--", "--exact",
    ]
    gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("frontend retention case was not an exact Rust test")
  }
  let change = gates.plan_paths(ctx, tests.Verification, ["src/frontend_stats.rs"], [], gates.Change)?
  assert "test-frontend-memory" in [entry.id for entry in change.cases]
}

test test_constraint_solver_edit_selects_reference_continuations {
  let ctx = fixtures.linux_context(/repo)?
  let id = "compiler/constraint-solver/correctness"
  for source_path in ["src/sema/constraints.rs", "src/sema/constraints/operations.rs"] {
    let plan = gates.plan_paths(ctx, tests.Verification, [source_path], [], gates.Edit)?
    assert [entry.id for entry in plan.cases] == [id]
    assert plan.products == []
    match plan.cases[0].execution {
      gates.RunCommand(spec) => assert spec.argv[5..] == [
        "-p", "xsh", "--lib", "sema::constraints::reference_tests::production_trials_restore_generalization_and_row_continuations", "--", "--exact",
      ]
      gates.DocumentationCheck | gates.Alias | gates.ManualFull | gates.ManualPreparation(_) | gates.ManualCommand(_) | gates.ManualNative | gates.ManualDocumentation => test.fail("constraint solver case was not an exact Rust test")
    }
    let change = gates.plan_paths(ctx, tests.Verification, [source_path], [], gates.Change)?
    assert "test-sema" in [entry.id for entry in change.cases]
  }
}

test test_source_edit_cases_deduplicate_and_preserve_unresolved_paths {
  let ctx = fixtures.linux_context(/repo)?
  let plan = gates.plan_paths(ctx, tests.Verification,
    ["core/uniq.xsh", "core/find.xsh", "core/uniq.xsh", "unknown/example.xsh"], [], gates.Edit)?
  assert [entry.id for entry in plan.cases] == ["core/uniq/records", "core/find/traversal"]
  assert plan.products == [tests.Xsh, tests.Xsht]
  assert plan.unresolved == ["unknown verification owner: unknown/example.xsh"]
}

test test_gate_status_reports_current_stale_and_not_run_evidence_without_starting_work { |ctx|
  test.timeout(ctx, 120s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-status")?
  for name in ["Cargo.toml", "rust-toolchain.toml", "xsht-config.ini"] { fp"{root}/{name}".write("") }
  fp"{root}/dev".mkdir()
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl/verification".mkdir()
  for product in ["xsh", "xsht", "xshi"] {
    fp"{root}/target/x86_64-unknown-linux-musl/verification/{product}".write("fixture\n", mode: 0o755)
  }
  fp"{root}/tools".mkdir()
  fp"{root}/dev/gate_case.xsh".write(fp"{repository}/dev/gate_case.xsh".read_bytes()?)
  fp"{root}/dev/gate_product.xsh".write(fp"{repository}/dev/gate_product.xsh".read_bytes()?)
  let cargo_calls = fp"{root}/cargo-calls"
  let harness_calls = fp"{root}/harness-calls"
  let harness = fp"{root}/target/prepared-test"
  let harness_source = f"""#!{ctx.xsh_bin}
let name = args[0] + "::case"
if "--list" in args {{
  print json.encode({{type: "suite", event: "discovery"}})?
  print json.encode({{type: "test", event: "discovered", name: name, ignore: false}})?
  print json.encode({{type: "suite", event: "completed", tests: 1, benchmarks: 0, total: 1, ignored: 0}})?
}} else {{
  let output = p"{harness_calls}"
  let previous = if output.exists()? {{ output.read_text()? }} else {{ "" }}
  output.write(previous + args[0] + "\\n")?
  print json.encode({{type: "suite", event: "started", test_count: 1}})?
  print json.encode({{type: "test", event: "started", name: name}})?
  print json.encode({{type: "test", event: "ok", name: name}})?
  print json.encode({{type: "suite", event: "ok", passed: 1, failed: 0, ignored: 0}})?
}}
"""
  harness.write(harness_source, mode: 0o755)
  fp"{root}/tools/cargo".write(f"""#!{ctx.xsh_bin}
let output = p"{cargo_calls}"
let previous = if output.exists()? {{ output.read_text()? }} else {{ "" }}
output.write(previous + args.join(" ") + "\\n")?
if args[0] == "test" {{
  if "--no-run" not in args {{ exit 99 }}
  print json.encode({{reason: "compiler-artifact", profile: {{test: true}}, executable: "{harness}"}})?
  print json.encode({{reason: "build-finished", success: true}})?
}}
""", mode: 0o755)
  let docker_marker = fp"{root}/docker-started"
  fp"{root}/tools/docker".write(f"#!/bin/sh\ntouch '{docker_marker}'\nexit 1\n", mode: 0o755)
  init_gate_repository(root, repository)?
  let inherited = env.get_or("PATH", "")?
  let environment = {PATH: f"{root}/tools:{inherited}", XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: ""}
  let run_source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.lane(fixtures.linux_context(p"{root}")?, ["--case", "test-route-witness-unit"])?
}}
main()?
"""
  let status_source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.execute(fixtures.linux_context(p"{root}")?, ["--status", "--case", "test-route-witness-unit"])?
}}
main()?
"""
  let id = "test-route-witness-unit"
  let record = fp"{root}/.work/gate/cases/{id}.json"
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: not run", "Full: not run"], args: [], env: environment)?
  assert ! record.exists()?
  assert ! cargo_calls.exists()?
  let _ = test.expect(ctx, run_source, status: 0, args: [], env: environment)?
  assert record.exists()?
  let compile_calls = cargo_calls.read_lines()?.len()
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: current", "Full: not run"], args: [], env: environment)?
  harness.write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: stale (product changed: {harness}", "Full: not run"], args: [], env: environment)?
  harness.write(harness_source, mode: 0o755)
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: current"], args: [], env: environment)?
  fp"{root}/Cargo.toml".write("edited\n")
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: stale (source changed)", "Full: not run"], args: [], env: environment)?
  fp"{root}/Cargo.toml".write("")
  record.write("{not json")
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: stale (unreadable record"], args: [], env: environment)?
  record.remove()?
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: not run"], args: [], env: environment)?
  assert cargo_calls.read_lines()?.len() == compile_calls
  assert harness_calls.read_lines()?.len() == 1
  assert ! docker_marker.exists()?
}

test test_gate_status_tracks_laputa_scenario_evidence_after_the_external_run { |ctx|
  test.timeout(ctx, 120s)
  let repository = fs.cwd()?
  let root = test.temp_dir(ctx, name: "gate-status-laputa")?
  for name in ["Cargo.toml", "rust-toolchain.toml", "xsht-config.ini"] { fp"{root}/{name}".write("") }
  fp"{root}/dev".mkdir()
  fp"{root}/target".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl".mkdir()
  fp"{root}/target/x86_64-unknown-linux-musl/verification".mkdir()
  for product in ["xsh", "xsht", "xshi"] {
    fp"{root}/target/x86_64-unknown-linux-musl/verification/{product}".write("fixture\n", mode: 0o755)
  }
  fp"{root}/dev/gate_product.xsh".write(fp"{repository}/dev/gate_product.xsh".read_bytes()?)
  fp"{root}/dev/musl-linker.sh".write("#!/bin/sh\n", mode: 0o755)
  let laputa = test.temp_dir(ctx, name: "gate-status-laputa-root")?
  let _ = run.capture --text git -C $laputa init --quiet
  let cargo_home = test.temp_dir(ctx, name: "gate-status-cargo-home")?
  fp"{cargo_home}/bin".mkdir(parents: true)
  fp"{cargo_home}/bin/kache".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let python = (run.text which python3)?.trim()
  fp"{root}/tools".mkdir()
  fp"{root}/tools/python3".write(f"""#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = "--xsh" ]; then exit 0; fi
  if [ "$arg" = "--observe" ]; then printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\n'; exit 0; fi
done
exec '{python}' "$@"
""", mode: 0o755)
  fp"{root}/tools/docker".write(f"""#!{ctx.xsh_bin}
if args[0] == "image" {{
  if "--format" in args {{ print "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }}
  exit 0
}}
exit 0
""", mode: 0o755)
  init_gate_repository(root, repository)?
  let inherited = env.get_or("PATH", "")?
  let environment = {PATH: f"{root}/tools:{inherited}", CARGO_HOME: cargo_home,
    XSH_MODULE_PATH: f"{repository}/dev/tests:{repository}/dev", XSH_DEV_BUILD_PRELOAD: "",
    XSH_TEST_IMAGE_BUILD: "0", XSH_TEST_IMAGE: "fixture"}
  let id = "laputa/pm.make/plan"
  let request = f"""["--case", "{id}", "--laputa-root", "{laputa}"]"""
  let run_source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.execute(fixtures.linux_context(p"{root}")?, {request})?
}}
main()?
"""
  let status_source = f"""
use gates
use fixtures
proc main() [fs, process, env, time, error, io] {{
  gates.execute(fixtures.linux_context(p"{root}")?, ["--status", "--case", "{id}", "--laputa-root", "{laputa}"])?
}}
main()?
"""
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: not run"], args: [], env: environment)?
  let _ = test.expect(ctx, run_source, status: 0, args: [], env: environment)?
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: current"], args: [], env: environment)?
  fp"{root}/Cargo.toml".write("edited\n")
  let _ = test.expect(ctx, status_source, status: 0, stdout: [f"{id}: stale (source changed)"], args: [], env: environment)?
}

test test_container_ownership_repair_returns_gate_evidence_to_the_host_user { |ctx|
  let root = test.temp_dir(ctx, name: "ownership-repair")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let log = fp"{root}/chown-args"
  fp"{tools}/chown".write("""#!/bin/sh
printf '%s\\n' "$@" > "$CHOWN_ARGS"
""", mode: 0o755)
  let checkout = fp"{root}/checkout"
  fp"{checkout}/.work/gate/cases".mkdir(parents: true)
  let gate_ctx = fixtures.linux_context(checkout, "verification")?
  env ({PATH: tools, HOST_UID: "1000", HOST_GID: "1000", CHOWN_ARGS: log}) {
    container_internal.repair_mounted_outputs(gate_ctx)?
  }
  assert log.read_lines()? == ["-R", "1000:1000", gate_ctx.target_dir.display(), fp"{checkout}/.work/gate".display()]
}

