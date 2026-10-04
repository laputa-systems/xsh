# XSHI Interactive Specification

This is the authoritative contract for `xshi`. `xsh` remains a strict script
runner and `xsht` remains tooling-only. Interactive conveniences exist only
inside `xshi`; they must not change `.xsh` script syntax, checking, runtime
behavior, examples, docs generation, or tooling.

## 1. Reference Shell And Deviations

`xshi` is behaviorally the `ish` shell built on XSH's process,
filesystem, and stdlib facilities. What an `ish` user sees or gets from a
keystroke is the contract: rendered screens and cursor positions, command
results and statuses, and the files a session leaves behind. `xshi` ported
`ish`'s algorithms (input decoding, line buffer, rendering, completion, history
ranking, listing, denv); it does not embed `ish` or its shell engine and does
not shell out for work the shell owns.

The differential PTY scenarios in §14 are the executable form of this section.
A behavior that differs from `ish` is either a bug or one of the deviations
below; every deviation is covered by an `xshi`-only test.

Where `ish`'s README documents behavior its implementation lacks, `xshi`
follows the README:

- `&>` and `&>>` redirect both streams; `|&` pipes both (`ish` runs `&>` as a
  background marker and rejects `|&`);
- `**` matches recursively, and a pattern that matches nothing is an error with
  status `2` (`ish` passes the pattern through literally);
- quoted and escaped glob characters are literal (`ish` globs them anyway);
- `$NAME<Tab>` completes shell variables (`ish` completes files);
- `fg` continues an AND-OR list that was stopped while running one of its
  commands (`ish` drops the rest of the list);
- typing `exit` a second time forces the exit past a suspended job (`ish`
  clears the warning before it looks at the line, so only Ctrl-D forces it),
  and the forced exit sends the job `SIGTERM` then `SIGCONT`.

Other deliberate differences:

- history is safe under concurrent shells (§8): `ish` can lose entries when
  shells exit around one another, treats a torn record as a command, and can
  stop syncing on an invalid-UTF-8 line;
- `ssh`/`scp` remote path completion passes the host as one process argument;
  `ish` interpolates it into a local `sh -c` string;
- `-c COMMAND` runs one command line; `--config PATH` replaces `ish`'s `-c PATH`;
- `eval` and `exec` exist; `exec COMMAND` runs the command and then leaves the
  shell (a command that cannot start leaves the shell running);
- shell functions, `trap`, `readonly`, `shift`, `local`, and `init.sh` are not
  implemented, and option forms such as `set -e` are accepted and ignored;
- messages are prefixed `xshi:` instead of `ish:`, state lives under `xshi`
  (`~/.config/xshi`, `~/.local/share/xshi`, `~/.cache/xshi/dump-<hex>`), and the
  layout-dump builtin is `xshi-dump`;
- `xshi`-only additions that leave `ish`-shaped input alone: XSH source at the
  prompt (§5), trailing `&` for one simple command, `bg`, `z`, `:`, and
  `history` subcommands.

There is no compatibility layer for earlier `xshi` behavior: its history
records, `XH` cache, `config.ini`/`config.xsh`, and pipefail statuses are gone.

## 2. Boundaries

`xshi` is an adapter over XSH facilities plus an interactive shell frontend. It
is not a second script language.

- `xsh` does not accept shell syntax such as bare `git status`.
- `xsht check`, formatting, docs generation, examples, and tests never read
  `~/.config/xshi/config.ish` or `~/.local/share/xshi/history`.
- Aliases, prompt state, denv trust, history, completion, and autosuggestions
  are unavailable to scripts.
- Library changes outside `crates/xshi` must stay neutral for existing scripts
  unless an interactive option is passed. The one such change so far is
  `ProcessRedirection::ChildDup`, which applies descriptor copies such as
  `2>&1` in the child in list order; scripts cannot express it.

Source layout under `crates/xshi/src/interactive/`:

| Module | Owns |
| --- | --- |
| `repl.rs` | the read loop, key handling per mode, layout dump |
| `input.rs`, `line.rs` | key decoding, bracketed paste, the line buffer and kill ring |
| `render.rs`, `term.rs`, `prompt.rs` | repainting geometry, terminal control, prompt text |
| `complete.rs`, `path.rs` | candidates, grid layout, PATH executable cache |
| `history.rs`, `history/store.rs` | in-memory history and search, on-disk log, cache, lock, reset |
| `app.rs`, `session.rs`, `builtin.rs`, `alias.rs`, `config.rs` | line execution, session state, builtins, aliases, config |
| `shell/` | lexer, parser, syntax tree, glob expansion |
| `denv.rs`, `z.rs`, `listing.rs` | directory environments, frecency jumps, `l` |
| `signal.rs`, `sys.rs` | SIGWINCH self-pipe and small descriptor helpers |

