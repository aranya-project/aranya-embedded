use core::ops::Mul;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RgbU8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Mul<f32> for RgbU8 {
    type Output = RgbU8;

    fn mul(self, rhs: f32) -> RgbU8 {
        RgbU8 {
            red: (self.red as f32 * rhs) as u8,
            green: (self.green as f32 * rhs) as u8,
            blue: (self.blue as f32 * rhs) as u8,
        }
    }
}

impl From<RgbU8> for (u8, u8, u8) {
    fn from(value: RgbU8) -> Self {
        (value.red, value.green, value.blue)
    }
}

impl From<(u8, u8, u8)> for RgbU8 {
    fn from(value: (u8, u8, u8)) -> Self {
        RgbU8 {
            red: value.0,
            green: value.1,
            blue: value.2,
        }
    }
}
