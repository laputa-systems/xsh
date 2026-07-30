# XSHI Interactive Specification

This is the authoritative implementation contract for `xshi`. `xsh` remains a
strict script runner and `xsht` remains tooling-only. Interactive conveniences
exist only inside `xshi`; they must not change `.xsh` script syntax, normal
checking, normal runtime behavior, examples, docs generation, or tooling.

The target user experience is the existing `ish` shell at `~/d/ish`. For
terminal rendering and completion UI, `xshi` should borrow the `ish`
implementation as directly as practical. In particular, rendering code from
`ish/src/render.rs` and completion code from `ish/src/complete.rs` are
correctness baselines, not loose inspiration. Do not replace them with a new
terminal abstraction or third-party library. `ish` handles this with Rust and
`libc`; `xshi` should do the same.

## 1. Boundaries

`xshi` is an adapter layer over XSH facilities plus an interactive shell-subset
frontend. It is not a second script language.

Required separation:

- `xsh` does not accept shell-subset syntax such as bare `git status`.
- `xsht check`, formatting, docs generation, examples, and tests do not read
  `~/.config/xshi/config.xsh` or `~/.local/share/xshi/history`.
- Aliases, prompt state, denv trust, history search, completion, and
  autosuggestions are unavailable to scripts.
- Interactive compatibility commands are enabled only through explicit
  checker/runtime options.
- Any reusable library API added outside `crates/xshi/src/interactive/` must be neutral for
  existing scripts unless an interactive option is explicitly passed.

The expected source layout is:

```text
crates/xshi/src/main.rs
crates/xshi/src/interactive/
  mod.rs
  app.rs
  session.rs
  shell/
    lex.rs
    parse.rs
    ast.rs
    lower.rs
    glob.rs
  config.rs
  denv.rs
  edit.rs
  render.rs
  complete.rs
  history.rs
  prompt.rs
  listing.rs
  z.rs
```

`edit.rs` decodes keys and mutates the line buffer. `render.rs` owns terminal
geometry and repainting. `complete.rs` owns completion classification,
candidates, metadata, and grid state. These modules may depend on `Session`;
normal parser/checker/runtime modules must not depend on them.

## 2. Startup And Session State

Normal `xshi` startup requires both stdin and stdout to be TTYs. Non-TTY startup
exits `2` with a clear diagnostic. `xshi --help` works without a TTY. `xshi
--no-config` starts without reading config.

When attached to a TTY, `xshi` initializes shell-style job-control state for
itself only: it owns a shell process group, takes terminal foreground control,
ignores or safely handles job-control signals in the shell, and restores default
signal dispositions in external children. This policy must not make `xsh` or
`xsht` ignore Ctrl-C.

Persistent prompt-to-prompt state:

- current working directory, always absolute;
- a current-directory snapshot refreshed after successful cwd changes, containing
  entry names, metadata, symlink targets, and the snapshot time for immediate
  `l` rendering and cwd-root completion;
- environment as byte strings, with `PWD` and `OLDPWD` maintained on cwd
  changes;
- PATH executable names cached from the session environment for command-position
  completion and refreshed after config, denv, `set`, `unset`, assignment-only
  `PATH=...`, and cwd-triggered denv changes;
- aliases loaded from config and defined during the session;
- denv trust and dirty state;
- in-memory and on-disk history;
- last shell status as `i32`;
- previous process status for `$?` seeding;
- one optional interactive job slot for a running-background or stopped job;
- prompt cache inputs such as user, host, cwd, git branch, denv marker, and
  color mode.

Not persistent across prompt entries:

- top-level `let` or `var`;
- `proc`, `pure`, `type`, and `use`;
- imported modules;
- local values and checker scopes.

The process cwd may be updated for compatibility, but runtime calls should also
receive cwd explicitly. All successful cwd changes use one shared path so `cd`,
`z`, and denv hooks update `PWD`, `OLDPWD`, prompt inschema checks, and denv state
consistently.

After successful cwd changes, `xshi` eagerly reads the new directory once. The
snapshot is used for no-argument `l` and for completion of entries directly
under the cwd, including a new first token. `l` from this snapshot must not
perform directory, metadata, symlink-target, username, or clock reads while
rendering. The snapshot is invalidated before executing XSH prompt code,
interactive compatibility commands, or external commands, because those may
change filesystem state.

