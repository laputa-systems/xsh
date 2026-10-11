# GNU native-port oracle

`Dockerfile.gnu-oracle` builds standalone GNU coreutils 9.12 executables from
the prepared reference tree pinned in `dev/compat/upstream.lock.json`. It copies
and cleans the source's existing build artifacts before configuring for Debian
glibc; the original reference tree is never modified. Optional commands
`arch`, `hostname`, `kill`, and `uptime` are enabled. NLS translations are disabled;
glibc locale behavior is retained. SELinux development headers are omitted
because the pinned Debian base and unstable development package currently
require different libselinux versions; SELinux-specific behavior is unavailable.

Build from the repository root, using a fresh scratch context:

```sh
mkdir -p .work/gnu-oracle/context
cp -a ../ref/gnu-coreutils-9.12 .work/gnu-oracle/context/source
DOCKER_BUILDKIT=0 docker build --memory=4g --memory-swap=4g \
  -t xsh-oracle-gnu-9.12 \
  -f dev/compat/port/Dockerfile.gnu-oracle .work/gnu-oracle/context
```

The Dockerfile bounds compilation to eight jobs. The legacy builder enforces
the four GiB build-container limit; BuildKit ignores those memory options.
The base image is pinned by digest; Debian package installation still uses the
unstable package index. The runtime has account and group `compat` at UID/GID
1000, the `C.utf8` and `zh_CN.gb18030` locales, ISO-8859-1 `en_US` and `fr_FR`,
and UTF-8 locales for `en_US`, `fr_FR`, `de_DE`, `es_ES`, `it_IT`, `pt_BR`,
`ja_JP`, `zh_CN`, `hu_HU`, `fa_IR`, `th_TH`, and `am_ET`. Both `en_US.UTF-8`
and `en_US.utf-8` resolve to the UTF-8 locale. The image build checks the exact
locale names used by the frozen fixtures. `strace`, `setfacl`, `getfacl`, and `filefrag` support fixtures that
inspect system calls, ACLs, and file allocation. Debian's BusyBox is also
installed; it is not the pinned BusyBox reference in `upstream.lock.json`.
`/usr/local/bin` precedes Debian commands, preserving each GNU executable's
own command name and diagnostics.

Select this image when running unchanged native ports:

```sh
XSH_ORACLE_IMAGE=xsh-oracle-gnu-9.12 XSH_BIN=/path/to/release/xsh \
  python3 dev/compat/port/oracle_port.py core/tests/test-uu-basename.xsh
```

The image build asserts GNU versions, glibc linkage, identity, and locale
availability. Logs and further probe output can remain in `.work/gnu-oracle`.
