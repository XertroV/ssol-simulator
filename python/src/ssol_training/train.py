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
from collections import deque
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
# Enable TensorFloat-32 for faster matrix multiplications on Ampere+ GPUs (RTX 30xx/40xx)
torch.backends.cuda.matmul.allow_tf32 = True
torch.backends.cudnn.allow_tf32 = True

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
        # Wrap with Monitor, passing through 'success' key for curriculum tracking
        env = Monitor(env, info_keywords=("success",))
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


def compute_max_episode_steps(num_orbs: int) -> int:
    """
    Compute maximum episode steps based on number of orbs.

    Formula: 200 + 75 * num_orbs
    This allows more time for episodes with more orbs to collect.

    Args:
        num_orbs: Number of orbs in the current curriculum level

    Returns:
        Maximum steps allowed for the episode
    """
    return 200 + 75 * num_orbs


class CurriculumCallback(BaseCallback):
    """
    Callback for automatic curriculum progression.

    Tracks success rate (episodes where all orbs were collected) over a rolling
    window and increases orb count when the success threshold is met.

    Episode length is dynamically adjusted based on orb count using the formula:
    max_episode_steps = 200 + 75 * num_orbs
    """

    def __init__(
        self,
        env,
        start_orbs: int = 3,
        max_orbs: int = 100,
        success_threshold: float = 0.5,
        window_size: int = 100,
        verbose: int = 0,
    ):
        """
        Initialize curriculum callback.

        Args:
            env: The vectorized environment (DummyVecEnv or SubprocVecEnv)
            start_orbs: Initial number of orbs
            max_orbs: Maximum number of orbs to progress to
            success_threshold: Success rate threshold (0.0-1.0) to trigger progression
            window_size: Number of episodes to average for success rate calculation
            verbose: Verbosity level
        """
        super().__init__(verbose)
        self.env = env
        self.current_orbs = start_orbs
        self.max_orbs = max_orbs
        self.success_threshold = success_threshold
        self.window_size = window_size
        self.success_history: deque[bool] = deque(maxlen=window_size)
        self.progression_count = 0
        self._last_logged_success_rate: Optional[float] = None

    def _on_training_start(self) -> None:
        """Apply initial curriculum settings when training starts."""
        self._apply_curriculum()

    def _on_step(self) -> bool:
        """Check for episode completions and handle curriculum progression."""
        for info in self.locals.get("infos", []):
            if "episode" in info:
                # Episode completed - check if it was successful
                # 'success' is True if terminated (all orbs collected), False if truncated
                success = info.get("success", False)
                self.success_history.append(success)

                # Check for progression when we have enough data
                if len(self.success_history) >= self.window_size:
                    success_rate = sum(self.success_history) / len(self.success_history)

                    # Log success rate to TensorBoard
                    self.logger.record("curriculum/success_rate", success_rate)
                    self.logger.record("curriculum/current_orbs", self.current_orbs)
                    self.logger.record("curriculum/progressions", self.progression_count)

                    if success_rate >= self.success_threshold:
                        self._progress_curriculum()

        return True

    def _progress_curriculum(self) -> None:
        """Increase difficulty by adding one more orb."""
        if self.current_orbs >= self.max_orbs:
            return

        self.current_orbs += 1
        self.progression_count += 1
        self.success_history.clear()  # Reset history after progression

        print(
            f"\n[Curriculum] Progressing to {self.current_orbs} orbs "
            f"(progression #{self.progression_count})"
        )

        self._apply_curriculum()

        # Log progression event
        self.logger.record("curriculum/current_orbs", self.current_orbs)
        self.logger.record("curriculum/progressions", self.progression_count)

        # TODO: Future enhancement - regression handling
        # If success rate drops significantly after progression, could temporarily
        # revert curriculum level. Would need to track post-progression success
        # and compare against a regression threshold (e.g., < 0.2 for N episodes).
        # For now, we trust the agent to adapt without rollback.

    def _apply_curriculum(self) -> None:
        """Apply current curriculum settings to all environments."""
        new_max_steps = compute_max_episode_steps(self.current_orbs)

        print(
            f"[Curriculum] Setting {self.current_orbs} orbs, "
            f"max_episode_steps={new_max_steps}"
        )

        # Use env_method to call methods on underlying environments
        # This works for both DummyVecEnv and SubprocVecEnv
        self.env.env_method("set_curriculum", max_orbs=self.current_orbs)
        self.env.env_method("set_max_episode_steps", new_max_steps)


