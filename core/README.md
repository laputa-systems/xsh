# Core Applet Style

Each `core/*.xsh` applet is a standalone command script by default. Duplicate
small helpers locally when that keeps an applet self-contained and easy to
inspect. Shared libraries under `core/lib/` are reserved for audited command
families whose behavior must stay consistent across multiple applets, such as
auth account parsing and shadow-file updates.

Prefer typed standard-module APIs over shelling out or parsing command text.
Use `cli.parse` for ordinary option records, including short aliases and
clusters, and reserve `cli.tokens` for applets whose option grammar is itself
the feature. Keep usage errors local and explicit, preserve Unix-compatible
stdout shapes, and cover behavior through `core/tests/*.xsh`.
