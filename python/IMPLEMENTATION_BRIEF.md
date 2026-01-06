# Python AI Training Infrastructure - Implementation Brief

## Overview

Build the Python-side training infrastructure for "A Slower Speed of Light" RL agent training. The Rust game engine (Bevy) already has full AI infrastructure implemented. This phase adds:

1. **ZMQ Bridge** - Communication layer between Python and Rust
2. **Gymnasium Environment** - `SSOLEnv` wrapper for SB3 compatibility
3. **Custom Feature Extractor** - Handle mixed observation space with embeddings
4. **Training Script** - RecurrentPPO with proper configuration

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Python Side                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │ Training     │───▶│ SSOLEnv      │───▶│ ZMQ Client       │   │
│  │ Script       │    │ (Gymnasium)  │    │ (REQ socket)     │   │
│  │ RecurrentPPO │◀───│              │◀───│                  │   │
│  └──────────────┘    └──────────────┘    └──────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                                │
                          ZMQ REQ/REP
                          MessagePack
                                │
┌─────────────────────────────────────────────────────────────────┐
│                          Rust Side                               │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │ Game Engine  │◀──▶│ AI Module    │◀──▶│ ZMQ Server       │   │
│  │ (Bevy)       │    │ Observations │    │ (REP socket)     │   │
│  │ Physics 100Hz│    │ Actions      │    │                  │   │
│  └──────────────┘    └──────────────┘    └──────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Part 1: Rust ZMQ Server (Add to existing AI module)

### Dependencies to add to Cargo.toml
```toml
zeromq = "0.4"  # or zmq = "0.10"
rmp-serde = "1.1"
```

### New file: `src/ai/bridge.rs`

Create a ZMQ REP server that runs on a background thread, communicating with the Bevy app via channels.

```rust
// Message types for ZMQ communication
#[derive(Serialize, Deserialize)]
pub enum ClientMessage {
    Reset,
    Step { action: ActionData },
    GetObservation,
    Close,
}

#[derive(Serialize, Deserialize)]
pub struct ActionData {
    pub look: [f32; 2],      // [pitch_delta, yaw_delta] in radians
    pub move_dir: [f32; 2],  // [forward/back, left/right] in [-1, 1]
}

#[derive(Serialize, Deserialize)]
pub struct ObservationData {
    pub orb_checklist: [f32; 100],
    pub player_position: [f32; 3],
    pub player_orientation: [f32; 3],
    pub player_velocity_local: [f32; 3],
    pub player_velocity_world: [f32; 3],
    pub speed_of_light_ratio: f32,
    pub combo_timer: f32,
    pub speed_multiplier: f32,
    pub wall_rays: [f32; 16],
    pub orb_targets: [[f32; 5]; 10],  // [dir_x, dir_y, dir_z, distance, orb_id]
}

#[derive(Serialize, Deserialize)]
pub struct StepResponse {
    pub observation: ObservationData,
    pub reward: f32,
    pub terminated: bool,
    pub truncated: bool,
    pub info: HashMap<String, f32>,
}

#[derive(Serialize, Deserialize)]
pub struct ResetResponse {
    pub observation: ObservationData,
    pub info: HashMap<String, f32>,
}
```

### Protocol
- Socket: `tcp://127.0.0.1:5555` (configurable via CLI `--zmq-port`)
- Serialization: MessagePack (compact binary, faster than JSON)
- Pattern: REQ/REP (synchronous, one request = one response)

### Timing
- Physics runs at 100Hz (fixed timestep)
- `action_repeat = 4` means each agent step = 4 physics ticks = 40ms
- Effective agent decision rate: 25Hz

---

## Part 2: Python Gymnasium Environment

### File: `python/ssol_env.py`

