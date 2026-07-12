.PHONY: lint docs test test-native cov test-core test-linux test-linux-priv test-linux-os-stress test-linux-cpumax test-trace test-trace-save test-trace-compare perf-linux perf-linux-extension-count perf-linux-flamegraph perf-linux-showcases perf-linux-showcase-flamegraphs prof prof-layout prof-parse-corpus prof-pgo prof-dhat prof-callgrind prof-cachegrind prof-valgrind prof-compare prof-baseline prof-baseline-frontend prof-baseline-runtime install-darwin install-linux dist dist-ci

DARWIN_CODESIGN_FLAGS ?=
ifneq ($(DARWIN_CODESIGN_ENTITLEMENTS),)
DARWIN_CODESIGN_FLAGS += --entitlements $(DARWIN_CODESIGN_ENTITLEMENTS)
endif
TARGET ?= x86_64-unknown-linux-musl
COV_BACKEND ?= $(shell if [ "$$(uname -s)" = Linux ] && [ "$$(uname -m)" = x86_64 ] && [ -f /etc/alpine-release ]; then echo native; else echo docker; fi)
COV_CARGO ?= $(shell command -v cargo 2>/dev/null || printf '%s/.cargo/bin/cargo\n' "$$HOME")
COV_CARGO_BIN ?= $(patsubst %/,%,$(dir $(COV_CARGO)))
COV_NATIVE_LINKER ?= $(shell command -v cc 2>/dev/null || command -v clang 2>/dev/null || command -v gcc 2>/dev/null)
PROF_BACKEND ?= $(COV_BACKEND)
PROF_CARGO ?= $(COV_CARGO)
PROF_CARGO_BIN ?= $(patsubst %/,%,$(dir $(PROF_CARGO)))
PROF_NATIVE_LINKER ?= $(COV_NATIVE_LINKER)
DIST_PROFILE ?= dist
DIST_PROFILE_DIR = $(if $(filter release,$(DIST_PROFILE)),release,$(DIST_PROFILE))
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
	cargo clippy --fix --allow-dirty --all-targets --all-features --quiet -- -D warnings
	cargo build -p xsht
	./target/debug/xsht lint --fix
	./target/debug/xsht fmt

install: install-$(shell uname -s)

install-Darwin: install-darwin

install-Linux: install-linux