## 3. Status Model

`xshi` tracks:

- `last_status: i32`, the shell-compatible prompt status;
- `last_process_status: Option[ProcessStatus]`, the structured value exposed to
  XSH `$?`.

Status mapping:

- successful session builtin: `0`;
- session builtin usage error: `2`;
- failed `cd`, `z`, denv operation, or utility command: `1` unless documented
  otherwise;
- command not found: `127`;
- found but not executable or exec failure other than not-found: `126`;
- external command exit code: that code;
- external command signal: `128 + signal`;
- XSH parse/check error at prompt: `2`;
- XSH runtime traceback at prompt: runtime output status;
- Ctrl-C canceled prompt buffer: `130`;
- EOF/Ctrl-D on an empty buffer: exit with `last_status`;
- `exit`: exit with `last_status`;
- `exit N`: exit with `N` clamped to `0..255`; invalid `N` exits `2`.
- successful `cmd &` background spawn: `0`;
- foreground external command stopped by Ctrl-Z and auto-backgrounded: `148`;
- `fg` with no job: `1`;
- `bg` with no job or an already-running job: `1`;
- successful `bg` resume: `0`.

Pipelines use pipefail: the status is the rightmost nonzero segment status, or
`0` if every segment succeeds.

## 4. Input Classification

Each submitted buffer is classified before execution. Classification is lexical
and conservative; it should not require full XSH parsing to decide whether shell
fallback is forbidden.

Always XSH, with no shell-subset retry:

- declaration/control starts: `let`, `var`, `proc`, `pure`, `use`, `export`,
  `if`, `for`, `while`, `match`, `return`, `defer`, `guard`;
- `type NAME = ...`;
- expression-looking starts: `{`, `[`, `(`, string/path/fmt literals, numeric
  literals, `null`;
- XSH command forms: `run`, `print`, `eprint`;
- module-qualified starts such as `fs.write`, `json.decode`, or
  `process.which`.

Reserved session commands are handled before alias expansion or external
execution:

```text
exit cd set unset alias z denv c l w which history fg bg :
```

`true` and `false` are ambiguous:

- bare `true` and bare `false` at command position are interactive command
  forms with statuses `0` and `1`;
- expression-shaped uses such as `false or true`, `let x = false`, `[false]`,
  and `{ok: true}` are XSH;
- chain/pipeline uses such as `true && echo ok`, `false || echo ok`, and
  `true | wc -l` are shell-subset.

Assignment-looking input:

- `NAME=value` updates the session environment if `NAME` is valid;
- `NAME=value cmd` is a per-command environment assignment;
- invalid assignment-looking input such as `BAD-NAME=value` is a syntax error
  with status `2`, not a plain shell word.

## 5. Shell-Subset Frontend

The shell-subset lexer/parser/lowerer lives only under `crates/xshi/src/interactive/shell/`.
It lowers to normal XSH run/process structures or narrow runtime APIs. It must
not teach the core parser shell syntax.

Required shell-subset behavior:

- bare external commands run without `run`;
- chains support `;`, `&&`, and `||`;
- pipelines support `|` and `|&`;
- redirections support `<`, `>`, `>>`, `2>`, `2>>`, `2>&1`, and `1>&2`;
- a single trailing `&` is accepted only for one simple external command;
- assignment-only env updates and per-command env assignments work;
- tilde, variables, arithmetic expansion, command substitution, quotes, sorted globs, `**`, and
  no-match glob errors follow the documented expansion order;
- session builtins inside pipelines are rejected before any segment runs;
- `sudo` argv are not rewritten; delegated commands resolve through the normal
  PATH rules of the invoked privilege tool;
- nonzero external exits are ordinary statuses, not XSH tracebacks.

Expansion order:

1. alias expansion in command position only;
2. tilde expansion for `~` and `~/...`;
3. variable expansion;
4. arithmetic expansion;
5. command substitution;
6. quote removal;
7. glob expansion.