```python
import gymnasium as gym
from gymnasium import spaces
import numpy as np
import zmq
import msgpack

class SSOLEnv(gym.Env):
    """Gymnasium environment for A Slower Speed of Light."""

    metadata = {"render_modes": ["human", "rgb_array"], "render_fps": 60}

    def __init__(self,
                 zmq_address: str = "tcp://127.0.0.1:5555",
                 render_mode: str = None,
                 max_episode_steps: int = 3750):  # 150 seconds at 25Hz
        super().__init__()

        self.zmq_address = zmq_address
        self.render_mode = render_mode
        self.max_episode_steps = max_episode_steps
        self._step_count = 0

        # ZMQ setup
        self.context = zmq.Context()
        self.socket = self.context.socket(zmq.REQ)
        self.socket.connect(zmq_address)

        # Define observation space
        self.observation_space = spaces.Dict({
            # Binary checklist of which orbs are collected
            "orb_checklist": spaces.Box(low=0, high=1, shape=(100,), dtype=np.float32),

            # Player state (normalized)
            "player_position": spaces.Box(low=-500, high=500, shape=(3,), dtype=np.float32),
            "player_orientation": spaces.Box(low=-np.pi, high=np.pi, shape=(3,), dtype=np.float32),
            "player_velocity_local": spaces.Box(low=-50, high=50, shape=(3,), dtype=np.float32),
            "player_velocity_world": spaces.Box(low=-50, high=50, shape=(3,), dtype=np.float32),

            # Game state scalars
            "speed_of_light_ratio": spaces.Box(low=0, high=1, shape=(1,), dtype=np.float32),
            "combo_timer": spaces.Box(low=0, high=10, shape=(1,), dtype=np.float32),
            "speed_multiplier": spaces.Box(low=0, high=2, shape=(1,), dtype=np.float32),

            # Wall detection (16 rays)
            "wall_rays": spaces.Box(low=0, high=1, shape=(16,), dtype=np.float32),

            # Nearest orb targets (10 orbs x 5 values each)
            # [direction_x, direction_y, direction_z, path_distance, orb_id]
            "orb_targets_direction": spaces.Box(low=-1, high=1, shape=(10, 3), dtype=np.float32),
            "orb_targets_distance": spaces.Box(low=0, high=1000, shape=(10,), dtype=np.float32),
            "orb_targets_id": spaces.Box(low=-1, high=99, shape=(10,), dtype=np.float32),
        })

        # Define action space (continuous)
        # [yaw_delta, forward/back, left/right]
        # Note: pitch_delta removed - AI has no control over pitch (doesn't affect movement)
        self.action_space = spaces.Box(
            low=np.array([-0.1, -1.0, -1.0]),
            high=np.array([0.1, 1.0, 1.0]),
            dtype=np.float32
        )

    def _send_message(self, message: dict) -> dict:
        """Send a message and receive response via ZMQ."""
        self.socket.send(msgpack.packb(message))
        response = msgpack.unpackb(self.socket.recv())
        return response

    def _parse_observation(self, obs_data: dict) -> dict:
        """Convert raw observation data to gymnasium format."""
        orb_targets = np.array(obs_data["orb_targets"])  # (10, 5)

        return {
            "orb_checklist": np.array(obs_data["orb_checklist"], dtype=np.float32),
            "player_position": np.array(obs_data["player_position"], dtype=np.float32),
            "player_orientation": np.array(obs_data["player_orientation"], dtype=np.float32),
            "player_velocity_local": np.array(obs_data["player_velocity_local"], dtype=np.float32),
            "player_velocity_world": np.array(obs_data["player_velocity_world"], dtype=np.float32),
            "speed_of_light_ratio": np.array([obs_data["speed_of_light_ratio"]], dtype=np.float32),
            "combo_timer": np.array([obs_data["combo_timer"]], dtype=np.float32),
            "speed_multiplier": np.array([obs_data["speed_multiplier"]], dtype=np.float32),
            "wall_rays": np.array(obs_data["wall_rays"], dtype=np.float32),
            "orb_targets_direction": orb_targets[:, :3].astype(np.float32),
            "orb_targets_distance": orb_targets[:, 3].astype(np.float32),
            "orb_targets_id": orb_targets[:, 4].astype(np.float32),
        }

    def reset(self, seed=None, options=None):
        super().reset(seed=seed)
        self._step_count = 0

        response = self._send_message({"type": "Reset"})
        obs = self._parse_observation(response["observation"])
        info = response.get("info", {})

        return obs, info

    def step(self, action):
        self._step_count += 1

        # Send action to Rust
        message = {
            "type": "Step",
            "action": {
                "look": [float(action[0]), float(action[1])],
                "move_dir": [float(action[2]), float(action[3])],
            }
        }

        response = self._send_message(message)

        obs = self._parse_observation(response["observation"])
        reward = response["reward"]
        terminated = response["terminated"]
        truncated = response["truncated"] or (self._step_count >= self.max_episode_steps)
        info = response.get("info", {})

        return obs, reward, terminated, truncated, info

    def close(self):
        self._send_message({"type": "Close"})
        self.socket.close()
        self.context.term()
```

