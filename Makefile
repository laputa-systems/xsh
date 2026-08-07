RUSTYBENCH ?= cargo run --quiet --manifest-path ../../rustybench/Cargo.toml --

.PHONY: build lint docs test cov test-linux test-linux-ci test-macos-ci bench bench-fast bench-pgo bench-syscalls pgo-instrument pgo-profile release-pgo release-pgo-linux-docker install-darwin install-linux dist dist-native dist-Linux-docker dist-ci

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
XSH_TEST_IMAGE ?= xsh-test
XSH_TEST_IMAGE_BUILD ?= 1
DOCKER_PLATFORM ?= $(if $(filter x86_64-unknown-linux-musl,$(TARGET)),linux/amd64,linux/arm64)
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
	MACOSX_DEPLOYMENT_TARGET="$(DARWIN_DEPLOYMENT_TARGET)" RUSTFLAGS="$(strip $(DARWIN_DIST_RUSTFLAGS))" cargo build --release --bin xsh --bin xsht --bin xshi --no-default-features --features "native-tests net tools"
	for bin in xsh xsht xshi; do cp "./target/release/$$bin" "$(HOME)/usr/bin/$$bin"; codesign -fs - $(DARWIN_CODESIGN_FLAGS) "$(HOME)/usr/bin/$$bin"; xattr -d com.apple.quarantine "$(HOME)/usr/bin/$$bin" 2>/dev/null || true; done

LINUX_INSTALL_CRT_DIR = target/llvm-crt
LINUX_INSTALL_CRT_OBJS = $(LINUX_INSTALL_CRT_DIR)/Scrt1.o $(LINUX_INSTALL_CRT_DIR)/crti.o $(LINUX_INSTALL_CRT_DIR)/crtn.o
LINUX_INSTALL_RUSTFLAGS ?= -C linker=clang -C link-arg=-B$(CURDIR)/$(LINUX_INSTALL_CRT_DIR) -C link-arg=-B$(CURDIR)/tools -C link-arg=-fuse-ld=lld
LINUX_INSTALL_ENV = PATH="$$HOME/.cargo/bin:$$PATH" CC=clang AR=llvm-ar CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=clang CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(LINUX_INSTALL_RUSTFLAGS))"

install-linux: $(LINUX_INSTALL_CRT_OBJS)
	$(LINUX_INSTALL_ENV) cargo build --release --bin xsh --bin xsht --bin xshi --no-default-features --features "native-tests net tools"
	for bin in xsh xsht xshi; do cp "./target/release/$$bin" "$(HOME)/usr/bin/$$bin"; done

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
DIST_BINS = xsh xsht xshi
DIST_MUSL_RUSTFLAGS = -C target-feature=+crt-static
DIST_MUSL_RUSTFLAGS += -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol
# Both musl targets use rust-lld and the self-contained musl CRT from rustc.
DIST_FULL_RUSTFLAGS ?= $(RUSTFLAGS) $(DIST_RUSTFLAGS) $(DIST_TARGET_RUSTFLAGS) $(DIST_MUSL_RUSTFLAGS)
DIST_ENV =
ifeq ($(TARGET),x86_64-unknown-linux-musl)
DIST_ENV = CFLAGS_x86_64_unknown_linux_musl="$(strip $(CFLAGS_x86_64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_CFLAGS_x86_64_unknown_linux_musl="$(strip $(AWS_LC_SYS_CFLAGS_x86_64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_NO_JITTER_ENTROPY=1 CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(DIST_FULL_RUSTFLAGS))"
endif
ifeq ($(TARGET),aarch64-unknown-linux-musl)
DIST_ENV = CFLAGS_aarch64_unknown_linux_musl="$(strip $(CFLAGS_aarch64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_CFLAGS_aarch64_unknown_linux_musl="$(strip $(AWS_LC_SYS_CFLAGS_aarch64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_NO_JITTER_ENTROPY=1 CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(DIST_FULL_RUSTFLAGS))"
endif

# Dockerfile.test already supplies the compiler, C library, target CFLAGS, and
# aws-lc environment. Pass only the Rust flags needed by the dist build so the
# Docker path can invoke Cargo directly without replacing that environment.
DIST_DOCKER_ENV =
ifeq ($(TARGET),x86_64-unknown-linux-musl)
DIST_DOCKER_ENV = CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(DIST_FULL_RUSTFLAGS))"
endif
ifeq ($(TARGET),aarch64-unknown-linux-musl)
DIST_DOCKER_ENV = CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(DIST_FULL_RUSTFLAGS))"
endif

