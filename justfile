# Room4Doom recipes — `just <recipe>` (run `just` with no args to list).

default:
	@just --list

# --- one-time setup ---

# Point git at .githooks/ for pre-commit + pre-push. Run once after clone.
install-hooks:
	chmod +x .githooks/pre-commit .githooks/pre-push
	git config core.hooksPath .githooks
	@echo "git core.hooksPath -> .githooks"
	@echo "  pre-commit: fmt staged Rust + .slint (folded in) + tests; demo regression if gameplay/level/math touched"
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

# Run the map editor. Extra args pass through: just editor --map E1M1
editor *ARGS:
	cargo run --release -p editor -- --iwad data/doom1.wad {{ARGS}}

# --- fmt / lint / test ---

# rustfmt across the workspace.
fmt:
	cargo fmt --all

# slint-lsp format all .slint files in place.
fmt-slint:
	find tools/editor/ui -name '*.slint' | xargs slint-lsp format -i

# rustfmt --check only (no writes). CI gate.
check-fmt:
	cargo fmt --all -- --check

# Workspace clippy + editor pattern lint, warnings are errors.
lint: lint-editor-patterns
	cargo clippy --workspace --all-targets -- -D warnings

# Grep-based lint for editor anti-patterns the compiler can't catch (the rules
# in CODE_REVIEW.md). Fails when a pattern reappears outside its allowed home.
lint-editor-patterns:
	#!/usr/bin/env sh
	set -e
	fail=0

	# $1 ripgrep pattern, $2 file glob, $3 exclude regex (or '$^' for none),
	# $4 message. Fires when the pattern survives the exclude.
	check() {
		hits=$(rg -n "$1" tools/editor -g "$2" | grep -vE "$3" || true)
		if [ -n "$hits" ]; then
			echo "✗ $4"
			echo "$hits"
			fail=1
		fi
	}

	# Hardcoded colours in .slint → use a Theme token. theme.slint defines the
	# tokens; globals.slint holds ThemeController's pre-push property defaults.
	check 'color:\s*#[0-9a-fA-F]|background:\s*#[0-9a-fA-F]' '*.slint' \
		'theme\.slint|globals\.slint' \
		'Hardcoded colour in .slint — bind to a Theme.* token (chrome) or render/style.rs (canvas)'

	# Silent Sender::send — drops the worker result with no trace.
	check 'let _ = [^;]*\.send\(' '*.rs' '\$^' \
		'Silent channel send — handle the Err (log::warn!), not let _ = .send()'

	# slint::Timer in a static/thread_local (matches the `static NAME: Timer` line
	# in both the bare-static and `thread_local! { static NAME: Timer … }` forms;
	# a non-Timer scratch thread_local does not match). Documented exceptions (all
	# armed while a SharedState borrow is live, so the timer must NOT sit in that
	# RefCell): the hover timer, the light/camera/build-animation timers, and the
	# macOS menu timers (app-lifetime native-menu plumbing built before
	# SharedState exists). See CODE_REVIEW.md.
	check 'static [A-Z_]+: *Timer' '*.rs' \
		'level_editor/preview\.rs|macos/menu\.rs|render/sync\.rs|views/view_canvas\.rs|bsp_anim\.rs' \
		'slint::Timer in static/thread_local — keep it in SharedState (see CODE_REVIEW.md Timer exception)'

	# Inline multi-segment crate:: path outside a use statement. Excludes `use`
	# lines (any visibility: pub, pub(crate), pub(super)) and doc-comment
	# intra-doc links (`///`, `//!`), where a multi-segment path is the idiomatic
	# link form, not a code-site qualifier.
	check 'crate::[a-z_]+::[A-Za-z]' '*.rs' ':[0-9]+:[[:space:]]*(pub(\((crate|super)\))? )?use |:[0-9]+:[[:space:]]*//[/!]' \
		'Inline multi-segment crate:: path — hoist a use import (single-segment crate::/super:: is fine)'

	[ "$fail" -eq 0 ] && echo "lint-editor-patterns ok" || { echo "lint-editor-patterns failed (see CODE_REVIEW.md)"; exit 1; }

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
