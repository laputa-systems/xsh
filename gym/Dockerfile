# The XSH and XSHT binaries are built locally by `make dist-Linux` (or
# `dist-Linux-docker` on macOS) and staged in .dist before this image is built.
# The image remains platform-specific.
FROM alpine:3.24.1@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b

ARG PI_RELEASE_URL=https://github.com/joshuarli/pi/releases/download/v0.83.0/pi-776ea644a-bun-1.3.14-linux-arm64-musl
ARG PI_RELEASE_SHA256=9d881d312d1b7ff478d679e5f04cbe605f92a2c40cc5a78653412ef282dc1ca1

# Alpine supplies the BusyBox base. Keep the only additional network tools and
# trust store needed to use the baked-in Pi binary and task-approved curl.
# libstdc++ is required at runtime by the pi (bun) binary baked in below.
RUN apk add --no-cache ca-certificates curl libstdc++

COPY .dist/xsh /usr/local/bin/xsh
COPY .dist/xsht /usr/local/bin/xsht
RUN chmod 0755 /usr/local/bin/xsh /usr/local/bin/xsht

# Prerelease pi (bun) binary baked in so no runtime mount is needed. It is
# fetched from the release URL at build time; ADD does not verify checksums, so
# the pinned sha256 is checked against the downloaded file.
ADD --chmod=755 "$PI_RELEASE_URL" /usr/local/bin/pi
RUN echo "$PI_RELEASE_SHA256  /usr/local/bin/pi" | sha256sum -c -

USER root
WORKDIR /work

# These files are also available when /work is replaced with a bind mount at
# runtime. The task workspace can be mounted over this directory unchanged.
COPY runtime/agents.md runtime/handbook.md runtime/review.md /work/