TEST_DOCKER_RUSTFLAGS = -C target-feature=+crt-static
TEST_DOCKER_RUSTFLAGS += -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol
TEST_DOCKER_ENV =
ifeq ($(TARGET),x86_64-unknown-linux-musl)
TEST_DOCKER_ENV = CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(TEST_DOCKER_RUSTFLAGS))"
endif
ifeq ($(TARGET),aarch64-unknown-linux-musl)
TEST_DOCKER_ENV = CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(TEST_DOCKER_RUSTFLAGS))"
endif

# Dockerfile.test provides rust-src so the dist profile's immediate-abort
# panic strategy can be applied consistently to the musl standard library.
DIST_DOCKER_BUILD_STD_FLAGS ?= -Z build-std=std

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

# Cross-build a Linux musl distribution from macOS (or another non-Linux host)
# using the CI-like toolchain in Dockerfile.test.
dist-Linux-docker:
	if [ "$(XSH_TEST_IMAGE_BUILD)" = "1" ]; then docker build --platform=$(DOCKER_PLATFORM) -t $(XSH_TEST_IMAGE) -f Dockerfile.test .; else docker image inspect $(XSH_TEST_IMAGE) >/dev/null; fi
	mkdir -p target
	docker run --rm \
		--platform=$(DOCKER_PLATFORM) \
		-v $(CURDIR):/work \
		-v $(CURDIR)/target:/work/target \
		-v xsh-cargo-registry:/root/.cargo/registry \
		-w /work \
		-e TARGET=$(TARGET) \
		-e CARGO_TARGET_DIR=/work/target \
		-e CARGO_BUILD_WARNINGS=$(CARGO_BUILD_WARNINGS) \
		-e HOST_UID=$$(id -u) \
		-e HOST_GID=$$(id -g) \
		$(XSH_TEST_IMAGE) \
		sh -c ' \
			chown_target() { chown -R "$${HOST_UID}:$${HOST_GID}" /work/target; } && \
			trap chown_target EXIT && \
			SR=$$(rustc --target $(TARGET) --print sysroot)/lib/rustlib/$(TARGET)/lib && \
			ln -sf /usr/lib/libgcc_s.so.1 "$$SR/libgcc_s.so" && \
			ln -sf /usr/lib/libgcc_s.so.1 "$$SR/libgcc_s.so.1" && \
			ln -sf /usr/lib/libc.so "$$SR/libc.so" && \
			$(DIST_DOCKER_ENV) cargo build --locked --profile $(DIST_PROFILE) $(DIST_DOCKER_BUILD_STD_FLAGS) --target $(TARGET) --no-default-features --features "net tools" --bin xsh --bin xsht --bin xshi && \
			if [ "$(DIST_PROFILE_DIR)" != dist ]; then mkdir -p target/$(TARGET)/dist && for bin in $(DIST_BINS); do cp -f target/$(TARGET)/$(DIST_PROFILE_DIR)/$$bin target/$(TARGET)/dist/$$bin; done; fi \
		'

release-pgo-linux-docker:
	if [ "$(XSH_TEST_IMAGE_BUILD)" = "1" ]; then docker build --platform=$(DOCKER_PLATFORM) -t $(XSH_TEST_IMAGE) -f Dockerfile.test .; else docker image inspect $(XSH_TEST_IMAGE) >/dev/null; fi
	mkdir -p target
	docker run --rm --privileged \
		--platform=$(DOCKER_PLATFORM) \
		-v $(CURDIR):/work \
		-v $(CURDIR)/target:/work/target \
		-v xsh-cargo-registry:/root/.cargo/registry \
		-w /work \
		-e TARGET=$(TARGET) \
		-e CARGO_TARGET_DIR=/work/target \
		-e CARGO_BUILD_WARNINGS=$(CARGO_BUILD_WARNINGS) \
		-e HOST_UID=$$(id -u) \
		-e HOST_GID=$$(id -g) \
		$(XSH_TEST_IMAGE) \
		sh -c ' \
			git config --global --add safe.directory /work && \
			chown_target() { chown -R "$${HOST_UID}:$${HOST_GID}" /work/target; } && \
			trap chown_target EXIT && \
			SR=$$(rustc --target $(TARGET) --print sysroot)/lib/rustlib/$(TARGET)/lib && \
			ln -sf /usr/lib/libgcc_s.so.1 "$$SR/libgcc_s.so" && \
			ln -sf /usr/lib/libgcc_s.so.1 "$$SR/libgcc_s.so.1" && \
			ln -sf /usr/lib/libc.so "$$SR/libc.so" && \
			$(DIST_DOCKER_ENV) make release-pgo PGO_TARGET=$(TARGET) PGO_BUILD_STD_FLAGS="$(DIST_DOCKER_BUILD_STD_FLAGS)" \
		'

