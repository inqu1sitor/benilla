//! Lua of the launcher's own: functions it installs into each new VM, and one glue addon of its
//! own, loaded off disk after the stock GlueXML and benilla's glue layer.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;

/// Which UI a VM runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmKind {
    /// The glue screens: login, character select, the loading screen's caller.
    Glue,
    /// The in-game UI, built at each world entry and `ReloadUI()`.
    World,
}

/// Installs globals into a fresh VM before any interface file runs on it. Called once per VM; an
/// error is logged and the VM goes on without what failed.
pub type ScriptHook = Arc<dyn Fn(&mlua::Lua, VmKind) -> mlua::Result<()> + Send + Sync>;

use super::mlua;

/// A glue addon the launcher ships: `<root>/<name>/<name>.toc` and its files, loaded after the
/// stock `GlueXML.toc` and benilla's glue layer, before `FRAMES_LOADED`. Its paths resolve under
/// `Interface\AddOns\<name>\`, as an in-game addon's do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlueAddon {
    pub root: PathBuf,
    pub name: String,
}

/// The launcher's script hooks. Insert or edit it from [`crate::run_with`]'s `extend`; the glue
/// VM built at `Startup` already sees it.
#[derive(Resource, Default, Clone)]
pub struct ScriptHooks {
    hooks: Vec<ScriptHook>,
    glue_addon: Option<GlueAddon>,
}

impl ScriptHooks {
    /// Add `hook`, run on every VM from now on, after the ones added before it.
    pub fn add(&mut self, hook: ScriptHook) -> &mut Self {
        self.hooks.push(hook);
        self
    }

    /// Load `addon` on every glue build.
    pub fn set_glue_addon(&mut self, addon: GlueAddon) -> &mut Self {
        self.glue_addon = Some(addon);
        self
    }

    pub(crate) fn glue_addon(&self) -> Option<&GlueAddon> {
        self.glue_addon.as_ref()
    }

    /// Run every hook on `lua`; a failing hook is logged and the rest still run.
    pub(crate) fn install(&self, lua: &mlua::Lua, kind: VmKind) {
        for hook in &self.hooks {
            if let Err(e) = hook(lua, kind) {
                error!("embed: a script hook failed on the {kind:?} VM: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hook_runs_in_order_and_a_failure_does_not_stop_the_rest() {
        let lua = mlua::Lua::new();
        let mut hooks = ScriptHooks::default();
        hooks
            .add(Arc::new(|lua, kind| {
                lua.globals().set("FIRST", kind == VmKind::Glue)?;
                Ok(())
            }))
            .add(Arc::new(|_, _| Err(mlua::Error::RuntimeError("no".into()))))
            .add(Arc::new(|lua, _| {
                let f = lua.create_function(|_, ()| Ok(7))?;
                lua.globals().set("Seven", f)?;
                Ok(())
            }));
        hooks.install(&lua, VmKind::Glue);
        let first: bool = lua.globals().get("FIRST").unwrap();
        assert!(first);
        let seven: i64 = lua.load("return Seven()").eval().unwrap();
        assert_eq!(seven, 7);
    }
}
