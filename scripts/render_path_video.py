#!/usr/bin/env python3
"""Render a top-down path video from phase1_eval trajectory dumps.

Trajectory row (preferred):
  [x, y, z, yaw, pitch, act_x, act_y, act_yaw]

Features:
  - Map backdrop (level_zero_route_graph / topdown meta)
  - Remaining orbs by score
  - Look direction arrow (yaw)
  - Input bars (strafe / forward / yaw_rate)

Usage:
  python scripts/render_path_video.py \\
    --traj .../wr_seed0_path.npy --scores ..._scores.npy --meta ...json \\
    --map-meta screenshots/level_zero_topdown_orbs_meta.json \\
    --map-image screenshots/level_zero_route_graph.png \\
    --out data/videos/proof_wr7_path.mp4
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np


def _load_orbs(meta_path: Path | None) -> list[dict]:
    if meta_path is None or not meta_path.is_file():
        return []
    data = json.loads(meta_path.read_text())
    # Prefer ordered list with id,x,z
    orbs = data.get("orbs_by_distance_from_spawn") or data.get("orbs") or []
    return list(orbs)


def _map_extent(meta_path: Path | None):
    if meta_path is None or not meta_path.is_file():
        return None
    data = json.loads(meta_path.read_text())
    if all(k in data for k in ("xmin", "xmax", "zmin", "zmax")):
        return float(data["xmin"]), float(data["xmax"]), float(data["zmin"]), float(data["zmax"])
    return None


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--traj", type=Path, required=True, help="*_path.npy")
    p.add_argument("--scores", type=Path, default=None, help="*_scores.npy optional")
    p.add_argument("--meta", type=Path, default=None)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--fps", type=int, default=30)
    p.add_argument("--stride", type=int, default=1, help="Keep every Nth sample")
    p.add_argument(
        "--map-image",
        type=Path,
        default=Path("screenshots/level_zero_route_graph.png"),
    )
    p.add_argument(
        "--map-meta",
        type=Path,
        default=Path("screenshots/level_zero_topdown_orbs_meta.json"),
    )
    p.add_argument(
        "--look-len",
        type=float,
        default=8.0,
        help="Look-direction arrow length in world units",
    )
    args = p.parse_args()

    path = np.load(args.traj)
    if path.ndim != 2 or path.shape[1] < 2:
        raise SystemExit(f"bad path shape {path.shape}")
    has_yaw = path.shape[1] >= 4
    has_act = path.shape[1] >= 8

    scores = None
    if args.scores and args.scores.is_file():
        scores = np.load(args.scores)

    # Drop trailing auto-reset artifact: last sample teleports back to spawn.
    # (phase1_eval used to record DummyVecEnv's post-reset obs on the terminal step.)
    if len(path) >= 2:
        jump = float(np.linalg.norm(path[-1, :3] - path[-2, :3]))
        near_spawn = float(np.linalg.norm(path[-1, :3] - path[0, :3])) <= 3.0
        if jump >= 15.0 and near_spawn:
            path = path[:-1]
            if scores is not None and len(scores) > 0:
                scores = scores[:-1]
            print(
                f"render_path_video: stripped terminal spawn jump "
                f"(jump={jump:.1f}u, near_spawn) → {len(path)} samples",
                flush=True,
            )

    meta = {}
    if args.meta and args.meta.is_file():
        meta = json.loads(args.meta.read_text())

    path = path[:: max(1, args.stride)]
    if scores is not None and len(scores) >= len(path):
        scores = scores[:: max(1, args.stride)][: len(path)]

    # World x,z — Bevy z is already in game space; map meta uses Unity-style +z
    # Scene conversion uses z_bevy = -z_unity. Player path from sim is Bevy coords.
    # Map meta x/z look like Unity (spawn 0,0; first orb +14,+4.5). Convert path
    # Bevy z → Unity z for overlay: z_unity = -z_bevy.
    xs = path[:, 0].astype(np.float64)
    zs_bevy = path[:, 2].astype(np.float64) if path.shape[1] >= 3 else path[:, 1].astype(np.float64)
    zs = -zs_bevy  # align with map meta / route graph
    yaws = path[:, 3].astype(np.float64) if has_yaw else np.zeros(len(path))
    # yaw in Bevy: convert look vector to Unity map axes (x, z_unity=-z_bevy)
    # forward in Bevy xz: (sin(yaw), cos(yaw)) for yaw around Y — check sign
    # Player uses euler YXZ; forward is typically -Z or +Z depending on convention.
    # Use target_yaw style: yaw_err uses atan2 — train scripted uses sin/cos of yaw.
    # From player.rs ghost: (sin_y, cos_y) for forward on x/z Bevy.
    # Bevy forward xz ≈ (sin(yaw), cos(yaw)); Unity map (x, z_u) with z_u=-z_b
    # → map dir (sin(yaw), -cos(yaw))

    orbs = _load_orbs(args.map_meta)
    num_orbs = int(meta.get("num_orbs") or 7)
    # Curriculum: first num_orbs by distance-from-spawn order in meta
    active_orbs = orbs[:num_orbs] if orbs else []

    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        from matplotlib.animation import FFMpegWriter, FuncAnimation
        from matplotlib.patches import FancyArrowPatch, Rectangle, Circle
        from matplotlib.lines import Line2D
    except ImportError as e:
        raise SystemExit(f"matplotlib required: {e}")

    fig = plt.figure(figsize=(11, 7), dpi=120)
    # Map takes left; input HUD on right
    ax = fig.add_axes([0.04, 0.08, 0.72, 0.86])
    ax_hud = fig.add_axes([0.78, 0.08, 0.20, 0.86])
    ax_hud.set_xlim(0, 1)
    ax_hud.set_ylim(0, 1)
    ax_hud.axis("off")
    ax.set_aspect("equal")
    ax.set_xlabel("x")
    ax.set_ylabel("z (map)")

    extent = _map_extent(args.map_meta)
    if args.map_image.is_file() and extent:
        xmin, xmax, zmin, zmax = extent
        img = plt.imread(str(args.map_image))
        # imshow: extent (left, right, bottom, top); map py uses zmax at top
        ax.imshow(
            img,
            extent=(xmin, xmax, zmin, zmax),
            origin="upper",
            aspect="equal",
            alpha=0.85,
            zorder=0,
        )
        ax.set_xlim(xmin, xmax)
        ax.set_ylim(zmin, zmax)
    else:
        pad = 15.0
        ax.set_xlim(float(xs.min()) - pad, float(xs.max()) + pad)
        ax.set_ylim(float(zs.min()) - pad, float(zs.max()) + pad)

    title = (
        f"SSOL path  seed={meta.get('seed')}  "
        f"orbs={meta.get('orbs')}/{meta.get('num_orbs')}  success={meta.get('success')}"
    )
    ax.set_title(title, fontsize=11)

    # Static orb markers (updated alpha by remaining)
    orb_scat = ax.scatter(
        [o["x"] for o in active_orbs],
        [o["z"] for o in active_orbs],
        s=36,
        c="#ffcc33",
        edgecolors="k",
        linewidths=0.4,
        zorder=3,
        label="orbs",
    )
    (trail,) = ax.plot([], [], color="#00e5ff", lw=2.0, alpha=0.9, zorder=4)
    (dot,) = ax.plot([], [], "o", color="#ff3355", ms=9, zorder=6)
    look_line = ax.plot([], [], color="#ffffff", lw=2.2, zorder=5)[0]
    # move input direction (relative to yaw)
    move_line = ax.plot([], [], color="#88ff88", lw=1.8, zorder=5, alpha=0.9)[0]

    score_txt = ax.text(
        0.01,
        0.99,
        "",
        transform=ax.transAxes,
        va="top",
        ha="left",
        color="white",
        fontsize=11,
        bbox=dict(facecolor="black", alpha=0.55, boxstyle="round,pad=0.35"),
        zorder=10,
    )

    # HUD bars
    ax_hud.text(0.5, 0.95, "inputs", ha="center", va="top", color="white", fontsize=12)
    labels = ["strafe", "forward", "yaw_rate"]
    bar_bases = [0.68, 0.42, 0.16]
    bar_rects = []
    for y0, lab in zip(bar_bases, labels):
        ax_hud.text(0.5, y0 + 0.18, lab, ha="center", color="#cccccc", fontsize=9)
        # background track [-1,1]
        ax_hud.add_patch(
            Rectangle((0.15, y0), 0.7, 0.08, facecolor="#333333", edgecolor="#666666")
        )
        r = Rectangle((0.5, y0), 0.0, 0.08, facecolor="#44aaff")
        ax_hud.add_patch(r)
        bar_rects.append(r)
        ax_hud.plot([0.5, 0.5], [y0, y0 + 0.08], color="#aaaaaa", lw=1)

    def init():
        trail.set_data([], [])
        dot.set_data([], [])
        look_line.set_data([], [])
        move_line.set_data([], [])
        score_txt.set_text("")
        return trail, dot, look_line, move_line, score_txt, orb_scat, *bar_rects

    def update(i):
        trail.set_data(xs[: i + 1], zs[: i + 1])
        x, z = float(xs[i]), float(zs[i])
        dot.set_data([x], [z])
        yaw = float(yaws[i]) if has_yaw else 0.0
        # Look on map (Unity z)
        lx = np.sin(yaw) * args.look_len
        lz = -np.cos(yaw) * args.look_len
        look_line.set_data([x, x + lx], [z, z + lz])

        sc = int(scores[i]) if scores is not None and i < len(scores) and scores[i] >= 0 else 0
        # Hide collected orbs (first `sc` in curriculum distance order)
        if active_orbs:
            remaining = active_orbs[sc:]
            if remaining:
                orb_scat.set_offsets(np.array([[o["x"], o["z"]] for o in remaining]))
                orb_scat.set_alpha(1.0)
            else:
                orb_scat.set_offsets(np.empty((0, 2)))
        score_txt.set_text(f"score={sc}/{num_orbs}  step={i}/{len(xs)-1}")

        if has_act and path.shape[1] >= 8:
            ax_v, ay_v, ayaw_v = float(path[i, 5]), float(path[i, 6]), float(path[i, 7])
            # Move vector in body frame: +ay forward, +ax strafe right
            # body forward map: (sin yaw, -cos yaw); right: (cos yaw, sin yaw)
            fx, fz = np.sin(yaw), -np.cos(yaw)
            rx, rz = np.cos(yaw), np.sin(yaw)
            mx = (rx * ax_v + fx * ay_v) * args.look_len * 0.85
            mz = (rz * ax_v + fz * ay_v) * args.look_len * 0.85
            move_line.set_data([x, x + mx], [z, z + mz])
            vals = [ax_v, ay_v, np.clip(ayaw_v, -1, 1)]
            colors = ["#ff8844", "#44ff88", "#4488ff"]
            for rect, v, c in zip(bar_rects, vals, colors):
                v = float(np.clip(v, -1, 1))
                # map [-1,1] → [0.15, 0.85] width from center 0.5
                if v >= 0:
                    rect.set_x(0.5)
                    rect.set_width(0.35 * v)
                else:
                    rect.set_x(0.5 + 0.35 * v)
                    rect.set_width(-0.35 * v)
                rect.set_facecolor(c)
        else:
            move_line.set_data([], [])

        return trail, dot, look_line, move_line, score_txt, orb_scat, *bar_rects

    n = len(xs)
    anim = FuncAnimation(
        fig, update, frames=n, init_func=init, blit=False, interval=1000 / args.fps
    )
    args.out.parent.mkdir(parents=True, exist_ok=True)
    writer = FFMpegWriter(fps=args.fps, bitrate=2400)
    anim.save(str(args.out), writer=writer)
    plt.close(fig)
    print(f"wrote {args.out} frames={n} size={args.out.stat().st_size}")


if __name__ == "__main__":
    main()
