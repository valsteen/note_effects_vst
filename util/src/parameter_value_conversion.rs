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

#[inline]
pub fn u14_to_f32(value: u16) -> f32 {
    // pitchbend goes from 0x0000 ( -48 semitones ) to 0x3FFF ( +48 semitones )
    value as f32 / (0x3FFF_usize) as f32
}

#[inline]
pub fn f32_to_u14(value: f32) -> u16 {
    (value * (0x3FFF_usize) as f32) as u16
}


// TODO better try to find a type that fits in 32 bits and store it as binary into the f32,
// disregarding what f32 is suppose to contain

#[inline]
pub fn usize_to_f32(value: usize) -> f32 {
    value as f32
}

#[inline]
pub fn f32_to_usize(value: f32) -> usize {
    value as usize
}