def _signal_handler(signum, frame):
    """Handle Ctrl+C by setting interrupt flag."""
    global _interrupt_requested
    if _interrupt_requested:
        # Second Ctrl+C - force exit
        print("\n[Interrupt] Forcing exit...")
        sys.exit(1)
    print("\n[Interrupt] Ctrl+C received, finishing current step...")
    _interrupt_requested = True


def create_argument_parser() -> argparse.ArgumentParser:
    """Create and configure the argument parser for training."""
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
        "--learning-rate", type=float, default=1e-4,
        help="Learning rate",
    )
    parser.add_argument(
        "--n-steps", type=int, default=None,
        help="Steps per rollout per environment (default: 32768 / num_envs for consistent buffer size)",
    )
    parser.add_argument(
        "--batch-size", type=int, default=1024,
        help="Minibatch size (larger = fewer updates, better GPU utilization)",
    )
    parser.add_argument(
        "--n-epochs", type=int, default=2,
        help="Number of training epochs per update (fewer = faster updates with separate LSTMs)",
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
        "--clip-range", type=float, default=0.15,
        help="PPO clip range (smaller = more conservative updates)",
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
        "--hidden-dim", type=int, default=512,
        help="Hidden layer dimension (larger = more capacity)",
    )
    parser.add_argument(
        "--lstm-hidden-size", type=int, default=384,
        help="LSTM hidden state size (larger = better memory for sequences, but slower with separate LSTMs)",
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
        help="Fixed number of orbs (disables auto-curriculum). If not set, uses auto-curriculum.",
    )

    # Curriculum settings
    parser.add_argument(
        "--start-orbs", type=int, default=3,
        help="Starting number of orbs for auto-curriculum",
    )
    parser.add_argument(
        "--max-orbs", type=int, default=100,
        help="Maximum number of orbs for auto-curriculum progression",
    )
    parser.add_argument(
        "--success-threshold", type=float, default=0.5,
        help="Success rate threshold (0.0-1.0) to trigger curriculum progression",
    )
    parser.add_argument(
        "--curriculum-window", type=int, default=100,
        help="Number of episodes to average for success rate calculation",
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

    return parser


def create_vectorized_env(args) -> tuple:
    """
    Create vectorized environment with curriculum settings.

    Args:
        args: Parsed command-line arguments

    Returns:
        Tuple of (env, use_auto_curriculum, initial_orbs)
    """
    use_auto_curriculum = args.num_orbs is None

    if use_auto_curriculum:
        initial_orbs = args.start_orbs
        initial_max_steps = compute_max_episode_steps(initial_orbs)
        print(f"Auto-curriculum enabled: starting with {initial_orbs} orbs")
        print(f"  Success threshold: {args.success_threshold:.0%}")
        print(f"  Window size: {args.curriculum_window} episodes")
        print(f"  Max orbs: {args.max_orbs}")
    else:
        initial_orbs = args.num_orbs
        initial_max_steps = compute_max_episode_steps(initial_orbs)
        print(f"Fixed curriculum: {initial_orbs} orbs (auto-curriculum disabled)")

    print(f"Initial max_episode_steps: {initial_max_steps}")

    env_fns = [
        make_env(
            args.base_port,
            i,
            initial_max_steps,
            max_orbs=initial_orbs,
        )
        for i in range(args.num_envs)
    ]

    if args.num_envs > 1:
        env = SubprocVecEnv(env_fns)
    else:
        env = DummyVecEnv(env_fns)

    return env, use_auto_curriculum, initial_orbs


def create_model(args, env, log_path: Path):
    """
    Create or load the RecurrentPPO model.

    Args:
        args: Parsed command-line arguments
        env: Vectorized environment
        log_path: Path for TensorBoard logs

    Returns:
        RecurrentPPO model
    """
    policy_kwargs = dict(
        features_extractor_class=SSOLFeatureExtractor,
        features_extractor_kwargs=dict(
            orb_embedding_dim=args.orb_embedding_dim,
            hidden_dim=args.hidden_dim,
        ),
        lstm_hidden_size=args.lstm_hidden_size,
        n_lstm_layers=1,
        shared_lstm=False,  # Separate LSTMs for policy and critic (more stable)
        enable_critic_lstm=True,  # Critic gets its own LSTM for independent learning
    )

    if args.resume:
        print(f"Resuming from {args.resume}")
        # Load model and override hyperparameters via custom_objects
        # This is the proper sb3 way to override learning rate on resume
        custom_objects = {
            "learning_rate": args.learning_rate,
            "ent_coef": args.ent_coef,
            "n_epochs": args.n_epochs,
            "batch_size": args.batch_size,
            "clip_range": args.clip_range,
        }
        model = RecurrentPPO.load(
            args.resume,
            env=env,
            device=args.device,
            custom_objects=custom_objects,
        )
        # Also update the optimizer's learning rate directly
        for param_group in model.policy.optimizer.param_groups:
            param_group['lr'] = args.learning_rate
        # Override attributes that custom_objects doesn't handle
        model.n_epochs = args.n_epochs
        model.batch_size = args.batch_size
        # clip_range must be a callable (schedule), not a raw float
        model.clip_range = lambda _: args.clip_range
        print(f"  Overriding: lr={args.learning_rate}, ent_coef={args.ent_coef}, "
              f"batch_size={args.batch_size}, n_epochs={args.n_epochs}, clip_range={args.clip_range}")
        return model

    print("Creating new model...")
    return RecurrentPPO(
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
        clip_range_vf=args.clip_range,  # Apply same clipping to value function for stability
        ent_coef=args.ent_coef,
        vf_coef=0.5,
        max_grad_norm=0.5,
        verbose=1,
        tensorboard_log=str(log_path),
        device=args.device,
    )


def create_callbacks(args, env, checkpoint_path: Path, use_auto_curriculum: bool) -> CallbackList:
    """
    Create training callbacks.

    Args:
        args: Parsed command-line arguments
        env: Vectorized environment
        checkpoint_path: Path for saving checkpoints
        use_auto_curriculum: Whether auto-curriculum is enabled

    Returns:
        CallbackList with all configured callbacks
    """
    callback_list = [
        InterruptCallback(),
        CheckpointCallback(
            save_freq=args.checkpoint_freq // args.num_envs,
            save_path=str(checkpoint_path),
            name_prefix="ssol_model",
        ),
        TensorboardCallback(),
    ]

    if use_auto_curriculum:
        curriculum_callback = CurriculumCallback(
            env=env,
            start_orbs=args.start_orbs,
            max_orbs=args.max_orbs,
            success_threshold=args.success_threshold,
            window_size=args.curriculum_window,
        )
        callback_list.append(curriculum_callback)

    return CallbackList(callback_list)


def main():
    # Set up signal handler early
    signal.signal(signal.SIGINT, _signal_handler)
    if hasattr(signal, 'SIGTERM'):
        signal.signal(signal.SIGTERM, _signal_handler)

    parser = create_argument_parser()
    args = parser.parse_args()

    # Set default n_steps based on num_envs if not specified
    # Using 16384 total buffer for lower memory usage and faster updates
    if args.n_steps is None:
        args.n_steps = 16384 // args.num_envs
        print(f"Using default n_steps: {args.n_steps} ({args.num_envs} envs × {args.n_steps} = {args.num_envs * args.n_steps} total buffer size)")

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
        env, use_auto_curriculum, initial_orbs = create_vectorized_env(args)

        # Create or load model
        model = create_model(args, env, log_path)

        # Create callbacks
        callbacks = create_callbacks(args, env, checkpoint_path, use_auto_curriculum)

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
