.PHONY: build lint docs test cov test-linux test-linux-ci test-macos-ci bench bench-fast bench-pgo bench-syscalls pgo-profile release-pgo install-darwin install-linux dist dist-native dist-ci

DARWIN_CODESIGN_FLAGS ?=
ifneq ($(DARWIN_CODESIGN_ENTITLEMENTS),)
DARWIN_CODESIGN_FLAGS += --entitlements $(DARWIN_CODESIGN_ENTITLEMENTS)
endif
# Keep Rust and native dependencies on the same minimum macOS version. Without
# this, the current SDK can stamp aws-lc-sys objects with a newer deployment
# target than the Rust linker target.
DARWIN_DEPLOYMENT_TARGET ?= 26.0
TARGET ?= x86_64-unknown-linux-musl
COV_BACKEND ?= $(shell if [ "$$(uname -s)" = Linux ] && [ "$$(uname -m)" = x86_64 ] && [ -f /etc/alpine-release ]; then echo native; else echo docker; fi)
COV_CARGO ?= $(shell command -v cargo 2>/dev/null || printf '%s/.cargo/bin/cargo\n' "$$HOME")
COV_CARGO_BIN ?= $(patsubst %/,%,$(dir $(COV_CARGO)))
COV_NATIVE_LINKER ?= $(shell command -v cc 2>/dev/null || command -v clang 2>/dev/null || command -v gcc 2>/dev/null)
DIST_PROFILE ?= dist
DIST_PROFILE_DIR = $(if $(filter release,$(DIST_PROFILE)),release,$(DIST_PROFILE))
CARGO_BUILD_WARNINGS = deny
export CARGO_BUILD_WARNINGS
DIST_BUILD_STD_FLAGS ?= -Z build-std=std
DIST_RUSTFLAGS = -Zlocation-detail=none -Zunstable-options -Cpanic=immediate-abort
DIST_TARGET_RUSTFLAGS =
DIST_TARGET_CFLAGS =

ifeq ($(TARGET),x86_64-unknown-linux-musl)
DIST_TARGET_RUSTFLAGS += -C target-cpu=x86-64-v3
DIST_TARGET_CFLAGS += -march=x86-64-v3
endif
ifeq ($(TARGET),aarch64-unknown-linux-musl)
# neoverse-n2 enables SVE and SVE2, which Apple Silicon does not support. Disable
# both so the release binary runs on both Graviton (where SVE/SVE2 are available)
# and Apple Silicon Linux VMs (OrbStack / Docker arm64 VMs on macOS), which trap
# SVE/SVE2 instructions with SIGILL.
DIST_TARGET_RUSTFLAGS += -C target-cpu=neoverse-n2 -C target-feature=-sve,-sve2
DIST_TARGET_CFLAGS += -mcpu=neoverse-n2+nosve+nosve2
endif
ifeq ($(TARGET),aarch64-apple-darwin)
DIST_TARGET_RUSTFLAGS += -C target-cpu=apple-m1
DIST_TARGET_CFLAGS += -mcpu=apple-m1
endif

DARWIN_DIST_RUSTFLAGS ?= $(RUSTFLAGS) -C linker=rust-lld -C linker-flavor=ld64.lld -C link-arg=--icf=safe
DIST_DARWIN_RUSTFLAGS ?= $(DARWIN_DIST_RUSTFLAGS) $(DIST_RUSTFLAGS) $(DIST_TARGET_RUSTFLAGS)

lint:
	cargo fmt --all
	cargo clippy --fix --allow-dirty --all-targets --all-features --quiet
	cargo build -p xsht
	./target/debug/xsht lint --fix
	./target/debug/xsht fmt

install: install-$(shell uname -s)

install-Darwin: install-darwin

install-Linux: install-linux

install-darwin:
	MACOSX_DEPLOYMENT_TARGET="$(DARWIN_DEPLOYMENT_TARGET)" RUSTFLAGS="$(strip $(DARWIN_DIST_RUSTFLAGS))" cargo build --release -p xsh-multicall --no-default-features --features "native-tests net tools"
	cp ./target/release/xsh-multicall ~/usr/bin/xsh-multicall
	ln -sf xsh-multicall ~/usr/bin/xsh
	ln -sf xsh-multicall ~/usr/bin/xshi
	ln -sf xsh-multicall ~/usr/bin/xsht
	codesign -fs - $(DARWIN_CODESIGN_FLAGS) ~/usr/bin/xsh-multicall
	xattr -d com.apple.quarantine ~/usr/bin/xsh-multicall 2>/dev/null || true

LINUX_INSTALL_CRT_DIR = target/llvm-crt
LINUX_INSTALL_CRT_OBJS = $(LINUX_INSTALL_CRT_DIR)/Scrt1.o $(LINUX_INSTALL_CRT_DIR)/crti.o $(LINUX_INSTALL_CRT_DIR)/crtn.o
LINUX_INSTALL_RUSTFLAGS ?= -C linker=clang -C link-arg=-B$(CURDIR)/$(LINUX_INSTALL_CRT_DIR) -C link-arg=-B$(CURDIR)/tools -C link-arg=-fuse-ld=lld
LINUX_INSTALL_ENV = PATH="$$HOME/.cargo/bin:$$PATH" CC=clang AR=llvm-ar CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=clang CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(LINUX_INSTALL_RUSTFLAGS))"

