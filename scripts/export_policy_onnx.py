#!/usr/bin/env python3
"""Export Open SSOL Phase-1 policies to ONNX for Netron / runtime inspection.

Rebuilds pure MLPs from checkpoint weights so export does not depend on
SB3's squashed-Gaussian distribution path (which breaks torch.export).

Exports (default n7 residual ladder + BC teacher):
  - sac_actor.onnx     obs(39) → residual action(3)  [tanh(μ), deterministic]
  - sac_qf0.onnx       [obs‖act](42) → Q
  - sac_qf1.onnx       [obs‖act](42) → Q
  - bc_teacher.onnx    obs(39) → a_bc (tanh move, tanh×2.5 yaw)

Example:
  PYTHONPATH=python/src python scripts/export_policy_onnx.py \\
    --sac-model data/sac_ladder/n7_mix_300k/sac_model.zip \\
    --bc-policy data/bc_policy.pt \\
    --out data/onnx

Open *.onnx at https://netron.app/
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import torch
import torch.nn as nn

OBS_DIM = 39
ACT_DIM = 3
YAW_SCALE = 2.5
HIDDEN = 256


class MlpActor(nn.Module):
    """Deterministic SAC-style actor: latent 256-256 → tanh(μ)."""

    def __init__(self):
        super().__init__()
        self.fc1 = nn.Linear(OBS_DIM, HIDDEN)
        self.fc2 = nn.Linear(HIDDEN, HIDDEN)
        self.mu = nn.Linear(HIDDEN, ACT_DIM)

    def forward(self, obs: torch.Tensor) -> torch.Tensor:
        x = torch.relu(self.fc1(obs))
        x = torch.relu(self.fc2(x))
        return torch.tanh(self.mu(x))


class MlpQ(nn.Module):
    """Single Q: [obs || action] → scalar."""

    def __init__(self):
        super().__init__()
        self.fc1 = nn.Linear(OBS_DIM + ACT_DIM, HIDDEN)
        self.fc2 = nn.Linear(HIDDEN, HIDDEN)
        self.out = nn.Linear(HIDDEN, 1)

    def forward(self, obs_action: torch.Tensor) -> torch.Tensor:
        x = torch.relu(self.fc1(obs_action))
        x = torch.relu(self.fc2(x))
        return self.out(x)


class BcTeacher(nn.Module):
    """BC teacher: SiLU MLP → tanh move + scaled yaw."""

    def __init__(self):
        super().__init__()
        self.fc1 = nn.Linear(OBS_DIM, HIDDEN)
        self.fc2 = nn.Linear(HIDDEN, HIDDEN)
        self.fc3 = nn.Linear(HIDDEN, ACT_DIM)

    def forward(self, obs: torch.Tensor) -> torch.Tensor:
        x = torch.nn.functional.silu(self.fc1(obs))
        x = torch.nn.functional.silu(self.fc2(x))
        raw = self.fc3(x)
        move = torch.tanh(raw[:, :2])
        yaw = torch.tanh(raw[:, 2:3]) * YAW_SCALE
        return torch.cat([move, yaw], dim=-1)


def _copy_linear(dst: nn.Linear, src: nn.Linear) -> None:
    with torch.no_grad():
        dst.weight.copy_(src.weight)
        dst.bias.copy_(src.bias)


def _export(
    model: nn.Module,
    path: Path,
    dummy: torch.Tensor,
    input_names: list[str],
    output_names: list[str],
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    model.eval()
    # Prefer legacy exporter (more reliable for plain MLPs on recent torch).
    kwargs = dict(
        input_names=input_names,
        output_names=output_names,
        dynamic_axes={
            input_names[0]: {0: "batch"},
            output_names[0]: {0: "batch"},
        },
        opset_version=17,
        do_constant_folding=True,
    )
    with torch.no_grad():
        try:
            torch.onnx.export(model, dummy, str(path), dynamo=False, **kwargs)
        except TypeError:
            # Older torch without dynamo= flag
            torch.onnx.export(model, dummy, str(path), **kwargs)
    print(f"wrote {path}  ({path.stat().st_size} bytes)")


def export_sac(sac_path: Path, out: Path, skip_q: bool) -> list[Path]:
    from stable_baselines3 import SAC

    print(f"loading SAC {sac_path} …")
    model = SAC.load(str(sac_path), device="cpu")
    actor_sb3 = model.policy.actor

    # SB3: latent_pi = Sequential(Linear, ReLU, Linear, ReLU); mu / log_std heads
    actor = MlpActor()
    _copy_linear(actor.fc1, actor_sb3.latent_pi[0])
    _copy_linear(actor.fc2, actor_sb3.latent_pi[2])
    _copy_linear(actor.mu, actor_sb3.mu)

    paths: list[Path] = []
    dummy_obs = torch.zeros(1, OBS_DIM, dtype=torch.float32)
    actor_path = out / "sac_actor.onnx"
    _export(actor, actor_path, dummy_obs, ["obs"], ["action"])
    paths.append(actor_path)

    # Sanity: match SB3 deterministic action
    with torch.no_grad():
        a_ref = actor_sb3(dummy_obs, deterministic=True)
        a_our = actor(dummy_obs)
        err = (a_ref - a_our).abs().max().item()
        print(f"  actor match vs SB3 max|Δ|={err:.3e}")
        if err > 1e-4:
            print("  WARN: actor mismatch larger than expected", file=sys.stderr)

    if not skip_q:
        critic = model.policy.critic
        dummy_sa = torch.zeros(1, OBS_DIM + ACT_DIM, dtype=torch.float32)
        for name, q_seq in (("qf0", critic.qf0), ("qf1", critic.qf1)):
            q = MlpQ()
            # Sequential: Linear, ReLU, Linear, ReLU, Linear
            _copy_linear(q.fc1, q_seq[0])
            _copy_linear(q.fc2, q_seq[2])
            _copy_linear(q.out, q_seq[4])
            q_path = out / f"sac_{name}.onnx"
            _export(q, q_path, dummy_sa, ["obs_action"], ["q_value"])
            paths.append(q_path)
            with torch.no_grad():
                err_q = (q_seq(dummy_sa) - q(dummy_sa)).abs().max().item()
                print(f"  {name} match max|Δ|={err_q:.3e}")

    # Also dump human-readable structure note
    note = {
        "source": str(sac_path),
        "actor": "obs(39) → Linear256-ReLU → Linear256-ReLU → Linear3 → tanh  (deterministic μ)",
        "critic": "obs||act(42) → Linear256-ReLU → Linear256-ReLU → Linear1  × twin qf0/qf1",
        "note": "log_std head not exported (eval uses deterministic mean)",
    }
    (out / "sac_structure.json").write_text(json.dumps(note, indent=2) + "\n")
    return paths


def export_bc(bc_path: Path, out: Path) -> list[Path]:
    print(f"loading BC {bc_path} …")
    ckpt = torch.load(bc_path, map_location="cpu", weights_only=False)
    sd = ckpt["state_dict"] if "state_dict" in ckpt else ckpt

    # Keys like net.0.weight, net.2.weight, net.4.weight (Linear, SiLU, Linear, SiLU, Linear)
    def w(i: int) -> torch.Tensor:
        return sd[f"net.{i}.weight"]

    def b(i: int) -> torch.Tensor:
        return sd[f"net.{i}.bias"]

    bc = BcTeacher()
    with torch.no_grad():
        bc.fc1.weight.copy_(w(0))
        bc.fc1.bias.copy_(b(0))
        bc.fc2.weight.copy_(w(2))
        bc.fc2.bias.copy_(b(2))
        bc.fc3.weight.copy_(w(4))
        bc.fc3.bias.copy_(b(4))

    dummy_obs = torch.zeros(1, OBS_DIM, dtype=torch.float32)
    path = out / "bc_teacher.onnx"
    _export(bc, path, dummy_obs, ["obs"], ["action"])

    meta = {
        "source": str(bc_path),
        "arch": ckpt.get("arch", "mlp_256_256_silu"),
        "structure": "obs(39) → Linear256-SiLU → Linear256-SiLU → Linear3 → tanh move, tanh*2.5 yaw",
    }
    if "obs_mean" in ckpt:
        import numpy as np

        meta["obs_mean"] = np.asarray(ckpt["obs_mean"]).tolist()
        meta["obs_std"] = np.asarray(ckpt["obs_std"]).tolist()
    (out / "bc_teacher_meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    print(f"wrote {out / 'bc_teacher_meta.json'}")
    return [path]


def main() -> None:
    p = argparse.ArgumentParser(description="Export SAC/BC policies to ONNX (Netron)")
    p.add_argument(
        "--sac-model",
        type=Path,
        default=Path("data/sac_ladder/n7_mix_300k/sac_model.zip"),
    )
    p.add_argument("--bc-policy", type=Path, default=Path("data/bc_policy.pt"))
    p.add_argument("--out", type=Path, default=Path("data/onnx"))
    p.add_argument("--skip-sac", action="store_true")
    p.add_argument("--skip-bc", action="store_true")
    p.add_argument("--skip-q", action="store_true")
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    exported: list[Path] = []
    if not args.skip_sac:
        if not args.sac_model.is_file():
            print(f"WARN: missing {args.sac_model}", file=sys.stderr)
        else:
            exported.extend(export_sac(args.sac_model, args.out, args.skip_q))
    if not args.skip_bc:
        if not args.bc_policy.is_file():
            print(f"WARN: missing {args.bc_policy}", file=sys.stderr)
        else:
            exported.extend(export_bc(args.bc_policy, args.out))

    if not exported:
        raise SystemExit("nothing exported")

    print()
    print("Open in Netron: https://netron.app/  (drag-and-drop a .onnx file)")
    for path in exported:
        print(f"  {path}")


if __name__ == "__main__":
    main()
