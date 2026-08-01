# Build this image for linux/arm64; the release binary below is aarch64.
FROM alpine:3.24.1@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b

ARG XSH_RELEASE_URL=https://github.com/laputa-systems/xsh/releases/download/release-435af27be8b4a6eaf3a1ef9748e5397806dea9d6/xsh-multicall-release-435af27be8b4a6eaf3a1ef9748e5397806dea9d6-aarch64-linux-musl
ARG XSH_RELEASE_SHA256=5e2ab232f76381861e70d60afa1ff87858a7b0f400e0c1ad382d32c1561e5d74
ARG PI_RELEASE_URL=https://github.com/joshuarli/pi/releases/download/v0.83.0/pi-776ea644a-bun-1.3.14-linux-arm64-musl
ARG PI_RELEASE_SHA256=9d881d312d1b7ff478d679e5f04cbe605f92a2c40cc5a78653412ef282dc1ca1

# Alpine supplies the BusyBox base. Keep the only additional network tools and
# trust store needed to bootstrap and use the standalone XSH release.
# libstdc++ is required at runtime by the pi (bun) binary baked in below.
RUN apk add --no-cache ca-certificates curl libstdc++ \
    && curl --fail --location --show-error --silent --retry 3 \
        --output /usr/local/bin/xsh "$XSH_RELEASE_URL" \
    && echo "$XSH_RELEASE_SHA256  /usr/local/bin/xsh" | sha256sum -c - \
    && chmod 0755 /usr/local/bin/xsh \
    && ln -s /usr/local/bin/xsh /usr/local/bin/xsht

# Prerelease pi (bun) binary baked in so no runtime mount is needed. It is
# fetched from the release URL at build time; ADD does not verify checksums, so
# the pinned sha256 is checked against the downloaded file.
ADD --chmod=755 "$PI_RELEASE_URL" /usr/local/bin/pi
RUN echo "$PI_RELEASE_SHA256  /usr/local/bin/pi" | sha256sum -c -

USER root
WORKDIR /work

# These files are also available when /work is replaced with a bind mount at
# runtime. The task workspace can be mounted over this directory unchanged.
COPY agents.md handbook.md review.md /work/
