"""
Gymnasium environment for A Slower Speed of Light.

Provides the SSOLEnv wrapper that communicates with the Rust game engine
via ZMQ using MessagePack serialization.

Communication architecture:
- REQ socket (port): Request/reply for commands (Reset, Step, SetCurriculum, Close)
- PULL socket (port+1): Receives pushed observations for prefetching during inference
"""

import gymnasium as gym
from gymnasium import spaces
import numpy as np
import zmq
import msgpack
from typing import Any, Optional
import threading
from collections import deque


class ObservationPrefetcher:
    """
    Background thread that receives pushed observations from the game.
    Allows training to prefetch observations during neural network inference.
    """

    def __init__(self, pull_address: str, max_queue_size: int = 5):
        self.pull_address = pull_address
        self.max_queue_size = max_queue_size
        self._queue: deque = deque(maxlen=max_queue_size)
        self._lock = threading.Lock()
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._context: Optional[zmq.Context] = None
        self._socket: Optional[zmq.Socket] = None
        self._latest_obs: Optional[dict] = None

    def start(self) -> None:
        """Start the prefetch thread."""
        if self._running:
            return

        self._running = True
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        """Stop the prefetch thread."""
        self._running = False
        if self._thread is not None:
            self._thread.join(timeout=1.0)
            self._thread = None

    def _run(self) -> None:
        """Background thread that receives observations."""
        self._context = zmq.Context()
        self._socket = self._context.socket(zmq.PULL)
        self._socket.setsockopt(zmq.RCVTIMEO, 100)  # 100ms timeout
        self._socket.connect(self.pull_address)

        while self._running:
            try:
                msg = self._socket.recv()
                obs = msgpack.unpackb(msg, raw=False)
                with self._lock:
                    self._queue.append(obs)
                    self._latest_obs = obs
            except zmq.ZMQError:
                pass  # Timeout, just retry

        self._socket.close()
        self._context.term()

    def get_latest(self) -> Optional[dict]:
        """Get the most recent pushed observation (non-blocking)."""
        with self._lock:
            return self._latest_obs

    def drain_queue(self) -> list:
        """Get all queued observations and clear the queue."""
        with self._lock:
            result = list(self._queue)
            self._queue.clear()
            return result

    def peek_queue_size(self) -> int:
        """Get current queue size."""
        with self._lock:
            return len(self._queue)


