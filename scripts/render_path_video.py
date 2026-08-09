#!/usr/bin/env python3
"""Render a top-down path video from phase1_eval trajectory dumps.

Proof of a successful run: animated player path + orb score over time.
Usage:
  python scripts/render_path_video.py \\
    --traj data/eval/.../trajectories/wr_seed0_path.npy \\
    --meta data/eval/.../trajectories/wr_seed0.json \\
    --out proof.mp4
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--traj", type=Path, required=True, help="*_path.npy")
    p.add_argument("--scores", type=Path, default=None, help="*_scores.npy optional")
    p.add_argument("--meta", type=Path, default=None)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--fps", type=int, default=30)
    p.add_argument("--stride", type=int, default=2, help="Keep every Nth sample")
    args = p.parse_args()

    path = np.load(args.traj)
    if path.ndim != 2 or path.shape[1] < 2:
        raise SystemExit(f"bad path shape {path.shape}")
    scores = None
    if args.scores and args.scores.is_file():
        scores = np.load(args.scores)
    meta = {}
    if args.meta and args.meta.is_file():
        meta = json.loads(args.meta.read_text())

    # Subsample for reasonable length
    path = path[:: max(1, args.stride)]
    if scores is not None and len(scores) >= len(path):
        scores = scores[:: max(1, args.stride)][: len(path)]

    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        from matplotlib.animation import FFMpegWriter, FuncAnimation
    except ImportError as e:
        raise SystemExit(f"matplotlib required: {e}")

    xs, zs = path[:, 0], path[:, 2] if path.shape[1] >= 3 else path[:, 1]
    fig, ax = plt.subplots(figsize=(8, 8), dpi=100)
    ax.set_aspect("equal")
    ax.set_xlabel("x")
    ax.set_ylabel("z")
    title = (
        f"SSOL path seed={meta.get('seed')} orbs={meta.get('orbs')}/{meta.get('num_orbs')} "
        f"success={meta.get('success')}"
    )
    ax.set_title(title)
    (trail,) = ax.plot([], [], "c-", lw=1.5, alpha=0.8)
    (dot,) = ax.plot([], [], "ro", ms=8)
    pad = 5.0
    ax.set_xlim(float(xs.min()) - pad, float(xs.max()) + pad)
    ax.set_ylim(float(zs.min()) - pad, float(zs.max()) + pad)
    score_txt = ax.text(
        0.02, 0.98, "", transform=ax.transAxes, va="top", ha="left", color="yellow",
        fontsize=12, bbox=dict(facecolor="black", alpha=0.5),
    )

    def init():
        trail.set_data([], [])
        dot.set_data([], [])
        score_txt.set_text("")
        return trail, dot, score_txt

    def update(i):
        trail.set_data(xs[: i + 1], zs[: i + 1])
        dot.set_data([xs[i]], [zs[i]])
        if scores is not None and i < len(scores):
            score_txt.set_text(f"score={int(scores[i])}  t={i}")
        else:
            score_txt.set_text(f"t={i}/{len(xs)-1}")
        return trail, dot, score_txt

    n = len(xs)
    anim = FuncAnimation(fig, update, frames=n, init_func=init, blit=True, interval=1000 / args.fps)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    writer = FFMpegWriter(fps=args.fps, bitrate=1800)
    anim.save(str(args.out), writer=writer)
    plt.close(fig)
    print(f"wrote {args.out} frames={n} size={args.out.stat().st_size}")


if __name__ == "__main__":
    main()
