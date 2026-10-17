# Coverage discovery failure

Each failed coverage test starts a nested `xsht test --cov` command without a path. `xsht test` discovers scripts under its current directory before it handles list or exact-test selection. In the canonical run, the nested process ran as UID 1000 in linked worktree `/work`.

The failing processes reported `Permission denied (os error 13)` while descending into mode-000 fixtures under `.work/tmp/native-tests-1000-1000`. The direct probe was `xsht test --cov --list --exact tests/xsh/basic.xsh::test_pass`. It failed twice as UID/GID 1000 with the native test `PATH`, `HOME`, and `TMPDIR`. Its separate terminal output was not retained. The byte-exact canonical transcript is in [stdout.log](stdout.log).

The outer `test_runner_cov_list_does_not_execute_tests` and `test_runner_cov_api_opt_in_prints_api_sections` checks passed in separate normal and forced runs. Those runs did not encounter the concurrent mode-000 path. The results isolate discovery from execution of the selected test.

The walker failed to load an ignore file. The repository excludes `.work` through `info/exclude` in the common Git directory. As UID 1000, `git check-ignore -v` resolved `.work` to `.git/info/exclude:16:/.work/`. The UID could read the worktree metadata and its ancestors.

`/work/.git` points to `/home/josh/d/laputa-systems/xsh/.git/worktrees/consolidation-final-0dc9cff7c`. Its `commondir` entry is `../..`, which resolves to the repository's common `.git` directory. Unreadable Git metadata did not cause the failure.

[`discover_scripts`](../../../../crates/xsht/src/cli/files.rs#L54) scans `.` when callers provide no paths. [`collect_xsh_files_parallel`](../../../../crates/xsht/src/cli/files.rs#L175) enables Git excludes and sets `require_git(false)`. The frozen `Cargo.lock` pins `ignore` 0.4.25.

The pinned dependency makes the lookup decision from `require_git`:

```rust
let check_vcs_dir = self.0.opts.require_git
    && (self.0.opts.git_ignore || self.0.opts.git_exclude);
let git_type = if check_vcs_dir {
    dir.join(".git").metadata().ok().map(|md| md.file_type())
} else {
    None
};
```

When it resolves `info/exclude`, it uses that optional type:

```rust
if !git_type.map_or(false, |ft| ft.is_file()) {
    return Ok(dir.join(".git"));
}
```

These excerpts are from `ignore-0.4.25/src/dir.rs:259-265` and `:892-900`. The source calls the resolver for `info/exclude` at `:307-318`. With `git_type == None`, it returns `dir/.git` without reading the worktree pointer or its `commondir` entry. The walker misses the common `info/exclude` and does not prune `.work`.

The canonical diagnostic confirms the affected boundary. One failure reported:

```text
xsht: ./.work/tmp/native-tests-1000-1000/tmp/xsh-test-185357-1792197191881732022-3774-test_sync_reports_permission_denied_before_directory_sync/sync-no-permission-1/private: Permission denied (os error 13)
```

This evidence records the cause for the four coverage failures only. It does not explain the five other canonical failures or grant acceptance credit. The discovery repair belongs to its isolated lane; this evidence branch does not change the frozen source.
