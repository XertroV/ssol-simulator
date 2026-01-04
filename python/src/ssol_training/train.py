"""
Training script for SSOL RL Agent using RecurrentPPO.

Handles:
- Launching multiple game instances in parallel
- Setting up vectorized environments
- Training with LSTM policy
- Checkpointing and logging
"""

import argparse
import subprocess
import time
import sys
import signal
import os
import threading
from pathlib import Path
from typing import Callable, Optional

import torch

# Optimize PyTorch for inference speed
# Use all available threads for CPU inference
torch.set_num_threads(os.cpu_count() or 4)
# Enable optimized inference mode by default
torch.set_grad_enabled(True)  # Will be disabled during inference by SB3
# Use fast math for better performance (may have minor precision differences)
if hasattr(torch, 'set_float32_matmul_precision'):
    torch.set_float32_matmul_precision('high')

from stable_baselines3.common.callbacks import (
    BaseCallback,
    CheckpointCallback,
    CallbackList,
)
from stable_baselines3.common.vec_env import DummyVecEnv, SubprocVecEnv
from stable_baselines3.common.monitor import Monitor

try:
    from sb3_contrib import RecurrentPPO
    HAS_RECURRENT = True
except ImportError:
    HAS_RECURRENT = False
    print("Warning: sb3-contrib not installed, RecurrentPPO unavailable")

from .ssol_env import SSOLEnv
from .feature_extractor import SSOLFeatureExtractor


class GameInstanceManager:
    """Manages launching and cleanup of Rust game instances."""

    def __init__(self, executable_path: str = "../target/release/ssol_simulator"):
        self.executable_path = executable_path
        self.processes: list[subprocess.Popen] = []
        # Determine the project root (where assets folder is)
        # If executable_path contains target/release, work directory is parent of target
        exe_path = Path(executable_path).resolve()
        if "target" in exe_path.parts:
            # Find the target directory and get its parent
            # Walk up from exe until we find target, then go one more up
            current = exe_path
            while current.name != "target" and current.parent != current:
                current = current.parent
            self.work_dir = current.parent
        else:
            self.work_dir = Path.cwd()

        print(f"Game working directory: {self.work_dir}")

    def launch(
        self,
        port: int,
        headless: bool = True,
        speed: float = 1.0,
        instance_name: Optional[str] = None,
    ) -> subprocess.Popen:
        """
        Launch a single game instance.

        Args:
            port: ZMQ port for this instance
            headless: Run without window
            speed: Simulation speed multiplier
            instance_name: Optional name for this instance (used in logs)

        Returns:
            The subprocess handle
        """
        # Resolve executable to absolute path
        exe_path = Path(self.executable_path).resolve()

        # Use port as instance name if not provided
        name = instance_name or f"env{port}"

        cmd = [
            str(exe_path),
            "--ai-mode",
            f"--zmq-port={port}",
            f"--speed={speed}",
            f"--instance-name={name}",
        ]
        if headless:
            cmd.append("--headless")

        # Set up environment with BEVY_ASSET_ROOT pointing to project root
        env = os.environ.copy()
        env["BEVY_ASSET_ROOT"] = str(self.work_dir)

        print(f"Launching: {' '.join(cmd)}")
        print(f"  Working dir: {self.work_dir}")
        print(f"  BEVY_ASSET_ROOT: {env['BEVY_ASSET_ROOT']}")

        # Run from project root so assets can be found
        # Suppress only stdout in headless mode (keep stderr for error visibility)
        if headless:
            proc = subprocess.Popen(
                cmd,
                stdout=subprocess.DEVNULL,
                stderr=None,  # Keep stderr visible for debugging
                cwd=str(self.work_dir),
                env=env,
            )
        else:
            proc = subprocess.Popen(cmd, cwd=str(self.work_dir), env=env)

        self.processes.append(proc)
        return proc

    def launch_many(
        self,
        base_port: int,
        count: int,
        headless: bool = True,
        speed: float = 1.0,
        stagger_delay: float = 0.5,
        first_visible: bool = True,
    ) -> list[subprocess.Popen]:
        """
        Launch multiple game instances.

        Args:
            base_port: Starting port number
            count: Number of instances (each uses 2 ports: base_port+i*2 for REQ, base_port+i*2+1 for PUSH)
            headless: Run without windows
            speed: Simulation speed multiplier
            stagger_delay: Delay between launches (seconds)
            first_visible: If True and headless=True, first instance is visible

        Returns:
            List of subprocess handles
        """
        procs = []
        for i in range(count):
            # First instance visible if first_visible=True, rest always headless
            if first_visible and i == 0:
                instance_headless = False
            else:
                instance_headless = True
            instance_name = f"env{i}"
            # Each game uses 2 ports: base_port+i*2 for REQ, base_port+i*2+1 for PUSH
            proc = self.launch(base_port + i * 2, instance_headless, speed, instance_name)
            procs.append(proc)
            if i < count - 1:
                time.sleep(stagger_delay)
        return procs

    def shutdown_all(self) -> None:
        """Terminate all managed game instances."""
        for proc in self.processes:
            try:
                proc.terminate()
                proc.wait(timeout=2)  # Shorter timeout for faster shutdown
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=1)
            except Exception:
                pass
        self.processes.clear()


