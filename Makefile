# Compatibility facade for established local habits. Development policy lives in
# `dev/main.xsh`; use `cargo dev help` for the complete interface and options.
# Cargo rebuilds the launcher when sources change. Set XSH_DEV explicitly to
# run a chosen prebuilt binary; an invalid override then fails visibly.

XSH_DEV ?=
DEV = $(if $(strip $(XSH_DEV)),$(XSH_DEV) dev/main.xsh --,cargo dev)

export TARGET
export DIST_PROFILE
export DOCKER_PLATFORM
export XSH_TEST_IMAGE
export XSH_TEST_IMAGE_BUILD
export XSH_OS_STRESS_REPEAT

.PHONY: build check docs docs-check fuzz lint install install-darwin install-linux test test-xsh-native-only test-linux test-linux-priv test-linux-ci test-macos-ci cov cov-native cov-docker bench bench-fast bench-syscalls dist dist-native dist-Linux dist-Linux-docker dist-ci

build:
	$(DEV) build

FUZZ_DURATION ?= 120

# Soundness fuzzing over fresh seeds; failures are grouped by the tested binary.
fuzz:
	cargo run --release -p xsh --bin xsh -- dev/fuzz.xsh -- $(FUZZ_DURATION)

check:
	$(DEV) check lint

docs:
	$(DEV) docs

docs-check:
	$(DEV) docs check

lint:
	$(DEV) lint --fix

install install-darwin install-linux:
	$(DEV) install

test:
	$(DEV) test

test-xsh-native-only:
	$(DEV) test xsh

test-linux test-linux-priv:
	$(DEV) test linux

test-linux-ci:
	$(DEV) test linux --ci

test-macos-ci:
	$(DEV) test macos --ci

cov:
	$(DEV) coverage

cov-native:
	$(DEV) coverage --backend native

cov-docker:
	$(DEV) coverage --backend docker

bench:
	$(DEV) bench

bench-fast:
	$(DEV) bench --fast

bench-syscalls:
	$(DEV) bench --syscalls

dist:
	$(DEV) dist

dist-native dist-Linux:
	$(DEV) dist --docker never

dist-Linux-docker:
	$(DEV) dist --docker always

dist-ci:
	$(DEV) dist --ci
