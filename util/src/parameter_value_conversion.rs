#[inline]
pub fn bool_to_f32(value: bool) -> f32 {
    if value {
        1.0
    } else {
        0.0
    }
}

#[inline]
pub fn f32_to_bool(value: f32) -> bool {
    value > 0.5
}

#[inline]
pub fn byte_to_f32(value: u8) -> f32 {
    value as f32 / 127.
}

#[inline]
pub fn f32_to_byte(value: f32) -> u8 {
    (value * 127.) as u8
}

// TODO better try to find a type that fits in 32 bytes and store it as binary into the f32,
// disregarding what f32 is suppose to contain

#[inline]
pub fn usize_to_f32(value: usize) -> f32 {
    value as f32
}

#[inline]
pub fn f32_to_usize(value: f32) -> usize {
    value as usize
}