Tests live beside the code (`ported_tests.rs` holds the ports of `ish`'s unit
and integration tests; `history/tests.rs` holds the history store's).

## 3. Process And Command Line

```text
xshi [-c COMMAND] [--config PATH] [--no-config] [-V|--version] [-h|--help]
```

- Normal startup requires stdin and stdout to be terminals; otherwise it exits
  `2` naming the requirement. `--help` and `--version` need no terminal.
- Any positional argument is refused with exit `1`: the shell is
  interactive-only.
- `-c COMMAND` runs one line, without a terminal, and exits with its status.
  Standard input passes through to the command, so `xshi -c 'cat > file'` works
  as an `ssh host command` target. `/etc/profile` is read for `-c` only when
  argv0 begins with `-`.
- Interactive startup reads the profile, then the config (§7), then starts
  denv for the initial directory.
- `XSHI_HOSTNAME` replaces the machine's short name in the prompt.
- `XSHI_ALLOW_NON_TTY_FOR_TESTS=1` runs the read-eval loop from piped stdin
  with no terminal control. It exists for tests of shell semantics that need no
  screen and is not a supported interface.
- Exit codes reach the parent unmasked to the OS's range (`exit 300` exits
  `44`).

With a terminal, the shell puts itself in its own process group, takes the
foreground, handles job-control signals itself, and resets dispositions in
children. Raw mode and bracketed paste are on only while the editor is reading
and are restored on every exit path.

## 4. Session State And Statuses

Session state that survives from prompt to prompt: the working directory
(absolute, with `PWD`/`OLDPWD` kept in step), exported variables, unexported
shell variables, aliases, denv state, history, the prompt status, `$?`, and one
job slot. XSH bindings (`let`, `proc`, `use`, …) do not persist: each XSH
submission starts fresh.

Variables:

- exported variables are the environment children inherit; nothing else is;
- `NAME=value` alone updates an exported variable in place, otherwise sets an
  unexported shell variable; `NAME=value command` scopes the value to that
  command; `export`, `set NAME value...`, and `unset` manage exported ones;
- `$NAME`, `${NAME}`, `$?`, and `$$` expand; an invalid assignment prefix such
  as `BAD-NAME=value` reports `invalid environment assignment`.

