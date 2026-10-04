#!/bin/sh
# Temporary: delete this directory once the Laputa monorepo is migrated.
#
# Migrates `${expr}`/`$name` f-strings to `{expr}` f-strings in every `.xsh`
# and `.rs` file under each DIR (skipping `target` and `.git`) and in each
# FILE. The tool reads the old syntax, so it is built inside an export of the
# last commit before `{expr}` f-strings. Run it once per tree: a second run
# doubles literal braces again. NOTE lines name interpolations that need a
# manual fix (comments inside braces, line breaks in nested block strings).
#
#   dev/fstring-migrate/run.sh DIR_OR_FILE...
#
# The build is cached in $FSTRING_MIGRATE_WORK (default
# ${TMPDIR:-/tmp}/xsh-fstring-migrate); delete it after editing fmigrate.rs.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
old_rev=b9985cb9
work=${FSTRING_MIGRATE_WORK:-${TMPDIR:-/tmp}/xsh-fstring-migrate}
if [ ! -x "$work/target/release/fmigrate" ]; then
  rm -rf "$work"
  mkdir -p "$work"
  git -C "$repo" archive "$old_rev" | tar -x -C "$work"
  # The tool calls the old scanner, which is crate-private there.
  sed -i.bak 's/pub(crate) fn /pub fn /; s/pub(crate) enum /pub enum /; s/pub(crate) struct /pub struct /' "$work/src/syntax/literal.rs"
  cp "$here/fmigrate.rs" "$work/src/fmigrate.rs"
  printf '\n[[bin]]\nname = "fmigrate"\npath = "src/fmigrate.rs"\ntest = false\n' >> "$work/Cargo.toml"
  (cd "$work" && cargo build --release -j 4 --bin fmigrate)
fi
for target in "$@"; do
  if [ -d "$target" ]; then
    find "$target" \( -name target -o -name .git \) -prune -o -type f \( -name '*.xsh' -o -name '*.rs' \) -print0
  else
    printf '%s\0' "$target"
  fi
done | xargs -0 "$work/target/release/fmigrate"