Arithmetic expansion uses `$((EXPR))`, evaluates signed integer arithmetic,
and produces a single decimal word fragment. Supported operators are unary
`+`, `-`, `!`, binary `+`, `-`, `*`, `/`, `%`, and parentheses. Bare variable
names and `$NAME` read integer values from the session environment; unset or
empty variables evaluate as `0`.

Single quotes disable variable expansion, arithmetic expansion, command substitution, tilde
expansion, and glob expansion. `~user` is deferred.

Background execution is deliberately narrow in v1. `sleep 10 &` and
`FOO=bar sleep 10 &` may start an external command in the single job slot.
Chains, pipelines, session builtins, assignment-only input, and non-trailing
`&` are rejected. This syntax is xshi-only and does not affect `.xsh`, `xsh`,
or `xsht`.

## 6. Config

Config path: `~/.config/xshi/config.xsh`. Missing config is not an error.
`--no-config` skips config entirely.

Interactive `xshi` startup reads `/etc/profile` before user config. `xshi -c`
also reads `/etc/profile` when argv0 begins with `-`. The supported profile
subset is `NAME=value` and `export NAME=value`; unsupported lines are ignored.

V1 config is a data record:

```xsh
{
  env: [
    {name: "EDITOR", value: "/usr/bin/vim"},
  ],
  aliases: [
    {name: "gs", source: "git status -sb"},
  ],
}
```

Schema:

- `env: List[{name: Str, value: Str}]`;
- `aliases: List[{name: Str, source: Str}]`.

Unknown fields warn and are ignored. Invalid env entries or aliases warn and
are skipped. Alias sources are validated at load time for parseability,
recursion, and invalid pipeline/session-builtin contexts. Config warnings go to
stderr before the first prompt and do not prevent startup unless terminal
initialization itself fails.

Config never affects scripts or tooling.

## 7. History

History path: `~/.local/share/xshi/history`.

On-disk records are one entry per line:

```text
TIMESTAMP_MS COMMAND
```

`TIMESTAMP_MS` is an epoch-millisecond integer. `COMMAND` is the command source
with embedded newlines collapsed to spaces. Readers accept this compact format,
legacy plain command lines, temporary three-field `TIMESTAMP_MS 0 COMMAND`
records, and `ish` `:ish-history:v1` records for migration. Writers emit only
the two-field compact format.

For prompt-entry performance, `xshi` reads wall-clock time only when a command
is accepted into persistent history. Editing, prompt rendering, completion,
autosuggestion, history navigation, and history search must not read the clock.
The timestamp read is intentionally adjacent to the existing history append I/O,
so it is paid once per stored command and can reconcile ordering across multiple
`xshi` instances.

History rules:

- append submitted prompt entries after successful classification/lowering;
- do not store empty, whitespace-only, canceled, or EOF buffers;
- suppress consecutive duplicates;
- store in-memory history as one command arena plus compact offset and metadata
  arrays so prefix search, autosuggestions, history navigation, and fuzzy
  search return borrowed entries or entry indices rather than cloned commands;
- maintain a hash-to-index map so adding or syncing a duplicate command removes
  the older in-memory entry before appending the newer one;
- keep an implementation-private binary cache next to the text history file;
  startup loads the cache, syncs only the appended text tail, and compaction
  rewrites the cache atomically before truncating the text tail;
- synchronize history from other `xshi` instances with one file-size metadata
  check per prompt; when the file grew, read only bytes after the previous
  offset;
- protect cache compaction with a non-blocking lock and skip compaction if
  another shell is already compacting;
- store the expanded source that actually ran where the lowering path can
  provide it;
- support prefix navigation and fuzzy search;
- keep fuzzy search result buffers reusable and index-based; search results
  carry entry indices, fixed match-position storage, and score metadata;
- history lookup must not block prompt rendering on large files.

Autosuggestions use session history only. They are described in §10 because
they are rendered as prompt ghost text.

## 8. Prompt

The prompt displays user, host, shortened cwd, git branch, denv dirty marker,
and success/failure marker. Color mode follows TTY and `NO_COLOR`.

Rendering requirements:

- prompt width calculations ignore ANSI color sequences;
- deleted cwd must not panic prompt rendering;
- git branch lookup is cached outside prompt rendering and refreshed after cwd
  changes and after each accepted command, so local git metadata changes become
  visible on the next prompt without doing git filesystem reads during render;
