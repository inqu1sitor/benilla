//! A fixed overhead camera for a board game played in the world: the launcher names a point, a
//! facing, a pitch and a distance, and the world camera sits there instead of orbiting the
//! player. The cursor stays free (a press never starts mouse-look), and the wheel zooms within
//! the view's bounds. Clicks still reach the world, so units are targeted and hovered as usual.

use bevy::prelude::*;

use benilla_assets::coords::wow_to_bevy;
use benilla_world::view::{CameraPoseSet, WorldCamera};

use crate::player::{CameraControl, PlayerControlSet};

/// Yards per zoom notch: twice the stock wheel binding's `CameraZoomIn(1.0)`, as a board view
/// spans tens of yards.
const ZOOM_STEP: f32 = 2.0;

/// The rig's own distance while the board holds the camera: benilla's starting zoom, 15 yd.
const RIG_DISTANCE: f32 = 15.0;

/// One board view, in WoW coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardView {
    /// The point the camera looks at: WoW `[x, y, z]`.
    pub focus: [f32; 3],
    /// The direction the camera faces, radians, WoW's orientation convention (0 looks along +x,
    /// counter-clockwise seen from above).
    pub yaw: f32,
    /// How far the camera looks down, radians, WoW's pitch convention (negative looks down, -π/2
    /// straight down). A board camera never looks up, so the sign is not read: 0.9 and -0.9 are
    /// the same view.
    pub pitch: f32,
    /// Yards from the camera to `focus`.
    pub distance: f32,
    /// The zoom's bounds, yards.
    pub min_distance: f32,
    pub max_distance: f32,
}

impl BoardView {
    /// The camera's eye and look target in Bevy space at `distance`.
    pub fn pose(&self, distance: f32) -> (Vec3, Vec3) {
        let pitch = self.pitch.abs().min(std::f32::consts::FRAC_PI_2 - 1.0e-3);
        let look = [
            self.yaw.cos() * pitch.cos(),
            self.yaw.sin() * pitch.cos(),
            -pitch.sin(),
        ];
        let eye = [
            self.focus[0] - look[0] * distance,
            self.focus[1] - look[1] * distance,
            self.focus[2] - look[2] * distance,
        ];
        (wow_to_bevy(eye), wow_to_bevy(self.focus))
    }

    /// `d` inside the zoom's bounds; bounds given the wrong way round are taken as a range.
    fn clamp(&self, d: f32) -> f32 {
        let (lo, hi) = if self.min_distance <= self.max_distance {
            (self.min_distance, self.max_distance)
        } else {
            (self.max_distance, self.min_distance)
        };
        d.clamp(lo.max(0.5), hi.max(0.5))
    }
}

/// The board camera: `Some` holds the world camera on a [`BoardView`]; `None` (the default) is
/// the stock third-person camera.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct BoardCamera {
    view: Option<BoardView>,
    /// The live zoom, yards; set from the view's distance when a view is set.
    distance: f32,
}

impl BoardCamera {
    /// Hold the camera on `view`, zoomed to its distance.
    pub fn set(&mut self, view: BoardView) {
        self.distance = view.clamp(view.distance);
        self.view = Some(view);
    }

    /// Give the camera back to the player.
    pub fn clear(&mut self) {
        self.view = None;
    }

    pub fn view(&self) -> Option<&BoardView> {
        self.view.as_ref()
    }

    /// The live zoom distance, yards.
    pub fn distance(&self) -> f32 {
        self.distance
    }

    fn zoom(&mut self, notches: f32) {
        if let Some(view) = &self.view {
            self.distance = view.clamp(self.distance - notches * ZOOM_STEP);
        }
    }
}

pub(super) fn build(app: &mut App) {
    app.init_resource::<BoardCamera>().add_systems(
        Update,
        hold_board_camera
            .after(PlayerControlSet)
            .before(CameraPoseSet)
            .in_set(benilla_world::schedule::WorldStage::Input)
            .in_set(crate::client_state::InWorldGated),
    );
}

/// Seat the world camera on the board view after the controller wrote its own pose, take the
/// frame's zoom, and keep the rig from taking the cursor.
fn hold_board_camera(
    mut board: ResMut<BoardCamera>,
    mut rig: ResMut<CameraControl>,
    binds: Res<crate::bindings::BindingsState>,
    mut camera: Query<&mut Transform, With<WorldCamera>>,
) {
    let held = board.view.is_some();
    if rig.board_held != held {
        rig.board_held = held;
    }
    let Some(view) = board.view else {
        return;
    };
    let notches = binds.amount(crate::bindings::Input::CameraZoomIn)
        - binds.amount(crate::bindings::Input::CameraZoomOut);
    if notches != 0.0 {
        board.zoom(notches);
    }
    // The rig keeps a third-person distance, so the player's own model never fades out under
    // the stock first-person fade while the wheel zooms the board.
    rig.park_distance(RIG_DISTANCE);
    let (eye, target) = view.pose(board.distance);
    for mut t in &mut camera {
        let want = Transform::from_translation(eye).looking_at(target, Vec3::Y);
        if *t != want {
            *t = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> BoardView {
        BoardView {
            focus: [100.0, 200.0, 10.0],
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_4,
            distance: 30.0,
            min_distance: 20.0,
            max_distance: 40.0,
        }
    }

    #[test]
    fn the_eye_sits_behind_and_above_the_focus_along_the_yaw() {
        let (eye, target) = view().pose(30.0);
        let down = BoardView {
            pitch: -std::f32::consts::FRAC_PI_4,
            ..view()
        };
        assert_eq!(
            down.pose(30.0),
            (eye, target),
            "the pitch's sign is not read"
        );
        assert_eq!(target, wow_to_bevy([100.0, 200.0, 10.0]));
        let eye_wow = benilla_assets::coords::bevy_to_wow(eye);
        let r = 30.0 * std::f32::consts::FRAC_1_SQRT_2;
        assert!((eye_wow[0] - (100.0 - r)).abs() < 1e-3, "{eye_wow:?}");
        assert!((eye_wow[1] - 200.0).abs() < 1e-3, "{eye_wow:?}");
        assert!((eye_wow[2] - (10.0 + r)).abs() < 1e-3, "{eye_wow:?}");
    }

    #[test]
    fn the_zoom_stays_in_bounds() {
        let mut board = BoardCamera::default();
        board.set(BoardView {
            distance: 99.0,
            ..view()
        });
        assert_eq!(board.distance(), 40.0);
        board.zoom(100.0);
        assert_eq!(board.distance(), 20.0);
        board.zoom(-1.0);
        assert_eq!(board.distance(), 22.0);
        board.clear();
        assert!(board.view().is_none());
    }
}
