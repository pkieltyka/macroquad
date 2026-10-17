#!/usr/bin/env bash
# Run from the fork root under an isolated display: xvfb-run -a bash tests/browser_release.sh
set -euo pipefail

output=target/browser-release
mkdir -p "$output"
{
    git rev-parse HEAD
    rustc --version
    node --version
    "${CHROME_BIN:-google-chrome}" --version
    "${CHROMEDRIVER_BIN:-chromedriver}" --version
    "${FIREFOX_BIN:-firefox}" --version
    "${GECKODRIVER_BIN:-geckodriver}" --version
} > "$output/environment.txt"

export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="${CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS:-} -C link-arg=--allow-undefined"
cargo build --target wasm32-unknown-unknown --features audio \
    --example audio_lifecycle_regression --example graphics_ownership_regression \
    --example ui_material_regression --example image_codec_regression
node tools/check_browser_runtime.mjs --source-only
node tools/check_browser_runtime.mjs --wasm target/wasm32-unknown-unknown/debug/examples/audio_lifecycle_regression.wasm
for fixture in graphics_ownership_regression ui_material_regression image_codec_regression; do
    node tools/check_browser_runtime.mjs --wasm "target/wasm32-unknown-unknown/debug/examples/$fixture.wasm" --no-audio
done

openbox --sm-disable > "$output/openbox.log" 2>&1 &
browser_window_manager=$!
cleanup() {
    kill "$browser_window_manager" 2>/dev/null || true
    wait "$browser_window_manager" 2>/dev/null || true
}
trap cleanup EXIT

python3 tests/ui_material_browser.py --fixture tests/audio_lifecycle_regression.html \
    --marker 'audio lifecycle regression: passed' --runtime js/mq_js_bundle.js \
    --output "$output/audio" --unlock-audio --timeout 60
python3 tests/ui_material_browser.py --fixture tests/graphics_ownership_regression.html \
    --marker 'graphics ownership regression: passed' --runtime js/mq_js_bundle.js \
    --output "$output/ownership" --timeout 60
python3 tests/ui_material_browser.py --output "$output/ui" --timeout 60
python3 tests/ui_material_browser.py --fixture tests/image_codec_regression.html \
    --marker 'image codec regression: passed' --output "$output/codecs" --timeout 60
python3 tests/texture_cache/browser.py --wasm-dir "$output/webgl" --runtime js/mq_js_bundle.js

sha256sum js/mq_js_bundle.js \
    target/wasm32-unknown-unknown/debug/examples/audio_lifecycle_regression.wasm \
    target/wasm32-unknown-unknown/debug/examples/graphics_ownership_regression.wasm \
    target/wasm32-unknown-unknown/debug/examples/ui_material_regression.wasm \
    target/wasm32-unknown-unknown/debug/examples/image_codec_regression.wasm \
    "$output/webgl/webgl1.wasm" "$output/webgl/webgl2.wasm" > "$output/checksums.sha256"
