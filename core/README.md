# Core commands

`core/` contains XSH scripts for system and text utilities. Run an applet from
a source checkout with `xsh core/NAME.xsh [ARGUMENTS...]`. For example:

```sh
xsh core/system-report.xsh --json
xsh core/btop.xsh -- --replay BUNDLE
```

Use the applet's usage output for its options. The XSH separator before btop's
options passes them to the script.

The [system-report guide](SYSTEM-REPORT.md) explains Linux inventory reports,
and the [btop guide](BTOP.md) describes the monitor and replay bundles. See the
[project README](../README.md) for getting and installing XSH.
