# TUICommander Makefile
# Builds, signs, and packages the Tauri app for macOS distribution

APP_NAME=TUICommander
BINARY_NAME=tuicommander
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

.PHONY: all clean dev test build build-dmg check sign verify-sign notarize release dist \
       build-github-release publish-github-release github-release preview

all: build sign

# Run in development mode with hot reload
dev:
	npm run tauri dev

# Build frontend + launch Tauri dev (for quick manual testing)
test:
	@echo "Building Vite frontend..."
	@npx vite build
	@echo "Starting Tauri dev..."
	npm run tauri dev

# Build .app only (default, fast — skips DMG)
build:
	@echo "Building TUICommander $(VERSION)..."
	npm run tauri build

# Build .app + DMG (for distribution)
build-dmg:
	@echo "Building TUICommander $(VERSION) with DMG..."
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

# --- GitHub Release workflow ---

# Trigger a CI build by pushing current branch + tag.
# Deletes and re-creates the tag so it points to HEAD, then pushes both.
build-github-release:
	@TAG=$$(git describe --tags --abbrev=0 2>/dev/null); \
	if [ -z "$$TAG" ]; then echo "ERROR: no tag found. Create one first: git tag vX.Y.Z" && exit 1; fi; \
	BRANCH=$$(git rev-parse --abbrev-ref HEAD); \
	echo "Triggering CI build for $$TAG on $$BRANCH..."; \
	git tag -d "$$TAG" && git tag "$$TAG"; \
	git push origin "$$BRANCH"; \
	git push origin :"refs/tags/$$TAG" 2>/dev/null || true; \
	git push origin "$$TAG"; \
	echo "Pushed. Monitor at: gh run list --limit 1"

# Publish a draft GitHub release (makes it visible to everyone).
publish-github-release:
	@TAG=$$(git describe --tags --abbrev=0 2>/dev/null); \
	if [ -z "$$TAG" ]; then echo "ERROR: no tag found." && exit 1; fi; \
	echo "Publishing draft release $$TAG..."; \
	gh release edit "$$TAG" --draft=false; \
	echo "Release $$TAG published: $$(gh release view $$TAG --json url --jq .url)"

# Full GitHub release: tag from package.json, push, wait for CI, publish.
# Usage: make github-release
github-release:
	@VER=$$(node -p "require('./package.json').version"); \
	TAG="v$$VER"; \
	BRANCH=$$(git rev-parse --abbrev-ref HEAD); \
	echo "==> Releasing $$TAG from $$BRANCH"; \
	echo "--- Tagging and pushing..."; \
	git tag -d "$$TAG" 2>/dev/null || true; \
	git tag "$$TAG"; \
	git push origin "$$BRANCH"; \
	git push origin :"refs/tags/$$TAG" 2>/dev/null || true; \
	git push origin "$$TAG"; \
	echo "--- Waiting for CI build..."; \
	sleep 5; \
	RUN_ID=$$(gh run list --branch "$$TAG" --limit 1 --json databaseId --jq '.[0].databaseId'); \
	if [ -z "$$RUN_ID" ]; then echo "ERROR: no CI run found" && exit 1; fi; \
	echo "--- Watching run $$RUN_ID (Ctrl+C to detach)..."; \
	gh run watch "$$RUN_ID" --exit-status; \
	echo "--- Publishing release $$TAG..."; \
	gh release edit "$$TAG" --draft=false; \
	echo "==> Released: $$(gh release view $$TAG --json url --jq .url)"

# Build a preview release with a different app name to avoid conflicts with the real app.
# The resulting .app is named "TUIC-preview" with a separate bundle ID, so macOS and
# tests won't confuse it with the production TUICommander.
preview:
	@echo "Building TUIC-preview $(VERSION)..."
	npm run tauri build -- --bundles app --config '{"productName":"TUIC-preview","identifier":"com.tuic.preview","bundle":{"createUpdaterArtifacts":false},"app":{"windows":[{"title":"TUIC-preview","width":1200,"height":800,"minWidth":800,"minHeight":600,"decorations":true,"transparent":false,"resizable":true,"fullscreen":false,"hiddenTitle":true,"titleBarStyle":"Overlay","trafficLightPosition":{"x":13,"y":20},"backgroundColor":"#000000","dragDropEnabled":false}]}}'
	@echo "Launching TUIC-preview..."
	open "$(TAURI_TARGET)/TUIC-preview.app"

# Clean build artifacts
clean:
	rm -rf $(DIST_DIR)
	cd src-tauri && cargo clean
