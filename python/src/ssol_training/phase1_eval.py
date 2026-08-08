"""Frozen eval of residual SAC (or BC-only) via --train-stdio.

Phase-1 gate (from training plan):
  --route-mode wr and greedy, ≥20 seeds each, num_orbs=7 → success rate ≥90%.

Streams one JSON line per episode (JSONL) and a running summary so monitoring
can abort / retrain early if the policy is clearly failing.

Examples:
  # Full gate (n7 residual + BC)
  PYTHONPATH=python/src python -u -m ssol_training.phase1_eval \\
    --sac-model data/sac_ladder/n7_mix_300k/sac_model.zip \\
    --vecnormalize data/sac_ladder/n7_mix_300k/vecnormalize.pkl \\
    --bc-policy data/bc_policy.pt \\
    --num-orbs 7 --routes wr greedy --seeds 0-19 \\
    --out data/eval_n7_gate

  # Teacher-only control (no SAC residual)
  ... --policy bc --bc-policy data/bc_policy.pt ...
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from datetime import datetime
from pathlib import Path
from typing import Any, Iterable, Optional

import numpy as np

os.environ.setdefault("PYTHONUNBUFFERED", "1")
try:
    sys.stdout.reconfigure(line_buffering=True)  # type: ignore[attr-defined]
    sys.stderr.reconfigure(line_buffering=True)  # type: ignore[attr-defined]
except Exception:
    pass

from ssol_training.phase1_sac import (  # noqa: E402
    ACT_DIM,
    OBS_DIM,
    ResidualActionWrapper,
    SSOLStdioEnv,
    YAW_SCALE,
    _iso_now,
)


def _parse_seed_spec(spec: str) -> list[int]:
    """'0-19' or '0,1,2' or '0 1 2'."""
    spec = spec.strip()
    if not spec:
        return []
    out: list[int] = []
    for part in spec.replace(" ", ",").split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            a, b = part.split("-", 1)
            lo, hi = int(a), int(b)
            if hi < lo:
                lo, hi = hi, lo
            out.extend(range(lo, hi + 1))
        else:
            out.append(int(part))
    # unique preserve order
    seen: set[int] = set()
    uniq: list[int] = []
    for s in out:
        if s not in seen:
            seen.add(s)
            uniq.append(s)
    return uniq


def _load_sac(model_path: Path, venv):
    from stable_baselines3 import SAC

    return SAC.load(str(model_path), env=venv, device="cpu")


def _make_vec_env(
    *,
    sim_bin: Path,
    num_orbs: int,
    route_mode: str,
    seed: int,
    max_episode_secs: float,
    act_hz: float,
    speed: float,
    bc_policy: Optional[Path],
    policy_mode: str,
):
    """Single DummyVecEnv for one episode config (fresh each seed for clean seed)."""
    from stable_baselines3.common.monitor import Monitor
    from stable_baselines3.common.vec_env import DummyVecEnv
    from gymnasium import Env
    from gymnasium import spaces
    import gymnasium as gym

    def _thunk():
        base = SSOLStdioEnv(
            sim_bin=sim_bin,
            num_orbs=num_orbs,
            route_mode=route_mode,
            seed=seed,
            max_episode_secs=max_episode_secs,
            act_hz=act_hz,
            speed=speed,
        )
        if policy_mode in ("sac", "bc") and bc_policy:
            # residual wrapper: SAC residual on BC, or zero residual for bc-only
            inner = ResidualActionWrapper(base, bc_policy if policy_mode in ("sac", "bc") else None)
        else:
            inner = base

        class GymWrap(gym.Env):
            metadata = {"render_modes": []}

            def __init__(self):
                super().__init__()
                self.inner = inner
                self.observation_space = spaces.Box(
                    -np.inf, np.inf, (OBS_DIM,), np.float32
                )
                self.action_space = spaces.Box(-1, 1, (ACT_DIM,), np.float32)

            def reset(self, *, seed=None, options=None):
                super().reset(seed=seed)
                obs, info = self.inner.reset(seed=seed, options=options)
                if hasattr(self.inner, "_last_obs"):
                    self.inner._last_obs = obs
                return obs, info

            def step(self, action):
                return self.inner.step(action)

            def close(self):
                self.inner.close()

        return Monitor(GymWrap())

    return DummyVecEnv([_thunk])


def run_episode(
    *,
    sim_bin: Path,
    sac_model: Optional[Path],
    vecnormalize: Optional[Path],
    bc_policy: Optional[Path],
    policy_mode: str,
    num_orbs: int,
    route_mode: str,
    seed: int,
    max_episode_secs: float,
    act_hz: float,
    speed: float,
    deterministic: bool,
) -> dict[str, Any]:
    from stable_baselines3.common.vec_env import VecNormalize

    t0 = time.time()
    venv = _make_vec_env(
        sim_bin=sim_bin,
        num_orbs=num_orbs,
        route_mode=route_mode,
        seed=seed,
        max_episode_secs=max_episode_secs,
        act_hz=act_hz,
        speed=speed,
        bc_policy=bc_policy,
        policy_mode=policy_mode,
    )

    model = None
    if policy_mode == "sac":
        if not sac_model or not sac_model.is_file():
            raise SystemExit(f"--sac-model required for policy=sac: {sac_model}")
        if vecnormalize and vecnormalize.is_file():
            venv = VecNormalize.load(str(vecnormalize), venv)
            venv.training = False
            venv.norm_reward = False
        else:
            # Match training obs scale poorly if missing — warn hard
            print(
                f"{_iso_now()} WARN: no vecnormalize; eval obs unnormalized",
                flush=True,
            )
        model = _load_sac(sac_model, venv)
    elif policy_mode == "bc":
        # Residual wrapper with BC; send zero residual each step
        pass
    elif policy_mode == "zero":
        pass
    else:
        raise SystemExit(f"unknown policy_mode {policy_mode}")

    obs = venv.reset()
    # SB3 DummyVecEnv reset returns obs only (old API) or may be array
    if isinstance(obs, tuple):
        obs = obs[0]

    total_rew = 0.0
    steps = 0
    last_info: dict = {}
    terminated = False
    truncated = False
    dead = False

    while True:
        if policy_mode == "sac":
            assert model is not None
            action, _ = model.predict(obs, deterministic=deterministic)
        elif policy_mode == "bc":
            # zero residual → pure BC through ResidualActionWrapper
            action = np.zeros((1, ACT_DIM), dtype=np.float32)
        else:
            action = np.zeros((1, ACT_DIM), dtype=np.float32)

        obs, rewards, dones, infos = venv.step(action)
        total_rew += float(rewards[0])
        steps += 1
        info = infos[0] if infos else {}
        last_info = info
        if info.get("dead"):
            dead = True
            truncated = True
            break
        if bool(dones[0]):
            # Monitor may put terminal in info
            terminated = bool(info.get("done", False)) or bool(
                info.get("terminal_observation") is not None and info.get("TimeLimit.truncated") is not True
            )
            # Prefer explicit flags from our env
            if "done" in info:
                terminated = bool(info["done"])
            if "truncated" in info:
                truncated = bool(info["truncated"])
            # Monitor episode stats
            if "episode" in info:
                # success if we got finish (done) not timeout
                pass
            break
        # safety: absurd step count
        if steps > int(max_episode_secs * act_hz * 4) + 100:
            truncated = True
            break

    wall = time.time() - t0
    # Orbs / score from last info
    score = last_info.get("score")
    nb_orbs = last_info.get("nb_orbs", num_orbs)
    # Monitor wraps terminal infos sometimes under episode
    ep = last_info.get("episode") or {}

    # Success: sim set done=true (finish). Timeout → truncated without success.
    success = bool(last_info.get("done", False)) and not bool(last_info.get("truncated", False))
    if dead:
        success = False

    # Orbs collected: prefer score if it tracks orbs; else nb from obs not available
    orbs = score if isinstance(score, (int, float)) else None
    # Fallback: success implies all orbs
    if orbs is None and success:
        orbs = num_orbs

    result = {
        "ts": _iso_now(),
        "policy": policy_mode,
        "route_mode": route_mode,
        "num_orbs": num_orbs,
        "seed": seed,
        "success": success,
        "orbs": orbs,
        "nb_orbs": nb_orbs,
        "steps": steps,
        "ep_rew": float(total_rew),
        "wall_secs": round(wall, 3),
        "dead": dead,
        "terminated": terminated,
        "truncated": truncated,
        "deterministic": deterministic,
        "ep_info": {k: ep[k] for k in ("r", "l", "t") if k in ep} if ep else {},
    }
    try:
        venv.close()
    except Exception:
        pass
    return result


def summarize(rows: list[dict[str, Any]], gate_rate: float = 0.9) -> dict[str, Any]:
    by: dict[str, list[dict]] = {}
    for r in rows:
        by.setdefault(r["route_mode"], []).append(r)

    groups = {}
    for mode, rs in by.items():
        n = len(rs)
        succ = sum(1 for r in rs if r.get("success"))
        orbs = [r["orbs"] for r in rs if isinstance(r.get("orbs"), (int, float))]
        walls = [r["wall_secs"] for r in rs if isinstance(r.get("wall_secs"), (int, float))]
        rate = succ / n if n else 0.0
        groups[mode] = {
            "n": n,
            "successes": succ,
            "success_rate": round(rate, 4),
            "gate_pass": rate >= gate_rate if n else False,
            "median_orbs": float(np.median(orbs)) if orbs else None,
            "mean_orbs": float(np.mean(orbs)) if orbs else None,
            "median_wall_s": float(np.median(walls)) if walls else None,
        }

    all_n = len(rows)
    all_succ = sum(1 for r in rows if r.get("success"))
    modes_ok = all(g.get("gate_pass") for g in groups.values()) if groups else False
    # Gate requires every listed route to pass
    return {
        "episodes": all_n,
        "successes": all_succ,
        "overall_success_rate": round(all_succ / all_n, 4) if all_n else 0.0,
        "by_route": groups,
        "phase1_gate_pass": modes_ok and len(groups) >= 1,
        "gate_threshold": gate_rate,
    }


def main() -> None:
    p = argparse.ArgumentParser(description="Frozen residual SAC / BC eval matrix")
    p.add_argument("--sim-bin", type=Path, default=Path("target/release/ssol_simulator"))
    p.add_argument("--sac-model", type=Path, default=None)
    p.add_argument("--vecnormalize", type=Path, default=None)
    p.add_argument("--bc-policy", type=Path, default=None)
    p.add_argument(
        "--policy",
        choices=("sac", "bc", "zero"),
        default="sac",
        help="sac=residual SAC+BC, bc=BC only (zero residual), zero=zero actions",
    )
    p.add_argument("--num-orbs", type=int, default=7)
    p.add_argument(
        "--routes",
        nargs="+",
        default=["wr", "greedy"],
        help="route modes to eval (default: wr greedy)",
    )
    p.add_argument(
        "--seeds",
        default="0-19",
        help="seed list: '0-19' or '0,1,2' (default 0-19 = 20 seeds)",
    )
    p.add_argument("--max-episode-secs", type=float, default=60.0)
    p.add_argument("--act-hz", type=float, default=10.0)
    p.add_argument("--speed", type=float, default=200.0)
    p.add_argument("--stochastic", action="store_true", help="SAC sample actions (default: deterministic)")
    p.add_argument("--out", type=Path, default=Path("data/eval_phase1"))
    p.add_argument("--gate-rate", type=float, default=0.9)
    p.add_argument(
        "--early-fail-after",
        type=int,
        default=0,
        help="If >0, after N episodes per route if success_rate < early-fail-rate, stop that route",
    )
    p.add_argument(
        "--early-fail-rate",
        type=float,
        default=0.2,
        help="With --early-fail-after: abort route if rate below this (default 0.2)",
    )
    args = p.parse_args()

    if not args.sim_bin.is_file():
        raise SystemExit(f"sim binary not found: {args.sim_bin}")
    if args.policy == "sac" and (not args.sac_model or not args.sac_model.is_file()):
        raise SystemExit(f"--sac-model required and must exist for policy=sac")
    if args.policy in ("sac", "bc") and args.bc_policy and not args.bc_policy.is_file():
        raise SystemExit(f"--bc-policy not found: {args.bc_policy}")
    if args.policy == "sac" and not args.bc_policy:
        print(
            f"{_iso_now()} WARN: policy=sac without --bc-policy (pure SAC actions, not residual)",
            flush=True,
        )

    seeds = _parse_seed_spec(args.seeds)
    if not seeds:
        raise SystemExit("no seeds parsed from --seeds")

    args.out.mkdir(parents=True, exist_ok=True)
    jsonl_path = args.out / "episodes.jsonl"
    summary_path = args.out / "summary.json"
    # Fresh run append marker
    with open(args.out / "run_meta.json", "w") as f:
        json.dump(
            {
                "started": _iso_now(),
                "policy": args.policy,
                "sac_model": str(args.sac_model) if args.sac_model else None,
                "vecnormalize": str(args.vecnormalize) if args.vecnormalize else None,
                "bc_policy": str(args.bc_policy) if args.bc_policy else None,
                "num_orbs": args.num_orbs,
                "routes": args.routes,
                "seeds": seeds,
                "speed": args.speed,
                "max_episode_secs": args.max_episode_secs,
                "gate_rate": args.gate_rate,
            },
            f,
            indent=2,
        )

    print(
        f"{_iso_now()} phase1_eval start policy={args.policy} orbs={args.num_orbs} "
        f"routes={args.routes} seeds={seeds[0]}..{seeds[-1]} (n={len(seeds)}) "
        f"speed={args.speed} out={args.out}",
        flush=True,
    )

    rows: list[dict[str, Any]] = []
    # Truncate jsonl for this run
    jsonl_path.write_text("")

    for route in args.routes:
        route_rows: list[dict[str, Any]] = []
        print(f"{_iso_now()} === route={route} ===", flush=True)
        for i, seed in enumerate(seeds):
            try:
                r = run_episode(
                    sim_bin=args.sim_bin,
                    sac_model=args.sac_model,
                    vecnormalize=args.vecnormalize,
                    bc_policy=args.bc_policy,
                    policy_mode=args.policy,
                    num_orbs=args.num_orbs,
                    route_mode=route,
                    seed=seed,
                    max_episode_secs=args.max_episode_secs,
                    act_hz=args.act_hz,
                    speed=args.speed,
                    deterministic=not args.stochastic,
                )
            except Exception as e:
                r = {
                    "ts": _iso_now(),
                    "policy": args.policy,
                    "route_mode": route,
                    "num_orbs": args.num_orbs,
                    "seed": seed,
                    "success": False,
                    "orbs": None,
                    "steps": 0,
                    "ep_rew": 0.0,
                    "wall_secs": 0.0,
                    "dead": True,
                    "error": str(e),
                }
                print(f"{_iso_now()} ERROR seed={seed} route={route}: {e}", flush=True)

            rows.append(r)
            route_rows.append(r)
            with open(jsonl_path, "a") as f:
                f.write(json.dumps(r) + "\n")

            succ_n = sum(1 for x in route_rows if x.get("success"))
            rate = succ_n / len(route_rows)
            print(
                f"{_iso_now()} ep route={route} seed={seed} "
                f"success={r.get('success')} orbs={r.get('orbs')} steps={r.get('steps')} "
                f"rew={r.get('ep_rew'):.2f} wall={r.get('wall_secs')}s "
                f"| running {route}: {succ_n}/{len(route_rows)} ({100*rate:.0f}%)",
                flush=True,
            )

            # Early abort if clearly hopeless (optional)
            if (
                args.early_fail_after > 0
                and len(route_rows) >= args.early_fail_after
                and rate < args.early_fail_rate
            ):
                print(
                    f"{_iso_now()} EARLY-FAIL route={route} "
                    f"rate={rate:.2f} < {args.early_fail_rate} after {len(route_rows)} eps — skipping rest of route",
                    flush=True,
                )
                break

        mid = summarize(route_rows, gate_rate=args.gate_rate)
        print(
            f"{_iso_now()} route={route} done: {json.dumps(mid['by_route'].get(route, mid))}",
            flush=True,
        )

    summary = summarize(rows, gate_rate=args.gate_rate)
    summary["finished"] = _iso_now()
    summary["out"] = str(args.out)
    with open(summary_path, "w") as f:
        json.dump(summary, f, indent=2)

    print(f"{_iso_now()} === EVAL SUMMARY ===", flush=True)
    print(json.dumps(summary, indent=2), flush=True)
    gate = summary.get("phase1_gate_pass")
    print(
        f"{_iso_now()} phase1_gate_pass={gate} (need ≥{args.gate_rate:.0%} on every route)",
        flush=True,
    )
    # Non-zero exit if gate fails (CI-friendly)
    raise SystemExit(0 if gate else 2)


if __name__ == "__main__":
    main()