---

## Part 3: Custom Feature Extractor

### File: `python/feature_extractor.py`

```python
import torch
import torch.nn as nn
from stable_baselines3.common.torch_layers import BaseFeaturesExtractor
from gymnasium import spaces

class SSOLFeatureExtractor(BaseFeaturesExtractor):
    """
    Custom feature extractor for SSOL observations.

    Handles:
    - Orb checklist (100 binary values)
    - Player state (continuous values)
    - Wall rays (16 values)
    - Orb targets with embedding for orb IDs
    """

    def __init__(self, observation_space: spaces.Dict,
                 orb_embedding_dim: int = 16,
                 hidden_dim: int = 256):

        # Calculate total features dimension
        # Orb checklist: 100
        # Player state: 3+3+3+3+1+1+1 = 15
        # Wall rays: 16
        # Orb targets: 10 * (3 + 1 + orb_embedding_dim) = 10 * 20 = 200
        features_dim = 100 + 15 + 16 + 10 * (3 + 1 + orb_embedding_dim)

        super().__init__(observation_space, features_dim=hidden_dim)

        self.orb_embedding_dim = orb_embedding_dim

        # Embedding layer for orb IDs
        # 101 entries: 0-99 for orbs, 100 for padding (mapped from -1)
        self.orb_embedding = nn.Embedding(101, orb_embedding_dim, padding_idx=100)

        # MLP to process combined features
        raw_features_dim = 100 + 15 + 16 + 10 * (3 + 1 + orb_embedding_dim)
        self.mlp = nn.Sequential(
            nn.Linear(raw_features_dim, hidden_dim),
            nn.ReLU(),
            nn.Linear(hidden_dim, hidden_dim),
            nn.ReLU(),
        )

    def forward(self, observations: dict) -> torch.Tensor:
        batch_size = observations["orb_checklist"].shape[0]

        # Extract flat features
        orb_checklist = observations["orb_checklist"]  # (B, 100)

        # Player state
        player_state = torch.cat([
            observations["player_position"],        # (B, 3)
            observations["player_orientation"],     # (B, 3)
            observations["player_velocity_local"],  # (B, 3)
            observations["player_velocity_world"],  # (B, 3)
            observations["speed_of_light_ratio"],   # (B, 1)
            observations["combo_timer"],            # (B, 1)
            observations["speed_multiplier"],       # (B, 1)
        ], dim=-1)  # (B, 15)

        wall_rays = observations["wall_rays"]  # (B, 16)

        # Orb targets with embedding
        orb_directions = observations["orb_targets_direction"]  # (B, 10, 3)
        orb_distances = observations["orb_targets_distance"].unsqueeze(-1)  # (B, 10, 1)
        orb_ids = observations["orb_targets_id"]  # (B, 10)

        # Convert orb IDs: -1 -> 100 (padding), 0-99 stay as is
        orb_ids_for_embedding = orb_ids.long().clamp(min=-1)
        orb_ids_for_embedding = torch.where(
            orb_ids_for_embedding < 0,
            torch.full_like(orb_ids_for_embedding, 100),
            orb_ids_for_embedding
        )

        orb_embeds = self.orb_embedding(orb_ids_for_embedding)  # (B, 10, embed_dim)

        # Combine orb target features
        orb_features = torch.cat([
            orb_directions,  # (B, 10, 3)
            orb_distances,   # (B, 10, 1)
            orb_embeds,      # (B, 10, embed_dim)
        ], dim=-1)  # (B, 10, 3+1+embed_dim)

        orb_features_flat = orb_features.view(batch_size, -1)  # (B, 10*(3+1+embed_dim))

        # Combine all features
        combined = torch.cat([
            orb_checklist,
            player_state,
            wall_rays,
            orb_features_flat,
        ], dim=-1)

        return self.mlp(combined)
```

