# Core commands

Run a core applet from the repository root with
`xsh core/NAME.xsh -- [ARGUMENTS...]`. For example:

```sh
xsh core/system-report.xsh -- --json
xsh core/btop.xsh -- --replay BUNDLE
```

Use `--help` for applet options. The `--` separator passes them to the script.

See the [system-report guide](../docs/SYSTEM-REPORT.md) for Linux inventory
reports and the [btop guide](../docs/BTOP.md) for the monitor and replay
bundles. The [project README](../README.md) covers installation.

See [core command behavior](../docs/CORE.md) for byte, error, and host-boundary
contracts.
