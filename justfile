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
