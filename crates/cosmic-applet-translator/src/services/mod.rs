//! Side effects used by the applet: HUD messaging, overlay processes and the region
//! selector. `update` handlers call these and wrap the result in a `Task`.

pub mod hud;
pub mod process;
pub mod selector;

pub use hud::HudService;
pub use selector::RegionKind;
