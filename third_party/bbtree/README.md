# Vendored bbtree

Source: https://github.com/laputa-systems/bbtree, version 0.1.5.

This directory contains ordinary source files, not a Git submodule. XSH uses
the single-owner ordered map and set in `bbtree::unsync`. The included license
permits use under MIT or Apache-2.0.

The pinned `vstd` dependency supplies the upstream proof support and adds Verus
macro dependencies to the build graph. No file-map or inspection feature is
enabled in production. Keep the upstream source intact when refreshing it and
run its map/set differential, drop and iterator tests against the standard
library, then XSH's codec and compiler checks. Production size, memory and CPU
acceptance use the existing campaign gates.
