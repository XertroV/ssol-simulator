"""
Custom feature extractor for SSOL observations.

Handles the mixed observation space with proper embeddings for orb IDs
and combines all features through an MLP.
"""

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

    def __init__(
        self,
        observation_space: spaces.Dict,
        orb_embedding_dim: int = 16,
        hidden_dim: int = 256,
    ):
        """
        Initialize the feature extractor.

        Args:
            observation_space: The observation space from the environment
            orb_embedding_dim: Dimension of orb ID embeddings
            hidden_dim: Size of hidden layers and output features
        """
        # features_dim is the output dimension
        super().__init__(observation_space, features_dim=hidden_dim)

        self.orb_embedding_dim = orb_embedding_dim
        self.hidden_dim = hidden_dim

        # Calculate input feature dimensions:
        # - orb_checklist: 100
        # - player_position: 3
        # - camera_yaw: 1
        # - camera_pitch: 1
        # - player_velocity_local: 3
        # - player_velocity_world: 3
        # - speed_of_light_ratio: 1
        # - combo_timer: 1
        # - speed_multiplier: 1
        # - wall_rays: 16
        # Total player state: 14
        #
        # - orb_targets: 10 * (3 direction + 1 distance + orb_embedding_dim)

        player_state_dim = 3 + 1 + 1 + 3 + 3 + 1 + 1 + 1  # 14 (position, yaw, pitch, vel_local, vel_world, sol, combo, speed_mult)
        orb_target_dim = 10 * (3 + 1 + orb_embedding_dim)

        raw_features_dim = 100 + player_state_dim + 16 + orb_target_dim

        # Embedding layer for orb IDs
        # 101 entries: 0-99 for orbs, 100 for padding (mapped from -1)
        self.orb_embedding = nn.Embedding(
            num_embeddings=101,
            embedding_dim=orb_embedding_dim,
            padding_idx=100,
        )

        # MLP to process combined features
        self.mlp = nn.Sequential(
            nn.Linear(raw_features_dim, hidden_dim),
            nn.ReLU(),
            nn.Linear(hidden_dim, hidden_dim),
            nn.ReLU(),
        )

    def forward(self, observations: dict) -> torch.Tensor:
        """
        Extract features from observations.

        Args:
            observations: Dictionary of observation tensors

        Returns:
            Feature tensor of shape (batch_size, hidden_dim)
        """
        batch_size = observations["orb_checklist"].shape[0]

        # Orb checklist (100 binary values)
        orb_checklist = observations["orb_checklist"]  # (B, 100)

        # Player state - concatenate all continuous values
        player_state = torch.cat([
            observations["player_position"],        # (B, 3)
            observations["camera_yaw"],             # (B, 1)
            observations["camera_pitch"],           # (B, 1)
            observations["player_velocity_local"],  # (B, 3)
            observations["player_velocity_world"],  # (B, 3)
            observations["speed_of_light_ratio"],   # (B, 1)
            observations["combo_timer"],            # (B, 1)
            observations["speed_multiplier"],       # (B, 1)
        ], dim=-1)  # (B, 14)

        # Wall rays
        wall_rays = observations["wall_rays"]  # (B, 16)

        # Orb targets with embedding
        orb_directions = observations["orb_targets_direction"]  # (B, 10, 3)
        orb_distances = observations["orb_targets_distance"].unsqueeze(-1)  # (B, 10, 1)
        orb_ids = observations["orb_targets_id"]  # (B, 10)

        # Convert orb IDs for embedding:
        # - Valid IDs: 0-99 stay as-is
        # - Invalid/empty: -1 maps to 100 (padding index)
        orb_ids_long = orb_ids.long()
        orb_ids_for_embedding = torch.where(
            orb_ids_long < 0,
            torch.full_like(orb_ids_long, 100),
            orb_ids_long.clamp(0, 99),
        )

        # Get embeddings for orb IDs
        orb_embeds = self.orb_embedding(orb_ids_for_embedding)  # (B, 10, embed_dim)

        # Combine orb target features: direction + distance + embedding
        orb_features = torch.cat([
            orb_directions,  # (B, 10, 3)
            orb_distances,   # (B, 10, 1)
            orb_embeds,      # (B, 10, embed_dim)
        ], dim=-1)  # (B, 10, 3 + 1 + embed_dim)

        # Flatten orb features
        orb_features_flat = orb_features.view(batch_size, -1)  # (B, 10 * (4 + embed_dim))

        # Combine all features
        combined = torch.cat([
            orb_checklist,      # (B, 100)
            player_state,       # (B, 15)
            wall_rays,          # (B, 16)
            orb_features_flat,  # (B, 10 * (4 + embed_dim))
        ], dim=-1)

        # Pass through MLP
        return self.mlp(combined)


class SSOLFeatureExtractorLight(BaseFeaturesExtractor):
    """
    Lighter feature extractor that skips orb ID embeddings.

    Useful for faster training or when orb identity doesn't matter.
    Uses the raw orb ID as a normalized scalar instead of an embedding.
    """

    def __init__(
        self,
        observation_space: spaces.Dict,
        hidden_dim: int = 256,
    ):
        super().__init__(observation_space, features_dim=hidden_dim)

        # Feature dimensions (no embedding):
        # - orb_checklist: 100
        # - player_state: 15
        # - wall_rays: 16
        # - orb_targets: 10 * (3 direction + 1 distance + 1 normalized_id) = 50
        raw_features_dim = 100 + 15 + 16 + 50

        self.mlp = nn.Sequential(
            nn.Linear(raw_features_dim, hidden_dim),
            nn.ReLU(),
            nn.Linear(hidden_dim, hidden_dim),
            nn.ReLU(),
        )

    def forward(self, observations: dict) -> torch.Tensor:
        batch_size = observations["orb_checklist"].shape[0]

        orb_checklist = observations["orb_checklist"]

        player_state = torch.cat([
            observations["player_position"],
            observations["camera_yaw"],
            observations["camera_pitch"],
            observations["player_velocity_local"],
            observations["player_velocity_world"],
            observations["speed_of_light_ratio"],
            observations["combo_timer"],
            observations["speed_multiplier"],
        ], dim=-1)

        wall_rays = observations["wall_rays"]

        # Orb targets without embedding - normalize ID to [0, 1]
        orb_directions = observations["orb_targets_direction"]  # (B, 10, 3)
        orb_distances = observations["orb_targets_distance"].unsqueeze(-1)  # (B, 10, 1)
        orb_ids = observations["orb_targets_id"].unsqueeze(-1)  # (B, 10, 1)

        # Normalize orb IDs: map [-1, 99] to [0, 1]
        # -1 (empty) -> 0, 0-99 -> 0.01-1.0
        orb_ids_normalized = (orb_ids + 1) / 100.0

        orb_features = torch.cat([
            orb_directions,
            orb_distances / 1000.0,  # Normalize distance
            orb_ids_normalized,
        ], dim=-1)

        orb_features_flat = orb_features.view(batch_size, -1)

        combined = torch.cat([
            orb_checklist,
            player_state,
            wall_rays,
            orb_features_flat,
        ], dim=-1)

        return self.mlp(combined)
