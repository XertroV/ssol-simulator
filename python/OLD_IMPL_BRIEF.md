This document serves as the **Implementation Brief** for an autonomous coding agent (Claude Opus 4.5) to build the AI training infrastructure for *A Slower Speed of Light*.

## 1. Project Overview & Goals

**Objective:** Train a Reinforcement Learning (RL) agent to play a custom Rust/Bevy port of *A Slower Speed of Light* at a superhuman level.
**Success Metric:** The agent must learn a route that maximizes speed and orb collection efficiency, likely discovering non-intuitive paths (skipping orbs to maintain velocity, backtracking later).
**Key Constraint:** The map is static. We want the agent to "memorize" this specific level similar to how a human speedrunner learns a specific game.

### Technical Approach

* **Engine:** Rust (Bevy) via the `ssol_simulator` crate.
* **ML Framework:** Python (Stable-Baselines3 + PyTorch).
* **Architecture:** **Recurrent PPO (PPO + LSTM)**.
* **Interface:** `bevy_rl` (or custom socket/FFI bridge) to pass observations and actions.

---

## 2. The Architecture

We are avoiding the "Vision (CNN)" approach in favor of a **"Blind God Mode"** approach. We will feed the agent precise geometric data and game state, stripping away the relativistic visual distortion that makes the game hard for humans.

### The "Global Memory, Local Tactics" Logic

The observation space is designed to solve two problems:

1. **Strategy:** "Which orbs are left on the map?" (Global Checklist).
2. **Navigation:** "How do I move to the specific orb I chose?" (Local NavMesh Vectors).

To bridge these two, we use **Explicit ID Tagging**: The local navigation vectors will carry the ID of the orb they point to, allowing the Neural Network to correlate "The orb to my left" with "Orb #42 (which is strategic to skip)."

---

## 3. Rust Implementation (`ssol_simulator`)

The Rust side is responsible for "Pre-Processing" raw game data into a highly digestible format for the Neural Network.

### A. The Observation Space (Input)

Implement a system that collects the following data every tick and flattens it into a single `Float32` array or a Dictionary.

**1. Global State (Strategy)**

* `orb_checklist`: `[f32; 100]` (Fixed size).
* `1.0` = Orb Active (Uncollected).
* `0.0` = Orb Collected.



**2. Proprioception (Body State)**

* `player_velocity_local`: `[f32; 3]` (Velocity relative to player facing).
* `current_speed_of_light`: `f32` (The central game variable affecting friction/FOV).
* `combo_timer`: `f32` (Time remaining before speed boost decays).
* `can_jump`: `f32` (1.0 or 0.0).

**3. Local Sensing (The "Eyes")**

* `wall_rays`: `[f32; 16]`
* Raycasts in a circle at waist height.
* Return normalized distance (0.0 = touching wall, 1.0 = far).
* **Crucial:** Use Euclidean geometry, *ignoring* relativistic length contraction.



**4. NavMesh Guidance (The "Compass")**

* **Logic:** For the **Nearest 5 Active Orbs** (sorted by path distance), query the NavMesh.
* **Do not** point to the Orb. **Point to the next waypoint** (corner) on the path.
* **Data Structure per Target (x5):**
* `dir_x, dir_y, dir_z`: Normalized vector to the *Next Waypoint* (Local Space).
* `distance`: Path distance to the actual orb.
* `id_tag`: `[f32; 7]` (Binary encoding of the Orb's integer ID).



### B. The Action Space (Output)

The Agent will output a vector that Rust must interpret:

1. **Mouse Look:** 2 Continuous Floats (Pitch, Yaw delta). *Apply strictly as camera rotation.*
2. **Movement:** Multi-Discrete or Discrete.
* *Recommendation:* **Multi-Discrete** `[3, 3]` (Forward/Back/None, Left/Right/None) + `[2]` (Jump/None).



### C. The Reward Function (Rust Side)

Calculate this per tick and send it to Python.

* `+10.0`: Collected Orb.
* `-0.01`: Per tick (Existence penalty to force speed).
* `+0.05 * (player_speed / max_speed)`: Momentum bonus. Encourages maintaining the speed boost.

---

## 4. Python Implementation (`/scripts` directory)

Use **Stable-Baselines3 Contrib** to access `RecurrentPPO`.

### A. Environment Setup

* Create a custom Gym Environment (`SSOLEnv`) that wraps the communication with the Bevy binary.
* Handle the "reset" signal (teleport player to start, reset orbs).
* **Headless Training:** Ensure the python script launches the Bevy app with graphics disabled (e.g., specific flag or simply not adding the `DefaultPlugins` rendering) to maximize FPS.

### B. Network Configuration

* **Algorithm:** `RecurrentPPO`.
* **Policy:** `MlpLstmPolicy`.
* **Hyperparameters:**
* `n_steps`: 2048 (Longer horizons are better for navigation).
* `batch_size`: 64.
* `learning_rate`: 3e-4.
* `ent_coef`: 0.01 (Entropy coefficient to encourage exploration early on).



---

## 5. Potential Pitfalls & "Gotchas"

1. **The Coordinate Disconnect:**
* *Issue:* The game code likely simulates Relativistic Warping on the CPU for the player.
* *Fix:* Ensure the `AiSensors` read the **absolute, non-warped** coordinates (Newtonian truth). The AI is playing the "Server State," not the "Client View."


2. **NavMesh Jitter:**
* *Issue:* As the player moves, the "Next Waypoint" might snap effectively behind them or oscillate.
* *Fix:* Implement a small "acceptance radius" in Rust. If `dist(player, waypoint) < 0.5m`, immediately target the *subsequent* waypoint.


3. **Binding Confusion:**
* *Issue:* If the ID tags are not strictly binary (e.g., using a float 0.0-100.0), the network will fail to distinguish ID 50 from ID 51.
* *Fix:* Verify the 7-bit encoding is clean `0.0` or `1.0`.



## 6. Roadmap for Opus

1. **Rust System:** Implement `NavMesh` integration and the `AiSensors` struct. Verify the "Next Waypoint" logic visually (debug lines) before training.
2. **Python Interface:** Build the gym wrapper and verify it can receive data and step the Bevy game.
3. **Training:** Run a baseline PPO (non-LSTM) to see if it can walk. Then switch to LSTM for the full strategy training.

## 7. Ideas for the Future (Out of Scope)

* **Transformer Model:** If PPO fails to learn complex skipping strategies, we will switch to an Entity Transformer architecture. We are designing the data (List of Orbs) such that we can easily swap to this later.
* **Curriculum Learning:** If the agent fails, we may need to implement a mode where only 10 orbs spawn, then 20, then 100.
