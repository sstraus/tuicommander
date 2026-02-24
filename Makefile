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
       nightly github-release preview

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

# --- GitHub CI workflows ---

# Push main to origin, triggering the Nightly workflow (builds tip release).
# Also force-moves the tip git tag to HEAD so the release points to the latest commit.
# Usage: make nightly
nightly:
	@BRANCH=$$(git rev-parse --abbrev-ref HEAD); \
	if [ "$$BRANCH" != "main" ]; then echo "ERROR: must be on main (currently on $$BRANCH)" && exit 1; fi; \
	if [ -n "$$(git status --porcelain)" ]; then echo "ERROR: working tree is dirty — commit or stash first" && exit 1; fi; \
	echo "==> Pushing main and updating tip tag..."; \
	git tag -f tip; \
	git push origin main; \
	git push origin tip --force; \
	echo "==> Nightly triggered. Monitor: gh run list -w Nightly --limit 1"

# Full versioned release: bump version, commit, tag, push, wait for CI, publish.
# Usage: make github-release BUMP=patch  (patch|minor|major, default: patch)
# NOTE: sed -i '' is macOS syntax — run this from macOS only.
BUMP ?= patch
github-release:
	@BRANCH=$$(git rev-parse --abbrev-ref HEAD); \
	if [ "$$BRANCH" != "main" ]; then echo "ERROR: must be on main (currently on $$BRANCH)" && exit 1; fi; \
	if [ -n "$$(git status --porcelain)" ]; then echo "ERROR: working tree is dirty — commit or stash first" && exit 1; fi; \
	CUR=$$(grep '^version' src-tauri/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'); \
	IFS='.' read -r MAJOR MINOR PATCH <<< "$$CUR"; \
	case "$(BUMP)" in \
		major) MAJOR=$$((MAJOR+1)); MINOR=0; PATCH=0;; \
		minor) MINOR=$$((MINOR+1)); PATCH=0;; \
		patch) PATCH=$$((PATCH+1));; \
		*) echo "ERROR: BUMP must be patch|minor|major (got $(BUMP))" && exit 1;; \
	esac; \
	NEW="$$MAJOR.$$MINOR.$$PATCH"; \
	TAG="v$$NEW"; \
	echo "==> Releasing $$TAG (was $$CUR)"; \
	echo "--- Bumping version to $$NEW..."; \
	sed -i '' "s/^version = \"$$CUR\"/version = \"$$NEW\"/" src-tauri/Cargo.toml; \
	sed -i '' "s/\"version\": \"$$CUR\"/\"version\": \"$$NEW\"/" src-tauri/tauri.conf.json; \
	cd src-tauri && cargo check --quiet; cd ..; \
	echo "--- Committing and tagging..."; \
	git add src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/Cargo.lock; \
	git commit -m "chore: bump version to $$TAG"; \
	git tag "$$TAG"; \
	COMMIT=$$(git rev-parse HEAD); \
	echo "--- Pushing..."; \
	git push origin main --tags; \
	echo "--- Waiting for Release workflow on $$COMMIT..."; \
	sleep 10; \
	RUN_ID=""; \
	for i in 1 2 3 4 5; do \
		RUN_ID=$$(gh run list -w Release --limit 5 --json databaseId,headSha --jq ".[] | select(.headSha == \"$$COMMIT\") | .databaseId" | head -1); \
		if [ -n "$$RUN_ID" ]; then break; fi; \
		echo "  run not found yet, retrying ($$i/5)..."; \
		sleep 5; \
	done; \
	if [ -z "$$RUN_ID" ]; then echo "ERROR: no Release workflow run found for $$COMMIT" && exit 1; fi; \
	echo "--- Watching run $$RUN_ID (Ctrl+C to detach)..."; \
	gh run watch "$$RUN_ID" --exit-status; \
	echo "--- Publishing draft release..."; \
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
