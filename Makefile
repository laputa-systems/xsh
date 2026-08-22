# Compatibility facade for established local habits. Development policy lives in
# `dev/main.xsh`; use `cargo dev help` for the complete interface and options.
# A local development binary avoids Cargo startup after the first bootstrap.

XSH_DEV ?= target/debug/xsh
DEV = $(if $(wildcard $(XSH_DEV)),$(XSH_DEV) dev/main.xsh --,cargo dev)

export TARGET
export DIST_PROFILE
export DOCKER_PLATFORM
export XSH_TEST_IMAGE
export XSH_TEST_IMAGE_BUILD
export XSH_OS_STRESS_REPEAT

.PHONY: build lint install install-darwin install-linux test test-xsh-native-only test-linux test-linux-priv test-linux-ci test-macos-ci cov cov-native cov-docker bench bench-fast bench-syscalls dist dist-native dist-Linux dist-Linux-docker dist-ci

build:
	$(DEV) build

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
