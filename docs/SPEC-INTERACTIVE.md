# XSHI Interactive Specification

This is the authoritative contract for `xshi`. `xsh` remains a strict script
runner and `xsht` remains tooling-only. Interactive conveniences exist only
inside `xshi`; they must not change `.xsh` script syntax, checking, runtime
behavior, examples, docs generation, or tooling.

The interactive implementation is enabled by the `laputa-asterinas` Cargo
feature, which Laputa opts into only for its Asterinas shell guest. Builds
without that feature retain a small executable stub and do not compile the
interactive shell.

## 1. Reference Shell And Deviations

`xshi` is behaviorally the `ish` shell built on XSH's process,
filesystem, and stdlib facilities. What an `ish` user sees or gets from a
keystroke is the contract: rendered screens and cursor positions, command
results and statuses, and the files a session leaves behind. `xshi` ported
`ish`'s algorithms (input decoding, line buffer, rendering, completion, history
ranking, listing, denv); it does not embed `ish` or its shell engine and does
not shell out for work the shell owns.

The retained PTY transcripts in §14 record behavior captured from `ish` and are
replayed against the real `xshi` binary by
`cargo test -p xshi --features laputa-asterinas --test parity`. A behavior that
differs from `ish` is either a bug or one of the deviations below; every
deviation is covered by an `xshi`-only test.

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
- the forced exit past a suspended job sends the job `SIGTERM` then `SIGCONT`,
  so a stopped job can terminate.

Other deliberate differences:

- `ssh`/`scp` remote path completion passes the host as one process argument;
  `ish` interpolates it into a local `sh -c` string;
- `-c COMMAND` is evaluated by `xshi` itself rather than run through
  `/bin/sh -c`;
- `eval` and `exec` exist; `exec COMMAND` runs the command and then leaves the
  shell (a command that cannot start leaves the shell running);
- shell functions, `trap`, `readonly`, `shift`, `local`, and `init.sh` are not
  implemented;
- messages are prefixed `xshi:` instead of `ish:`, state lives under `xshi`
  (`~/.config/xshi`, `~/.local/share/xshi`, `~/.cache/xshi/dump-<hex>`), and the
  layout-dump builtin is `xshi-dump`;
- `xshi`-only additions that leave `ish`-shaped input alone: XSH source at the
  prompt (§5), trailing `&` for one simple command, `bg`, `z`, and `:`.

There is no compatibility layer for earlier `xshi` behavior: its text history
log, binary cache, `XH` cache, `config.ini`/`config.xsh`, and pipefail statuses
are gone; the old `history`/`history.bin` files are imported once into the
current log and removed.

## 2. Boundaries

`xshi` is an adapter over XSH facilities plus an interactive shell frontend. It
is not a second script language.

- `xsh` does not accept shell syntax such as bare `git status`.
- `xsht check`, formatting, docs generation, examples, and tests never read
  `~/.config/xshi/config.ish` or `~/.local/share/xshi/history.log`.
- Aliases, prompt state, denv trust, history, completion, and autosuggestions
  are unavailable to scripts.
- Library changes outside `crates/xshi` must stay neutral for existing scripts
  unless an interactive option is passed. The one such change so far is
  `ProcessRedirection::ChildDup`, which applies descriptor copies such as
  `2>&1` in the child in list order; scripts cannot express it.

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
- Setting `XSHI_ALLOW_NON_TTY_FOR_TESTS` to any value runs the read-eval loop
  from piped stdin with no terminal control. It exists for tests of shell semantics that need no
  screen and is not a supported interface.
- Exit codes reach the parent unmasked to the OS's range (`exit 300` exits
  `44`).

With a terminal, the shell puts itself in its own process group, takes the
foreground, handles job-control signals itself, and resets dispositions in
children. Raw mode and bracketed paste are on only while the editor is reading
and are restored on every exit path. When the shell
starts with SIGTERM or SIGHUP at its default action, that signal restores the
attributes saved when the editor first entered raw mode and then ends the
shell by the same signal, so a supervisor observes death by that signal. An
inherited ignored disposition, as under `nohup`, stays ignored. SIGKILL cannot
be handled and leaves the terminal as it was.

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