- status color and marker use `last_status`;
- denv dirty marker is visible before the command marker.

## 9. Editing

The editor runs in raw mode only after CLI, TTY, and config preflight succeed.
Bracketed paste is enabled while raw mode is active and disabled on every exit
path.

Required key behavior:

- UTF-8 insertion and cursor movement use byte-safe character boundaries;
- Left/Right move by character;
- Alt/Control Left/Right move by word;
- Home/Ctrl-A and End/Ctrl-E move to start/end;
- Backspace/Delete delete backward/forward;
- Ctrl-K, Ctrl-U, Ctrl-W, and Ctrl-Y implement kill/yank;
- Ctrl-C cancels the current buffer, sets status `130`, and does not add
  history;
- Ctrl-D on an empty buffer exits with `last_status`;
- Ctrl-L clears the visible screen;
- Ctrl-R enters interactive fuzzy history search with a separate query buffer
  and does not commit history text into the prompt until the search is accepted;
- in history search, Enter accepts the selected history entry into the prompt,
  while Escape and Ctrl-C cancel the search and restore the original buffer;
- bracketed paste inserts text and does not execute until Enter outside paste;
- multiline continuation handles trailing shell operators, trailing backslash,
  and unterminated shell quotes/substitutions.

## 10. Rendering And Autosuggestions

Terminal rendering is correctness-sensitive. `xshi` should port the `ish`
rendering model directly:

- `RenderedRegion` tracks whether the previous prompt render is anchored, how
  many rows were painted, and the cursor row/column;
- prompt repainting begins by returning to the top of the previous rendered
  region and clearing the larger of previous/new row counts;
- single-line layout accounts for prompt display width, line display width,
  cursor display column, autosuggestion width, terminal width, and exact
  terminal-edge pending wrap;
- multiline layout gives subsequent lines a continuation prompt and computes
  cursor row/column by segment;
- cursor restoration moves from the rendered region end back to the logical
  cursor position;
- rendering must avoid writing into the terminal’s last column when drawing
  completion grids;
- all terminal size, raw mode, cursor movement, polling, and subprocess helper
  behavior uses standard library plus `libc`, matching `ish`.
- terminal size is sampled once per input/render cycle and threaded through
  completion and repaint helpers; tight edit loops must not perform separate
  size ioctls for completion classification, autosuggestion rendering, and final
  repaint in the same cycle.

Autosuggestion ghost text:

- source is the most recent history entry with the current buffer as a prefix;
- shown only when the buffer length is at least 3, the buffer is single-line,
  and the cursor is at end of line;
- rendered in dim gray after the line content;
- does not become part of the buffer until accepted;
- Right Arrow at end of line accepts the full suggestion;
- ghost text is absent during completion menus, history search, command
  submission, cancellation, and child-process handoff.

History search UI:

- renders a transient `search: ` header containing the query, with fuzzy matches
  listed below it;
- highlights the selected result with reverse video and matched characters in
  unselected rows;
- accepts a result into the prompt on Enter but does not submit it until a later
  Enter in normal editing mode;
- suppresses autosuggestions and completion menus while active.

## 11. Completion

Completion is scoped to the current buffer. It must not rely on XSH top-level
bindings from previous prompt entries because those do not persist.

Candidate sources:

- XSH keywords, current-buffer local names, standard modules/APIs, methods,
  fields where checker context is available, and type names in XSH contexts;
- aliases;
- xshi session builtins;
- shell command positions;
- PATH executables;
- paths and directories;
- `$` environment variables;
- SSH hosts and remote paths in ssh-like command contexts.

Completion state and rendering should follow `ish/src/complete.rs`:

- `Completions` stores names in one arena string plus compact entry offsets;
- `CompEntry` stores mtime, name offset/length, display width, and flags for
  directory, symlink, executable, and host;
- sorting is deterministic, with mtime sorting for path entries and
  case-insensitive alphabetical tie-breakers;
- display width is computed locally, including combining marks and wide CJK or
  emoji ranges; do not add a unicode-width dependency;
- `compute_grid(entries, term_cols)` tries up to 6 columns and returns the
  widest column-major layout that fits the terminal;