dist-native:
	$(DIST_ENV) cargo build --locked --profile $(DIST_PROFILE) $(DIST_BUILD_STD_FLAGS) --target $(TARGET) --no-default-features --features "net tools" --bin xsh --bin xsht --bin xshi
	@if [ "$(DIST_PROFILE_DIR)" != dist ]; then mkdir -p target/$(TARGET)/dist && for bin in $(DIST_BINS); do cp -f target/$(TARGET)/$(DIST_PROFILE_DIR)/$$bin target/$(TARGET)/dist/$$bin; done; fi
	@for bin in $(DIST_BINS); do \
		test "$$(wc -c < target/$(TARGET)/dist/$$bin)" -ge 1024 || { echo "missing or implausibly small $$bin" >&2; exit 1; }; \
		test -x target/$(TARGET)/dist/$$bin || { echo "$$bin is not executable" >&2; exit 1; }; \
		case "$(TARGET)" in *-linux-*) magic="$$(od -An -t x1 -N 4 target/$(TARGET)/dist/$$bin | tr -d ' \n')"; test "$$magic" = 7f454c46 || { echo "$$bin is not an ELF executable" >&2; exit 1; };; esac; \
		case "$(TARGET)" in *-linux-*) case "$(TARGET)" in x86_64-*) readelf -h target/$(TARGET)/dist/$$bin | grep -F "Machine:" | grep -F "Advanced Micro Devices X86-64" >/dev/null || { echo "$$bin has the wrong target architecture" >&2; exit 1; };; aarch64-*) readelf -h target/$(TARGET)/dist/$$bin | grep -F "Machine:" | grep -F "AArch64" >/dev/null || { echo "$$bin has the wrong target architecture" >&2; exit 1; };; esac;; esac; \
	done

dist-ci: dist
	@echo "=== verifying static linkage ==="
	@for bin in $(DIST_BINS); do \
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
	        git config --global --add safe.directory /work; \
	        cargo build --bin xsh --bin xsh-test-sleeper; \
	        ln -sf /work/target/debug/xsh /bin/xsh; \
	        cargo test --features linux-priv-tests; \
	        env CARGO_BIN_EXE_xsh-test-sleeper=/work/target/debug/xsh-test-sleeper target/debug/xsht test'

test-linux-ci:
	if [ "$(XSH_TEST_IMAGE_BUILD)" = "1" ]; then docker build --platform=$(DOCKER_PLATFORM) -t $(XSH_TEST_IMAGE) -f Dockerfile.test .; else docker image inspect $(XSH_TEST_IMAGE) >/dev/null; fi
	mkdir -p target
	docker run --rm --privileged \
		--platform=$(DOCKER_PLATFORM) \
		-v $(CURDIR):/work \
		-v $(CURDIR)/target:/work/target \
		-v xsh-cargo-registry:/root/.cargo/registry \
		-w /work \
		-e TARGET=$(TARGET) \
		-e CARGO_TARGET_DIR=/work/target \
		-e CARGO_BUILD_WARNINGS=$(CARGO_BUILD_WARNINGS) \
		-e HOST_UID=$$(id -u) \
		-e HOST_GID=$$(id -g) \
		$(XSH_TEST_IMAGE) \
		sh -c ' \
			git config --global --add safe.directory /work && \
			chown_target() { chown -R "$${HOST_UID}:$${HOST_GID}" /work/target; } && \
			trap chown_target EXIT && \
			$(TEST_DOCKER_ENV) cargo test --locked --profile $(DIST_PROFILE) --features "linux-priv-tests net tools" --target $(TARGET) -- --nocapture \
		'

test-macos-ci:
	MACOSX_DEPLOYMENT_TARGET="$(DARWIN_DEPLOYMENT_TARGET)" cargo test --locked --profile $(DIST_PROFILE) --features "net tools" --target $(TARGET) -- --nocapture

