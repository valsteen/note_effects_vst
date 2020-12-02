pub const PRESSURE: u8 = 0xD0;
pub const PITCHWHEEL: u8 = 0xE0;
pub const ZEROVALUE: u8 = 0x40;
pub const CC: u8 = 0xB0;
pub const TIMBRECC: u8 = 0x4A;
pub const NOTE_OFF: u8 = 0x80;
pub const NOTE_ON: u8 = 0x90;
pub const AFTERTOUCH: u8 = 0x90;
pub const C0: i8 = 0x18;

pub static NOTE_NAMES: &[&str; 12] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
