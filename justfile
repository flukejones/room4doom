# Room4Doom recipes — `just <recipe>` (run `just` with no args to list).

default:
	@just --list

# --- one-time setup ---

# Point git at .githooks/ for pre-commit + pre-push. Run once after clone.
install-hooks:
	chmod +x .githooks/pre-commit .githooks/pre-push
	git config core.hooksPath .githooks
	@echo "git core.hooksPath -> .githooks"
	@echo "  pre-commit: fmt staged Rust (folded in) + tests; demo regression if gameplay/level/math touched"
	@echo "  pre-push:   clippy -D warnings + demo regression"

# --- run ---

# Run the game. Extra args pass through: just run -e 1 -m 5
run *ARGS:
	cargo run --release -- --iwad data/doom1.wad {{ARGS}}

# Run the BSP viewer. Extra args pass through.
bsp-viewer *ARGS:
	cargo run --release -p bsp-viewer -- {{ARGS}}

# Run the voxel viewer. Extra args pass through.
voxel-viewer *ARGS:
	cargo run --release -p voxel-viewer -- {{ARGS}}

# --- fmt / lint / test ---

# rustfmt across the workspace.
fmt:
	cargo fmt --all

# rustfmt --check only (no writes). CI gate.
check-fmt:
	cargo fmt --all -- --check

# Workspace clippy, warnings are errors.
lint:
	cargo clippy --workspace --all-targets -- -D warnings

# Compile-check the workspace.
check:
	cargo check --workspace --message-format=short

# Workspace tests.
test:
	cargo test --workspace

# Demo determinism regression (headless playback vs golden hashes).
# Always builds a fresh release binary first so the run reflects current code.
demo-check:
	cargo build --release -p room4doom
	bash tools/demo-regression.sh

# Demo regression against a fresh debug build (slower playback, dev opt-level).
demo-check-debug:
	cargo build -p room4doom
	BIN="{{justfile_directory()}}/target/debug/room4doom" bash tools/demo-regression.sh

# --- xcode / packaging ---

# Build the macOS .app bundle (Release). Pass CONFIG=Debug to override.
xcode-build CONFIG="Release":
	xcodebuild -project packaging/xcode/room4doom.xcodeproj -scheme room4doom -configuration {{CONFIG}} build

# Archive the macOS .app for distribution into build/room4doom.xcarchive.
xcode-package:
	xcodebuild -project packaging/xcode/room4doom.xcodeproj -scheme room4doom -configuration Release -archivePath build/room4doom.xcarchive archive
