"""Phase 1: BC warm-start + residual SAC on dumped transitions and live subprocess envs.

Architecture (research default):
  obs(39) → MLP 256-SiLU-256 → residual action (3)
  a = clip(a_teacher(s) + π_θ(s), bounds)
  z (32) kept private / identity for Phase 1

Offline path (no live sim):
  1) BC on JSONL dumps
  2) Optional offline residual fine-tune is limited without env — prefer BC first

Live residual SAC (requires built ssol_simulator on PATH or --sim-bin):
  Uses multi-process subprocess envs that dump short rollouts; simpler path is
  BC-only then evaluate by injecting policy later.

This module focuses on:
  - robust JSONL loading (schema v2)
  - BC train + save
  - residual SAC with a *scripted teacher* inside a Gymnasium env wrapping
    transition dumps is NOT possible for on-policy interaction.

For live SAC we provide SubprocessSSOLEnv that launches one episode per reset
via the binary with dump to a temp file and feeds offline... that's wrong.

Live approach: SubprocessSSOLEnv runs the sim and we need a bridge. Without
a live step bridge, Phase 1b delivers:
  1. Demo collection script (shell)
  2. BC training that produces data/bc_policy.pt
  3. SAC training on a *vectorized offline imitation* is not SAC

Actual residual SAC needs live env. Implement a minimal file-based lockstep
is too heavy. Instead:

  - BC is fully working offline
  - ResidualSACTrainer trains SAC where the "env" is a Gym wrapper that
    shells out is too slow.

Better live env: use Gymnasium + subprocess that we control via stdin/stdout
JSON lines — not implemented in sim yet.

Pragmatic Phase 1b deliverable:
  1. Fixed dump alignment (Rust)
  2. collect_demos.sh
  3. BC with train/val split + metrics
  4. Residual SAC *policy module* + training loop that uses
     DummyVecEnv of OfflineReplayEnv? No that's not SAC.

I'll implement live SAC via a lightweight approach:
  Gym env that for each step writes action to a shared file is too hacky.

**Ship BC + residual SAC network definition + train residual SAC on
synthetic gym CartPole-like is wrong.**

Implement `SSOLSubprocessEnv` that:
  - On reset: spawn sim with --dump-transitions temp and wait for process? 
    That only gives offline data after full episode.

True residual SAC needs step API. Add a simple **JSON step protocol** over
stdio for one episode? Too big for this turn.

Deliver:
1. BC (solid)
2. `ResidualActor` torch module matching plan
3. Script that can fine-tune residual with SAC using **transitions where we
   have (s, a_teacher, a*, r, s')** — we only have a_teacher in dumps.
   
For residual BC: learn residual of 0 (teacher is optimal under dump).
Useless for residual without better actions.

**Conclusion:** Phase 1b = dump fix + mass demo collection + BC quality +
document SAC live env as next (or implement a thin stdin action protocol).

I'll add a minimal stdin protocol to the sim for live control:
  --train-stdio: each act wait for JSON line {"move":[x,y], "yaw":r} on stdin,
  print TRAIN_OBS_JSON {...} each act.

That's the right path for SAC. Let me implement --train-stdio in Rust quickly.
"""

# The docstring above is design notes; implementation follows.
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

import numpy as np

OBS_DIM = 39
ACT_DIM = 3
LATENT_DIM = 32
YAW_SCALE = 2.5


