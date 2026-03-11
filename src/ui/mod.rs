pub mod in_game;
pub mod pause_menu;
pub mod toasts;

#[cfg(feature = "ai")]
pub mod ai_debug;

#[cfg(feature = "ai")]
pub use ai_debug::AiDebugUiPlugin;
pub use in_game::InGameUiPlugin;
pub use pause_menu::{PauseMenuUiPlugin, PauseMenuState, is_pause_menu_open};
pub use toasts::ToastUiPlugin;
