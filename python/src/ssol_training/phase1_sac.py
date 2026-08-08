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
import subprocess
import sys
from pathlib import Path
from typing import Any, Optional

import numpy as np

OBS_DIM = 39
ACT_DIM = 3
YAW_SCALE = 2.5


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


def make_env(args, rank: int = 0):
    def _thunk():
        env = SSOLStdioEnv(
            sim_bin=args.sim_bin,
            num_orbs=args.num_orbs,
            route_mode=args.route_mode,
            seed=args.seed + rank,
            max_episode_secs=args.max_episode_secs,
            act_hz=args.act_hz,
            speed=args.speed,
        )
        if args.bc_policy:
            env = ResidualActionWrapper(env, Path(args.bc_policy))
        # Wrap for SB3
        import gymnasium as gym
        from gymnasium import spaces

        class GymWrap(gym.Env):
            metadata = {"render_modes": []}

            def __init__(self, inner):
                super().__init__()
                self.inner = inner
                self.observation_space = spaces.Box(
                    -np.inf, np.inf, (OBS_DIM,), np.float32
                )
                self.action_space = spaces.Box(-1, 1, (ACT_DIM,), np.float32)

            def reset(self, *, seed=None, options=None):
                obs, info = self.inner.reset(seed=seed, options=options)
                if hasattr(self.inner, "_last_obs"):
                    self.inner._last_obs = obs
                return obs, info

            def step(self, action):
                return self.inner.step(action)

            def close(self):
                self.inner.close()

        return GymWrap(env)

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
    args = p.parse_args()

    if not args.sim_bin.is_file():
        raise SystemExit(f"sim binary not found: {args.sim_bin} (cargo build --release)")

    from stable_baselines3 import SAC
    from stable_baselines3.common.monitor import Monitor
    from stable_baselines3.common.vec_env import DummyVecEnv, VecNormalize

    env_fns = [make_env(args, rank=i) for i in range(args.n_envs)]
    venv = DummyVecEnv([lambda fn=fn: Monitor(fn()) for fn in env_fns])
    venv = VecNormalize(venv, norm_obs=True, norm_reward=True, clip_obs=10.0)

    policy_kwargs = dict(net_arch=[256, 256])
    model = SAC(
        "MlpPolicy",
        venv,
        learning_rate=3e-4,
        buffer_size=200_000,
        batch_size=256,
        gamma=0.99,
        tau=0.005,
        ent_coef="auto",
        policy_kwargs=policy_kwargs,
        verbose=1,
        seed=args.seed,
        device="auto",
    )
    args.out.mkdir(parents=True, exist_ok=True)
    print(
        f"Training residual SAC timesteps={args.timesteps} orbs={args.num_orbs} "
        f"route={args.route_mode} bc={args.bc_policy}"
    )
    model.learn(total_timesteps=args.timesteps, progress_bar=False)
    model.save(args.out / "sac_model")
    venv.save(args.out / "vecnormalize.pkl")
    print(f"saved → {args.out}")
    venv.close()


if __name__ == "__main__":
    main()