class SSOLEnv(gym.Env):
    """Gymnasium environment for A Slower Speed of Light."""

    metadata = {"render_modes": ["human", "rgb_array"], "render_fps": 60}

    def __init__(
        self,
        zmq_address: str = "tcp://127.0.0.1:5555",
        render_mode: Optional[str] = None,
        max_episode_steps: int = 3750,  # 150 seconds at 25Hz
        timeout_ms: int = 5000,  # 5 second timeout for ZMQ operations
        enable_prefetch: bool = True,
    ):
        """
        Initialize the SSOL environment.

        Args:
            zmq_address: ZMQ endpoint for commands (port for REQ, port+1 for PULL)
            render_mode: Rendering mode (not currently used)
            max_episode_steps: Maximum steps before truncation
            timeout_ms: Timeout in milliseconds for ZMQ operations
            enable_prefetch: Enable observation prefetching via PULL socket
        """
        super().__init__()

        self.zmq_address = zmq_address
        self.render_mode = render_mode
        self.max_episode_steps = max_episode_steps
        self.timeout_ms = timeout_ms
        self._step_count = 0
        self._connected = False
        self._enable_prefetch = enable_prefetch

        # ZMQ setup
        self.context: Optional[zmq.Context] = None
        self.socket: Optional[zmq.Socket] = None

        # Observation prefetcher
        self._prefetcher: Optional[ObservationPrefetcher] = None
        if enable_prefetch:
            # Parse port from address and create PULL address
            # e.g., "tcp://127.0.0.1:5555" -> "tcp://127.0.0.1:5556"
            parts = zmq_address.rsplit(':', 1)
            if len(parts) == 2:
                pull_port = int(parts[1]) + 1
                pull_address = f"{parts[0]}:{pull_port}"
                self._prefetcher = ObservationPrefetcher(pull_address)

        # Define observation space
        self.observation_space = spaces.Dict({
            # Binary checklist of which orbs are collected (1.0 = active, 0.0 = collected)
            "orb_checklist": spaces.Box(low=0, high=1, shape=(100,), dtype=np.float32),

            # Player state (normalized)
            "player_position": spaces.Box(low=-500, high=500, shape=(3,), dtype=np.float32),
            "camera_yaw": spaces.Box(low=-np.pi, high=np.pi, shape=(1,), dtype=np.float32),
            "camera_pitch": spaces.Box(low=-np.pi/2, high=np.pi/2, shape=(1,), dtype=np.float32),
            "player_velocity_local": spaces.Box(low=-50, high=50, shape=(3,), dtype=np.float32),
            "player_velocity_world": spaces.Box(low=-50, high=50, shape=(3,), dtype=np.float32),

            # Game state scalars
            "speed_of_light_ratio": spaces.Box(low=0, high=1, shape=(1,), dtype=np.float32),
            "combo_timer": spaces.Box(low=0, high=10, shape=(1,), dtype=np.float32),
            "speed_multiplier": spaces.Box(low=0, high=2, shape=(1,), dtype=np.float32),

            # Wall detection (16 rays, 0 = touching wall, 1 = no wall)
            "wall_rays": spaces.Box(low=0, high=1, shape=(16,), dtype=np.float32),

            # Nearest orb targets (10 orbs)
            "orb_targets_direction": spaces.Box(low=-1, high=1, shape=(10, 3), dtype=np.float32),
            "orb_targets_distance": spaces.Box(low=0, high=1000, shape=(10,), dtype=np.float32),
            "orb_targets_id": spaces.Box(low=-1, high=99, shape=(10,), dtype=np.float32),
        })

        # Define action space (continuous)
        # [pitch_delta, yaw_delta, forward/back, left/right]
        self.action_space = spaces.Box(
            low=np.array([-0.1, -0.1, -1.0, -1.0], dtype=np.float32),
            high=np.array([0.1, 0.1, 1.0, 1.0], dtype=np.float32),
            dtype=np.float32
        )

    def _connect(self) -> None:
        """Establish ZMQ connection if not already connected."""
        if self._connected:
            return

        self.context = zmq.Context()
        self.socket = self.context.socket(zmq.REQ)
        self.socket.setsockopt(zmq.RCVTIMEO, self.timeout_ms)
        self.socket.setsockopt(zmq.SNDTIMEO, self.timeout_ms)
        self.socket.setsockopt(zmq.LINGER, 0)
        self.socket.connect(self.zmq_address)
        self._connected = True

        # Start prefetcher
        if self._prefetcher is not None:
            self._prefetcher.start()

    def _disconnect(self) -> None:
        """Close ZMQ connection."""
        # Stop prefetcher
        if self._prefetcher is not None:
            self._prefetcher.stop()

        if self.socket is not None:
            self.socket.close()
            self.socket = None
        if self.context is not None:
            self.context.term()
            self.context = None
        self._connected = False

    def _send_message(self, message: dict) -> dict:
        """
        Send a message and receive response via ZMQ.

        Args:
            message: Dictionary to send (will be MessagePack encoded)

        Returns:
            Decoded response dictionary

        Raises:
            ConnectionError: If communication fails
        """
        self._connect()

        try:
            packed = msgpack.packb(message, use_bin_type=True)
            self.socket.send(packed)
            response_bytes = self.socket.recv()
            return msgpack.unpackb(response_bytes, raw=False)
        except zmq.ZMQError as e:
            self._disconnect()
            raise ConnectionError(f"ZMQ communication error: {e}")

    def _parse_observation(self, obs_data: list) -> dict:
        """
        Convert raw observation data from Rust to gymnasium format.

        The Rust ObservationData struct serializes as a list with fields in order:
        [orb_checklist, player_position, camera_yaw, camera_pitch,
         player_velocity_local, player_velocity_world, speed_of_light_ratio,
         combo_timer, speed_multiplier, wall_rays, orb_targets]

        Args:
            obs_data: Raw observation list from Rust (MessagePack serialization of struct)

        Returns:
            Formatted observation dictionary matching observation_space
        """
        # Unpack the struct fields in order
        (orb_checklist, player_position, camera_yaw, camera_pitch,
         player_velocity_local, player_velocity_world, speed_of_light_ratio,
         combo_timer, speed_multiplier, wall_rays, orb_targets) = obs_data

        orb_targets = np.array(orb_targets, dtype=np.float32)  # (10, 5)

        return {
            "orb_checklist": np.array(orb_checklist, dtype=np.float32),
            "player_position": np.array(player_position, dtype=np.float32),
            "camera_yaw": np.array([camera_yaw], dtype=np.float32),
            "camera_pitch": np.array([camera_pitch], dtype=np.float32),
            "player_velocity_local": np.array(player_velocity_local, dtype=np.float32),
            "player_velocity_world": np.array(player_velocity_world, dtype=np.float32),
            "speed_of_light_ratio": np.array([speed_of_light_ratio], dtype=np.float32),
            "combo_timer": np.array([combo_timer], dtype=np.float32),
            "speed_multiplier": np.array([speed_multiplier], dtype=np.float32),
            "wall_rays": np.array(wall_rays, dtype=np.float32),
            "orb_targets_direction": orb_targets[:, :3],
            "orb_targets_distance": orb_targets[:, 3],
            "orb_targets_id": orb_targets[:, 4],
        }

    def reset(
        self,
        seed: Optional[int] = None,
        options: Optional[dict] = None,
    ) -> tuple[dict, dict]:
        """
        Reset the environment and return initial observation.

        Args:
            seed: Random seed (passed to parent, not used by Rust side)
            options: Additional options (not currently used)

        Returns:
            Tuple of (observation, info)
        """
        super().reset(seed=seed)
        self._step_count = 0

        # Clear prefetch queue on reset
        if self._prefetcher is not None:
            self._prefetcher.drain_queue()

        response = self._send_message({"type": "Reset"})

        # MessagePack serializes Rust enum as: ["VariantName", field1, field2, ...]
        # For Reset: ["Reset", observation_data, info_dict]
        variant_name = response[0]
        obs_data = response[1]
        info = response[2] if len(response) > 2 else {}

        obs = self._parse_observation(obs_data)

        return obs, info

    def step(self, action: np.ndarray) -> tuple[dict, float, bool, bool, dict]:
        """
        Take a step in the environment.

        Args:
            action: Action array [pitch_delta, yaw_delta, forward/back, left/right]

        Returns:
            Tuple of (observation, reward, terminated, truncated, info)
        """
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

        # MessagePack serializes Rust enum as: ["VariantName", field1, field2, ...]
        # For Step: ["Step", observation_data, reward, terminated, truncated, info_dict]
        variant_name = response[0]
        obs_data = response[1]
        reward = float(response[2])
        terminated = bool(response[3])
        truncated = bool(response[4]) or (self._step_count >= self.max_episode_steps)
        info = response[5] if len(response) > 5 else {}

        obs = self._parse_observation(obs_data)
        info["step_count"] = self._step_count
        info["success"] = terminated  # True if episode ended by winning (all orbs collected)

        # Add prefetch stats to info
        if self._prefetcher is not None:
            info["prefetch_queue_size"] = self._prefetcher.peek_queue_size()

        return obs, reward, terminated, truncated, info

    def get_prefetched_observation(self) -> Optional[dict]:
        """
        Get the latest prefetched observation (non-blocking).

        This can be called during neural network inference to get the
        most recent observation without waiting for a step() response.

        Returns:
            Parsed observation dict if available, None otherwise
        """
        if self._prefetcher is None:
            return None

        latest = self._prefetcher.get_latest()
        if latest is None:
            return None

        # Parse the pushed observation format
        # PushedObservation: {seq, observation, pending_reward, terminated, truncated, episode_ticks}
        obs_data = latest.get('observation')
        if obs_data is None:
            return None

        return self._parse_observation(obs_data)

    def get_prefetch_info(self) -> dict:
        """
        Get info about prefetched observations.

        Returns:
            Dict with 'queue_size' and 'latest' (if available)
        """
        if self._prefetcher is None:
            return {'enabled': False}

        latest = self._prefetcher.get_latest()
        return {
            'enabled': True,
            'queue_size': self._prefetcher.peek_queue_size(),
            'latest_seq': latest.get('seq') if latest else None,
            'latest_reward': latest.get('pending_reward') if latest else None,
        }

    def set_curriculum(
        self,
        orb_spawn_radius: Optional[float] = None,
        max_orbs: Optional[int] = None,
    ) -> None:
        """
        Update curriculum settings in the Rust game.

        Args:
            orb_spawn_radius: Maximum distance for orb spawning (None = no limit)
            max_orbs: Maximum number of orbs to spawn (None = no limit)
        """
        message = {
            "type": "SetCurriculum",
            "orb_spawn_radius": orb_spawn_radius,
            "max_orbs": max_orbs,
        }
        self._send_message(message)

    def set_max_episode_steps(self, max_steps: int) -> None:
        """
        Dynamically update the maximum episode steps.

        Used by curriculum callback to allow more time for more orbs.

        Args:
            max_steps: New maximum steps per episode
        """
        self.max_episode_steps = max_steps

    def close(self) -> None:
        """Close the environment and ZMQ connection."""
        if self._connected:
            try:
                self._send_message({"type": "Close"})
            except:
                pass  # Ignore errors during shutdown
            self._disconnect()

    def render(self) -> Optional[np.ndarray]:
        """
        Render the environment.

        Note: Actual rendering is done by the Rust game engine.
        This method is a no-op unless extended for screenshot capture.
        """
        if self.render_mode == "rgb_array":
            # TODO: Implement screenshot capture from Rust
            return None
        return None


def make_ssol_env(
    port: int = 5555,
    rank: int = 0,
    seed: int = 0,
    max_episode_steps: int = 3750,
    enable_prefetch: bool = True,
) -> SSOLEnv:
    """
    Factory function to create an SSOL environment.

    Args:
        port: Base ZMQ port
        rank: Environment rank (each env uses 2 ports: port+rank*2 for REQ, port+rank*2+1 for PULL)
        seed: Random seed
        max_episode_steps: Maximum steps before truncation
        enable_prefetch: Enable observation prefetching via PULL socket

    Returns:
        Configured SSOLEnv instance
    """
    # Each env uses 2 ports: port+rank*2 for REQ, port+rank*2+1 for PULL
    env = SSOLEnv(
        zmq_address=f"tcp://127.0.0.1:{port + rank * 2}",
        max_episode_steps=max_episode_steps,
        enable_prefetch=enable_prefetch,
    )
    return env
