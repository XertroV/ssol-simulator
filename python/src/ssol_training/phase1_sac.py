"""Phase 1 residual SAC against live SSOL via --train-stdio.

Env protocol:
  Sim prints: TRAIN_STEP_JSON {"obs":[39], "reward":f, "done":bool, "truncated":bool, ...}
  Agent writes: {"action":[mx, my, yaw_rate]}\\n

Residual: a = clip(a_bc(s) + a_res(s), bounds) when --bc-policy is set;
otherwise pure SAC on actions.

Example:
  cargo build --release
  uv run python -m ssol_training.phase1_sac \\
    --sim-bin ../target/release/ssol_simulator \\
    --bc-policy data/bc_policy.pt \\
    --num-orbs 3 --timesteps 50000
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional, TextIO

import numpy as np

# Always unbuffered for live ETA / ladder monitoring (also set PYTHONUNBUFFERED=1 in shells).
os.environ.setdefault("PYTHONUNBUFFERED", "1")
try:
    sys.stdout.reconfigure(line_buffering=True)  # type: ignore[attr-defined]
    sys.stderr.reconfigure(line_buffering=True)  # type: ignore[attr-defined]
except Exception:
    pass

OBS_DIM = 39
ACT_DIM = 3
YAW_SCALE = 2.5


def _iso_now() -> str:
    return datetime.now().astimezone().isoformat(timespec="seconds")


def _make_timestamped_logger(stdout: TextIO | None = None):
    """SB3 logger that stamps each metric dump with local datetime (flushed)."""
    from stable_baselines3.common.logger import HumanOutputFormat, Logger

    out = stdout or sys.stdout

    class TimestampedHumanOutputFormat(HumanOutputFormat):
        def write(
            self,
            key_values: dict[str, Any],
            key_excluded: dict[str, tuple[str, ...]],
            step: int = 0,
        ) -> None:
            self.file.write(f"=== {_iso_now()} timesteps={step} ===\n")
            super().write(key_values, key_excluded, step=step)
            self.file.flush()

    return Logger(folder=None, output_formats=[TimestampedHumanOutputFormat(out)])


def _make_heartbeat_callback(every: int = 5_000):
    """Log a one-liner every N env steps (SB3 metric dumps are episode-gated and can go quiet)."""
    from stable_baselines3.common.callbacks import BaseCallback

    class HeartbeatCallback(BaseCallback):
        def __init__(self, every_steps: int):
            super().__init__()
            self.every_steps = max(1, int(every_steps))
            self._last = 0
            self._t0 = None

        def _on_training_start(self) -> None:
            import time

            self._t0 = time.time()
            self._last = 0

        def _on_step(self) -> bool:
            import time

            ts = int(self.num_timesteps)
            if ts - self._last < self.every_steps:
                return True
            self._last = ts
            elapsed = max(1e-6, time.time() - (self._t0 or time.time()))
            fps = ts / elapsed
            # Prefer SB3 ep stats when present
            ep_rew = None
            ep_len = None
            if self.model is not None and getattr(self.model, "ep_info_buffer", None):
                buf = list(self.model.ep_info_buffer)
                if buf:
                    rews = [float(x["r"]) for x in buf if "r" in x]
                    lens = [float(x["l"]) for x in buf if "l" in x]
                    if rews:
                        ep_rew = sum(rews) / len(rews)
                    if lens:
                        ep_len = sum(lens) / len(lens)
            extra = ""
            if ep_rew is not None:
                extra += f" ep_rew_mean={ep_rew:.3g}"
            if ep_len is not None:
                extra += f" ep_len_mean={ep_len:.0f}"
            print(
                f"{_iso_now()} heartbeat timesteps={ts} fps≈{fps:.1f}{extra}",
                flush=True,
            )
            return True

    return HeartbeatCallback(every)


class SSOLStdioEnv:
    """Minimal Gymnasium-like env (reset/step) over sim stdio."""

    metadata = {"render_modes": []}

    def __init__(
        self,
        sim_bin: Path,
        num_orbs: int = 3,
        route_mode: str = "mix",
        seed: int = 0,
        max_episode_secs: float = 60.0,
        act_hz: float = 10.0,
        speed: float = 50.0,
    ):
        import gymnasium as gym
        from gymnasium import spaces

        self._gym = gym
        self.sim_bin = Path(sim_bin).resolve()
        # Assets load from CWD — run sim from repo root (parent of target/).
        self.cwd = self.sim_bin.parent.parent if self.sim_bin.parent.name == "release" else self.sim_bin.parent
        if not (self.cwd / "assets" / "scenes" / "level-zero.json").is_file():
            # Fallback: walk up from bin
            for parent in self.sim_bin.parents:
                if (parent / "assets" / "scenes" / "level-zero.json").is_file():
                    self.cwd = parent
                    break
        self.num_orbs = num_orbs
        self.route_mode = route_mode
        self.seed0 = seed
        self.max_episode_secs = max_episode_secs
        self.act_hz = act_hz
        self.speed = speed
        self.observation_space = spaces.Box(
            low=-np.inf, high=np.inf, shape=(OBS_DIM,), dtype=np.float32
        )
        # Normalized action space for SAC: [-1,1]^3 then scale yaw
        self.action_space = spaces.Box(
            low=-1.0, high=1.0, shape=(ACT_DIM,), dtype=np.float32
        )
        self._proc: Optional[subprocess.Popen] = None
        self._episode = 0

    def _scale_action(self, a: np.ndarray) -> np.ndarray:
        a = np.asarray(a, dtype=np.float32).reshape(3)
        out = a.copy()
        out[0] = float(np.clip(a[0], -1, 1))
        out[1] = float(np.clip(a[1], -1, 1))
        out[2] = float(np.clip(a[2], -1, 1) * YAW_SCALE)
        return out

    def reset(self, *, seed: Optional[int] = None, options: Optional[dict] = None):
        self.close()
        s = self.seed0 if seed is None else int(seed)
        cmd = [
            str(self.sim_bin),
            "--headless",
            "--no-audio",
            f"--speed={self.speed}",
            "--train-stdio",
            f"--num-orbs={self.num_orbs}",
            f"--route-mode={self.route_mode}",
            f"--seed={s}",
            f"--act-hz={self.act_hz}",
            f"--max-episode-secs={self.max_episode_secs}",
            "--num-episodes=1",
        ]
        self._proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
            cwd=str(self.cwd),
        )
        self._episode += 1
        obs, info = self._read_step(expect_first=True)
        return obs, info

    def _read_step(self, expect_first: bool = False) -> tuple[np.ndarray, dict]:
        assert self._proc and self._proc.stdout
        while True:
            line = self._proc.stdout.readline()
            if not line:
                # Process died
                obs = np.zeros(OBS_DIM, dtype=np.float32)
                return obs, {"dead": True, "done": True}
            line = line.strip()
            if not line.startswith("TRAIN_STEP_JSON "):
                continue
            payload = json.loads(line[len("TRAIN_STEP_JSON ") :])
            obs = np.asarray(payload["obs"], dtype=np.float32)
            if obs.shape[0] != OBS_DIM:
                raise RuntimeError(f"obs dim {obs.shape[0]} != {OBS_DIM}")
            info = {
                "reward": float(payload.get("reward", 0.0)),
                "done": bool(payload.get("done", False)),
                "truncated": bool(payload.get("truncated", False)),
                "score": payload.get("score"),
                "nb_orbs": payload.get("nb_orbs"),
            }
            return obs, info

    def step(self, action: np.ndarray):
        if self._proc is None or self._proc.poll() is not None:
            obs = np.zeros(OBS_DIM, dtype=np.float32)
            return obs, 0.0, False, True, {"dead": True}
        assert self._proc.stdin is not None
        a = self._scale_action(action)
        try:
            self._proc.stdin.write(json.dumps({"action": a.tolist()}) + "\n")
            self._proc.stdin.flush()
        except BrokenPipeError:
            obs = np.zeros(OBS_DIM, dtype=np.float32)
            self.close()
            return obs, 0.0, False, True, {"dead": True}
        obs, info = self._read_step()
        reward = float(info.get("reward", 0.0))
        terminated = bool(info.get("done", False))
        truncated = bool(info.get("truncated", False))
        if info.get("dead"):
            truncated = True
            terminated = False
        return obs, reward, terminated, truncated, info

    def close(self):
        if self._proc is not None:
            try:
                if self._proc.stdin:
                    self._proc.stdin.close()
            except Exception:
                pass
            try:
                self._proc.kill()
                self._proc.wait(timeout=2)
            except Exception:
                pass
            self._proc = None

class ResidualActionWrapper:
    """a = clip(a_bc + residual, bounds) with residual in [-1,1]^3 scaled."""

    def __init__(self, env: SSOLStdioEnv, bc_path: Optional[Path]):
        self.env = env
        self.observation_space = env.observation_space
        self.action_space = env.action_space
        self._bc = None
        self._mean = None
        self._std = None
        if bc_path and Path(bc_path).is_file():
            import torch

            ckpt = torch.load(bc_path, map_location="cpu", weights_only=False)
            self._mean = ckpt["obs_mean"]
            self._std = ckpt["obs_std"]

            class Actor(torch.nn.Module):
                def __init__(self):
                    super().__init__()
                    self.net = torch.nn.Sequential(
                        torch.nn.Linear(OBS_DIM, 256),
                        torch.nn.SiLU(),
                        torch.nn.Linear(256, 256),
                        torch.nn.SiLU(),
                        torch.nn.Linear(256, ACT_DIM),
                    )

                def forward(self, x):
                    raw = self.net(x)
                    move = torch.tanh(raw[:, :2])
                    yaw = torch.tanh(raw[:, 2:3]) * YAW_SCALE
                    return torch.cat([move, yaw], dim=-1)

            m = Actor()
            m.load_state_dict(ckpt["state_dict"])
            m.eval()
            self._bc = m
            print(f"Loaded BC teacher from {bc_path}")

    def reset(self, **kwargs):
        return self.env.reset(**kwargs)

    def step(self, action):
        # action is residual in [-1,1]^3 from SAC
        res = np.asarray(action, dtype=np.float32).reshape(3)
        if self._bc is not None:
            import torch

            obs = getattr(self, "_last_obs", None)
            if obs is None:
                base = np.zeros(3, dtype=np.float32)
            else:
                x = (obs - self._mean) / self._std
                with torch.no_grad():
                    base = self._bc(torch.from_numpy(x[None].astype(np.float32))).numpy()[0]
            # residual scales: move ±0.5, yaw ±1.0
            delta = np.array(
                [res[0] * 0.5, res[1] * 0.5, res[2] * 1.0], dtype=np.float32
            )
            full = base + delta
            full[0] = np.clip(full[0], -1, 1)
            full[1] = np.clip(full[1], -1, 1)
            full[2] = np.clip(full[2], -YAW_SCALE, YAW_SCALE)
            # convert yaw back to env's [-1,1] * YAW_SCALE space
            env_a = np.array(
                [full[0], full[1], full[2] / YAW_SCALE], dtype=np.float32
            )
        else:
            env_a = res
        obs, rew, term, trunc, info = self.env.step(env_a)
        self._last_obs = obs
        return obs, rew, term, trunc, info

    def close(self):
        self.env.close()


def _gym_wrap(inner):
    """Picklable Gymnasium Env wrapper for SubprocVecEnv."""
    import gymnasium as gym
    from gymnasium import spaces

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

    return GymWrap()


def _make_env_kwargs(
    sim_bin: Path,
    num_orbs: int,
    route_mode: str,
    seed: int,
    max_episode_secs: float,
    act_hz: float,
    speed: float,
    bc_policy: Optional[Path],
    rank: int,
):
    """Top-level factory args (picklable) for SubprocVecEnv."""
    return {
        "sim_bin": str(sim_bin),
        "num_orbs": num_orbs,
        "route_mode": route_mode,
        "seed": seed + rank,
        "max_episode_secs": max_episode_secs,
        "act_hz": act_hz,
        "speed": speed,
        "bc_policy": str(bc_policy) if bc_policy else None,
    }


def make_env_from_kwargs(kwargs: dict):
    """Must be top-level for multiprocessing pickle."""
    from stable_baselines3.common.monitor import Monitor

    env = SSOLStdioEnv(
        sim_bin=Path(kwargs["sim_bin"]),
        num_orbs=int(kwargs["num_orbs"]),
        route_mode=str(kwargs["route_mode"]),
        seed=int(kwargs["seed"]),
        max_episode_secs=float(kwargs["max_episode_secs"]),
        act_hz=float(kwargs["act_hz"]),
        speed=float(kwargs["speed"]),
    )
    bc = kwargs.get("bc_policy")
    if bc:
        env = ResidualActionWrapper(env, Path(bc))
    return Monitor(_gym_wrap(env))


def make_env(args, rank: int = 0):
    kw = _make_env_kwargs(
        args.sim_bin,
        args.num_orbs,
        args.route_mode,
        args.seed,
        args.max_episode_secs,
        args.act_hz,
        args.speed,
        args.bc_policy,
        rank,
    )

    def _thunk():
        return make_env_from_kwargs(kw)

    return _thunk


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "--sim-bin",
        type=Path,
        default=Path("target/release/ssol_simulator"),
    )
    p.add_argument("--bc-policy", type=Path, default=None)
    p.add_argument("--num-orbs", type=int, default=3)
    p.add_argument("--route-mode", default="mix")
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--max-episode-secs", type=float, default=60.0)
    p.add_argument("--act-hz", type=float, default=10.0)
    p.add_argument("--speed", type=float, default=80.0)
    p.add_argument("--timesteps", type=int, default=50_000)
    p.add_argument("--n-envs", type=int, default=1)
    p.add_argument("--out", type=Path, default=Path("data/sac_residual"))
    p.add_argument(
        "--load-model",
        type=Path,
        default=None,
        help="Continue from SAC zip (fine-tune / corrective training)",
    )
    p.add_argument(
        "--load-vecnormalize",
        type=Path,
        default=None,
        help="VecNormalize stats to resume (use with --load-model)",
    )
    p.add_argument(
        "--learning-rate",
        type=float,
        default=3e-4,
        help="SAC lr (lower e.g. 1e-4 for fine-tune)",
    )
    args = p.parse_args()

    if not args.sim_bin.is_file():
        raise SystemExit(f"sim binary not found: {args.sim_bin} (cargo build --release)")

    from stable_baselines3 import SAC
    from stable_baselines3.common.vec_env import DummyVecEnv, SubprocVecEnv, VecNormalize

    env_fns = [make_env(args, rank=i) for i in range(args.n_envs)]
    if args.n_envs > 1:
        # True parallelism: one Bevy process per env (each may use multiple threads).
        venv = SubprocVecEnv(env_fns, start_method="spawn")
    else:
        venv = DummyVecEnv(env_fns)

    if args.load_vecnormalize and args.load_vecnormalize.is_file():
        venv = VecNormalize.load(str(args.load_vecnormalize), venv)
        venv.training = True
        venv.norm_reward = True
        print(f"{_iso_now()} loaded VecNormalize {args.load_vecnormalize}", flush=True)
    else:
        venv = VecNormalize(venv, norm_obs=True, norm_reward=True, clip_obs=10.0)
        if args.load_model:
            print(
                f"{_iso_now()} WARN: --load-model without --load-vecnormalize "
                f"(fresh obs stats)",
                flush=True,
            )

    if args.load_model and args.load_model.is_file():
        model = SAC.load(str(args.load_model), env=venv, device="auto")
        # Optional lr override for fine-tune
        if args.learning_rate is not None:
            model.learning_rate = args.learning_rate
            try:
                # SB3 schedule may be constant float or callable
                model.lr_schedule = lambda _: args.learning_rate
            except Exception:
                pass
        print(
            f"{_iso_now()} loaded SAC {args.load_model} (continue / fine-tune)",
            flush=True,
        )
    else:
        if args.load_model:
            raise SystemExit(f"--load-model not found: {args.load_model}")
        policy_kwargs = dict(net_arch=dict(pi=[256, 256], qf=[256, 256]))
        model = SAC(
            "MlpPolicy",
            venv,
            learning_rate=args.learning_rate,
            buffer_size=1_000_000,
            batch_size=256,
            gamma=0.99,
            tau=0.005,
            ent_coef="auto",
            train_freq=1,
            gradient_steps=args.n_envs,  # keep update/sample ratio ~1 with multi-env
            policy_kwargs=policy_kwargs,
            verbose=1,
            seed=args.seed,
            device="auto",
        )
    # Timestamped dumps (SB3 default blocks lack wall-clock; needed for ladder ETA).
    model.set_logger(_make_timestamped_logger())
    args.out.mkdir(parents=True, exist_ok=True)
    print(
        f"{_iso_now()} Training residual SAC timesteps={args.timesteps} orbs={args.num_orbs} "
        f"route={args.route_mode} bc={args.bc_policy} n_envs={args.n_envs} "
        f"lr={args.learning_rate} load={args.load_model} device=auto",
        flush=True,
    )
    # log_interval=1: dump metrics every episode; heartbeat covers long episode gaps.
    # reset_num_timesteps=False keeps counters when fine-tuning from a checkpoint.
    model.learn(
        total_timesteps=args.timesteps,
        progress_bar=False,
        log_interval=1,
        callback=_make_heartbeat_callback(every=5_000),
        reset_num_timesteps=args.load_model is None,
    )
    model.save(str(args.out / "sac_model"))
    venv.save(str(args.out / "vecnormalize.pkl"))
    print(f"{_iso_now()} saved → {args.out}", flush=True)
    venv.close()


if __name__ == "__main__":
    main()
