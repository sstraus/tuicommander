# TUI Commander Makefile
# Builds, signs, and packages the Tauri app for macOS distribution

APP_NAME=TUI Commander
BINARY_NAME=tui-commander
BUNDLE_ID=com.tuic.commander
VERSION=$(shell git describe --tags --always --dirty 2>/dev/null || echo "dev")

# Code signing identity: override with SIGN_IDENTITY env var.
# Auto-detection order: Developer ID > ad-hoc.
SIGN_IDENTITY?=-

# Tauri build output
TAURI_TARGET=src-tauri/target/release/bundle/macos
APP_BUNDLE=$(TAURI_TARGET)/$(APP_NAME).app

# whisper-rs-sys bundles whisper.cpp which uses std::filesystem::path (macOS 10.15+).
# Must be exported so CMake subprocesses inherit it (.cargo/config.toml [env] alone is insufficient).
export MACOSX_DEPLOYMENT_TARGET ?= 10.15

# Distribution output
DIST_DIR=dist-release

.PHONY: all clean dev build build-dmg check sign verify-sign notarize release dist

all: build sign

# Run in development mode with hot reload
dev:
	npm run tauri dev

# Build .app only (default, fast — skips DMG)
build:
	@echo "Building TUI Commander $(VERSION)..."
	npm run tauri build

# Build .app + DMG (for distribution)
build-dmg:
	@echo "Building TUI Commander $(VERSION) with DMG..."
	npm run tauri build -- --bundles app,dmg

# Type-check, lint, and test (no Tauri build)
check:
	@echo "Running checks..."
	@npx tsc --noEmit && echo "  tsc ✓"
	@cd src-tauri && cargo clippy --release -- -D warnings && echo "  clippy ✓"
	@cd src-tauri && cargo test && echo "  cargo test ✓"
	@npx vitest run --reporter=dot 2>&1 | tail -3
	@npm audit --audit-level=high && echo "  npm audit ✓"

# Sign the built .app bundle
sign:
	@# Auto-detect best available signing certificate
	@if [ "$(SIGN_IDENTITY)" != "-" ]; then \
		SIGN_ID="$(SIGN_IDENTITY)"; \
	else \
		SIGN_ID=$$(security find-identity -v -p codesigning 2>/dev/null | grep "Developer ID Application:" | head -1 | sed 's/.*"\(.*\)".*/\1/'); \
		if [ -z "$$SIGN_ID" ]; then \
			echo "WARNING: No Developer ID found — using ad-hoc signing. Recipients will need to right-click > Open."; \
			SIGN_ID="-"; \
		fi; \
	fi; \
	echo "Signing with: $$SIGN_ID"; \
	codesign --force --deep --sign "$$SIGN_ID" \
		--identifier "$(BUNDLE_ID)" \
		--options runtime \
		"$(APP_BUNDLE)"; \
	echo "Signed: $(APP_BUNDLE)"

# Verify code signature
verify-sign:
	codesign -dvvv "$(APP_BUNDLE)"

# Notarize with Apple (requires stored credentials).
# First run: xcrun notarytool store-credentials "TUICommander" --apple-id YOUR_ID --team-id YOUR_TEAM
notarize: sign
	@echo "Creating zip for notarization..."
	@mkdir -p $(DIST_DIR)
	ditto -c -k --keepParent "$(APP_BUNDLE)" "$(DIST_DIR)/$(BINARY_NAME)-notarize.zip"
	@echo "Submitting to Apple notary service..."
	xcrun notarytool submit "$(DIST_DIR)/$(BINARY_NAME)-notarize.zip" --keychain-profile "TUICommander" --wait
	@echo "Stapling notarization ticket..."
	xcrun stapler staple "$(APP_BUNDLE)"
	@rm -f "$(DIST_DIR)/$(BINARY_NAME)-notarize.zip"
	@echo "Notarization complete."

# Build, sign, notarize, and create distributable zip
release: build-dmg sign notarize
	@echo "Creating distributable zip..."
	@mkdir -p $(DIST_DIR)
	ditto -c -k --keepParent "$(APP_BUNDLE)" "$(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"
	@echo "Release artifact: $(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"
	@ls -lh "$(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"

# Quick distribution without notarization (friends can right-click > Open)
dist: build sign
	@echo "Creating distributable zip (not notarized)..."
	@mkdir -p $(DIST_DIR)
	ditto -c -k --keepParent "$(APP_BUNDLE)" "$(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"
	@echo "Distributable: $(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"
	@echo "NOTE: Recipients must right-click > Open on first launch (not notarized)."
	@ls -lh "$(DIST_DIR)/$(BINARY_NAME)-$(VERSION).zip"

# Clean build artifacts
clean:
	rm -rf $(DIST_DIR)
	cd src-tauri && cargo clean