def load_jsonl(path: Path) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Load obs, actions, rewards from dump. Filters schema==2."""
    obs_list, act_list, rew_list = [], [], []
    with path.open() as f:
        for line_no, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            if row.get("schema", 2) != 2:
                continue
            o, a = row["obs"], row["action"]
            if len(o) != OBS_DIM or len(a) != ACT_DIM:
                raise ValueError(f"{path}:{line_no} dim mismatch obs={len(o)} act={len(a)}")
            obs_list.append(o)
            act_list.append(a)
            rew_list.append(float(row.get("reward", 0.0)))
    if not obs_list:
        raise ValueError(f"no schema-v2 transitions in {path}")
    return (
        np.asarray(obs_list, dtype=np.float32),
        np.asarray(act_list, dtype=np.float32),
        np.asarray(rew_list, dtype=np.float32),
    )


def load_many(paths: list[Path]) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    os_, as_, rs_ = [], [], []
    for p in paths:
        o, a, r = load_jsonl(p)
        os_.append(o)
        as_.append(a)
        rs_.append(r)
        print(f"  {p}: {len(o)} transitions")
    return np.concatenate(os_), np.concatenate(as_), np.concatenate(rs_)


def train_bc(
    obs: np.ndarray,
    act: np.ndarray,
    epochs: int,
    lr: float,
    batch: int,
    out: Path,
    val_frac: float = 0.1,
) -> Path:
    import torch
    import torch.nn as nn
    from torch.utils.data import DataLoader, TensorDataset

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    n = len(obs)
    idx = np.random.permutation(n)
    n_val = max(1, int(n * val_frac))
    val_idx, tr_idx = idx[:n_val], idx[n_val:]

    mean = obs[tr_idx].mean(axis=0)
    std = obs[tr_idx].std(axis=0) + 1e-6
    # Keep ray dims (23:) roughly [0,1] — still normalize with care
    obs_n = (obs - mean) / std

    class Actor(nn.Module):
        def __init__(self):
            super().__init__()
            self.net = nn.Sequential(
                nn.Linear(OBS_DIM, 256),
                nn.SiLU(),
                nn.Linear(256, 256),
                nn.SiLU(),
                nn.Linear(256, ACT_DIM),
            )

        def forward(self, x):
            raw = self.net(x)
            move = torch.tanh(raw[:, :2])
            yaw = torch.tanh(raw[:, 2:3]) * YAW_SCALE
            return torch.cat([move, yaw], dim=-1)

    model = Actor().to(device)
    opt = torch.optim.Adam(model.parameters(), lr=lr)

    def run_epoch(indices, train: bool):
        ds = TensorDataset(
            torch.from_numpy(obs_n[indices]),
            torch.from_numpy(act[indices]),
        )
        loader = DataLoader(ds, batch_size=batch, shuffle=train)
        total, count = 0.0, 0
        model.train(mode=train)
        with torch.set_grad_enabled(train):
            for xb, yb in loader:
                xb, yb = xb.to(device), yb.to(device)
                pred = model(xb)
                loss = nn.functional.mse_loss(pred, yb)
                if train:
                    opt.zero_grad()
                    loss.backward()
                    opt.step()
                total += float(loss.item()) * xb.size(0)
                count += xb.size(0)
        return total / max(count, 1)

    best_val = math.inf
    for ep in range(epochs):
        tr = run_epoch(tr_idx, True)
        va = run_epoch(val_idx, False)
        print(f"epoch {ep+1}/{epochs} train_mse={tr:.6f} val_mse={va:.6f}")
        if va < best_val:
            best_val = va
            out.parent.mkdir(parents=True, exist_ok=True)
            torch.save(
                {
                    "state_dict": model.state_dict(),
                    "obs_mean": mean,
                    "obs_std": std,
                    "obs_dim": OBS_DIM,
                    "act_dim": ACT_DIM,
                    "latent_dim": LATENT_DIM,
                    "yaw_scale": YAW_SCALE,
                    "arch": "mlp_256_256_silu_bc",
                    "val_mse": best_val,
                },
                out,
            )
    print(f"wrote best BC → {out} (val_mse={best_val:.6f})")
    return out


def train_residual_sac_offline_hint() -> None:
    print(
        "Residual SAC needs a live env step API. "
        "After BC, use phase1_sac.py with --sim-bin once stdio protocol is enabled, "
        "or continue collecting demos and improve teacher."
    )


def main() -> None:
    p = argparse.ArgumentParser(description="Phase 1 BC (+ SAC stub)")
    p.add_argument(
        "data",
        type=Path,
        nargs="+",
        help="JSONL dump files or a directory of *.jsonl",
    )
    p.add_argument("--epochs", type=int, default=40)
    p.add_argument("--lr", type=float, default=1e-3)
    p.add_argument("--batch", type=int, default=512)
    p.add_argument("--out", type=Path, default=Path("data/bc_policy.pt"))
    p.add_argument("--seed", type=int, default=0)
    args = p.parse_args()
    np.random.seed(args.seed)

    paths: list[Path] = []
    for d in args.data:
        if d.is_dir():
            paths.extend(sorted(d.glob("*.jsonl")))
        else:
            paths.append(d)
    paths = [p for p in paths if p.name != "all_merged.jsonl" or True]
    if not paths:
        raise SystemExit("no jsonl files found")

    print("Loading:")
    obs, act, rew = load_many(paths)
    print(f"total {len(obs)} transitions  reward mean={rew.mean():.4f}")
    train_bc(obs, act, args.epochs, args.lr, args.batch, args.out)
    train_residual_sac_offline_hint()


if __name__ == "__main__":
    main()
