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

Prefer typed standard-module APIs over shelling out or parsing command text.
Use `cli.parse` for ordinary option records, including short aliases and
clusters, and reserve `cli.tokens` for applets whose option grammar is itself
the feature. Keep usage errors local and explicit, preserve Unix-compatible
stdout shapes, and cover behavior through `core/tests/*.xsh`.
