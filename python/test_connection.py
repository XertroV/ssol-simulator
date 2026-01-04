"""Simple test script to verify ZMQ communication with the Rust game."""

import sys
sys.path.insert(0, "src")

from ssol_training.ssol_env import SSOLEnv
import numpy as np

def main():
    print("Creating SSOLEnv...")
    env = SSOLEnv(zmq_address="tcp://127.0.0.1:5555")

    print("Resetting environment...")
    obs, info = env.reset()

    print(f"\n=== Initial Observation ===")
    print(f"  orb_checklist shape: {obs['orb_checklist'].shape}, sum: {obs['orb_checklist'].sum():.0f}")
    print(f"  player_position: {obs['player_position']}")
    print(f"  camera_yaw: {obs['camera_yaw'][0]:.3f}")
    print(f"  camera_pitch: {obs['camera_pitch'][0]:.3f}")
    print(f"  speed_of_light_ratio: {obs['speed_of_light_ratio'][0]:.3f}")
    print(f"  wall_rays shape: {obs['wall_rays'].shape}")
    print(f"  orb_targets_direction shape: {obs['orb_targets_direction'].shape}")
    print(f"  info: {info}")

    print("\n=== Taking random actions ===")
    for step in range(10):
        # Random action
        action = env.action_space.sample()
        obs, reward, terminated, truncated, info = env.step(action)

        print(f"  Step {step+1}: reward={reward:.4f}, term={terminated}, trunc={truncated}")

        if terminated or truncated:
            print("  Episode ended, resetting...")
            obs, info = env.reset()

    print("\n=== Testing curriculum update ===")
    env.set_curriculum(orb_spawn_radius=50.0)
    print("  Set orb_spawn_radius to 50.0")

    print("\nClosing environment...")
    env.close()
    print("Done!")

if __name__ == "__main__":
    main()
