SHELL := /bin/sh
.SHELLFLAGS := -eu -c
.DEFAULT_GOAL := build
.NOTPARALLEL:

CARGO ?= cargo
RUSTFMT ?= rustfmt
NODE ?= node
PYTHON ?= python3
XVFB_RUN ?= xvfb-run
BROWSER_DISPLAY = $(XVFB_RUN) -a -s "-screen 0 1280x960x24 -dpi 96"

.PHONY: build test-ci test release-check check-core check-runtime check-vendor check-format check-shared browser-smoke browser-full

# Check both supported WASM feature configurations; browser targets link fixtures.
build:
	$(CARGO) check --all-targets --target wasm32-unknown-unknown --no-default-features
	$(CARGO) check --all-targets --target wasm32-unknown-unknown --no-default-features --features audio

check-runtime:
	$(NODE) tools/regenerate_browser_runtime.mjs --check
	$(NODE) --test tests/browser_runtime.test.mjs tests/audio_unlock_gate.test.mjs tests/webgl_selection.test.mjs tests/browser_fixture.test.mjs tests/browser_scene/oracle.test.mjs tests/browser_scene/submission.test.mjs
	$(PYTHON) tests/test_browser_audio_gate.py
	$(PYTHON) tests/test_browser_process.py
	$(PYTHON) tests/test_browser_input.py
	$(PYTHON) tests/browser_scene/test_protocol.py
	$(PYTHON) tests/test_prepare_release.py

check-vendor:
	$(PYTHON) tools/vendor_miniquad.py

check-format:
	for manifest in Cargo.toml macroquad_macro/Cargo.toml physics-platformer/Cargo.toml profiler/Cargo.toml tiled/Cargo.toml tests/texture_cache/Cargo.toml tests/browser_scene/Cargo.toml tests/browser_capacity/Cargo.toml tests/browser_input/Cargo.toml; do \
		$(CARGO) fmt --manifest-path "$$manifest" -- --check; \
	done
	$(RUSTFMT) --edition 2018 --check vendor/miniquad/src/conf.rs vendor/miniquad/src/native/wasm.rs vendor/miniquad/src/graphics/gl.rs vendor/miniquad/src/graphics/gl/cache.rs vendor/miniquad/src/native/linux_x11/clipboard.rs vendor/miniquad/src/native/linux_x11/drag_n_drop.rs

check-core: build check-runtime check-vendor

browser-smoke:
	env LIBGL_ALWAYS_SOFTWARE=1 $(BROWSER_DISPLAY) bash tests/browser_release.sh --smoke

browser-full:
	env LIBGL_ALWAYS_SOFTWARE=1 $(BROWSER_DISPLAY) bash tests/browser_release.sh

# CI coverage for browser configurations; native/MSRV/Miri jobs remain separate.
test-ci: check-format check-core browser-smoke

# Full strict local browser acceptance, including actual audio resumption.
test: check-core browser-full

check-shared:
	$(CARGO) check --all-targets --no-default-features
	$(CARGO) check --all-targets --features audio
	$(XVFB_RUN) -a $(CARGO) test --tests --no-default-features
	$(XVFB_RUN) -a $(CARGO) test --lib quad_gl::capacity_tests::capacity_regressions --no-default-features -- --exact --ignored
	$(XVFB_RUN) -a $(CARGO) test --lib ui::ui_context::tests::ui_material_is_lazy_and_reused_after_empty_frames --no-default-features -- --exact --ignored
	$(XVFB_RUN) -a env CARGO_TARGET_DIR=target/texture-cache-native $(CARGO) run --manifest-path tests/texture_cache/Cargo.toml
	$(XVFB_RUN) -a $(CARGO) run --example image_transfer_regression --no-default-features
	$(XVFB_RUN) -a $(CARGO) run --example graphics_ownership_regression --no-default-features
	$(XVFB_RUN) -a $(CARGO) test --doc --no-default-features
	CARGO_TARGET_DIR=target/miniquad-tests $(CARGO) test --manifest-path vendor/miniquad/Cargo.toml --locked --lib
	CARGO_TARGET_DIR=target/miniquad-tests $(XVFB_RUN) -a $(CARGO) test --manifest-path vendor/miniquad/Cargo.toml --locked --lib live_x11_ -- --ignored

# Local pre-release gate. Exact-commit public CI and asset checks are also required.
release-check: check-format check-core check-shared browser-full
