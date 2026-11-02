//! Hooks for a launcher built on top of benilla ([`crate::run_with`]): small, generic pieces a
//! game mode needs that 1.12.1 itself never had. None of them changes what stock benilla does;
//! each is inert until the embedding crate turns it on.
//!
//! - [`install`]: find or ask for the player's 1.12.1 install before the client starts.
//! - [`ScriptHooks`]: Lua functions of the launcher's own, installed into every glue and in-game
//!   VM before its files load, and one glue addon of its own loaded after the stock GlueXML.
//! - [`BoardCamera`]: a fixed overhead camera with a free cursor and a bounded zoom.
//! - [`CursorWorldPoint`]: the world point under the cursor, every frame in the world.

pub mod install;

mod board_camera;
mod cursor_world;
mod script;

pub use board_camera::{BoardCamera, BoardView};
pub use cursor_world::CursorWorldPoint;
pub use script::{GlueAddon, ScriptHook, ScriptHooks, VmKind};

/// The interpreter the UI runs on, for a [`ScriptHook`] to build functions with.
pub use benilla_ui::mlua;

use bevy::prelude::*;

/// Registers the resources and systems behind the hooks; always on, inert until used.
pub(crate) struct EmbedPlugin;

impl Plugin for EmbedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptHooks>();
        board_camera::build(app);
        cursor_world::build(app);
    }
}
