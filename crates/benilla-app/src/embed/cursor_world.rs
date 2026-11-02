//! The world point under the cursor: a ray from the world camera through the cursor, cast against
//! what a body walks on (terrain, WMO floors, doodads), every frame in the world.

use bevy::camera::Projection;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use benilla_assets::coords::bevy_to_wow;
use benilla_world::collision::WorldCollision;
use benilla_world::view::{CameraPoseSet, WorldCamera, CAM_FOVY};

/// How far the ray reaches, yards.
const REACH: f32 = 500.0;

/// The point under the cursor in WoW `[x, y, z]`, or `None` when the cursor is outside the
/// window or over nothing solid. Read it in `Update` after `CameraPoseSet`, or from a script
/// call, which sees the last frame's.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct CursorWorldPoint(pub Option<[f32; 3]>);

pub(super) fn build(app: &mut App) {
    app.init_resource::<CursorWorldPoint>().add_systems(
        Update,
        track_cursor_world_point
            .after(CameraPoseSet)
            .in_set(crate::client_state::InWorldGated),
    );
}

/// The ray through `cursor` (logical pixels from the top left of a `size` window) from a camera
/// at `camera` with vertical field of view `fovy` and `aspect`.
pub(crate) fn cursor_ray(
    cursor: Vec2,
    size: Vec2,
    camera: &GlobalTransform,
    fovy: f32,
    aspect: f32,
) -> Option<(Vec3, Dir3)> {
    if size.x <= 0.0 || size.y <= 0.0 {
        return None;
    }
    let ndc = Vec2::new(cursor.x / size.x * 2.0 - 1.0, 1.0 - cursor.y / size.y * 2.0);
    let half = (fovy * 0.5).tan();
    let local = Vec3::new(ndc.x * half * aspect, ndc.y * half, -1.0);
    let dir = Dir3::new(camera.rotation() * local).ok()?;
    Some((camera.translation(), dir))
}

fn track_cursor_world_point(
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&GlobalTransform, Option<&Projection>), With<WorldCamera>>,
    collide: WorldCollision,
    mut out: ResMut<CursorWorldPoint>,
) {
    let point = (|| {
        let window = window.single().ok()?;
        let cursor = window.cursor_position()?;
        let (gt, projection) = camera.iter().next()?;
        let size = Vec2::new(window.width(), window.height());
        let (fovy, aspect) = match projection {
            Some(Projection::Perspective(p)) => (p.fov, size.x / size.y),
            _ => (CAM_FOVY, size.x / size.y),
        };
        let (origin, dir) = cursor_ray(cursor, size, gt, fovy, aspect)?;
        let hit = collide.ray_body(origin, dir, REACH)?;
        Some(bevy_to_wow(origin + *dir * hit.distance))
    })();
    if out.0 != point {
        out.0 = point;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_centre_of_the_window_looks_down_the_camera_axis() {
        let cam = GlobalTransform::from(
            Transform::from_xyz(0.0, 10.0, 0.0).looking_at(Vec3::new(0.0, 0.0, -10.0), Vec3::Y),
        );
        let (origin, dir) = cursor_ray(
            Vec2::new(400.0, 300.0),
            Vec2::new(800.0, 600.0),
            &cam,
            CAM_FOVY,
            800.0 / 600.0,
        )
        .unwrap();
        assert_eq!(origin, Vec3::new(0.0, 10.0, 0.0));
        assert!((*dir - *cam.forward()).length() < 1e-5);
    }

    #[test]
    fn the_top_edge_looks_up_by_half_the_field_of_view() {
        let cam = GlobalTransform::IDENTITY;
        let (_, dir) = cursor_ray(
            Vec2::new(400.0, 0.0),
            Vec2::new(800.0, 600.0),
            &cam,
            CAM_FOVY,
            800.0 / 600.0,
        )
        .unwrap();
        let up = dir.y.atan2(-dir.z);
        assert!((up - CAM_FOVY * 0.5).abs() < 1e-5, "{up}");
        assert!(cursor_ray(Vec2::ZERO, Vec2::ZERO, &cam, CAM_FOVY, 1.0).is_none());
    }
}
