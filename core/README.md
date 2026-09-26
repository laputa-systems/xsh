# Core Applet Style

Each `core/*.xsh` applet is a standalone command script by default. Duplicate
small helpers locally when that keeps an applet self-contained and easy to
inspect. Shared libraries under `core/lib/` are reserved for audited command
families whose behavior must stay consistent across multiple applets, such as
auth account parsing and shadow-file updates. `lib/text_input.xsh` preserves
the file and stdin operand order shared by the line-oriented applets.

`dev/release.xsh::package_core` installs applets without the `.xsh` suffix as
executable commands. It keeps that suffix for `core/lib/` modules so adjacent
`use lib.auth` imports resolve after extraction; modules install with mode
`0644`.

The `core/` audit found one stable repeated host boundary: reading text files
and `-` stdin operands in order. `fold`, `rev`, `shuf`, and `tr` now share
`lib/text_input.xsh` with `cut`, `head`, `tail`, `uniq`, and `sort`. Applet usage
errors remain local because their messages and accepted operand shapes differ;
`cli.applet` and `cli.parse` already own their option parsing.

Prefer typed standard-module APIs over shelling out or parsing command text.
Use `cli.parse` for ordinary option records, including short aliases and
clusters, and reserve `cli.tokens` for applets whose option grammar is itself
the feature. Keep usage errors local and explicit, preserve Unix-compatible
stdout shapes, and cover behavior through `core/tests/*.xsh`.

`system-report` keeps its typed snapshot model and pure presentation helpers in
`core/lib/system_report.xsh`; bounded source parsing lives in
`core/lib/system_report_collect.xsh`, and rooted Linux collection orchestration
lives in `core/lib/system_report_live.xsh`. `collect_from_root` accepts an
explicit source root for fixtures and captured views; local mount capacity
queries are disabled there unless `include_local_mount_usage` is enabled.
`collect_live` rejects active Linux dry-run mode, enables eligible rooted
capacity queries, and reads the current process-visible Linux view.
`--from FILE` validates saved v1 JSON and renders or projects that snapshot
without collecting host data. Scripts can import the model and collector
modules to make typed policy and device-relationship decisions.
Valid partial reports exit successfully and carry missing or limited source
coverage as section states and collection issues. Invalid options or section
names produce usage diagnostics (`cli.applet` parser errors or
`SystemReportError.Usage`), unreadable or malformed replay input uses
`SystemReportError.InvalidInput`, and live collection on a non-Linux host uses
`SystemReportError.Unsupported`. Unexpected collection failures keep their
source error. Unhandled command errors use XSH's runtime-failure exit status
`3`; successful help, version, and report output use status zero.

`core/wc.xsh` reads raw bytes for line and byte counts, so `-l` and `-c`
accept non-UTF-8 input. Word counts still use `Str.count_words()` and require
valid UTF-8.
`core/cat.xsh` and `core/tee.xsh` preserve file and stdin bytes through stdout.
`tee` append reads the existing destination and writes the concatenated bytes.