---

## Part 4: Training Script

### File: `python/train.py`

```python
import argparse
import subprocess
import time
import os
from pathlib import Path

from stable_baselines3 import PPO
from sb3_contrib import RecurrentPPO
from stable_baselines3.common.callbacks import (
    CheckpointCallback,
    EvalCallback,
    CallbackList
)
from stable_baselines3.common.vec_env import DummyVecEnv, SubprocVecEnv
from stable_baselines3.common.monitor import Monitor

from ssol_env import SSOLEnv
from feature_extractor import SSOLFeatureExtractor


def make_env(port: int, rank: int, seed: int = 0):
    """Factory function for creating SSOL environments."""
    def _init():
        env = SSOLEnv(zmq_address=f"tcp://127.0.0.1:{port + rank}")
        env = Monitor(env)
        return env
    return _init


def launch_game_instance(port: int, headless: bool = True) -> subprocess.Popen:
    """Launch a Rust game instance."""
    cmd = [
        "../target/release/ssol_simulator",
        "--ai-mode",
        f"--zmq-port={port}",
    ]
    if headless:
        cmd.append("--headless")

    return subprocess.Popen(cmd)


def main():
    parser = argparse.ArgumentParser(description="Train SSOL RL Agent")
    parser.add_argument("--num-envs", type=int, default=8, help="Number of parallel environments")
    parser.add_argument("--timesteps", type=int, default=10_000_000, help="Total training timesteps")
    parser.add_argument("--base-port", type=int, default=5555, help="Base ZMQ port")
    parser.add_argument("--headless", action="store_true", help="Run games in headless mode")
    parser.add_argument("--checkpoint-freq", type=int, default=100_000, help="Save checkpoint every N steps")
    parser.add_argument("--log-dir", type=str, default="./logs", help="Tensorboard log directory")
    parser.add_argument("--resume", type=str, default=None, help="Path to checkpoint to resume from")
    args = parser.parse_args()

    # Create log directory
    Path(args.log_dir).mkdir(parents=True, exist_ok=True)

    # Launch game instances
    print(f"Launching {args.num_envs} game instances...")
    processes = []
    for i in range(args.num_envs):
        proc = launch_game_instance(args.base_port + i, headless=args.headless)
        processes.append(proc)
        time.sleep(0.5)  # Stagger startup

    # Wait for games to initialize
    print("Waiting for games to initialize...")
    time.sleep(5)

    try:
        # Create vectorized environment
        env_fns = [make_env(args.base_port, i) for i in range(args.num_envs)]

        if args.num_envs > 1:
            env = SubprocVecEnv(env_fns)
        else:
            env = DummyVecEnv(env_fns)

        # Policy kwargs with custom feature extractor
        policy_kwargs = dict(
            features_extractor_class=SSOLFeatureExtractor,
            features_extractor_kwargs=dict(
                orb_embedding_dim=16,
                hidden_dim=256,
            ),
            lstm_hidden_size=256,
            n_lstm_layers=1,
            shared_lstm=True,
            enable_critic_lstm=True,
        )

        # Create or load model
        if args.resume:
            print(f"Resuming from {args.resume}")
            model = RecurrentPPO.load(args.resume, env=env)
        else:
            model = RecurrentPPO(
                "MultiInputLstmPolicy",
                env,
                policy_kwargs=policy_kwargs,
                learning_rate=3e-4,
                n_steps=2048,
                batch_size=64,
                n_epochs=10,
                gamma=0.99,
                gae_lambda=0.95,
                clip_range=0.2,
                clip_range_vf=None,
                ent_coef=0.01,
                vf_coef=0.5,
                max_grad_norm=0.5,
                verbose=1,
                tensorboard_log=args.log_dir,
            )

        # Callbacks
        checkpoint_callback = CheckpointCallback(
            save_freq=args.checkpoint_freq // args.num_envs,
            save_path=f"{args.log_dir}/checkpoints",
            name_prefix="ssol_model",
        )

        # Train
        print("Starting training...")
        model.learn(
            total_timesteps=args.timesteps,
            callback=checkpoint_callback,
            progress_bar=True,
        )

        # Save final model
        model.save(f"{args.log_dir}/ssol_final")
        print(f"Training complete! Model saved to {args.log_dir}/ssol_final")

    finally:
        # Cleanup
        print("Shutting down game instances...")
        for proc in processes:
            proc.terminate()
            proc.wait()


if __name__ == "__main__":
    main()
```