LLVM_BIN := $(shell rustc --print sysroot)/lib/rustlib/$(shell rustc -vV | awk '/^host:/ {print $$2}')/bin
PGO_HOST_TARGET := $(shell rustc -vV | awk '/^host:/ {print $$2}')
PGO_TARGET ?= $(PGO_HOST_TARGET)
PGO_DIR := $(CURDIR)/target/pgo-profiles/$(PGO_TARGET)
PGO_MERGED := $(PGO_DIR)/merged.profdata
PGO_INSTRUMENT_TARGET_DIR := $(CURDIR)/target/pgo-instrument
PGO_DRIVER_TARGET_DIR := $(CURDIR)/target/pgo-driver
PGO_USE_TARGET_DIR := $(CURDIR)/target/pgo-use
PGO_INSTRUMENT_BINARY := $(PGO_INSTRUMENT_TARGET_DIR)/$(PGO_TARGET)/release/xshi
PGO_USE_BINARY := $(PGO_USE_TARGET_DIR)/$(PGO_TARGET)/release/xshi
PGO_USE_RUSTFLAGS := -Cprofile-use=$(PGO_MERGED) -Cllvm-args=-pgo-warn-missing-function
PGO_GENERATE_RUSTFLAGS := -Cprofile-generate=$(PGO_DIR)
PGO_BUILD_STD_FLAGS ?=

bench:
	@$(RUSTYBENCH) baseline --root "$(CURDIR)" --baseline "$(CURDIR)/crates/xshi/benches/baseline.json" -- cargo bench -p xshi --bench bench --features benchmark

bench-fast:
	@$(RUSTYBENCH) baseline --root "$(CURDIR)" --baseline "$(CURDIR)/crates/xshi/benches/fast-baseline.json" --fast -- cargo bench -p xshi --bench bench --features benchmark

bench-syscalls:
	@$(RUSTYBENCH) syscalls --root "$(CURDIR)"

pgo-instrument:
	rm -rf $(PGO_DIR) $(PGO_INSTRUMENT_TARGET_DIR) $(PGO_DRIVER_TARGET_DIR) $(PGO_USE_TARGET_DIR)
	mkdir -p $(PGO_DIR)
	env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo rustc --locked -p xshi --bin xshi --release $(PGO_BUILD_STD_FLAGS) --target $(PGO_TARGET) --target-dir $(PGO_INSTRUMENT_TARGET_DIR) -- $(PGO_GENERATE_RUSTFLAGS)

pgo-profile: pgo-instrument
	env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo test --locked --test integration --no-run $(PGO_BUILD_STD_FLAGS) --features "native-tests net tools" --target-dir $(PGO_DRIVER_TARGET_DIR)
	env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS LLVM_PROFILE_FILE="$(PGO_DIR)/xshi-%p.profraw" XSH_PGO_BINARY="$(PGO_INSTRUMENT_BINARY)" CARGO_TARGET_DIR="$(PGO_DRIVER_TARGET_DIR)" cargo test --locked --test integration $(PGO_BUILD_STD_FLAGS) --features "native-tests net tools" runtime::interactive::xshi_pgo_profile_workload -- --ignored --exact --test-threads=1
	@raw_profiles="$$(find "$(PGO_DIR)" -type f -name '*.profraw' -print)"; \
		test -n "$$raw_profiles"; \
		$(LLVM_BIN)/llvm-profdata merge -o "$(PGO_MERGED)" $$raw_profiles; \
		$(LLVM_BIN)/llvm-profdata show --all-functions "$(PGO_MERGED)" | grep -q 'xshi'; \
		! $(LLVM_BIN)/llvm-profdata show --all-functions "$(PGO_MERGED)" | grep -Eiq 'rustybench|xshi.*interactive.*bench|(^|[[:space:]])xsh\.|(^|[[:space:]])xsht\.'

$(PGO_MERGED):
	$(MAKE) pgo-profile

bench-pgo:
	@:

release-pgo: $(PGO_MERGED)
	env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo rustc --locked -p xshi --bin xshi --release $(PGO_BUILD_STD_FLAGS) --target $(PGO_TARGET) --target-dir $(PGO_USE_TARGET_DIR) -- $(PGO_USE_RUSTFLAGS)
	@! $(LLVM_BIN)/llvm-nm "$(PGO_USE_BINARY)" 2>/dev/null | grep -Eq '(__llvm_profile|llvm_profile)'
	@! $(LLVM_BIN)/llvm-objdump -h "$(PGO_USE_BINARY)" 2>/dev/null | grep -Eiq '(__llvm_prf|llvm_prf)'

$(LINUX_INSTALL_CRT_DIR)/%.o: /usr/lib/%.o
	mkdir -p $(LINUX_INSTALL_CRT_DIR)
	llvm-objcopy --strip-debug $< $@
