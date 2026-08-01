# Build this image for linux/arm64; the release binary below is aarch64.
FROM alpine:3.24.1@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b

ARG XSH_RELEASE_URL=https://github.com/laputa-systems/xsh/releases/download/release-435af27be8b4a6eaf3a1ef9748e5397806dea9d6/xsh-multicall-release-435af27be8b4a6eaf3a1ef9748e5397806dea9d6-aarch64-linux-musl
ARG XSH_RELEASE_SHA256=5e2ab232f76381861e70d60afa1ff87858a7b0f400e0c1ad382d32c1561e5d74

# Alpine supplies the BusyBox base. Keep the only additional network tools and
# trust store needed to bootstrap and use the standalone XSH release.
# libstdc++ is required at runtime by the pi (bun) binary baked in below.
RUN apk add --no-cache ca-certificates curl libstdc++ \
    && curl --fail --location --show-error --silent --retry 3 \
        --output /usr/local/bin/xsh "$XSH_RELEASE_URL" \
    && echo "$XSH_RELEASE_SHA256  /usr/local/bin/xsh" | sha256sum -c - \
    && chmod 0755 /usr/local/bin/xsh \
    && ln -s /usr/local/bin/xsh /usr/local/bin/xsht

# Prerelease pi (bun) binary baked in so no runtime mount is needed. This is
# the musl variant (build with `make bun-linux-arm64-musl` in ~/d/pi): a glibc
# pi cannot run inside the Alpine container.
COPY pi-bun /usr/local/bin/pi
RUN chmod 0755 /usr/local/bin/pi

USER root
WORKDIR /work

# These files are also available when /work is replaced with a bind mount at
# runtime. The task workspace can be mounted over this directory unchanged.
COPY agents.md handbook.md /work/
