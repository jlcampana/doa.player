use glam::{Mat4, Vec2, Vec3};

/// Orbit camera: rotates around a target point with zoom.
pub struct Camera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,   // radians, rotation around Y axis
    pub pitch: f32, // radians, elevation
    pub fov_y: f32, // radians
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    /// Where drags turn around, set by a click. None turns around the target.
    pub pivot: Option<Vec3>,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            target: Vec3::new(0.0, 1.0, 0.0),
            distance: 5.0,
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.3,
            fov_y: 60.0_f32.to_radians(),
            aspect,
            near: 0.01,
            far: 1000.0,
            pivot: None,
        }
    }

    pub fn position(&self) -> Vec3 {
        let x = self.distance * self.yaw.sin() * self.pitch.cos();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.yaw.cos() * self.pitch.cos();
        self.target + Vec3::new(x, y, z)
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position(), self.target, Vec3::Y)
    }

    pub fn proj_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far)
    }

    pub fn view_proj(&self) -> Mat4 {
        self.proj_matrix() * self.view_matrix()
    }

    /// Handle drag input. delta is in pixels.
    pub fn orbit(&mut self, delta: Vec2) {
        let held = self.pivot.map(|pivot| (pivot, self.view_matrix().transform_point3(pivot)));
        self.yaw -= delta.x * 0.005;
        self.pitch += delta.y * 0.005;
        self.pitch = self.pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
        // The pivot stays where it is on screen, and the view moves around it.
        if let Some((pivot, on_screen)) = held {
            let offset = self.position() - self.target;
            let turn = Mat4::look_at_rh(offset, Vec3::ZERO, Vec3::Y);
            let eye = pivot - turn.inverse().transform_vector3(on_screen);
            self.target = eye - offset;
        }
    }

    /// The ray through a window position, as origin and direction.
    pub fn ray(&self, x: f32, y: f32, width: f32, height: f32) -> (Vec3, Vec3) {
        let ndc = glam::Vec4::new(x / width * 2.0 - 1.0, 1.0 - y / height * 2.0, 1.0, 1.0);
        let far = self.view_proj().inverse() * ndc;
        let origin = self.position();
        (origin, (far.truncate() / far.w - origin).normalize())
    }

    /// Radius per unit of distance of a number of pixels, for a window height.
    pub fn pixel_spread(&self, pixels: f32, height: f32) -> f32 {
        (self.fov_y * 0.5).tan() * 2.0 * pixels / height
    }

    /// Handle scroll for zoom.
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance - delta * 0.3 * (self.distance * 0.3).min(1.0)).clamp(0.1, 50.0);
    }

    /// Pan the target in camera-relative X/Y.
    pub fn pan(&mut self, delta: Vec2) {
        let right = (self.position() - self.target).cross(Vec3::Y).normalize();
        let up = right.cross((self.position() - self.target).normalize());
        self.target += right * (delta.x * 0.002 * self.distance)
            + up * (delta.y * 0.002 * self.distance);
    }

    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    /// Reset to default framing for a character of given height.
    pub fn frame_character(&mut self, height: f32) {
        let h = height.max(1.0);
        self.target = Vec3::new(0.0, h * 0.5, 0.0);
        self.distance = h * 1.2;
        self.pitch = 0.1;
        self.yaw = std::f32::consts::FRAC_PI_4;
        self.pivot = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pivot_stays_in_place_on_screen_while_orbiting() {
        let mut camera = Camera::new(1.5);
        let point = Vec3::new(0.3, 1.4, -0.2);
        camera.pivot = Some(point);
        let before = camera.view_matrix().transform_point3(point);
        let (position, target) = (camera.position(), camera.target);
        camera.orbit(Vec2::new(40.0, -25.0));
        assert!(camera.view_matrix().transform_point3(point).distance(before) < 1e-3);
        assert!(camera.position().distance(position) > 0.01 && camera.target.distance(target) > 0.01);
    }

    #[test]
    fn without_a_pivot_orbiting_keeps_the_target() {
        let mut camera = Camera::new(1.5);
        let target = camera.target;
        camera.orbit(Vec2::new(40.0, -25.0));
        assert_eq!(camera.target, target);
    }

    #[test]
    fn the_ray_through_the_center_looks_at_the_target() {
        let camera = Camera::new(1.5);
        let (origin, direction) = camera.ray(300.0, 200.0, 600.0, 400.0);
        let to_target = (camera.target - origin).normalize();
        assert!(direction.distance(to_target) < 1e-4, "{direction} {to_target}");
    }
}
