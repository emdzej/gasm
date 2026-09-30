//! Minimal column-major 4x4 matrices (WGSL layout, WebGPU clip z in [0, 1]).

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Mat4(pub [f32; 16]);

#[derive(Clone, Copy, Debug)]
pub struct Vec3(pub f32, pub f32, pub f32);

impl Vec3 {
    fn sub(self, o: Vec3) -> Vec3 { Vec3(self.0 - o.0, self.1 - o.1, self.2 - o.2) }
    fn dot(self, o: Vec3) -> f32 { self.0 * o.0 + self.1 * o.1 + self.2 * o.2 }
    fn cross(self, o: Vec3) -> Vec3 {
        Vec3(self.1 * o.2 - self.2 * o.1, self.2 * o.0 - self.0 * o.2, self.0 * o.1 - self.1 * o.0)
    }
    fn norm(self) -> Vec3 {
        let l = self.dot(self).sqrt();
        if l > 0.0 { Vec3(self.0 / l, self.1 / l, self.2 / l) } else { self }
    }
}

impl Mat4 {
    pub fn mul(&self, b: &Mat4) -> Mat4 {
        let mut r = [0.0; 16];
        for c in 0..4 {
            for row in 0..4 {
                r[c * 4 + row] = (0..4).map(|k| self.0[k * 4 + row] * b.0[c * 4 + k]).sum();
            }
        }
        Mat4(r)
    }

    /// translate * rotate_y(angle) * scale
    pub fn trs(t: Vec3, angle: f32, s: Vec3) -> Mat4 {
        let (n, c) = angle.sin_cos();
        Mat4([c * s.0, 0.0, -n * s.0, 0.0, 0.0, s.1, 0.0, 0.0, n * s.2, 0.0, c * s.2, 0.0, t.0, t.1, t.2, 1.0])
    }

    pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fovy * 0.5).tan();
        let mut m = [0.0; 16];
        m[0] = f / aspect;
        m[5] = f;
        m[10] = far / (near - far);
        m[11] = -1.0;
        m[14] = near * far / (near - far);
        Mat4(m)
    }

    pub fn look_at(eye: Vec3, at: Vec3, up: Vec3) -> Mat4 {
        let f = at.sub(eye).norm();
        let s = f.cross(up).norm();
        let u = s.cross(f);
        Mat4([
            s.0, u.0, -f.0, 0.0, s.1, u.1, -f.1, 0.0, s.2, u.2, -f.2, 0.0,
            -s.dot(eye), -u.dot(eye), f.dot(eye), 1.0,
        ])
    }
}
