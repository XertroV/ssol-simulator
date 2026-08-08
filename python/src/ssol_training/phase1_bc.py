"""Phase 1 behavior cloning on dumped train transitions (schema v2).

Does not require a live sim. Input: JSONL from `--dump-transitions`.

Obs dim = 39 (23 base + 16 wall rays). Act dim = 3.
Private residual z is NOT in the file — it lives only inside a later Actor.

Example:
  uv run python -m ssol_training.phase1_bc data/scripted_mix_n7.jsonl --epochs 20
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np

OBS_DIM = 39
ACT_DIM = 3
LATENT_DIM = 32  # policy-private; not loaded from dumps


def load_jsonl(path: Path) -> tuple[np.ndarray, np.ndarray]:
    obs_list: list[list[float]] = []
    act_list: list[list[float]] = []
    with path.open() as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            o = row["obs"]
            a = row["action"]
            if len(o) != OBS_DIM:
                raise ValueError(f"obs dim {len(o)} != {OBS_DIM} (schema mismatch)")
            if len(a) != ACT_DIM:
                raise ValueError(f"action dim {len(a)} != {ACT_DIM}")
            # Skip pure timeout terminal rows if desired — keep all for now.
            obs_list.append(o)
            act_list.append(a)
    if not obs_list:
        raise ValueError(f"no transitions in {path}")
    return np.asarray(obs_list, dtype=np.float32), np.asarray(act_list, dtype=np.float32)


def train_bc(obs: np.ndarray, act: np.ndarray, epochs: int, lr: float, batch: int) -> None:
    try:
        import torch
        import torch.nn as nn
        from torch.utils.data import DataLoader, TensorDataset
    except ImportError as e:
        raise SystemExit(
            "PyTorch required for phase1_bc. Install torch in the uv env."
        ) from e

    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    # Running normalize
    mean = obs.mean(axis=0)
    std = obs.std(axis=0) + 1e-6
    # Rays already [0,1] — leave scale; still normalize for simplicity.
    obs_n = (obs - mean) / std

    model = nn.Sequential(
        nn.Linear(OBS_DIM, 256),
        nn.SiLU(),
        nn.Linear(256, 256),
        nn.SiLU(),
        nn.Linear(256, ACT_DIM),
        nn.Tanh(),  # actions roughly [-1,1]; yaw scaled after
    ).to(device)

    # Scale yaw to ±2.5 after tanh
    yaw_scale = 2.5

    opt = torch.optim.Adam(model.parameters(), lr=lr)
    ds = TensorDataset(
        torch.from_numpy(obs_n),
        torch.from_numpy(act),
    )
    loader = DataLoader(ds, batch_size=batch, shuffle=True)

    model.train()
    for ep in range(epochs):
        total = 0.0
        n = 0
        for xb, yb in loader:
            xb = xb.to(device)
            yb = yb.to(device)
            pred = model(xb)
            # Match teacher: move in [-1,1], yaw in [-2.5,2.5]
            pred_scaled = pred.clone()
            pred_scaled[:, 2] = pred[:, 2] * yaw_scale
            loss = nn.functional.mse_loss(pred_scaled, yb)
            opt.zero_grad()
            loss.backward()
            opt.step()
            total += float(loss.item()) * xb.size(0)
            n += xb.size(0)
        print(f"epoch {ep+1}/{epochs} mse={total / max(n, 1):.6f}")

    out = Path("data/bc_policy.pt")
    out.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "state_dict": model.state_dict(),
            "obs_mean": mean,
            "obs_std": std,
            "obs_dim": OBS_DIM,
            "act_dim": ACT_DIM,
            "latent_dim": LATENT_DIM,
            "yaw_scale": yaw_scale,
            "arch": "mlp_256_256_silu",
        },
        out,
    )
    print(f"wrote {out}")


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("jsonl", type=Path, help="Transition dump from --dump-transitions")
    p.add_argument("--epochs", type=int, default=30)
    p.add_argument("--lr", type=float, default=1e-3)
    p.add_argument("--batch", type=int, default=512)
    args = p.parse_args()
    obs, act = load_jsonl(args.jsonl)
    print(f"loaded {len(obs)} transitions from {args.jsonl}")
    train_bc(obs, act, args.epochs, args.lr, args.batch)


if __name__ == "__main__":
    main()
