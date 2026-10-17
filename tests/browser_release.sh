#!/usr/bin/env bash
# Run under the isolated display documented in docs/fork.md.
set -euo pipefail

case "${1:-}" in
    "") mode=full; output=target/browser-release ;;
    --smoke) mode=smoke; output=target/browser-smoke ;;
    *) echo "Usage: bash tests/browser_release.sh [--smoke]" >&2; exit 2 ;;
esac
if (( $# > 1 )); then
    echo "Usage: bash tests/browser_release.sh [--smoke]" >&2
    exit 2
fi
for required in git rustc cargo node python3 openbox sha256sum \
    "${CHROME_BIN:-google-chrome}" "${CHROMEDRIVER_BIN:-chromedriver}" \
    "${FIREFOX_BIN:-firefox}" "${GECKODRIVER_BIN:-geckodriver}"; do
    command -v "$required" >/dev/null || { echo "Missing required tool: $required" >&2; exit 1; }
done
mkdir -p "$output"
{
    echo "Browser gate: $mode"
    git rev-parse HEAD
    rustc --version
    node --version
    "${CHROME_BIN:-google-chrome}" --version
    "${CHROMEDRIVER_BIN:-chromedriver}" --version
    "${FIREFOX_BIN:-firefox}" --version
    "${GECKODRIVER_BIN:-geckodriver}" --version
} > "$output/environment.txt"

export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="${CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS:-} -C link-arg=--allow-undefined"
# Clean clones omit the ignored lockfile. Preserve an existing developer lock.
if [[ ! -f Cargo.lock ]]; then
    cargo generate-lockfile
fi
node tools/check_browser_runtime.mjs --source-only
if [[ "$mode" == full ]]; then
    cargo build --target wasm32-unknown-unknown --features audio \
        --example audio_lifecycle_regression --example graphics_ownership_regression \
        --example ui_material_regression --example image_codec_regression
    node tools/check_browser_runtime.mjs --wasm target/wasm32-unknown-unknown/debug/examples/audio_lifecycle_regression.wasm
    for fixture in graphics_ownership_regression ui_material_regression image_codec_regression; do
        node tools/check_browser_runtime.mjs --wasm "target/wasm32-unknown-unknown/debug/examples/$fixture.wasm" --no-audio
    done
fi

openbox --sm-disable > "$output/openbox.log" 2>&1 &
browser_window_manager=$!
cleanup() {
    kill "$browser_window_manager" 2>/dev/null || true
    wait "$browser_window_manager" 2>/dev/null || true
}
trap cleanup EXIT

if [[ "$mode" == full ]]; then
    python3 tests/ui_material_browser.py --fixture tests/audio_lifecycle_regression.html \
        --marker 'audio lifecycle regression: passed' --runtime js/mq_js_bundle.js \
        --output "$output/audio" --unlock-audio --timeout 60
    python3 tests/ui_material_browser.py --fixture tests/graphics_ownership_regression.html \
        --marker 'graphics ownership regression: passed' --runtime js/mq_js_bundle.js \
        --output "$output/ownership" --timeout 60
    python3 tests/ui_material_browser.py --output "$output/ui" --timeout 60
    python3 tests/ui_material_browser.py --fixture tests/image_codec_regression.html \
        --marker 'image codec regression: passed' --output "$output/codecs" --timeout 60
fi
if [[ "$mode" == full ]]; then
    python3 tests/browser_capacity/browser.py --attributes --output "$output/capacity"
else
    python3 tests/browser_capacity/browser.py --output "$output/capacity"
fi
python3 tests/texture_cache/browser.py --wasm-dir "$output/webgl" --runtime js/mq_js_bundle.js --selection
# The browser checks actual imports, including its genuine glGetError adapter.
if [[ "$mode" == full ]]; then
    python3 tests/browser_capacity/ui_browser.py --output "$output/ui-capacity"
    python3 tests/browser_input/browser.py --output "$output/input"
    python3 tests/browser_scene/browser.py --output "$output/scene"
fi

sha256sum js/mq_js_bundle.js "$output/webgl/webgl1.wasm" "$output/webgl/webgl2.wasm" \
    "$output/webgl/auto.wasm" > "$output/checksums.sha256"
if [[ "$mode" == full ]]; then
    sha256sum target/wasm32-unknown-unknown/debug/examples/audio_lifecycle_regression.wasm \
        target/wasm32-unknown-unknown/debug/examples/graphics_ownership_regression.wasm \
        target/wasm32-unknown-unknown/debug/examples/ui_material_regression.wasm \
        target/wasm32-unknown-unknown/debug/examples/image_codec_regression.wasm \
        "$output/scene/webgl1.wasm" "$output/scene/webgl2.wasm" >> "$output/checksums.sha256"
fi