def make_env(
    port: int,
    rank: int,
    max_episode_steps: int = 3750,
    max_orbs: Optional[int] = None,
    enable_prefetch: bool = True,
) -> Callable[[], SSOLEnv]:
    """
    Factory function for creating SSOL environments.

    Args:
        port: Base ZMQ port
        rank: Environment rank (each env uses 2 ports: port+rank*2 for REQ, port+rank*2+1 for PULL)
        max_episode_steps: Maximum steps per episode
        max_orbs: Curriculum setting for max orbs (None = game default)
        enable_prefetch: Enable observation prefetching via PULL socket

    Returns:
        Function that creates the environment
    """
    def _init() -> SSOLEnv:
        # Each env uses 2 ports: port+rank*2 for REQ, port+rank*2+1 for PULL
        env = SSOLEnv(
            zmq_address=f"tcp://127.0.0.1:{port + rank * 2}",
            max_episode_steps=max_episode_steps,
            enable_prefetch=enable_prefetch,
        )
        # Set curriculum before first reset if specified
        # Retry a few times in case game isn't ready yet
        if max_orbs is not None:
            for attempt in range(5):
                try:
                    env.set_curriculum(max_orbs=max_orbs)
                    break
                except ConnectionError as e:
                    if attempt < 4:
                        time.sleep(2)
                    else:
                        raise e
        env = Monitor(env)
        return env
    return _init


# Global flag for interrupt handling
_interrupt_requested = False


class InterruptCallback(BaseCallback):
    """Callback that checks for interrupt signal and stops training gracefully."""

    def __init__(self, verbose: int = 0):
        super().__init__(verbose)

    def _on_step(self) -> bool:
        global _interrupt_requested
        if _interrupt_requested:
            print("\n[Interrupt] Stopping training gracefully...")
            return False  # Stop training
        return True


class TensorboardCallback(BaseCallback):
    """Custom callback for additional tensorboard logging."""

    def __init__(self, verbose: int = 0):
        super().__init__(verbose)
        self.episode_rewards: list[float] = []
        self.episode_lengths: list[int] = []

    def _on_step(self) -> bool:
        # Log custom metrics if available in info
        for info in self.locals.get("infos", []):
            if "episode" in info:
                self.episode_rewards.append(info["episode"]["r"])
                self.episode_lengths.append(info["episode"]["l"])

                if len(self.episode_rewards) % 10 == 0:
                    avg_reward = sum(self.episode_rewards[-10:]) / 10
                    avg_length = sum(self.episode_lengths[-10:]) / 10
                    self.logger.record("custom/avg_episode_reward_10", avg_reward)
                    self.logger.record("custom/avg_episode_length_10", avg_length)

        return True


