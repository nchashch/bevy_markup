//! The agent/QA tool surface behind the crate's `dev-tools` feature:
//! [`bevy_mcp_harness`] serves the example over localhost BRP (JSON-RPC on
//! 127.0.0.1:15702 — `game/screenshot`, `game/ui`, `game/keyboard`,
//! `game/gamepad`, `game/mouse`) and MCP (127.0.0.1:15710/mcp), so an agent
//! can playtest, click and capture without touching the example's code.
//!
//! Off by default (`--features dev-tools`); the plugin compiles away
//! otherwise, so examples ship unchanged. Screenshots land in
//! `mcp_harness/screenshots/` (gitignored), full-resolution PNGs with a
//! state sidecar. Port flags: `--brp-port N`, `--mcp-port N`, `--no-render`.
//!
//! Dev/QA only: BRP is unauthenticated by design and binds to 127.0.0.1.

use bevy::prelude::*;

/// Adds [`bevy_mcp_harness::BevyMcpHarnessPlugin`] when the `dev-tools`
/// feature is on.
pub struct HarnessPlugin;

#[cfg(feature = "dev-tools")]
impl Plugin for HarnessPlugin {
    fn build(&self, app: &mut App) {
        let config = bevy_mcp_harness::McpHarnessConfig {
            // The examples' UIs declare clicks via `data-on-click` element
            // signals, not `bevy_ui::Interaction` — without this hook
            // `game/ui` would never report the buttons `clickable`.
            clickable: Some(std::sync::Arc::new(markup_clickable)),
            ..bevy_mcp_harness::McpHarnessConfig::from_env()
        };
        app.add_plugins(bevy_mcp_harness::BevyMcpHarnessPlugin { config });
    }
}

#[cfg(not(feature = "dev-tools"))]
impl Plugin for HarnessPlugin {
    fn build(&self, _app: &mut App) {}
}

/// An element carries `data-on-*` hooks: clickable under bevy_markup's
/// convention.
#[cfg(feature = "dev-tools")]
fn markup_clickable(world: &World, entity: Entity) -> bool {
    world
        .get::<bevy_markup::prelude::ElementSignals>(entity)
        .is_some_and(|signals| !signals.0.is_empty())
}
