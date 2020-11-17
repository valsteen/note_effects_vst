#[inline]
pub fn bool_to_f32(value: bool) -> f32 { if value { 1.0 } else { 0.0 } }

#[inline]
pub fn f32_to_bool(value: f32) -> bool { value > 0.5 }

#[inline]
pub fn byte_to_f32(value: u8) -> f32 { value as f32 / 127. }

#[inline]
pub fn f32_to_byte(value: f32) -> u8 { (value * 127.) as u8 }
