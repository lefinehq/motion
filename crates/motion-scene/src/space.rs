//! 2.5D space ([D58]): a 3D layer's matrix to its composition's space, the
//! camera a composition is seen through, and the projective matrix that lays
//! a layer's plane onto the canvas.
//!
//! Space is in px with X to the right, Y down and Z away from the viewer, so
//! a turn about Z is clockwise on screen, as `rotate` is on a flat layer.
//!
//! [D58]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera

use glam::{DMat3, DMat4, DQuat, DVec2, DVec3, DVec4, EulerRot};
use kurbo::{Affine, Point};
use motion_format::Value;
use motion_format::property::{Prop, default_zoom};

/// How close to the camera's plane a point may come before it counts as
/// behind it, in px of depth.
pub const NEAR: f64 = 1e-3;

/// The turn three orientation angles in degrees give: about X, then Y, then
/// Z as the layer sees it, so a point turns about Z first.
pub fn orientation(angles: [f64; 3]) -> DQuat {
    let [x, y, z] = angles.map(f64::to_radians);
    DQuat::from_rotation_x(x) * DQuat::from_rotation_y(y) * DQuat::from_rotation_z(z)
}

/// The three angles in degrees of `turn`, as [`orientation`] reads them.
pub fn angles(turn: DQuat) -> [f64; 3] {
    let (x, y, z) = turn.to_euler(EulerRot::XYZ);
    [x, y, z].map(f64::to_degrees)
}

/// A 3D node's own matrix from its values, one per layer property in
/// registry order: move the anchor to the origin, scale, turn about Z, then
/// Y, then X, then orient, then move to the position.
///
/// # Panics
/// If `values` does not follow the registry's types.
pub fn local_matrix(values: &[Value]) -> DMat4 {
    let number = |prop: Prop| values[prop.index()].as_number().expect("a number property");
    let point = |prop: Prop| values[prop.index()].as_point().expect("a point property");
    let [x, y] = point(Prop::POSITION);
    let [ax, ay] = point(Prop::ANCHOR);
    let [sx, sy] = point(Prop::SCALE);
    let orient = values[Prop::ORIENT.index()]
        .as_orientation()
        .expect("an orientation property");
    DMat4::from_translation(DVec3::new(x, y, number(Prop::Z)))
        * DMat4::from_quat(orientation(orient))
        * DMat4::from_rotation_x(number(Prop::ROTATE_X).to_radians())
        * DMat4::from_rotation_y(number(Prop::ROTATE_Y).to_radians())
        * DMat4::from_rotation_z(number(Prop::ROTATE).to_radians())
        * DMat4::from_scale(DVec3::new(sx, sy, number(Prop::SCALE_Z)))
        * DMat4::from_translation(-DVec3::new(ax, ay, number(Prop::ANCHOR_Z)))
}

/// A flat transform as a matrix of space that keeps depth.
pub fn lift(transform: Affine) -> DMat4 {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    DMat4::from_cols(
        DVec4::new(a, b, 0.0, 0.0),
        DVec4::new(c, d, 0.0, 0.0),
        DVec4::Z,
        DVec4::new(e, f, 0.0, 1.0),
    )
}

/// A matrix of space seen straight on, without depth or perspective: what a
/// flat layer under a 3D parent moves with, and how checks that read flat
/// boxes see a 3D layer.
pub fn flatten(matrix: DMat4) -> Affine {
    let [x, y, _, w] = matrix.to_cols_array_2d();
    Affine::new([x[0], x[1], y[0], y[1], w[0], w[1]])
}

/// The viewpoint a composition's 3D nodes are seen through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// From the composition's space to the camera's, whose origin is the
    /// camera, X right, Y down and Z the way it looks.
    pub matrix: DMat4,
    /// The distance in px at which a plane facing the camera shows at 100 %.
    pub zoom: f64,
    /// Where the camera's axis meets the canvas, its centre.
    pub centre: DVec2,
}

impl View {
    /// The default camera of a canvas `width` by `height` px: a 39.6°
    /// horizontal view, centred, at minus its zoom, so a plane at depth 0
    /// shows as a flat layer would.
    pub fn default_for(width: f64, height: f64) -> View {
        let zoom = default_zoom(width);
        let centre = DVec2::new(width / 2.0, height / 2.0);
        View {
            matrix: DMat4::from_translation(-centre.extend(-zoom)),
            zoom,
            centre,
        }
    }

