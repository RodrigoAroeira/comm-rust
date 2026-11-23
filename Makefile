# ESP32 gateway — task runner.
#
# Device recipes cd into crates/esp32 first: Cargo discovers .cargo/config.toml
# and rust-toolchain.toml from the working directory, never from the manifest
# path. Host recipes stay at the root so the esp toolchain is not selected.

CHIP ?= s3

ifeq ($(CHIP),s3)
  TARGET        := xtensa-esp32s3-espidf
  ESPFLASH_CHIP := esp32s3
  DEFAULT_PORT  := /dev/ttyACM0
else ifeq ($(CHIP),esp32)
  TARGET        := xtensa-esp32-espidf
  ESPFLASH_CHIP := esp32
  DEFAULT_PORT  := /dev/ttyUSB0
else
  $(error CHIP must be 's3' or 'esp32' (got '$(CHIP)'))
endif

PORT   ?= $(DEFAULT_PORT)
ESP_IP ?= 192.168.4.1
PYTHON ?= python3

CRATE_DIR := crates/esp32
BIN       := target/$(TARGET)/release/esp32
CARGO     := cd $(CRATE_DIR) && cargo

.PHONY: help prepare build all run auto detect flash monitor size smoke serial hil test lint fmt fmt-check check clean distclean
.DEFAULT_GOAL := help

help: ## Show this help
	@awk 'BEGIN { FS = ":.*##"; printf "\nUsage:\n  make <target> [CHIP=s3|esp32] [PORT=/dev/ttyXXX]\n\nTargets:\n" } \
	  /^[a-z0-9-]+:.*##/ { printf "  %-12s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@printf "\n  CHIP default: s3 (%s)\n  PORT default: %s\n  ESP_IP: %s\n\n" "$(TARGET)" "$(PORT)" "$(ESP_IP)"

prepare: ## Prepare environment for development
	espup install -f export-esp.sh -t esp32,esp32s3
	@echo "\nYou may need to source export-esp.sh"

build: ## Build firmware (always release: debug is ~3.7 MB and will not fit)
	$(CARGO) build --release --target $(TARGET)

all: ## Build both chips (the second one costs 15-40 min, once)
	$(MAKE) --no-print-directory CHIP=s3 build
	$(MAKE) --no-print-directory CHIP=esp32 build

detect: ## Print the chip and port of the attached board
	@scripts/detect-chip.sh

auto: ## Detect the attached chip, then build and flash it
	@out=$$(scripts/detect-chip.sh) || exit 1; \
	  chip=$${out%% *}; port=$${out##* }; \
	  echo "detected: $$chip on $$port"; \
	  $(MAKE) --no-print-directory CHIP=$$chip PORT=$$port run

run: build ## Flash and attach the serial monitor (Ctrl-C to exit)
	espflash flash --monitor --chip $(ESPFLASH_CHIP) --port $(PORT) $(BIN)

flash: build ## Flash without attaching the monitor
	espflash flash --chip $(ESPFLASH_CHIP) --port $(PORT) $(BIN)

monitor: ## Attach to the serial console (JSON REPL; does not flash)
	espflash monitor --chip $(ESPFLASH_CHIP) --port $(PORT)

size: build ## Binary size and partition headroom
	@ls -lh $(BIN) | awk '{ printf "  image: %s\n", $$5 }'
	@espflash save-image --chip $(ESPFLASH_CHIP) --merge $(BIN) target/esp32-$(CHIP).bin 2>&1 \
	  | grep 'App/part' || true

smoke: ## One POST against a running gateway
	curl -sS -m 5 -w '\n%{http_code}  %{time_total}s\n' \
	  -X POST http://$(ESP_IP)/api/run-test \
	  -H 'Content-Type: application/json' \
	  -d '{"test_type":"esr","cap_model":"A100F","pulse_ma":10000,"pulse_ms":1425}'

serial: ## HIL test for the serial JSON REPL (close espflash monitor first)
	$(PYTHON) tests/test_serial.py --port $(PORT)

hil: ## On-hardware suite (needs tests/run_all from the port)
	@test -x tests/run_all || { echo "missing tests/run_all - HIL suite not ported yet"; exit 1; }
	ESP_IP=$(ESP_IP) tests/run_all

test: ## Host tests for the shared protocol crate
	cargo test -p protocol

lint: ## Clippy on the host-testable crate only
	cargo clippy -p protocol --all-targets

fmt: ## Format the workspace
	cargo fmt --all

fmt-check: ## Verify formatting
	cargo fmt --all -- --check

check: fmt-check lint test ## Host-side gate (format, lints, unit tests)

clean: ## Remove Rust artifacts (keeps the ESP-IDF checkout)
	cargo clean --release

distclean: ## Remove artifacts AND ESP-IDF (next build: 15-40 min)
	@printf 'Delete .embuild/ and target/? [y/N] '; read ans; \
	  case "$$ans" in [yY]*) rm -rf .embuild target; echo removed ;; *) echo aborted ;; esac