install-linux: $(LINUX_INSTALL_CRT_OBJS)
	$(LINUX_INSTALL_ENV) cargo build --release -p xsh-multicall --no-default-features --features "native-tests net tools"
	cp ./target/release/xsh-multicall ~/usr/bin/xsh-multicall
	ln -sf xsh-multicall ~/usr/bin/xsh
	ln -sf xsh-multicall ~/usr/bin/xshi
	ln -sf xsh-multicall ~/usr/bin/xsht

# Debug build for native musl hosts. rust-lld can't find libc and libgcc_s in
# /usr/lib by default; symlink them into the toolchain sysroot so it resolves.
# This mirrors what the Docker CI does (dist-Linux at line 152).
BUILD_SYSROOT_LIB := $(shell rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-musl/lib
build:
	ln -sf /usr/lib/libgcc_s.so.1 $(BUILD_SYSROOT_LIB)/libgcc_s.so
	ln -sf /usr/lib/libgcc_s.so.1 $(BUILD_SYSROOT_LIB)/libgcc_s.so.1
	ln -sf /usr/lib/libc.so $(BUILD_SYSROOT_LIB)/libc.so
	cargo build

# Fully-static musl dist build. Musl targets default to +crt-static, so the
# binary embeds libc with no runtime dependency on ld-musl-*.so.1.
DIST_BIN = xsh-multicall
DIST_MUSL_RUSTFLAGS = -C target-feature=+crt-static
DIST_MUSL_RUSTFLAGS += -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol
ifeq ($(DOCKER_BUILD),1)
DIST_MUSL_RUSTFLAGS += -L native=/usr/lib
DIST_RUSTFLAGS_ENV = RUSTFLAGS="-L native=/usr/lib"
else
DIST_RUSTFLAGS_ENV =
endif
# Both musl targets use rust-lld and the self-contained musl CRT from rustc.
DIST_FULL_RUSTFLAGS ?= $(RUSTFLAGS) $(DIST_RUSTFLAGS) $(DIST_TARGET_RUSTFLAGS) $(DIST_MUSL_RUSTFLAGS)
DIST_ENV =
ifeq ($(TARGET),x86_64-unknown-linux-musl)
DIST_ENV = $(DIST_RUSTFLAGS_ENV) CFLAGS_x86_64_unknown_linux_musl="$(strip $(CFLAGS_x86_64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_CFLAGS_x86_64_unknown_linux_musl="$(strip $(AWS_LC_SYS_CFLAGS_x86_64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_NO_JITTER_ENTROPY=1 CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(DIST_FULL_RUSTFLAGS))"
endif
ifeq ($(TARGET),aarch64-unknown-linux-musl)
DIST_ENV = $(DIST_RUSTFLAGS_ENV) CFLAGS_aarch64_unknown_linux_musl="$(strip $(CFLAGS_aarch64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_CFLAGS_aarch64_unknown_linux_musl="$(strip $(AWS_LC_SYS_CFLAGS_aarch64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_NO_JITTER_ENTROPY=1 CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(DIST_FULL_RUSTFLAGS))"
endif
ifeq ($(TARGET),aarch64-apple-darwin)
DIST_ENV = MACOSX_DEPLOYMENT_TARGET="$(DARWIN_DEPLOYMENT_TARGET)" CFLAGS_aarch64_apple_darwin="$(strip $(DIST_TARGET_CFLAGS))" CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS="$(strip $(DIST_DARWIN_RUSTFLAGS))"
endif
ifeq ($(TARGET),x86_64-apple-darwin)
DIST_ENV = MACOSX_DEPLOYMENT_TARGET="$(DARWIN_DEPLOYMENT_TARGET)" CFLAGS_x86_64_apple_darwin="$(strip $(DIST_TARGET_CFLAGS))" CARGO_TARGET_X86_64_APPLE_DARWIN_RUSTFLAGS="$(strip $(DIST_DARWIN_RUSTFLAGS))"
endif

dist: dist-$(shell uname -s)

dist-Darwin:
ifeq ($(filter $(TARGET),aarch64-apple-darwin x86_64-apple-darwin),)
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm \
		-v $(CURDIR):/work \
		-v $(CURDIR)/target/docker-aarch64-release:/work/target \
		-v xsh-cargo-registry:/root/.cargo/registry \
		-w /work \
		-e TARGET=$(TARGET) \
		-e CARGO_TARGET_DIR=/work/target \
		xsh-test \
		sh -c ' \
			SR=$$(rustc --target $(TARGET) --print sysroot)/lib/rustlib/$(TARGET)/lib && \
			ln -sf /usr/lib/libgcc_s.so.1 "$$SR/libgcc_s.so" && \
			ln -sf /usr/lib/libgcc_s.so.1 "$$SR/libgcc_s.so.1" && \
			ln -sf /usr/lib/libc.so "$$SR/libc.so" && \
			make dist DOCKER_BUILD=1 DIST_BUILD_STD_FLAGS= \
		'
else
	$(MAKE) dist-native
endif

dist-Linux: dist-native

dist-native:
	$(DIST_ENV) cargo build -p xsh-multicall --locked --profile $(DIST_PROFILE) $(DIST_BUILD_STD_FLAGS) --target $(TARGET) --no-default-features --features "net tools"
	ln -sf $(DIST_BIN) target/$(TARGET)/dist/xsh

dist-ci: dist
	@echo "=== verifying static linkage ==="
	@for bin in $(DIST_BIN); do \
		if [ -f "target/$(TARGET)/$(DIST_PROFILE_DIR)/$$bin" ]; then \
			printf "  %-10s " "$$bin:"; \
			readelf -d "target/$(TARGET)/$(DIST_PROFILE_DIR)/$$bin" 2>/dev/null \
			| grep -q 'NEEDED' \
			&& echo "NOT STATIC (has NEEDED entries)" \
			|| echo "static"; \
		fi; \
	done

cov: cov-$(COV_BACKEND)

cov-docker:
	docker build -t xsh-test -f Dockerfile.test .
	mkdir -p target/cov
	docker run --rm --privileged \
	    -v $(CURDIR):/work \
	    -v xsh-test-cov-target:/work/target \
	    -v $(CURDIR)/target/cov:/work/target/cov \
	    -w /work \
	    xsh-test cargo run --bin xsh -- tools/cov-linux.xsh

cov-native:
	mkdir -p target/cov
	XSH_COV_CARGO_BIN="$(COV_CARGO_BIN)" \
	    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$(COV_NATIVE_LINKER)" \
	    CC_x86_64_unknown_linux_musl="$(COV_NATIVE_LINKER)" \
	    $(COV_CARGO) run --bin xsh -- tools/cov-linux.xsh

test:
	cargo test --release -- -Zunstable-options --report-time

test-xsh-native-only:
	cargo run --release -p xsht -- test

test-linux:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm \
	    --privileged \
	    -e XSH_OS_STRESS_REPEAT=$${XSH_OS_STRESS_REPEAT:-25} \
	    -v $(CURDIR):/work \
	    -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target \
	    -v xsh-cargo-registry:/root/.cargo/registry \
	    -w /work \
	    xsh-test \
	    sh -c 'set -eu; \
	        cargo build --bin xsh --bin xsh-test-sleeper; \
	        ln -sf /work/target/debug/xsh /bin/xsh; \
	        cargo test --features linux-priv-tests; \
	        env CARGO_BIN_EXE_xsh-test-sleeper=/work/target/debug/xsh-test-sleeper target/debug/xsht test'

test-linux-ci:
	cargo test --locked --profile $(DIST_PROFILE) --features "linux-priv-tests net tools" --target $(TARGET) -- --nocapture

test-macos-ci:
	MACOSX_DEPLOYMENT_TARGET="$(DARWIN_DEPLOYMENT_TARGET)" cargo test --locked --profile $(DIST_PROFILE) --features "net tools" --target $(TARGET) -- --nocapture

LLVM_BIN := $(shell rustc --print sysroot)/lib/rustlib/$(shell rustc -vV | awk '/^host:/ {print $$2}')/bin
PGO_DIR := $(CURDIR)/target/pgo-profiles
PGO_MERGED := $(PGO_DIR)/merged.profdata
PGO_USE_RUSTFLAGS := -Cprofile-use=$(PGO_MERGED)
REGULAR_BASELINE := $(shell scripts/bench-baseline.py --variant regular --print-path)
PGO_BASELINE := $(shell scripts/bench-baseline.py --variant pgo --print-path)

bench:
	@scripts/bench-baseline.py

bench-fast:
	@scripts/bench-baseline.py --fast

bench-syscalls:
	@scripts/bench-syscalls.py

pgo-profile:
	rm -rf $(PGO_DIR)
	mkdir -p $(PGO_DIR)
	RUSTFLAGS="-Cprofile-generate=$(PGO_DIR)" cargo bench -p xsh-multicall --bench bench -- --sample-size 1
	$(LLVM_BIN)/llvm-profdata merge -o $(PGO_MERGED) $(PGO_DIR)

$(PGO_MERGED):
	$(MAKE) pgo-profile

bench-pgo: $(PGO_MERGED)
	@scripts/bench-baseline.py --variant regular --quiet
	@RUSTFLAGS="$(PGO_USE_RUSTFLAGS)" scripts/bench-baseline.py --variant pgo --quiet
	@scripts/diff-baselines.py $(REGULAR_BASELINE) $(PGO_BASELINE)

release-pgo: $(PGO_MERGED)
	RUSTFLAGS="$(PGO_USE_RUSTFLAGS)" cargo build --release -p xsh-multicall --no-default-features --features "net tools"

$(LINUX_INSTALL_CRT_DIR)/%.o: /usr/lib/%.o
	mkdir -p $(LINUX_INSTALL_CRT_DIR)
	llvm-objcopy --strip-debug $< $@