- the selected candidate is highlighted with reverse video;
- directories, symlinks, executables, and hosts use distinct styles when color
  is enabled;
- visible grid rows are capped and scrolled around the selected row.

Path completion:

- `~` and `~/...` expand against the session home for lookup while preserving
  the user-facing tilde prefix on insertion/display;
- `~user` is not completed in V1;
- `cd` and `z` argument contexts complete only directories and symlinks to
  directories;
- hidden entries are omitted unless the typed prefix starts with `.`;
- `.` and `..`, invalid UTF-8 names, and names containing control characters
  are omitted;
- prefix matches win; only when no prefix matches exist may completion fall
  back to case-insensitive substring matches;
- directory completions append `/`;
- quoted insertions keep `~/` outside quotes so tilde expansion remains valid.
- non-cwd directory completion results are cached by directory path and directory
  mtime/nanosecond stamp; repeated completions in an unchanged directory reuse
  cached names and metadata, while mtime changes force a refresh. The eager cwd
  snapshot remains the source for entries directly under the current directory.

Partial path completion:

- `complete_partial_path` treats unresolved intermediate components as
  directory prefixes, fish-style;
- if the literal directory prefix already exists, ordinary path completion owns
  the result;
- expansion is capped to avoid combinatorial explosions;
- relative, absolute, and `~/` roots preserve their user-facing prefix.

Examples:

```text
cd ~/<Tab>             -> list directories under HOME, inserted as ~/name/
cd ~/d/pr/xs<Tab>     -> may resolve d -> d, pr -> projects, xs -> xsh
ls tom<Tab>           -> may find Cargo.toml only if no prefix matches exist
```

SSH completion:

- command contexts: `ssh`, `scp`, `rsync`, `sftp`, and `mosh`;
- host candidates come from `~/.ssh/config` `Host` entries and
  `~/.ssh/known_hosts`;
- wildcard hosts, `?` hosts, `.`, comments, empty lines, and hashed known-host
  lines are skipped;
- `host:path-prefix` invokes bounded remote path completion using
  `ssh -o BatchMode=yes -o ConnectTimeout=2`;
- remote output is parsed from `ls -dp`, strips the already typed remote
  directory prefix, and appends `/` for directories;
- timeout, auth failure, ssh failure, malformed output, or unavailable ssh
  returns no candidates rather than blocking or printing prompt noise.

Completion UI:

- Tab inserts a single unambiguous candidate or a longer common prefix;
- ambiguous Tab opens the completion grid;
- opening an ambiguous grid does not immediately accept or preview a candidate;
- repeated Tab and arrow keys navigate the selected candidate when a completion
  grid is active and preview that candidate in the prompt buffer;
- Enter accepts the selected candidate and closes the grid without submitting
  the command; a second Enter submits after completion mode has ended;
- Escape/Ctrl-C cancels the grid;
- typing while the grid is open filters/recomputes candidates;
- completion UI text must not explain shortcuts inside the prompt.

## 12. Session Builtins And Utilities

Builtins:

- `exit [N]`;
- `cd [PATH]`;
- `set NAME=VALUE`;
- `unset NAME`;
- `alias NAME=SOURCE`;
- `z QUERY`;
- `denv allow|deny|reload|status`;
- `c`;
- `l [PATH...]`;
- `w NAME` and `which NAME`;
- `fg`;
- `bg`;
- `:`.

`cd` with no path changes to `HOME`. `cd -` changes to `OLDPWD`. `cd` in a
pipeline is rejected. `cd src && git status` runs as a shell-subset chain with
session cwd mutation before the second segment.

`:` is the shell no-op builtin. It succeeds with no output and exists so
`xshi -c` can serve as the bootstrap `/bin/sh` for command chains such as
`: && cc ... && :`.

`l` renders deterministic listings and does not shell out to `ls`. Multiple
targets are rendered in argument order. The default long listing omits hardlink
count and group name, uses a uid-to-username map cached at `xshi` startup, and
aligns metadata columns so all filenames in the rendered listing start at the
same column.

Core utilities are ordinary PATH commands. `xshi` does not promote a separate
compatibility-builtin command set.

