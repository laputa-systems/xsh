# Threadripper Host

`threadripper` is the native amd64 Alpine iteration host for musl coverage,
PGO, and benchmark work.

Use an interactive Bash session:

```sh
ssh -tt threadripper bash
```

The remote XSH checkout is `/home/josh/d/laputa-systems/xsh`. Work there
directly; do not mirror threadripper-only changes back to the local checkout
unless the user explicitly asks for that.

Prefer the interactive session for remote work. One-shot SSH commands can be
parsed by the login shell before Bash starts.

Disable pagers for non-interactive Git inspection, for example `GIT_PAGER=cat`
or `git --no-pager ...`; otherwise `git log` may drop the session into `less`.

Cargo is not on the default remote PATH. Export
`PATH=/home/josh/.cargo/bin:$PATH`, use `/home/josh/.cargo/bin/cargo`, or run
`make cov` from that checkout and let the native coverage target pass the Cargo
path through.

`git-lfs` may not be installed on a fresh Alpine host. Install it with
`doas apk add git-lfs`, then run `git lfs install --local` in the checkout
before touching `perf/pgo/*.profdata`.

The host is intentionally native amd64 musl. Do not use Docker for coverage,
PGO, or benchmark work there unless the user specifically asks for a Docker
comparison.

The base Alpine image is sparse: Python and `patch` may be missing. Prefer
repository tools, shell, Perl, and Make targets that already exist, or install
missing utilities explicitly when they materially reduce risk.

Useful host setup that would make future coding smoother: Cargo on the login
PATH, `git-lfs` preinstalled and initialized for this checkout, a non-Tailscale
fallback resolver, and basic development utilities such as `patch` and Python.

If `github.com` stops resolving, check `/etc/resolv.conf`: Tailscale DNS may
have overwritten it with `100.100.100.100`. `doas tailscale set
--accept-dns=false` plus ordinary resolvers restored GitHub resolution on
June 14, 2026.