def _signal_handler(signum, frame):
    """Handle Ctrl+C by setting interrupt flag."""
    global _interrupt_requested
    if _interrupt_requested:
        # Second Ctrl+C - force exit
        print("\n[Interrupt] Forcing exit...")
        sys.exit(1)
    print("\n[Interrupt] Ctrl+C received, finishing current step...")
    _interrupt_requested = True


def main():
    # Set up signal handler early
    signal.signal(signal.SIGINT, _signal_handler)
    if hasattr(signal, 'SIGTERM'):
        signal.signal(signal.SIGTERM, _signal_handler)

    parser = argparse.ArgumentParser(
        description="Train SSOL RL Agent",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )

    # Environment settings
    parser.add_argument(
        "--num-envs", type=int, default=8,
        help="Number of parallel environments",
    )
    parser.add_argument(
        "--base-port", type=int, default=5555,
        help="Base ZMQ port (each env uses 2 ports: base_port+rank*2 for REQ, base_port+rank*2+1 for PUSH)",
    )
    parser.add_argument(
        "--max-episode-steps", type=int, default=3750,
        help="Maximum steps per episode (3750 = 150 seconds at 25Hz)",
    )

    # Training settings
    parser.add_argument(
        "--timesteps", type=int, default=10_000_000,
        help="Total training timesteps",
    )
    parser.add_argument(
        "--learning-rate", type=float, default=3e-4,
        help="Learning rate",
    )
    parser.add_argument(
        "--n-steps", type=int, default=2048,
        help="Steps per rollout per environment",
    )
    parser.add_argument(
        "--batch-size", type=int, default=64,
        help="Minibatch size",
    )
    parser.add_argument(
        "--n-epochs", type=int, default=10,
        help="Number of training epochs per update",
    )
    parser.add_argument(
        "--gamma", type=float, default=0.99,
        help="Discount factor",
    )
    parser.add_argument(
        "--gae-lambda", type=float, default=0.95,
        help="GAE lambda",
    )
    parser.add_argument(
        "--clip-range", type=float, default=0.2,
        help="PPO clip range",
    )
    parser.add_argument(
        "--ent-coef", type=float, default=0.01,
        help="Entropy coefficient",
    )

    # Feature extractor settings
    parser.add_argument(
        "--orb-embedding-dim", type=int, default=16,
        help="Dimension of orb ID embeddings",
    )
    parser.add_argument(
        "--hidden-dim", type=int, default=256,
        help="Hidden layer dimension",
    )
    parser.add_argument(
        "--lstm-hidden-size", type=int, default=256,
        help="LSTM hidden state size",
    )

    # Game settings
    parser.add_argument(
        "--headless", action="store_true",
        help="Run games in headless mode (no window)",
    )
    parser.add_argument(
        "--game-speed", type=float, default=10.0,
        help="Simulation speed multiplier (default 10x for fast training). "
             "Higher values run faster but may hit CPU limits. "
             "Use 999999 for uncapped speed.",
    )
    parser.add_argument(
        "--executable", type=str, default="../target/release/ssol_simulator",
        help="Path to game executable",
    )
    parser.add_argument(
        "--no-launch", action="store_true",
        help="Don't launch game instances (assume already running)",
    )
    parser.add_argument(
        "--num-orbs", type=int, default=None,
        help="Number of orbs to spawn (curriculum setting). If not set, uses game default.",
    )

    # Logging/saving
    parser.add_argument(
        "--log-dir", type=str, default="./logs",
        help="Tensorboard log directory",
    )
    parser.add_argument(
        "--checkpoint-freq", type=int, default=100_000,
        help="Save checkpoint every N steps",
    )
    parser.add_argument(
        "--resume", type=str, default=None,
        help="Path to checkpoint to resume from",
    )
    parser.add_argument(
        "--device", type=str, default="auto",
        help="Device to use: 'auto', 'cpu', or 'cuda'",
    )

    args = parser.parse_args()

    # Check for RecurrentPPO
    if not HAS_RECURRENT:
        print("Error: sb3-contrib is required for RecurrentPPO")
        print("Install with: pip install sb3-contrib")
        sys.exit(1)

    # Create log directory
    log_path = Path(args.log_dir)
    log_path.mkdir(parents=True, exist_ok=True)
    checkpoint_path = log_path / "checkpoints"
    checkpoint_path.mkdir(parents=True, exist_ok=True)

    # Game instance manager
    game_manager = GameInstanceManager(args.executable)

    # Set up signal handler for clean shutdown
    def signal_handler(sig, frame):
        print("\nShutting down...")
        game_manager.shutdown_all()
        sys.exit(0)

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    try:
        # Launch game instances
        if not args.no_launch:
            print(f"Launching {args.num_envs} game instances...")
            game_manager.launch_many(
                args.base_port,
                args.num_envs,
                headless=args.headless,
                speed=args.game_speed,
                first_visible=not args.headless,  # First visible unless --headless
                stagger_delay=1.0,  # Give each instance time to grab GPU resources
            )

            # Wait for games to initialize (brief delay for startup)
            wait_time = 2 + args.num_envs * 0.5
            print(f"Waiting {wait_time:.1f}s for games to initialize...")
            time.sleep(wait_time)

        # Create vectorized environment
        print("Creating environments...")
        if args.num_orbs is not None:
            print(f"Setting curriculum: max_orbs={args.num_orbs}")
        env_fns = [
            make_env(args.base_port, i, args.max_episode_steps, max_orbs=args.num_orbs)
            for i in range(args.num_envs)
        ]

        if args.num_envs > 1:
            env = SubprocVecEnv(env_fns)
        else:
            env = DummyVecEnv(env_fns)

        # Policy kwargs with custom feature extractor
        policy_kwargs = dict(
            features_extractor_class=SSOLFeatureExtractor,
            features_extractor_kwargs=dict(
                orb_embedding_dim=args.orb_embedding_dim,
                hidden_dim=args.hidden_dim,
            ),
            lstm_hidden_size=args.lstm_hidden_size,
            n_lstm_layers=1,
            shared_lstm=True,  # Actor and critic share the same LSTM
            enable_critic_lstm=False,  # Don't use separate critic LSTM (using shared instead)
        )

        # Create or load model
        if args.resume:
            print(f"Resuming from {args.resume}")
            model = RecurrentPPO.load(args.resume, env=env, device=args.device)
        else:
            print("Creating new model...")
            model = RecurrentPPO(
                "MultiInputLstmPolicy",
                env,
                policy_kwargs=policy_kwargs,
                learning_rate=args.learning_rate,
                n_steps=args.n_steps,
                batch_size=args.batch_size,
                n_epochs=args.n_epochs,
                gamma=args.gamma,
                gae_lambda=args.gae_lambda,
                clip_range=args.clip_range,
                clip_range_vf=None,
                ent_coef=args.ent_coef,
                vf_coef=0.5,
                max_grad_norm=0.5,
                verbose=1,
                tensorboard_log=str(log_path),
                device=args.device,
            )

        # Callbacks
        callbacks = CallbackList([
            InterruptCallback(),  # Check for Ctrl+C
            CheckpointCallback(
                save_freq=args.checkpoint_freq // args.num_envs,
                save_path=str(checkpoint_path),
                name_prefix="ssol_model",
            ),
            TensorboardCallback(),
        ])

        # Train
        print(f"Starting training for {args.timesteps:,} timesteps...")
        print("Press Ctrl+C once to stop gracefully, twice to force exit.")
        model.learn(
            total_timesteps=args.timesteps,
            callback=callbacks,
            progress_bar=True,
        )

        # Save final model
        final_path = log_path / "ssol_final"
        model.save(str(final_path))
        print(f"Training complete! Model saved to {final_path}")

    finally:
        # Cleanup
        print("Shutting down game instances...")
        game_manager.shutdown_all()


if __name__ == "__main__":
    main()
