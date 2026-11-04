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
        self.yaw -= delta.x * 0.005;
        self.pitch += delta.y * 0.005;
        self.pitch = self.pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.05,
            std::f32::consts::FRAC_PI_2 - 0.05,
        );
    }

    /// Handle scroll for zoom.
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance - delta * 0.3).clamp(0.5, 50.0);
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
    }
}