Two statuses exist. The prompt status reflects the last line, including
commands the shell answers itself, and controls the prompt color. `$?` is the status
of the last line: commands such as `cd`, `fg`, `w`, and `history` set it like
any other builtin.

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
starts (`json.write …`), and expression starts (`{`, `(`, a `[` not followed by
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
| `history.log` | append-only frame log |
| `history.log.lock` | advisory `flock` target |
| `history.log.corrupt-<MS>-<N>` | damaged bytes set aside by startup salvage |

The log starts with the 8-byte magic `ISHLOG\0\x01` and a little-endian `i64`
generation. Each record is a 16-byte frame header (payload length `u64`, CRC-32
of that length, four reserved bytes, CRC-32 of the payload) followed by the
payload: occurrence id `i64`, timestamp `u64` in milliseconds, session id
`u64`, directory length `u32` (`u32::MAX` means no directory), command length
`u32`, the raw directory bytes, and the UTF-8 command. Occurrence ids are
strictly increasing within a generation.

On first open without a log, the shell imports the retired text `history`
(plain lines and `:ish-history:v1`/`:ish-history:v2` records) and the `ISH\x05`
binary `history.bin` cache, prefers the newest metadata, skips a cache record
that duplicates the same command and timestamp in the text, then removes both
files. The import is idempotent; a damaged cache aborts it without touching
either file.

Entries: commands are stored verbatim, including embedded newlines and
surrounding spaces; an empty or whitespace-only line is not recorded. `history`
and the Ctrl-R pager display an entry on one row by replacing newline with `↵`
and other control characters with `�`, one character for one character. Each
distinct command text is one entry; using it again records another occurrence
and updates its usage rather than adding a second row.

Recording is ordered before execution, as in `ish`: the submitted line is
appended and fsynced under the log lock before the command runs, so a failing
command, `exit`, and `history` itself appear in the history they observe. A
failed append rolls the file back to its previous length, reports
`command was not saved to history: <error>` before the command output, and the
command still runs.

Log access takes an exclusive advisory `flock` on `history.log.lock`. Loading
reads the header and frames from the start; each later prompt and each Ctrl-R
reread only the frames appended since the last committed occurrence id. A short
or checksum-invalid final frame is a torn tail: the valid prefix is republished
under the next generation and reading continues. Damage before the final frame
is reported at startup; the shell keeps the valid prefix, moves the rest to
`history.log.corrupt-<MS>-<N>`, and continues.

Up-arrow and autosuggestions use the startup snapshot plus the current shell's
own commands as an ordered list; using a command again moves it to the end of
that list and leaves a tombstone where it was. A command another shell commits
after startup updates global usage and search immediately but does not enter
Up-arrow or autosuggestions until the next shell starts. Ctrl-R searches every
committed command, including entries other shells added later.

Search ranks a candidate by text tier (prefix, word-boundary substring,
substring, subsequence), then by the nearest recorded directory that is the
current directory or an ancestor of it, then by a bounded bonus built from
usage count and age, then by last-use time, then by latest occurrence id;
larger values win. An empty query orders by last-use time with the same tie
breaks. Ctrl-R keeps the best 200 ranked results and refines later keystrokes
within the previous match set.

Reset replaces the log with an empty one under the lock and increments the
generation. Every other shell detects the generation change at its next
snapshot or append, clears its entries and pending state, and cannot resurrect
older entries. A `history` invocation inside a pipeline or command substitution
is refused with `history requires the original shell process and cannot run in
a forked context` and leaves storage untouched.

`history` prints each distinct command once, oldest last use first. `history -h`
prints `Usage: history [reset]` and
`Show saved history, or clear it with `reset`.` with no trailing newline;
`history reset` clears storage. Any other argument is an unknown subcommand
with status `1`; in a pipeline or command substitution `history` is refused
with `history requires the original shell process and cannot run in a forked
context` and leaves storage untouched.

## 9. Prompt, Editing, Rendering

The prompt is `user@host cwd git-branch [*] $`: the cwd is shortened in the
middle (`~/.config/fish` → `~/.c/fish`), the branch comes from `.git/HEAD`
without a subprocess, a red `*` marks a denv with pending changes, and the
prompt is green after success and red after failure. Every prompt is preceded by
an OSC 7 working-directory report and OSC 133 prompt/command markers: `A` before
the prompt is drawn, `B` once it is on screen, `C` when a command starts, and
`D;STATUS` when it finishes. Cancelled input closes its prompt with `D;130` and
no `C`; refused input reports its status with `D` and no `C`. `PTY` suites set
`ISH_TEST_SUPPRESS_OSC133=1` to suppress the markers, as `ish` does.

Keys:

| Key | Action |
| --- | --- |
| Ctrl-A / Home, Ctrl-E / End | line start / end |
| Left, Right | one character |
| Ctrl-Left, Alt-B / Ctrl-Right, Alt-F | one word |
| Ctrl-_, Alt-R | undo, redo (bounded to 256 states) |
| Backspace, Delete, Ctrl-D | delete before / after the cursor; Ctrl-D on an empty line exits |
| Ctrl-K, Ctrl-U, Ctrl-W / Ctrl-Delete, Alt-D | kill to end, to start, word back, word forward |
| Ctrl-Backspace | pick from the directories this shell has visited, most recent first |
| Ctrl-Y | yank the kill ring (one ring, shared by all kills) |
| Ctrl-C | discard the line, status `130` |
| Ctrl-L | clear the screen, keep the line |
| Ctrl-P | write a layout dump to `~/.cache/xshi/dump-<hex>` without touching the screen; the `xshi-dump` builtin does the same |
| Up, Down | move by visual row; at the first or last row, prefix search through session history |
| Tab, Ctrl-R | completion (§10), history search |

Consecutive typed characters form one undo step; every other edit records its
own. Cursor motion starts a new typing step, and undo restores the cursor
position recorded with the text.

Text is UTF-8 throughout: cursor motion and deletion work on characters, widths
account for wide characters, combining marks, variation selectors, and emoji
skin-tone modifiers, and a wide character with one column left wraps before
drawing. Long lines wrap as a grid; pasted multi-line input keeps its lines and
Up/Down move between them. Bracketed paste inserts text without executing it
and is rejected above 8,192 bytes. Submitting a multi-line buffer follows shell
line rules: a backslash-newline joins, a newline inside quotes is data, a
newline after `|`, `&&`, `||`, or `;` continues the command, and any other
newline separates commands.

The autosuggestion ghost text shows the rest of the newest matching history
entry after the cursor when the buffer is at least three characters, single
line, and the cursor is at its end; Right, End, or Ctrl-E accepts it, while
Ctrl-Right accepts only the next word.

Repainting returns to the top of the previous region and clears the larger of
the old and new row counts, so output never stacks across resizes or wrapped
lines. SIGWINCH reaches the read loop through a self-pipe and repaints.

History search (Ctrl-R) is a pager: a `search:` header, matches below it with
matched characters highlighted, Up/Down to move, Enter to accept into the
prompt, Escape or Ctrl-C to cancel. The query is an editable line buffer; a
bracketed paste inserts at the query cursor, re-filters the matches from the
full history, and resets the selection.

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
grid of up to six columns and at most one fewer row than the terminal height
opens with no selection; Tab and the arrows move the selection and preview it,
typing filters live, Enter accepts without submitting, Escape and Ctrl-C cancel.

Remote path completion runs `ssh -o BatchMode=yes -o ConnectTimeout=2 HOST ls
-dp PATH*` with the host as one argument, a three-second deadline, and a 64 KiB
output cap; any failure yields no candidates.

## 11. Builtins

Answered by the shell itself: `cd` (`cd -`, `~`), `exit`, `fg`, `bg`, `export`,
`set`, `unset`, `alias`, `eval`, `exec`, `history`, `z`, `denv`,
`l`, `ll`, `c`, `w`/`which`/`type`, `copy-scrollback`, `xshi-dump`, and `:`.
`echo` (`-n`, `-e`, `-E`), `pwd`, `true`, and `false` are internal so they work
in pipelines. `w`, `which`, and `type` print `NAME: builtin` followed by the
builtin's usage block; `command`, `test`, `printf`, and the other POSIX names
are reported as builtins, and those that are not internal resolve on `PATH`.
`source` and `.` are refused as unsupported, as is any `set` whose first operand
is not a variable name (`set -e` reports `set: expected a variable name`).

- A one-word line naming a directory becomes `cd`, and `..`, `...`, … climb.
  Relative `cd` and `z` targets are recorded in history as absolute paths.
- `l [path…]` hides dotfiles; `ll [path…]` shows them. Entries are
  `name size age`: directories and symlinks omit the size, ages are relative,
  symlink targets are shown, and names carry type colors that fade with age.
- `c` clears the screen and starts the next prompt on a fresh row.
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
reported. A second Ctrl-Z while the slot is occupied reports
`xshi: a job is already suspended; resume it with fg before suspending another`
and keeps the new command running in the foreground instead of replacing the
slot. `exit` and Ctrl-D with a live job warn once. The `exit` warning says
`xshi: there is a suspended job. Exit again to force quit.` The Ctrl-D warning
says `xshi: there is a suspended job. Press Ctrl+D again to force quit.`
Repeating the same gesture exits and sends the job's process group `SIGTERM`
then `SIGCONT`, so a stopped job can terminate. Any other line re-arms the warning.

## 13. Denv And z

Denv loads a directory's environment from the nearest ancestor (the current
directory first) holding `.envrc`, `.env`, or both. `.envrc` is evaluated
interactively by `bash` when its first line mentions bash, as XSH source when it
mentions `xsh`, and by `sh` otherwise; `.env` (dotenv) applies on top of the
result, as data. Both apply on startup, after every directory change, and after
`denv reload`; leaving the tree restores previous values, including whether they
were exported.

Each file requires trust: `denv allow` records the canonical path and the
SHA-256 of the file's content under `~/.local/share/xshi/denv/allow`, so any
edit revokes it, and `denv deny` removes it and marks the environment dirty.
A candidate `.envrc` that fails, or that calls the retired `source_env`,
`source_env_if_exists`, `source_up`, `source_up_if_exists`, `dotenv`, or
`dotenv_if_exists` helpers, is rejected whole: none of its variables apply and
`__DENV_DIRTY=1` shows the red prompt marker. The state is exported as
`__DENV_*` variables, and a shell that exits normally removes the active record
it wrote beside `allow/`.

`z` reads its scores from the same history (§8).

## 14. Tests

[Testing](TESTING.md#interactive-parity) owns the interactive test commands,
feature requirements, the drift and re-record workflow, and the PTY transcripts
that `crates/xshi/tests/parity.rs` replays against `xshi`.

## 15. Deferrals

`~user` expansion, brace expansion, here-documents, process substitution,
shell functions, `trap`, `readonly`, `shift`, `local`, job specs (`%1`), more
than one job, and background pipelines or lists are not implemented.