install-darwin:
	RUSTFLAGS="$(strip $(DARWIN_DIST_RUSTFLAGS))" cargo build --release --no-default-features --features "native-tests net tools" --bin xsh-multicall
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
	$(LINUX_INSTALL_ENV) cargo build --release --no-default-features --features "native-tests net tools" --bin xsh-multicall
	cp ./target/release/xsh-multicall ~/usr/bin/xsh-multicall
	ln -sf xsh-multicall ~/usr/bin/xsh
	ln -sf xsh-multicall ~/usr/bin/xshi
	ln -sf xsh-multicall ~/usr/bin/xsht

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
# Profile-guided optimization is opt-in for release builds while the checked-in
# profiles are under investigation. Regenerate profiles with `make prof-pgo` and
# compare them with the methodology in PGO.md before enabling this in CI again.
# The profile is gathered on musl, so only the musl release targets may use it;
# darwin builds use DIST_DARWIN_RUSTFLAGS and skip it. When opted in,
# -pgo-warn-missing-function keeps functions absent from the profile non-fatal.
DIST_USE_PGO ?= 0
PGO_PROFILE ?= perf/pgo/xsh-$(TARGET).profdata
ifeq ($(DIST_USE_PGO),1)
ifneq ($(wildcard $(PGO_PROFILE)),)
DIST_FULL_RUSTFLAGS += -C profile-use=$(CURDIR)/$(PGO_PROFILE) -C llvm-args=-pgo-warn-missing-function
endif
endif
DIST_ENV =
ifeq ($(TARGET),x86_64-unknown-linux-musl)
DIST_ENV = $(DIST_RUSTFLAGS_ENV) CFLAGS_x86_64_unknown_linux_musl="$(strip $(CFLAGS_x86_64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_CFLAGS_x86_64_unknown_linux_musl="$(strip $(AWS_LC_SYS_CFLAGS_x86_64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_NO_JITTER_ENTROPY=1 CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(DIST_FULL_RUSTFLAGS))"
endif
ifeq ($(TARGET),aarch64-unknown-linux-musl)
DIST_ENV = $(DIST_RUSTFLAGS_ENV) CFLAGS_aarch64_unknown_linux_musl="$(strip $(CFLAGS_aarch64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_CFLAGS_aarch64_unknown_linux_musl="$(strip $(AWS_LC_SYS_CFLAGS_aarch64_unknown_linux_musl) $(DIST_TARGET_CFLAGS))" AWS_LC_SYS_NO_JITTER_ENTROPY=1 CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS="$(strip $(DIST_FULL_RUSTFLAGS))"
endif
ifeq ($(TARGET),aarch64-apple-darwin)
DIST_ENV = CFLAGS_aarch64_apple_darwin="$(strip $(DIST_TARGET_CFLAGS))" CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS="$(strip $(DIST_DARWIN_RUSTFLAGS))"
endif
ifeq ($(TARGET),x86_64-apple-darwin)
DIST_ENV = CFLAGS_x86_64_apple_darwin="$(strip $(DIST_TARGET_CFLAGS))" CARGO_TARGET_X86_64_APPLE_DARWIN_RUSTFLAGS="$(strip $(DIST_DARWIN_RUSTFLAGS))"
endif

dist: dist-$(shell uname -s)

dist-Darwin:
ifeq ($(TARGET),aarch64-apple-darwin)
	$(DIST_ENV) cargo build --target-dir $(CURDIR)/target/dist-build --locked --profile $(DIST_PROFILE) $(DIST_BUILD_STD_FLAGS) --target $(TARGET) --no-default-features --features "net tools" --bin $(DIST_BIN)
	mkdir -p target/$(TARGET)
	rm -rf target/$(TARGET)/dist target/$(TARGET)/$(DIST_PROFILE_DIR)
	ln -sfn ../dist-build/$(TARGET)/dist target/$(TARGET)/dist
	ln -sfn ../dist-build/$(TARGET)/$(DIST_PROFILE_DIR) target/$(TARGET)/$(DIST_PROFILE_DIR)
else ifeq ($(TARGET),x86_64-apple-darwin)
	$(DIST_ENV) cargo build --target-dir $(CURDIR)/target/dist-build --locked --profile $(DIST_PROFILE) $(DIST_BUILD_STD_FLAGS) --target $(TARGET) --no-default-features --features "net tools" --bin $(DIST_BIN)
	mkdir -p target/$(TARGET)
	rm -rf target/$(TARGET)/dist target/$(TARGET)/$(DIST_PROFILE_DIR)
	ln -sfn ../dist-build/$(TARGET)/dist target/$(TARGET)/dist
	ln -sfn ../dist-build/$(TARGET)/$(DIST_PROFILE_DIR) target/$(TARGET)/$(DIST_PROFILE_DIR)
else
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
endif

dist-Linux:
	$(DIST_ENV) cargo build --target-dir $(CURDIR)/target/dist-build --locked --profile $(DIST_PROFILE) $(DIST_BUILD_STD_FLAGS) --target $(TARGET) --no-default-features --features "net tools" --bin $(DIST_BIN)
	mkdir -p target/$(TARGET)
	rm -rf target/$(TARGET)/dist target/$(TARGET)/$(DIST_PROFILE_DIR)
	ln -sfn ../dist-build/$(TARGET)/dist target/$(TARGET)/dist
	ln -sfn ../dist-build/$(TARGET)/$(DIST_PROFILE_DIR) target/$(TARGET)/$(DIST_PROFILE_DIR)

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

docs:
	cargo fmt
	cargo build --bin xsh
	cargo run -p xsht --features docs-html -- docs build
	cargo run -p xsht --features docs-html -- docs check
	cargo test -p xsht --features docs-html docs
	cargo test --test runtime example_

test-native:
	cargo run -p xsht -- test

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
	cargo test
	cargo run -p xsht -- test

test-core:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "cargo build -p xsht && ln -sf /work/target/debug/xsh /bin/xsh && target/debug/xsht test --fail-fast"

test-linux:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "ln -sf /work/target/debug/xsh /bin/xsh && cargo test"

test-linux-priv:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm \
	    --cap-add SYS_ADMIN \
	    --cap-add MKNOD \
	    --cap-add NET_ADMIN \
	    --device /dev/loop-control \
	    -v $(CURDIR):/work \
	    -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target \
	    -w /work \
	    xsh-test cargo test --features linux-priv-tests --test linux_priv

test-linux-os-stress:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm \
	    -e XSH_OS_STRESS_REPEAT=$${XSH_OS_STRESS_REPEAT:-25} \
	    -v $(CURDIR):/work \
	    -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target \
	    -w /work \
	    xsh-test cargo test --test runtime os_stress -- --ignored --test-threads=1 --nocapture

test-linux-cpumax:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm --privileged \
	    -v $(CURDIR):/work \
	    -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target \
	    -w /work \
	    xsh-test sh -c 'set -eu; mnt=/tmp/xsh-cg; root=$$mnt/xsh-test-root; runner=$$root/runner; mkdir -p "$$mnt"; mount -t cgroup2 none "$$mnt"; cleanup() { set +e; echo $$$$ > "$$mnt/cgroup.procs" 2>/dev/null; rmdir "$$runner" 2>/dev/null; rmdir "$$root" 2>/dev/null; umount "$$mnt" 2>/dev/null; }; trap cleanup EXIT; mkdir "$$root" "$$runner"; echo $$$$ > "$$runner/cgroup.procs"; printf "+cpu\n" > "$$mnt/cgroup.subtree_control"; XSH_TEST_CGROUP_ROOT="$$root" XSH_TEST_CGROUP_MOUNT="$$mnt" cargo test --test runtime run_cpumax_uses_real_cgroup_v2_when_available -- --nocapture'

test-trace:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "cargo build --bin xsh && cargo build -p xsht && target/debug/xsht test --examples --nocapture --trace-top-syscalls 8"

test-trace-save:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "cargo build --bin xsh && cargo build -p xsht && target/debug/xsht test --examples --nocapture --trace-top-syscalls 12 --trace-json-out perf/baseline.json"

test-trace-compare:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "cargo build --bin xsh && cargo build -p xsht && target/debug/xsht test --examples --nocapture --trace-top-syscalls 12 --trace-json-out /tmp/current.json && target/debug/xsh perf/trace-compare.xsh -- perf/baseline.json /tmp/current.json"

perf-linux:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "cargo build --release --features perf-metrics --bin xsh && cargo build --release -p xsht"
	docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test target/release/xsh perf/run.xsh -- --scenario extension-count --scale $${XSH_PERF_SCALE:-8} --syscalls --no-build --xsh target/release/xsh --xsht target/release/xsht

perf-linux-extension-count:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "cargo build --release --features perf-metrics --bin xsh && cargo build --release -p xsht"
	docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test target/release/xsh perf/compare-extension-count.xsh -- --syscalls --no-build --xsh target/release/xsh --xsht target/release/xsht

perf-linux-flamegraph:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm --privileged --cap-add SYS_ADMIN --cap-add SYS_PTRACE --security-opt seccomp=unconfined -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "cargo build --release --features perf-metrics --bin xsh && mkdir -p target/perf && rm -f target/perf/extension-count.* && target/release/xsh perf/make-corpus.xsh -- --root /work/target/perf/corpus --scale $${XSH_PERF_SCALE:-8} && perf record -F $${XSH_PERF_FREQ:-999} -g -o target/perf/extension-count.perf.data -- target/release/xsh perf/scenarios/extension-count.xsh -- /work/target/perf/corpus && perf script --demangle -i target/perf/extension-count.perf.data > target/perf/extension-count.perf.script && target/release/xsh showcase/perf-collapse.xsh -- target/perf/extension-count.perf.script > target/perf/extension-count.folded && target/release/xsh showcase/flamegraph.xsh -- target/perf/extension-count.folded > target/perf/extension-count.svg && target/release/xsh showcase/perf-collapse.xsh -- --top 20 target/perf/extension-count.perf.script > target/perf/extension-count.top && head -20 target/perf/extension-count.top"
	docker run --rm -v $(CURDIR):/host alpine:3.21 sh -c "mkdir -p /host/target/perf && cp /host/target/aarch64-unknown-linux-musl/perf/extension-count.* /host/target/perf/"

perf-linux-showcases:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm --cap-add SYS_PTRACE --security-opt seccomp=unconfined -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "cargo build --release --features perf-metrics --bin xsh && cargo build --release -p xsht && target/release/xsh perf/showcase-tests.xsh -- --showcase $${SHOWCASE:-all} --syscalls --repeat $${XSH_PERF_REPEAT:-1} --min-duration-ms $${XSH_PERF_MIN_DURATION_MS:-0} --no-build --xsh target/release/xsh --xsht target/release/xsht"
	docker run --rm -v $(CURDIR):/host alpine:3.21 sh -c "mkdir -p /host/target/perf && cp -R /host/target/aarch64-unknown-linux-musl/perf/showcase-tests-* /host/target/perf/ 2>/dev/null || true"

perf-linux-showcase-flamegraphs:
	docker build -t xsh-test -f Dockerfile.test .
	docker run --rm --privileged --cap-add SYS_ADMIN --cap-add SYS_PTRACE --security-opt seccomp=unconfined -v $(CURDIR):/work -v $(CURDIR)/target/aarch64-unknown-linux-musl:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test sh -c "cargo build --release --features perf-metrics --bin xsh && cargo build --release -p xsht && target/release/xsh perf/showcase-tests.xsh -- --showcase $${SHOWCASE:-all} --syscalls --flamegraphs --repeat $${XSH_PERF_REPEAT:-1} --min-duration-ms $${XSH_PERF_MIN_DURATION_MS:-1000} --no-build --freq $${XSH_PERF_FREQ:-999} --xsh target/release/xsh --xsht target/release/xsht"
	docker run --rm -v $(CURDIR):/host alpine:3.21 sh -c "mkdir -p /host/target/perf && cp -R /host/target/aarch64-unknown-linux-musl/perf/showcase-tests-* /host/target/perf/ 2>/dev/null || true"

# Comprehensive profiling, all on the `profiling` profile with `net` excluded
# (tools-only), targeting only the `xsh` binary. PGO + the Valgrind big guns
# (Callgrind/Cachegrind) + in-process dhat allocs. A dedicated docker volume
# keeps these object trees separate from cov / normal builds. Override the
# workload with SCENARIO=... and XSH_PROF_SCALE=...
PROF_SCENARIO ?= extension-count
PROF_SCENARIOS ?= extension-count value-churn record-stream stream-heavy parse-check-heavy
PROF_RUN_SCENARIOS = $(if $(SCENARIO),$(SCENARIO),$(PROF_SCENARIOS))
PROF_SCALE ?= 8
PGO_SCALE ?= 64
PROF_BUILD = cargo build --profile profiling --no-default-features --features tools --bin xsh
PROF_RUN = docker run --rm --security-opt seccomp=unconfined -v $(CURDIR):/work -v xsh-test-prof-target:/work/target -v xsh-cargo-registry:/root/.cargo/registry -w /work xsh-test
PROF_COPY_OUT = docker run --rm -v $(CURDIR):/host -v xsh-test-prof-target:/target alpine:3.21 sh -c "mkdir -p /host/target/prof && cp -R /target/prof/* /host/target/prof/ 2>/dev/null || true"

# One-stop comprehensive run: core struct layout + parse/desugar/check/lower
# corpus metrics + PGO build + dhat allocs + Callgrind + Cachegrind.
prof: prof-layout prof-parse-corpus prof-pgo prof-valgrind

prof-layout:
	docker build -t xsh-test -f Dockerfile.test .
	$(PROF_RUN) sh -c 'set -eu; arch=$$(uname -m); mkdir -p target/prof; cargo run --quiet --bin xsh-layout-report > "target/prof/layout.$$arch.json"; echo "wrote target/prof/layout.$$arch.json"'
	$(PROF_COPY_OUT)

prof-parse-corpus:
	docker build -t xsh-test -f Dockerfile.test .
	$(PROF_RUN) sh -c 'set -eu; arch=$$(uname -m); mkdir -p target/prof; cargo build --profile profiling --no-default-features --features "tools perf-metrics" --bin xsh-parse-corpus-report; target/profiling/xsh-parse-corpus-report --root . --repeat "$${XSH_PARSE_CORPUS_REPEAT:-3}" > "target/prof/parse-corpus.$$arch.json"; echo "wrote target/prof/parse-corpus.$$arch.json"'
	$(PROF_COPY_OUT)

# Profile-guided optimization: instrument xsh, exercise it via the perf scenarios
# and the analysis showcases over this repo, merge, and rebuild the optimized
# binary. The merged profile is checked in at perf/pgo/xsh-$(TARGET).profdata;
# release builds consume it only with DIST_USE_PGO=1. The PGO xsh lands in
# target/pgo on the host.
prof-pgo: prof-pgo-$(PROF_BACKEND)

prof-pgo-docker:
	docker build -t xsh-test -f Dockerfile.test .
	mkdir -p target/pgo
	docker run --rm --privileged \
	    -e XSH_PROF_SCALE=$${XSH_PROF_SCALE:-$(PGO_SCALE)} \
	    -v $(CURDIR):/work \
	    -v xsh-test-prof-target:/work/target \
	    -v $(CURDIR)/target/pgo:/work/target/pgo \
	    -w /work \
	    xsh-test cargo run --bin xsh -- tools/prof-linux.xsh

prof-pgo-native:
	mkdir -p target/pgo
	XSH_PROF_SCALE=$${XSH_PROF_SCALE:-$(PGO_SCALE)} \
	    XSH_PROF_CARGO_BIN="$(PROF_CARGO_BIN)" \
	    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$(PROF_NATIVE_LINKER)" \
	    CC_x86_64_unknown_linux_musl="$(PROF_NATIVE_LINKER)" \
	    $(PROF_CARGO) run --bin xsh -- tools/prof-linux.xsh

prof-valgrind: prof-callgrind prof-cachegrind prof-dhat

# Per-call-stack allocation stats (in-process dhat, musl-native). The raw JSON
# is viewable in dhat/dh_view.html; the .summary.json is the diffable shape.
prof-dhat:
	docker build -t xsh-test -f Dockerfile.test .
	$(PROF_RUN) sh -c 'set -eu; scenarios="$(PROF_RUN_SCENARIOS)"; export XSH_DHAT_OUT=target/prof/dhat-scratch.json; cargo build --profile profiling --no-default-features --features tools --features dhat-heap --bin xsh; mkdir -p target/prof; target/profiling/xsh perf/make-corpus.xsh -- --root target/prof/corpus --scale "$${XSH_PROF_SCALE:-$(PROF_SCALE)}"; for scen in $$scenarios; do XSH_DHAT_OUT=target/prof/dhat.$$scen.json target/profiling/xsh perf/scenarios/$$scen.xsh -- target/prof/corpus; target/profiling/xsh tools/dhat-summarize.xsh -- target/prof/dhat.$$scen.json $$scen target/prof/dhat.$$scen.summary.json; done'
	$(PROF_COPY_OUT)

# Deterministic per-function instruction + call counts (Callgrind). Ir does not
# vary run-to-run, so this is the before/after regression signal.
prof-callgrind:
	docker build -t xsh-test -f Dockerfile.test .
	$(PROF_RUN) sh -c 'set -eu; scenarios="$(PROF_RUN_SCENARIOS)"; $(PROF_BUILD); mkdir -p target/prof; target/profiling/xsh perf/make-corpus.xsh -- --root target/prof/corpus --scale "$${XSH_PROF_SCALE:-$(PROF_SCALE)}"; for scen in $$scenarios; do valgrind --tool=callgrind --callgrind-out-file=target/prof/callgrind.$$scen.out --dump-instr=yes --cache-sim=no target/profiling/xsh perf/scenarios/$$scen.xsh -- target/prof/corpus; callgrind_annotate --threshold=99 target/prof/callgrind.$$scen.out > target/prof/callgrind.$$scen.txt; head -40 target/prof/callgrind.$$scen.txt; done'
	$(PROF_COPY_OUT)

# Per-function cache + branch-misprediction simulation (Cachegrind). Advisory:
# miss counts are address-sensitive, so not used to gate regressions.
prof-cachegrind:
	docker build -t xsh-test -f Dockerfile.test .
	$(PROF_RUN) sh -c 'set -eu; scenarios="$(PROF_RUN_SCENARIOS)"; $(PROF_BUILD); mkdir -p target/prof; target/profiling/xsh perf/make-corpus.xsh -- --root target/prof/corpus --scale "$${XSH_PROF_SCALE:-$(PROF_SCALE)}"; for scen in $$scenarios; do valgrind --tool=cachegrind --cachegrind-out-file=target/prof/cachegrind.$$scen.out --cache-sim=yes --branch-sim=yes target/profiling/xsh perf/scenarios/$$scen.xsh -- target/prof/corpus; cg_annotate target/prof/cachegrind.$$scen.out > target/prof/cachegrind.$$scen.txt; head -40 target/prof/cachegrind.$$scen.txt; done'
	$(PROF_COPY_OUT)

# Before/after validation across two git revisions (defaults: the btreemap/fxhash
# commit 816ea0a vs its parent). Builds each in an isolated worktree, runs
# Callgrind over a shared corpus, and diffs total instructions (deterministic).
# "after" should execute fewer instructions. For an allocation diff, run
# `make prof-dhat` at each revision and `xsh perf/dhat-compare.xsh`.
prof-compare:
	docker build -t xsh-test -f Dockerfile.test .
	$(PROF_RUN) sh -c 'set -eu; \
	  before=$${BEFORE:-5907fac}; after=$${AFTER:-816ea0a}; scen=$${SCENARIO:-$(PROF_SCENARIO)}; scale=$${XSH_PROF_SCALE:-$(PROF_SCALE)}; \
	  mkdir -p target/prof; \
	  git config --global --add safe.directory "*"; \
	  wb=/tmp/xsh-wt-before; wa=/tmp/xsh-wt-after; \
	  git worktree remove --force "$$wb" 2>/dev/null || true; \
	  git worktree remove --force "$$wa" 2>/dev/null || true; \
	  git worktree prune; \
	  git worktree add -f "$$wb" "$$before"; \
	  git worktree add -f "$$wa" "$$after"; \
	  ( cd "$$wb" && cargo build --profile profiling --no-default-features --features tools --bin xsh ); \
	  ( cd "$$wa" && cargo build --profile profiling --no-default-features --features tools --bin xsh ); \
	  xb=$$wb/target/profiling/xsh; xa=$$wa/target/profiling/xsh; \
	  "$$xa" "$$wa/perf/make-corpus.xsh" -- --root target/prof/corpus --scale "$$scale"; \
	  valgrind --tool=callgrind --callgrind-out-file=target/prof/cmp.before.out --dump-instr=yes --cache-sim=no "$$xb" "$$wb/perf/scenarios/$$scen.xsh" -- target/prof/corpus; \
	  valgrind --tool=callgrind --callgrind-out-file=target/prof/cmp.after.out --dump-instr=yes --cache-sim=no "$$xa" "$$wa/perf/scenarios/$$scen.xsh" -- target/prof/corpus; \
	  callgrind_annotate --threshold=99 target/prof/cmp.before.out > target/prof/cmp.before.txt; \
	  callgrind_annotate --threshold=99 target/prof/cmp.after.out > target/prof/cmp.after.txt; \
	  "$$xa" perf/callgrind-compare.xsh -- target/prof/cmp.before.txt target/prof/cmp.after.txt; \
	  git worktree remove --force "$$wb" || true; \
	  git worktree remove --force "$$wa" || true'
	$(PROF_COPY_OUT)

# Regenerate the checked-in, arch-specific prof reference baselines. Prefer the
# narrow target for the area you changed: front-end layout/corpus work should
# run prof-baseline-frontend, while runtime/module/evaluator work should run
# prof-baseline-runtime. Run full prof-baseline only when a change affects both.
prof-baseline: prof-baseline-frontend prof-baseline-runtime

# Front-end memory baseline: core struct layout
# (perf/layout-baseline-<arch>.json) plus parse/desugar/arena/check/lower corpus
# metrics (perf/parse-corpus-baseline-<arch>.json). This is the relevant checked
# baseline for AST/token/span/layout work.
prof-baseline-frontend:
	docker build -t xsh-test -f Dockerfile.test .
	$(PROF_RUN) sh -c 'set -eu; \
	  arch=$$(uname -m); \
	  mkdir -p target/prof; \
	  cargo run --quiet --bin xsh-layout-report > "perf/layout-baseline-$$arch.json"; \
	  cargo build --profile profiling --no-default-features --features "tools perf-metrics" --bin xsh-parse-corpus-report; \
	  target/profiling/xsh-parse-corpus-report --root . --repeat "$${XSH_PARSE_CORPUS_REPEAT:-3}" > "perf/parse-corpus-baseline-$$arch.json"; \
	  echo "wrote perf/layout-baseline-$$arch.json and perf/parse-corpus-baseline-$$arch.json"'

# Runtime allocation/instruction baseline for the default scenario: a dhat
# allocation summary (perf/dhat-baseline-<arch>.json) and a slim Callgrind
# total-instruction reference (perf/callgrind-baseline-<arch>.txt). A later run
# is compared against these with perf/dhat-compare.xsh /
# perf/callgrind-compare.xsh. Run on the arch you want a baseline for — the
# committed baselines are aarch64; x86_64 under emulation on this host is too noisy
# to be meaningful, so generate that one on native x86_64 hardware if ever needed.
prof-baseline-runtime:
	docker build -t xsh-test -f Dockerfile.test .
	$(PROF_RUN) sh -c 'set -eu; \
	  arch=$$(uname -m); scen=$(PROF_SCENARIO); scale=$${XSH_PROF_SCALE:-$(PROF_SCALE)}; \
	  mkdir -p target/prof; \
	  export XSH_DHAT_OUT=target/prof/dhat-scratch.json; \
	  cargo build --profile profiling --no-default-features --features tools --features dhat-heap --bin xsh; \
	  target/profiling/xsh perf/make-corpus.xsh -- --root target/prof/corpus --scale "$$scale"; \
	  XSH_DHAT_OUT=target/prof/dhat.$$scen.json target/profiling/xsh perf/scenarios/$$scen.xsh -- target/prof/corpus; \
	  target/profiling/xsh tools/dhat-summarize.xsh -- target/prof/dhat.$$scen.json "$$scen" "perf/dhat-baseline-$$arch.json" >/dev/null; \
	  cargo build --profile profiling --no-default-features --features tools --bin xsh; \
	  valgrind --tool=callgrind --callgrind-out-file=target/prof/callgrind.$$scen.out --dump-instr=yes --cache-sim=no target/profiling/xsh perf/scenarios/$$scen.xsh -- target/prof/corpus; \
	  { echo "# callgrind baseline: scenario=$$scen scale=$$scale arch=$$arch"; callgrind_annotate --threshold=90 target/prof/callgrind.$$scen.out | head -45; } > "perf/callgrind-baseline-$$arch.txt"; \
	  echo "wrote perf/dhat-baseline-$$arch.json and perf/callgrind-baseline-$$arch.txt"'

$(LINUX_INSTALL_CRT_DIR)/%.o: /usr/lib/%.o
	mkdir -p $(LINUX_INSTALL_CRT_DIR)
	llvm-objcopy --strip-debug $< $@