    /// The view of a camera whose matrix to the composition's space is
    /// `world`, with `zoom`, on a canvas `width` by `height` px.
    pub fn camera(world: DMat4, zoom: f64, width: f64, height: f64) -> View {
        View {
            matrix: world.inverse(),
            zoom,
            centre: DVec2::new(width / 2.0, height / 2.0),
        }
    }

    /// The perspective, from the camera's space to the canvas in
    /// homogeneous coordinates whose third is the depth.
    fn perspective(&self) -> DMat4 {
        DMat4::from_cols(
            DVec4::new(self.zoom, 0.0, 0.0, 0.0),
            DVec4::new(0.0, self.zoom, 0.0, 0.0),
            DVec4::new(self.centre.x, self.centre.y, 1.0, 0.0),
            DVec4::W,
        )
    }

    /// The projective matrix from the plane `z = 0` of a node whose matrix
    /// to the composition's space is `world` to the canvas: `(x, y, 1)` maps
    /// to `(X, Y, W)`, the point `(X/W, Y/W)`, with `W` its depth in front
    /// of the camera; at or below [`NEAR`] it is behind it.
    pub fn plane(&self, world: DMat4) -> DMat3 {
        let m = self.perspective() * self.matrix * world;
        // The perspective writes the depth into the third row, and the
        // plane's own depth is 0, so its column drops out.
        let rows = |c: DVec4| c.truncate();
        DMat3::from_cols(rows(m.x_axis), rows(m.y_axis), rows(m.w_axis))
    }

    /// The depth in front of the camera of `point` on the plane of a node
    /// whose matrix is `world`: what 3D nodes are sorted by.
    pub fn depth(&self, world: DMat4, point: Point) -> f64 {
        (self.matrix * world)
            .transform_point3(DVec3::new(point.x, point.y, 0.0))
            .z
    }
}

/// The matrix of a camera's own turn to look from `from` at `to`, both in
/// its parent's space: its Z towards `to`, its Y as close to down as it can.
/// `None` when the two points meet.
pub fn aim(from: DVec3, to: DVec3) -> Option<DMat4> {
    let z = (to - from).try_normalize()?;
    // Down, unless the camera looks straight up or down: then away.
    let reference = match z.cross(DVec3::Y).length_squared() < 1e-12 {
        true => DVec3::Z,
        false => DVec3::Y,
    };
    let x = reference.cross(z).normalize();
    let y = z.cross(x);
    Some(DMat4::from_cols(
        x.extend(0.0),
        y.extend(0.0),
        z.extend(0.0),
        DVec4::W,
    ))
}

/// A camera's own matrix from its values: its position, its turn towards
/// its target when it `aim`s, then its orientation and rotations. A camera's
/// scale and anchor do not apply.
///
/// # Panics
/// If `values` does not follow the registry's types.
pub fn camera_matrix(values: &[Value], aims: bool) -> DMat4 {
    let number = |prop: Prop| values[prop.index()].as_number().expect("a number property");
    let point = |prop: Prop| values[prop.index()].as_point().expect("a point property");
    let [x, y] = point(Prop::POSITION);
    let at = DVec3::new(x, y, number(Prop::Z));
    let [tx, ty] = point(Prop::TARGET);
    let turn = match aims {
        true => aim(at, DVec3::new(tx, ty, number(Prop::TARGET_Z))).unwrap_or(DMat4::IDENTITY),
        false => DMat4::IDENTITY,
    };
    let orient = values[Prop::ORIENT.index()]
        .as_orientation()
        .expect("an orientation property");
    DMat4::from_translation(at)
        * turn
        * DMat4::from_quat(orientation(orient))
        * DMat4::from_rotation_x(number(Prop::ROTATE_X).to_radians())
        * DMat4::from_rotation_y(number(Prop::ROTATE_Y).to_radians())
        * DMat4::from_rotation_z(number(Prop::ROTATE).to_radians())
}

/// The point `p` of a plane through the projective matrix `plane`, or
/// `None` when it is behind the camera.
pub fn project(plane: DMat3, p: Point) -> Option<Point> {
    let v = plane * DVec3::new(p.x, p.y, 1.0);
    (v.z > NEAR).then(|| Point::new(v.x / v.z, v.y / v.z))
}

#[cfg(test)]
#[path = "../tests/unit/space.rs"]
mod tests;
