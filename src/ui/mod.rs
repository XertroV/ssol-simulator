pub mod finish_screen;
pub mod in_game;
pub mod pause_menu;
pub mod screenshot_harness;
pub mod toasts;

#[cfg(feature = "ai")]
pub mod ai_debug;

#[cfg(feature = "ai")]
pub use ai_debug::AiDebugUiPlugin;
pub use finish_screen::FinishScreenUiPlugin;
pub use in_game::InGameUiPlugin;
pub use pause_menu::{PauseMenuUiPlugin, PauseMenuState, is_pause_menu_open};
pub use screenshot_harness::{UiScreenshotConfig, run_ui_screenshot_suite};
// Also available for harness consumers / tests:
// `UiScreenshotScenario`, `all_ui_screenshot_scenarios`
pub use toasts::ToastUiPlugin;
