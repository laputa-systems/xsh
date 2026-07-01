#!/usr/bin/env -S xsh --
# Git Digest
# Summarize commits, authors, and changed files ahead of a base branch.
# Usage: xsh showcase/git-digest.xsh -- [--base BRANCH] [--limit N]
# Example: xsh showcase/git-digest.xsh -- --base main --limit 20
type FileStats = {path: Str, added: Int, removed: Int, total: Int}

type Opts = {base: Str, limit: Int}

proc main(...argv: List[Str]) [fs, process, error] {
  let _ = fs.gitroot()?

  let opts: Opts = cli.parse(
    argv,
    {base: {form: "--base BRANCH", default: "main"}, limit: {form: "--limit N", kind: "UInt", default: 10, min: 1}},
  )?

  let range = f"${opts.base}..HEAD"

  # Commit summary
  let log_out = run.text "git" "log" "--oneline" $range ?
  let commits = log_out.lines() |> where . != ""
  let commit_count = commits.len()

  if commit_count == 0 {
    print f"no commits ahead of ${opts.base}"
    return
  }

  print f"${commit_count} commit(s) ahead of ${opts.base}"
  print ""

  # Author breakdown
  let shortlog_out = run.text "git" "shortlog" "-sn" $range ?

  let authors = shortlog_out.lines()
    |> where .trim() != ""
    |> map .trim()

  print "authors:"

  for a in authors {
    print f"  ${a}"
  }

  print ""

  # Per-file insertion/deletion counts from numstat
  let numstat_out = run.text "git" "diff" "--numstat" $range ?

  let file_stats: List[FileStats] = numstat_out.lines()
    |> where .trim() != ""
    |> where .split("\t").len() >= 3
    |> map { |line|
      let parts = line.split("\t")
      let added = json.decode(if parts[0] == "-" { "0" } else { parts[0] })?
      let removed = json.decode(if parts[1] == "-" { "0" } else { parts[1] })?
      let file_path = parts[2]
      {path: file_path, added: added, removed: removed, total: added + removed}
    }

  let total_added = file_stats
    |> map .added
    |> sum

  let total_removed = file_stats
    |> map .removed
    |> sum

  print f"${file_stats.len()} file(s) changed  +${total_added} -${total_removed}"
  print ""

  let top = file_stats
    |> sort-by --desc .total
    |> take(opts.limit)

  print f"top ${top.len()} file(s) by change volume:"
  print f"  ${"file":<60} ${"added":>6} ${"removed":>8}"

  for f in top {
    print f"  ${f.path:<60} ${f.added:>6} ${f.removed:>8}"
  }
}
