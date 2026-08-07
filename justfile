# Open SSOL justfile — common developer commands

# Default recipe: list available commands
default:
    @just --list

# Shared runner for UI screenshot capture.
# When DISPLAY is unset, uses Xvfb + Mesa lavapipe (software Vulkan) so Bevy
# can create a window surface without DRI3 (hardware Vulkan often fails under Xvfb).
[private]
_run-ui-screenshots out features="" *args:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{out}}"
    cargo_args=(run --release)
    if [[ -n "{{features}}" ]]; then
      cargo_args+=(--features "{{features}}")
    fi
    cargo_args+=(-- --ui-screenshots "{{out}}" --no-audio {{args}})
    if [[ -z "${DISPLAY:-}" ]] && command -v xvfb-run >/dev/null 2>&1; then
      lvp_icd="$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -n1 || true)"
      if [[ -z "${lvp_icd}" ]]; then
        echo "error: no lavapipe Vulkan ICD found under /usr/share/vulkan/icd.d/"
        echo "       install mesa-vulkan-drivers (provides lvp_icd.json) for headless capture"
        exit 1
      fi
      echo "No DISPLAY; capturing via xvfb-run + lavapipe (${lvp_icd})..."
      xvfb-run -a env VK_ICD_FILENAMES="${lvp_icd}" cargo "${cargo_args[@]}"
    else
      cargo "${cargo_args[@]}"
    fi
    echo ""
    echo "UI screenshots written to {{out}}:"
    ls -la "{{out}}"

# Generate PNG screenshots of every in-game Bevy UI screen/state.
# Output defaults to screenshots/ui/ (also writes INDEX.md).
ui-screenshots out="screenshots/ui" *args:
    just _run-ui-screenshots "{{out}}" "" {{args}}

# Same as ui-screenshots, but with the AI debug UI feature enabled.
ui-screenshots-ai out="screenshots/ui-ai" *args:
    just _run-ui-screenshots "{{out}}" "ai" {{args}}

# Unit tests for screenshot harness helpers (no GPU / window required).
test-ui-screenshots:
    cargo test screenshot_harness -- --nocapture

# Run the full test suite
test:
    cargo test

# Phase 0 train harness unit tests (route + scripted teacher)
test-train:
    cargo test --bin ssol_simulator train::

# Headless scripted baseline smoke (no --features ai). route: wr|greedy|mix|…
baseline-smoke n="3" secs="60" speed="100" route="mix" seed="0":
    cargo run --release -- --headless --no-audio --speed {{speed}} \
      --scripted-baseline --num-orbs {{n}} --act-hz 10 --max-episode-secs {{secs}} \
      --route-mode {{route}} --seed {{seed}}

# Multi-seed baseline matrix: modes × orbs × seeds → JSONL + summary.
# Defaults keep wall time reasonable (60s sim @ speed 200, 3 seeds).
# Example: just baseline-matrix
#          just baseline-matrix out=docs/baseline_matrix.jsonl secs=60 speed=200
baseline-matrix out="docs/baseline_matrix.jsonl" secs="60" speed="200" modes="wr greedy" orbs="1 3 7" seeds="0 1 2":
    #!/usr/bin/env bash
    set -euo pipefail
    OUT="{{out}}" SECS="{{secs}}" SPEED="{{speed}}" \
      MODES="{{modes}}" ORBS="{{orbs}}" SEEDS="{{seeds}}" \
      bash scripts/train_baseline_matrix.sh --out "{{out}}"