---

## Part 5: Dependencies

### File: `python/requirements.txt`

```
gymnasium>=0.29.0
stable-baselines3>=2.2.0
sb3-contrib>=2.2.0
torch>=2.0.0
numpy>=1.24.0
pyzmq>=25.0.0
msgpack>=1.0.0
tensorboard>=2.14.0
```

---

## Observation Space Summary

| Field | Shape | Range | Description |
|-------|-------|-------|-------------|
| `orb_checklist` | (100,) | [0, 1] | 1.0 = active, 0.0 = collected |
| `player_position` | (3,) | [-500, 500] | World XYZ position |
| `player_orientation` | (3,) | [-π, π] | Euler angles (yaw, pitch, roll) |
| `player_velocity_local` | (3,) | [-50, 50] | Velocity in player's frame |
| `player_velocity_world` | (3,) | [-50, 50] | Velocity in world frame |
| `speed_of_light_ratio` | (1,) | [0, 1] | current_sol / start_sol |
| `combo_timer` | (1,) | [0, 10] | Time remaining on speed boost |
| `speed_multiplier` | (1,) | [0, 2] | Current speed multiplier |
| `wall_rays` | (16,) | [0, 1] | 0 = touching wall, 1 = no wall |
| `orb_targets_direction` | (10, 3) | [-1, 1] | Unit vector to nearest orbs (local space) |
| `orb_targets_distance` | (10,) | [0, 1000] | Path distance to orbs |
| `orb_targets_id` | (10,) | [-1, 99] | Orb ID (-1 = empty slot) |

## Action Space Summary

| Index | Name | Range | Description |
|-------|------|-------|-------------|
| 0 | yaw_delta | [-0.1, 0.1] | Look left/right (radians per step) |
| 1 | forward/back | [-1, 1] | -1 = backward, +1 = forward |
| 2 | left/right | [-1, 1] | -1 = strafe left, +1 = strafe right |

*Note: pitch_delta was removed - AI has no control over pitch as it doesn't affect movement.*

---

## Timing & Synchronization

- **Physics timestep**: 100Hz (10ms per tick)
- **Action repeat**: 4 ticks per agent step
- **Agent decision rate**: 25Hz (40ms per step)
- **Max episode length**: 3750 steps = 150 seconds
- **Default episode timeout (Rust)**: 1500 ticks = 15 seconds (for testing)

---

## CLI Flags (Rust side, already implemented)

```bash
# Training mode
./ssol_simulator --ai-mode --headless --zmq-port 5555

# Testing mode (random actions)
./ssol_simulator --ai-test

# Visible training (for debugging)
./ssol_simulator --ai-mode --zmq-port 5555
```

**Note**: `--headless` and `--zmq-port` need to be added to the Rust CLI parser.

---

## Curriculum Learning (Future Enhancement)

The Rust side has `CurriculumConfig` with `orb_spawn_radius` support. To enable curriculum:

1. Start with small radius (e.g., 50 units) so only nearby orbs spawn
2. Gradually increase as agent improves
3. Send curriculum updates via ZMQ message:
   ```python
   {"type": "SetCurriculum", "orb_spawn_radius": 100.0}
   ```

---

## Testing Checklist

1. [ ] ZMQ server starts and accepts connections
2. [ ] Reset message triggers game reset and returns observation
3. [ ] Step message applies action and returns (obs, reward, term, trunc, info)
4. [ ] Observation data matches expected format
5. [ ] Actions affect player movement correctly
6. [ ] Episode terminates on game win (all orbs collected)
7. [ ] Episode truncates on timeout
8. [ ] Multiple parallel environments work
9. [ ] Training runs without errors
10. [ ] Tensorboard shows learning progress
