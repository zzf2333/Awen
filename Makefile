INSTALL_DIR := $(HOME)/.local/bin
CONFIG_DIR  := $(HOME)/.config/awen
DATA_DIR    := $(HOME)/.local/share/awen

.PHONY: dev release sync restart test lint clean

# Dev: debug build + sync + restart daemon (fastest iteration)
dev: sync-bin-debug sync restart
	@echo "\033[0;32m[dev]\033[0m ready — debug build"

# Release: optimized build + sync + restart daemon
release: sync-bin-release sync restart
	@echo "\033[0;32m[release]\033[0m ready — release build"

# Build debug binary and copy to install dir
sync-bin-debug:
	cargo build
	@mkdir -p $(INSTALL_DIR)
	cp target/debug/awen $(INSTALL_DIR)/awen
	@if [ "$$(uname)" = "Darwin" ]; then \
		xattr -cr $(INSTALL_DIR)/awen 2>/dev/null; \
		codesign -fs - $(INSTALL_DIR)/awen 2>/dev/null; \
	fi

# Build release binary and copy to install dir
sync-bin-release:
	cargo build --release
	@mkdir -p $(INSTALL_DIR)
	cp target/release/awen $(INSTALL_DIR)/awen
	@if [ "$$(uname)" = "Darwin" ]; then \
		xattr -cr $(INSTALL_DIR)/awen 2>/dev/null; \
		codesign -fs - $(INSTALL_DIR)/awen 2>/dev/null; \
	fi

# Sync plugin + specs only (no rebuild, for zsh-only changes)
sync:
	@mkdir -p $(CONFIG_DIR)/specs
	cp plugin/awen.zsh $(CONFIG_DIR)/awen.zsh
	cp specs/*.toml $(CONFIG_DIR)/specs/

# Restart daemon (stop if running, start fresh)
restart:
	@-$(INSTALL_DIR)/awen stop 2>/dev/null; true
	@sleep 0.2
	@$(INSTALL_DIR)/awen start &
	@sleep 0.5
	@echo "\033[0;32m[daemon]\033[0m restarted"

# Run all tests
test:
	cargo test
	shellcheck install.sh
	@if command -v zsh >/dev/null 2>&1; then \
		zsh tests/zsh_smoke_test.zsh; \
	fi

# Lint
lint:
	cargo clippy
	cargo fmt --check
	shellcheck install.sh

# Status check
status:
	$(INSTALL_DIR)/awen status

# Show daemon logs (last 30 lines)
logs:
	$(INSTALL_DIR)/awen logs -l 30

# Clean build artifacts
clean:
	cargo clean