`history [N]` is a session builtin, not a compatibility builtin. It prints
numbered entries from the current `xshi` history, optionally limited to the last
`N` entries, and remains unavailable to normal `.xsh` scripts.

External tools whose value is their existing interactive process behavior, such
as `less`, `man`, `ssh`, and `git`, are not promoted as native compatibility
builtins. Invoke them as external commands in `xshi` or through explicit `run`
in scripts.

`du` reports apparent sizes and supports `-h`, `-s`, `-a`, `-c`, default path
`.`, and deterministic recursive output.

`tree` supports path operands, `-a`, `-d`, `-L N`, symlink target display,
final counts, `-I`/`--no-ignore` as a compatibility no-op, and
`--color=auto|always|never` as a compatibility no-op.

`pstree` uses `process.list()` data. On macOS, its default view delegates to the
host `pstree -w` implementation so the output, process visibility, command
arguments, root selection, and tree glyphs have host-tool parity. Other
platforms use the XSH renderer, which defaults to psmisc-style `-Gatlp` output:
command arguments, VT100-style tree drawing, long lines, full process names,
and PID display. The XSH renderer accepts `-a`/`--arguments`, `-A`/`--ascii`,
`-c`/`--compact-not`, `-G`/`--vt100`, `-l`/`--long`, `-h`/`--help`,
`-p`/`--show-pids`, `-s`/`--show-parents`, `-t`/`--thread-names`, and
`-T`/`--hide-threads`, plus optional `PID` or `USER` selection, cycle
protection, and deterministic child ordering by pid.
Thread-oriented utilities can use `process.threads()` for per-thread records on
Linux and macOS.

`rg` supports pattern plus paths, `-e`, `-i`, `-F`, `-w`, `-x`, `-v`, `-n`,
`-H`, `-h`, `-l`, `-c`, `-q`, `--hidden`, `-I`/`--no-ignore`, `-g`/`--glob`,
and `--color=auto|always|never`. A match returns `0`; no matches return `1`;
usage errors return `2`.

`fd` supports optional pattern and roots, `-H`/`--hidden`,
`-I`/`--no-ignore`, `-d`/`--max-depth`, `-t`/`--type`, `-e`/`--extension`,
`-g`/`--glob`, `-i`/`--ignore-case`, `-a`/`--absolute-path`, `-0`/`--print0`,
and repeated `-E`/`--exclude`. No matches are still status `0`.

## 13. Single-Job Control

`xshi` has exactly one managed job slot. A job stores the managed child, pid,
process group, display command, state (`RunningBackground` or `Stopped`), saved
child terminal attributes when available, last known status, and whether a
notification has already been printed. This is an interactive-only feature; it
does not add shell job syntax to `.xsh` files.

Foreground external commands temporarily leave raw mode and bracketed paste,
spawn the child in a new process group with default child signal dispositions,
give the terminal foreground to the child process group, wait with stopped
status reporting enabled, reclaim terminal foreground for the shell, then
resume raw mode before prompting. Ctrl-C during a foreground job is delivered by
the terminal to the child process group and maps to `128 + signal`.

`cmd &` starts one simple external command without giving it terminal
foreground. The child inherits stdio, enters a new process group, and is stored
as `RunningBackground`. On success, `xshi` prints a short job notification,
sets prompt status to `0`, and invalidates the cwd snapshot. If the slot is
already occupied, the command is rejected with status `1`.

When a foreground external command stops due to Ctrl-Z, `xshi` captures the
child terminal attributes when possible, reclaims terminal foreground, stores
the job, immediately sends SIGCONT, marks it `RunningBackground`, prints
`xshi: backgrounded: ...`, and returns prompt status `148`. If the job slot is
already occupied, `xshi` must not silently drop either process; the
implementation reports the conflict and leaves the newly stopped process
unmanaged rather than replacing the existing slot.

`fg` is a session builtin. With no job it prints `xshi: fg: no background job`
and returns status `1`. Otherwise it polls the job first; an already-complete
job is reported, cleared, and returns its job status. A running-background job
is foregrounded and waited. A stopped job is foregrounded, restored terminal
attributes are applied when available, SIGCONT is sent, and the job is waited.
Exit or signal completion clears the slot and sets prompt status from the job.
Stopping again uses the same Ctrl-Z auto-background policy.

