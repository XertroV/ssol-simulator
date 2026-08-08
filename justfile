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

# Dump schema-v2 transitions for BC (JSONL). Example:
#   just dump-transitions out=data/scripted_wr_n7.jsonl n=7 route=wr secs=90
dump-transitions out="data/scripted.jsonl" n="7" secs="90" speed="200" route="mix" seed="0" episodes="1":
    mkdir -p "$(dirname {{out}})"
    cargo run --release -- --headless --no-audio --speed {{speed}} \
      --scripted-baseline --num-orbs {{n}} --act-hz 10 --max-episode-secs {{secs}} \
      --route-mode {{route}} --seed {{seed}} --num-episodes {{episodes}} \
      --dump-transitions {{out}}

# Collect multi-route demos for BC (writes data/demos/)
collect-demos out="data/demos":
    bash scripts/collect_demos.sh {{out}}

# Behavior cloning on dumps (needs python/.venv with torch)
bc-train data="data/demos/all_merged.jsonl" out="data/bc_policy.pt" epochs="30":
    #!/usr/bin/env bash
    set -euo pipefail
    cd python
    PYTHONPATH=src .venv/bin/python -m ssol_training.phase1_train "../{{data}}" --epochs {{epochs}} --out "../{{out}}"

# Residual SAC (live stdio env). Example: just sac-train n=1 steps=5000
sac-train n="1" steps="20000" route="mix" bc="data/bc_policy.pt" out="data/sac_residual":
    #!/usr/bin/env bash
    set -euo pipefail
    cd python
    PYTHONUNBUFFERED=1 PYTHONPATH=src .venv/bin/python -u -m ssol_training.phase1_sac \
      --sim-bin ../target/release/ssol_simulator \
      --bc-policy "../{{bc}}" \
      --num-orbs {{n}} --route-mode {{route}} --timesteps {{steps}} \
      --out "../{{out}}"

# Frozen Phase-1 gate eval (default: n7 residual, wr+greedy, 20 seeds).
# Example: just sac-eval
#          just sac-eval seeds=0-4  # smoke
sac-eval out="data/eval_n7_gate" seeds="0-19" routes="wr greedy" orbs="7" speed="200":
    #!/usr/bin/env bash
    set -euo pipefail
    OUT="{{out}}" SEEDS="{{seeds}}" ROUTES="{{routes}}" ORBS="{{orbs}}" SPEED="{{speed}}" \
      bash scripts/run_phase1_eval.sh

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