Two statuses exist. The prompt status (its color, and Ctrl-D's exit) reflects
the last line, including commands the shell answers itself. `$?` is the status
of the last command that ran a program or ordinary builtin; `cd`, `fg`, `w`,
`history`, and the other session commands leave it alone.

| Situation | Status |
| --- | --- |
| program exits `N` / dies of signal `S` | `N` / `128 + S` |
| command not found / not executable / bad interpreter | `127` / `126` / `126` |
| syntax error, glob with no match, session-builtin usage | `2` |
| pipeline | the last stage's |
| `cmd $(failing)` | the substitution's status is ignored |
| Ctrl-C at the prompt | `130` |
| foreground command stopped by Ctrl-Z | `148` |
| `exit [N]` | `N` (garbage or missing: `0`) |
| Ctrl-D on an empty line | success |

Exec failures print `NAME: not found`, `NAME: permission denied`, or
`NAME: INTERPRETER: bad interpreter: …`.

## 5. Input Classification

A submitted line is shell input unless it is lexically XSH. Always XSH:
declaration and control starts (`let var proc pure use if for while match
return defer guard`), `run`, `print`, `eprint`, `type NAME = …`, `export
let|var|proc|pure|stream|type …` and `export name: Type`, module-qualified
starts (`fs.write …`), and expression starts (`{`, `(`, a `[` not followed by
a space, string/path/format literals, `null`, digits). Ambiguous words go to the
shell form: `export NAME=v`, `type ls`, `set …`, `source f`, `alias name cmd`.
`true` and `false` are shell builtins unless the line is an expression
(`false or true`, `[false]`).

XSH source runs through the normal parser, checker, and runtime with the
session's environment and directory. A parse or check error prints the
diagnostic and the session continues.

## 6. Shell Language

Lines are tokenized as a POSIX-like interactive subset:

- words with `'…'`, `"…"`, and `\` quoting; `#` starts a comment at a word
  start; an unterminated quote, trailing `\`, or trailing `|`, `&&`, `||`
  continues the line on a new prompt;
- lists with `;`, `&&`, `||`; pipelines with `|` and `|&`; a single trailing
  `&` for one simple external command;
- redirections `<`, `>`, `>>`, `2>`, `2>>`, `>&2`, `2>&1`, `&>`, `&>>`,
  applied left to right (`> f 2>&1` and `2>&1 > f` differ, as in a POSIX
  shell), relative to the session directory;
- a builtin in a pipeline runs against a copy of the session, so `cd d | cat`
  leaves the shell where it was.

Expansion order: alias, tilde, parameters, arithmetic `$((…))`, command
substitution (`$(…)` and backticks), field splitting, pathname expansion, quote
removal.

- Aliases expand only the first word of a line, on the space that follows it in
  the editor and again when the line is submitted. They do not expand after
  `;` or `|`, in quotes, or in arguments.
- Substitution output has trailing newlines removed. Unquoted, it splits into
  fields and is then globbed; quoted, it is neither.
- Globs are sorted, skip dot files unless the pattern starts with `.`, support
  `*`, `?`, `[…]` (with `!`/`^` negation and ranges), and `**`. Quoted or
  escaped metacharacters are literal. A pattern that matches nothing is an
  error with status `2`.
- `sudo` and other wrappers receive their argv unchanged; command names resolve
  through the session `PATH`.

Command substitution runs against a detached copy of the session: it sees the
directory, environment, and aliases, and its side effects do not reach the
parent.

## 7. Config And Profile

`~/.config/xshi/config.ish`, or `--config PATH`, is read once at startup unless
`--no-config`. A missing default file is silent; a missing explicit file warns.
It is a list of directives, one per line, with `#` comments:

```text
set NAME value...
alias name command...
```

Variables in values expand. An unrecognized directive warns as
`xshi: PATH:LINE: unrecognized directive: …` and is skipped; warnings print
before the first prompt.

The profile (`/etc/profile`, overridable with `XSHI_PROFILE_PATH`) contributes
only `NAME=value` and `export NAME=value` lines and is read before the config.

## 8. History

Files, all beside each other under `~/.local/share/xshi/`:

| File | Content |
| --- | --- |
| `history` | append-only log, one record per line |
| `history.bin` | compacted cache, format `ISH\x05` |
| `history.lock` | advisory `flock` target |
| `history.reset` | generation written by `history reset` |
| `history.bin.corrupt` | an unreadable cache set aside by `history rebuild` |

Log records are `:ish-history:v2\tTIMESTAMP_MS\tSESSION\tCWD\tCOMMAND`
(`v1` has no cwd). `CWD` escapes `\\`, tab, newline, and carriage return. A plain
line is a legacy command with unknown time, session, and directory. A line that
carries the record prefix but does not parse is a torn record and is dropped.
The cache holds `[magic][count][arena size][cwd arena size]`, one little-endian
`u64` timestamp per entry (milliseconds since 1998-01-01), then NUL-terminated
command and directory arenas. Any structural inconsistency makes the whole
cache unreadable.

Entries: commands are trimmed, embedded newlines become spaces, and a command
that is empty, longer than 65,535 bytes, or contains NUL is not recorded. Each
command appears once, at the position of its latest use.

Every recorded command is appended to the log in one write before anything else
happens, so the log plus the cache always hold the whole history. Compaction is
a disk-to-disk merge that never trusts a shell's memory: under an exclusive
lock it reads the cache and the whole log, lets a log record replace a cached
entry only when strictly newer (a log left by a crashed compaction therefore
cannot reorder anything), writes the cache through a temporary file and an
atomic rename, then truncates the log. It runs when a shell exits and on
`history compact`, and yields quietly when another shell holds the lock,
because nothing is lost by skipping it.

Locking: appenders and readers hold the lock shared, compaction and reset hold
it exclusive, and every wait is bounded so a stuck peer cannot hang a prompt.
Loading never writes.

Sync, run before every prompt and every Ctrl-R, costs three `stat` calls when
nothing changed. It reads only the new complete lines of the log; a line still
being written is left for the next sync and a line that is not valid UTF-8 is
skipped. A shell recognizes its own records by session id. If the cache's or
log's identity changed (another shell compacted or replaced them) it re-reads
both. Entries other shells add after this shell started are merged into memory
but stay out of Up-arrow recall, Ctrl-R, and autosuggestions until the next
shell starts, so a session's own history does not shift under the user.

Reset: `history reset` deletes the log, cache, and quarantined cache and writes
a new generation. Running shells see the marker change at their next sync or
add, clear their memory, and cannot resurrect old entries at exit.

Recovery: an unreadable cache is announced once at startup; the shell loads the
log only and refuses to overwrite the cache until `history rebuild`, which sets
the file aside as `history.bin.corrupt` and writes a new cache from what is
recoverable. A torn tail on the log is closed with a newline before the next
record is appended.

Search ranks by tier (prefix, word-boundary substring, substring,
subsequence), then recency, with a boost for entries recorded in the current
directory or an ancestor. Autosuggestions use the most recent session-visible
entry having the buffer as a prefix.

`history` prints entries; `history compact`, `history rebuild`, `history reset`,
and `history -h` manage storage.

## 9. Prompt, Editing, Rendering

The prompt is `user@host cwd git-branch [*] $`: the cwd is shortened in the
middle (`~/.config/fish` → `~/.c/fish`), the branch comes from `.git/HEAD`
without a subprocess, a red `*` marks a denv with pending changes, and the
prompt is green after success and red after failure. Every prompt is preceded by
an OSC 7 working-directory report.

Keys:

| Key | Action |
| --- | --- |
| Ctrl-A / Home, Ctrl-E / End | line start / end |
| Left, Right | one character |
| Ctrl-Left, Alt-B / Ctrl-Right, Alt-F | one word |
| Backspace, Delete, Ctrl-D | delete before / after the cursor; Ctrl-D on an empty line exits |
| Ctrl-K, Ctrl-U, Ctrl-W / Ctrl-Delete, Alt-D | kill to end, to start, word back, word forward |
| Ctrl-Backspace | pick from the directories this shell has visited, most recent first |
| Ctrl-Y | yank the kill ring (one ring, shared by all kills) |
| Ctrl-C | discard the line, status `130` |
| Ctrl-L | clear the screen, keep the line |
| Ctrl-P | write a layout dump to `~/.cache/xshi/dump-<hex>` without touching the screen; the `xshi-dump` builtin does the same |
| Up, Down | move by visual row; at the first or last row, prefix search through session history |
| Tab, Ctrl-R | completion (§10), history search |

Text is UTF-8 throughout: cursor motion and deletion work on characters, widths
account for wide characters and combining marks, and a wide character with one
column left wraps before drawing. Long lines wrap as a grid; pasted multi-line
input keeps its lines and Up/Down move between them. Bracketed paste inserts
text without executing it and is rejected above 8,192 bytes.

The autosuggestion ghost text shows the rest of the newest matching history
entry after the cursor when the buffer is at least three characters, single
line, and the cursor is at its end; Right, End, or Ctrl-E accepts it.

Repainting returns to the top of the previous region and clears the larger of
the old and new row counts, so output never stacks across resizes or wrapped
lines. SIGWINCH reaches the read loop through a self-pipe and repaints.

History search (Ctrl-R) is a pager: a `search:` header, matches below it with
matched characters highlighted, Up/Down to move, Enter to accept into the
prompt, Escape or Ctrl-C to cancel.

## 10. Completion

Tab classifies the word at the cursor:

| Context | Candidates |
| --- | --- |
| empty line | inserts `cd ` |
| `$NAME` (also inside double quotes) | variable names, exported or not |
| first word, or after `\|`, `&&`, `;` | builtins, aliases, PATH executables, directories |
| after `cd` | directories only |
| `ssh scp rsync sftp mosh` | hosts from `~/.ssh/config` and `known_hosts`, then files; `host:path` lists remote paths |
| anything else | files and directories |

Path candidates are sorted by modification time, newest first, and colored
(directories blue, symlinks cyan, executables green). Hidden names appear only
for a prefix that starts with `.`; prefix matches win over substring matches;
`~/` is preserved; a path whose intermediate components are prefixes resolves
fish-style. Names needing quoting are single-quoted on insertion.

A single candidate or a longer common prefix is inserted. Otherwise a column-major
grid of up to six columns and ten visible rows opens with no selection; Tab and
the arrows move the selection and preview it, typing filters live, Enter
accepts without submitting, Escape and Ctrl-C cancel.

Remote path completion runs `ssh -o BatchMode=yes -o ConnectTimeout=2 HOST ls
-dp PATH*` with the host as one argument, a three-second deadline, and a 64 KiB
output cap; any failure yields no candidates.

## 11. Builtins

Answered by the shell itself: `cd` (`cd -`, `~`), `exit`, `fg`, `bg`, `export`,
`set`, `unset`, `alias`, `source`/`.`, `eval`, `exec`, `history`, `z`, `denv`,
`l`, `c`, `w`/`which`/`type`, `copy-scrollback`, `xshi-dump`, and `:`. `echo`
(`-n`, `-e`, `-E`), `pwd`, `true`, and `false` are internal so they work in
pipelines. `w`, `which`, and `type` also report `command`, `test`, `printf`,
and the other POSIX names as builtins; those that are not internal resolve on
`PATH`.

- A one-word line naming a directory becomes `cd`, and `..`, `...`, … climb.
  Relative `cd` and `z` targets are recorded in history as absolute paths.
- `l [path…]` is the native `ls -plAhG`: no fork, owner and group by name,
  sizes as `B/K/M/G`, symlink targets, colors on names only.
- `copy-scrollback` copies the session's typed lines to the clipboard with
  OSC 52.
- `z QUERY` jumps to the best frecency match among directories recorded by
  `cd` and `z` in history and announces the destination on stderr.
- Builtin output may be redirected and piped like a program's.

## 12. Jobs And Signals

One job slot. Ctrl-Z on a foreground command leaves it stopped, prints
`xshi: stopped: CMD (pgid=N)`, and gives status `148`. `fg` prints
`xshi: resuming: CMD`, gives the job the terminal, restores its saved terminal
modes, and waits; `fg` with no job is an error. If the command was stopped in
the middle of a list (`a && b`, `a || b`, `a; b`), the rest of the list waits
in the job and runs when `fg` sees the command finish, using its status.
`bg` continues a stopped job without the terminal (the rest of its list is
dropped); `cmd &` starts one simple external command in the slot.

Before each prompt the slot is polled without blocking and completions are
reported. `exit` and Ctrl-D with a live job warn once
(`xshi: there is a suspended job. Exit again to force quit.`); repeating the
same gesture exits and sends the job's process group `SIGTERM` then `SIGCONT`,
so a stopped job actually terminates. Any other line re-arms the warning.

## 13. Denv And z

Denv loads a directory's environment from the git root (or the current
directory): `.envrc` (bash-style, evaluated by `bash`, `sh`, or `xsh` for `.xsh`
files) and `.env` (dotenv). Both apply on startup, after every directory
change, and after `denv reload`; leaving the tree restores previous values.
`.envrc` requires trust: `denv allow` records the file's path and mtime under
`~/.local/share/xshi/denv/allow`, editing the file invalidates the trust, and
`denv deny` removes it and marks the environment dirty. `.env` needs no trust.
The state is exported as `__DENV_*` variables; `__DENV_DIRTY=1` shows the red
prompt marker.