`bg` is a session builtin. With no job it returns status `1`. If the job is
stopped, `bg` sends SIGCONT to the process group, marks it
`RunningBackground`, prints a resume notification, and returns status `0`. If
the job is already running in background, it prints `xshi: bg: job already
running` and returns status `1`. `bg` never takes terminal foreground and never
waits for completion.

Before each prompt render, `xshi` polls the job slot without blocking. Exited
or signaled background jobs are reported and cleared without overwriting
`last_status`; stopped background jobs are marked `Stopped` and reported.
Actual reaping happens in normal code, not in a signal handler. `exit` with a
live job is rejected with status `1`; the user must `fg` it, let it finish, or
kill it externally.

## 14. Denv And z

`z` derives scoring from xshi history only, using `cd` and `z` entries.
Successful jumps route through the same cwd-change path as `cd`.

Denv hooks run on startup, after cwd changes, and before each prompt render.
Denv uses a git-root directory snapshot cache to avoid filesystem reads while
the current directory stays inside the cached repository and no prompt command
has invalidated directory snapshots. Added or removed `.env` and `.envrc` files
at the git root are picked up after commands that may mutate the filesystem.
Trust state is explicit. Untrusted hooks warn and do not run. Dirty state is
visible in the prompt. Hook execution mutates only the xshi session environment.

## 15. Tests

Required unit coverage:

- classification ambiguity for XSH starts, `true`/`false`, `type`, and
  assignment-looking input;
- shell lexing/parsing/lowering, expansion order, aliases, globbing,
  redirections, status mapping, and config checks;
- shell parser and validation coverage for trailing `&`, `&&`, bare `&`,
  rejected background chains, rejected background pipelines, and rejected
  background builtins;
- session builtin detection for `fg` and `bg`, including aliases not
  overriding them;
- completion path behavior for `~/`, hidden files, directory-only contexts,
  prefix-vs-substring fallback, partial paths, host parsing, remote output
  parsing, and `compute_grid`;
- rendering geometry for ANSI prompt width, wrapped lines, multiline input,
  pending-wrap boundaries, completion grids, and autosuggestion ghost text;
- history prefix/fuzzy lookup and duplicate suppression.

Required PTY coverage:

- TTY-gated startup and `--help` on non-TTY;
- prompt loop execution;
- backspace and UTF-8 editing;
- Ctrl-C status and cancellation;
- bracketed paste;
- `cd ~/<Tab>` path completion;
- completion grid cursor preservation in narrow terminals;
- Right Arrow accepting autosuggestion;
- external commands run with terminal mode restored.
- `sleep 1 &` returns a prompt immediately and reports completion before a
  later prompt;
- a second background job is rejected while the single slot is occupied;
- `fg` can foreground a background job, and Ctrl-C then returns a prompt and
  clears the slot;
- Ctrl-Z on a foreground external command auto-backgrounds it, `bg` reports an
  already-running job, and `fg` can foreground it again;
- unsupported `cmd | cmd &`, `cmd && cmd &`, `cd /tmp &`, and compatibility
  builtin background forms are rejected.

Required separation coverage:

- `xsh -i` and `xsh --interactive` fail with status `2` and mention `xshi`;
- `xsh script.xsh` with `git status` fails unless explicit `run` is used;
- `xsht check` ignores xshi config aliases;
- docs generation and examples do not read xshi config/history;
- xshi denv/env/session changes do not leak into scripts except through the
  inherited OS environment of the launched process.

Verification gates for broad interactive changes:

```sh
cargo test --lib interactive
cargo test --test runtime xshi
make docs
cargo test
```

Never build release binaries, run pre-commit hooks, or push.

## 16. Explicit Deferrals

The following are not V1 behavior unless this spec is updated:

- POSIX shell compatibility as a goal;
- multi-job control, job spec grammar, `%1`, `%+`, `jobs`, `disown`, and
  `kill %1`;
- background pipelines, background chains, and background session builtins;
- shell functions;
- shell arithmetic syntax;
- arrays as shell syntax;
- process substitution;
- `~user`;
- persistence of aliases/env changes back to config.
