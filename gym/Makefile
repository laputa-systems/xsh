DOCKER ?= docker
PLATFORM ?= linux/arm64

GYM_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
BUILD_CONTEXT ?= $(GYM_DIR)
PROJECT_DIR := $(abspath $(GYM_DIR)/..)

# The Docker platform and local Rust target must describe the same Linux
# architecture. Override XSH_TARGET explicitly when using another platform.
XSH_TARGET ?= $(if $(filter linux/amd64,$(PLATFORM)),x86_64-unknown-linux-musl,aarch64-unknown-linux-musl)
XSH_DIST_TARGET ?= $(if $(filter Darwin,$(shell uname -s)),dist-Linux-docker,dist-Linux)
XSH_CARGO_TARGET_DIR ?= $(if $(filter dist-Linux-docker,$(XSH_DIST_TARGET)),$(PROJECT_DIR)/target/docker-$(XSH_TARGET)-release,$(PROJECT_DIR)/target)
XSH_DIST_DIR ?= $(XSH_CARGO_TARGET_DIR)/$(XSH_TARGET)/dist
XSH_STAGING_DIR ?= $(GYM_DIR)/.dist

BASE_IMAGE ?= xsh-gym:latest
TASK_IMAGE ?= xsh-gym-task-ecount:latest
SESSION_VOLUME ?= xsh-gym-task-ecount-session
WORK_DIR ?= $(GYM_DIR)/.work/task-ecount
OUTPUT_DIR ?= $(GYM_DIR)/.out/task-ecount

# Pi is still fetched from the pinned image URL and baked into the base image.
# XSH and XSHT are built locally by the xsh-dist prerequisite below.
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

TASK_TAGS_WORK_DIR ?= $(GYM_DIR)/.work/task-tags
TASK_TAGS_OUTPUT_DIR ?= $(GYM_DIR)/.out/task-tags
TASK_TAGS_SESSION_VOLUME ?= xsh-gym-task-tags-session

OUTER_DIR ?=
OUTER_ITERATIONS ?= 2
OUTER_PI_COMMAND ?= pi
OUTER_PI_PROVIDER ?= $(PI_PROVIDER)
OUTER_PI_MODEL ?= $(PI_MODEL)
OUTER_PI_THINKING ?= $(PI_THINKING)
OUTER_PI_OFFLINE ?= 0

.PHONY: task-ecount task-hello task-tags outer-loop base-image task-ecount-image xsh-dist

xsh-dist:
	$(MAKE) -C $(PROJECT_DIR) $(XSH_DIST_TARGET) TARGET=$(XSH_TARGET)
	mkdir -p $(XSH_STAGING_DIR)
	cp -fL $(XSH_DIST_DIR)/xsh $(XSH_STAGING_DIR)/xsh
	cp -fL $(XSH_DIST_DIR)/xsht $(XSH_STAGING_DIR)/xsht

base-image: xsh-dist
	$(DOCKER) build --platform $(PLATFORM) -f $(GYM_DIR)/Dockerfile -t $(BASE_IMAGE) $(BUILD_CONTEXT)

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
		xsh "$(GYM_DIR)/run-task-hello.xsh"

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
		xsh "$(GYM_DIR)/run-task-ecount.xsh"

task-tags: base-image
	@DOCKER="$(DOCKER)" \
		PLATFORM="$(PLATFORM)" \
		GYM_DIR="$(GYM_DIR)" \
		BASE_IMAGE="$(BASE_IMAGE)" \
		SESSION_VOLUME="$(TASK_TAGS_SESSION_VOLUME)" \
		WORK_DIR="$(TASK_TAGS_WORK_DIR)" \
		OUTPUT_DIR="$(TASK_TAGS_OUTPUT_DIR)" \
		PI_COMMAND="$(PI_COMMAND)" \
		PI_PROVIDER="$(PI_PROVIDER)" \
		PI_MODEL="$(PI_MODEL)" \
		PI_THINKING="$(PI_THINKING)" \
		PI_TELEMETRY="$(PI_TELEMETRY)" \
		PI_OFFLINE="$(PI_OFFLINE)" \
		PI_AUTH_FILE="$(PI_AUTH_FILE)" \
		PI_AGENT_DIR="$(PI_AGENT_DIR)" \
		xsh "$(GYM_DIR)/run-task-tags.xsh"

outer-loop: base-image
	@DOCKER="$(DOCKER)" \
		PLATFORM="$(PLATFORM)" \
		GYM_DIR="$(GYM_DIR)" \
		BASE_IMAGE="$(BASE_IMAGE)" \
		PI_COMMAND="$(PI_COMMAND)" \
		PI_PROVIDER="$(PI_PROVIDER)" \
		PI_MODEL="$(PI_MODEL)" \
		PI_THINKING="$(PI_THINKING)" \
		PI_TELEMETRY="$(PI_TELEMETRY)" \
		PI_OFFLINE="$(PI_OFFLINE)" \
		PI_AUTH_FILE="$(PI_AUTH_FILE)" \
		PI_AGENT_DIR="$(PI_AGENT_DIR)" \
		OUTER_DIR="$(OUTER_DIR)" \
		OUTER_ITERATIONS="$(OUTER_ITERATIONS)" \
		OUTER_PI_COMMAND="$(OUTER_PI_COMMAND)" \
		OUTER_PI_PROVIDER="$(OUTER_PI_PROVIDER)" \
		OUTER_PI_MODEL="$(OUTER_PI_MODEL)" \
		OUTER_PI_THINKING="$(OUTER_PI_THINKING)" \
		OUTER_PI_OFFLINE="$(OUTER_PI_OFFLINE)" \
		xsh "$(GYM_DIR)/run-outer-loop.xsh"
