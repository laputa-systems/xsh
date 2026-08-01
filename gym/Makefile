DOCKER ?= docker
PLATFORM ?= linux/arm64

GYM_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
BUILD_CONTEXT ?= $(GYM_DIR)

BASE_IMAGE ?= xsh-gym:latest
TASK_IMAGE ?= xsh-gym-task-ecount:latest
SESSION_VOLUME ?= xsh-gym-task-ecount-session
WORK_DIR ?= $(GYM_DIR)/.work/task-ecount
OUTPUT_DIR ?= $(GYM_DIR)/.out/task-ecount

# The prerelease pi binary is baked into the base image as pi-bun (see
# Dockerfile); build it with `make bun-linux-arm64-musl` in ~/d/pi. A glibc pi
# binary cannot run inside the Alpine container; a host macOS Node entrypoint
# is not usable either. PI_BINARY remains an optional runtime override mount.
PI_SOURCE ?= $(HOME)/d/pi/packages/coding-agent/binaries/linux-arm64-musl/pi-bun
PI_BINARY ?=
PI_COMMAND ?= pi
PI_PROVIDER ?= openrouter
PI_MODEL ?= deepseek/deepseek-v4-flash-0731
PI_THINKING ?= high
PI_TELEMETRY ?= 0
# Disable Pi startup network operations; the model request still needs access.
PI_OFFLINE ?= 1
PI_AUTH_FILE ?= $(HOME)/.pi/agent/auth.json
PI_AGENT_DIR ?= /run/pi-agent

TASK_HELLO_SESSION_VOLUME ?= xsh-gym-task-hello-session
TASK_HELLO_WORK_DIR ?= $(GYM_DIR)/.work/task-hello
TASK_HELLO_OUTPUT_DIR ?= $(GYM_DIR)/.out/task-hello

.PHONY: task-ecount task-hello base-image task-ecount-image

base-image: $(GYM_DIR)/pi-bun
	$(DOCKER) build --platform $(PLATFORM) -f $(GYM_DIR)/Dockerfile -t $(BASE_IMAGE) $(BUILD_CONTEXT)

$(GYM_DIR)/pi-bun: $(PI_SOURCE)
	cp $(PI_SOURCE) $(GYM_DIR)/pi-bun

task-ecount-image: base-image
	$(DOCKER) build --platform $(PLATFORM) \
		--build-arg BASE_IMAGE=$(BASE_IMAGE) \
		-f $(GYM_DIR)/Dockerfile.task-ecount \
		-t $(TASK_IMAGE) \
		$(BUILD_CONTEXT)

task-hello: base-image
	@DOCKER="$(DOCKER)" \
		PLATFORM="$(PLATFORM)" \
		GYM_DIR="$(GYM_DIR)" \
		BASE_IMAGE="$(BASE_IMAGE)" \
		SESSION_VOLUME="$(TASK_HELLO_SESSION_VOLUME)" \
		WORK_DIR="$(TASK_HELLO_WORK_DIR)" \
		OUTPUT_DIR="$(TASK_HELLO_OUTPUT_DIR)" \
		PI_COMMAND="$(PI_COMMAND)" \
		PI_PROVIDER="$(PI_PROVIDER)" \
		PI_MODEL="$(PI_MODEL)" \
		PI_THINKING="$(PI_THINKING)" \
		PI_TELEMETRY="$(PI_TELEMETRY)" \
		PI_OFFLINE="$(PI_OFFLINE)" \
		PI_AUTH_FILE="$(PI_AUTH_FILE)" \
		PI_AGENT_DIR="$(PI_AGENT_DIR)" \
		sh "$(GYM_DIR)/run-task-hello.sh"

task-ecount: task-ecount-image
	@DOCKER="$(DOCKER)" \
		PLATFORM="$(PLATFORM)" \
		GYM_DIR="$(GYM_DIR)" \
		BASE_IMAGE="$(BASE_IMAGE)" \
		TASK_IMAGE="$(TASK_IMAGE)" \
		SESSION_VOLUME="$(SESSION_VOLUME)" \
		WORK_DIR="$(WORK_DIR)" \
		OUTPUT_DIR="$(OUTPUT_DIR)" \
		PI_BINARY="$(PI_BINARY)" \
		PI_COMMAND="$(PI_COMMAND)" \
		PI_PROVIDER="$(PI_PROVIDER)" \
		PI_MODEL="$(PI_MODEL)" \
		PI_THINKING="$(PI_THINKING)" \
		PI_TELEMETRY="$(PI_TELEMETRY)" \
		PI_OFFLINE="$(PI_OFFLINE)" \
		PI_AUTH_FILE="$(PI_AUTH_FILE)" \
		PI_AGENT_DIR="$(PI_AGENT_DIR)" \
		sh "$(GYM_DIR)/run-task-ecount.sh"
