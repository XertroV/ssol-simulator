pub mod in_game;

#[cfg(feature = "ai")]
pub mod ai_debug;

#[cfg(feature = "ai")]
pub use ai_debug::AiDebugUiPlugin;
pub use in_game::InGameUiPlugin;