`z` reads its scores from the same history (§11).

## 14. Tests

| Layer | Where | Runs |
| --- | --- | --- |
| units: line buffer, input, render math, completion, prompt, listing, denv, aliases, config, history store | `crates/xshi/src/**` (`ported_tests.rs`, `history/tests.rs`) | `cargo test -p xshi` |
| CLI boundary | `crates/xshi/tests/cli.rs` | `cargo test --release -p xshi` |
| differential PTY scenarios | `tests/runtime/interactive/parity/{scenarios,extended}.rs`, goldens in `tests/fixtures/interactive-parity/<os>/` | `cargo test --release --test integration runtime::interactive::` |
| `xshi`-only behavior and the piped session | `tests/runtime/interactive.rs` | same |

A scenario drives a real shell through a PTY (`laputa-ptytest`: a `vt100` screen
model, event-driven waits, no sleeps), records frames — visible rows, styled
runs, cursor position and visibility — and effects (files, history, directory
listings, exit status), and compares the transcript with the golden recorded
from `ish`. Each run uses an isolated `HOME`; normalization is limited to the
scratch path, host name, shell name in messages, process-group ids, and the
timestamps, session ids, and random suffixes of persisted files.

- `XSHI_PARITY_ISH_BIN=/path/to/ish` also runs every scenario against `ish` and
  requires `ish` == golden == `xshi`.
- `XSHI_PARITY_RECORD=1` (with the variable above) rewrites goldens.
- `XSHI_PARITY_FULL=1` prints whole transcripts on a mismatch.
- A second shell sharing the `HOME` (`spawn_peer`) covers cross-session history.

`xshi`-only tests use the same harness through `run_xshi_only` or the piped
session. History concurrency (many shells appending, syncing, compacting, and
resetting), torn and stale files, and 200,000-line logs are exercised in
`history/tests.rs` against real files.

## 15. Deferrals

`~user` expansion, brace expansion, here-documents, process substitution,
shell functions, `trap`, `readonly`, `shift`, `local`, job specs (`%1`), more
than one job, and background pipelines or lists are not implemented.
