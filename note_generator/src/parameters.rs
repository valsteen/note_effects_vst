use std::sync::Mutex;
use util::parameter_value_conversion::{bool_to_f32, byte_to_f32, f32_to_bool, f32_to_byte};
use util::HostCallbackLock;
use vst::plugin::{HostCallback, PluginParameters};
use vst::util::ParameterTransfer;

static NOTE_NAMES: &[&str; 12] = &[
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const C0: i8 = 0x18;

pub struct NoteGeneratorPluginParameters {
    pub host: Mutex<HostCallbackLock>,
    pub transfer: ParameterTransfer,
}

impl NoteGeneratorPluginParameters {
    // TODO see https://doc.rust-lang.org/reference/items/enumerations.html
    // let baz_discriminant = Foo::Baz as u32;
    // [repr(i32)] can even be used on enums
    pub const CHANNEL: i32 = 0;
    pub const PITCH: i32 = 1;
    pub const VELOCITY: i32 = 2;
    pub const NOTE_OFF_VELOCITY: i32 = 3;
    pub const PRESSURE: i32 = 4;
    pub const TRIGGER: i32 = 5;
    pub const TRIGGERED_PITCH: i32 = 6;
    pub const TRIGGERED_CHANNEL: i32 = 7;

    #[inline]
    fn set_byte_parameter(&self, index: i32, value: u8) {
        self.transfer
            .set_parameter(index as usize, byte_to_f32(value))
    }

    #[inline]
    pub fn get_byte_parameter(&self, index: i32) -> u8 {
        f32_to_byte(self.transfer.get_parameter(index as usize))
    }

    #[inline]
    pub fn get_bool_parameter(&self, index: i32) -> bool {
        f32_to_bool(self.transfer.get_parameter(index as usize))
    }

    #[inline]
    fn set_bool_parameter(&self, index: i32, value: bool) {
        self.transfer
            .set_parameter(index as usize, bool_to_f32(value))
    }

    #[inline]
    fn get_displayable_channel(&self) -> u8 {
        // NOT the stored value, but the one used to show on the UI
        self.get_byte_parameter(Self::CHANNEL) / 8 + 1
    }

    fn get_pitch_label(&self) -> String {
        format!(
            "{}{}",
            NOTE_NAMES[self.get_byte_parameter(Self::PITCH) as usize % 12],
            ((self.get_byte_parameter(Self::PITCH) as i8) - C0) / 12
        )
    }

    #[inline]
    fn get_velocity(&self) -> u8 {
        self.get_byte_parameter(Self::VELOCITY)
    }

    #[inline]
    fn get_note_off_velocity(&self) -> u8 {
        self.get_byte_parameter(Self::NOTE_OFF_VELOCITY)
    }

    #[inline]
    fn get_pressure(&self) -> u8 {
        self.get_byte_parameter(Self::PRESSURE)
    }

    #[inline]
    fn get_trigger(&self) -> bool {
        self.get_bool_parameter(Self::TRIGGER)
    }

    #[inline]
    pub fn set_parameter_by_name(&self, index: i32, value: f32) {
        self.set_parameter(index as i32, value);
    }

    pub fn copy_parameter(&self, from_index: i32, to_index: i32) {
        self.set_parameter_by_name(to_index, self.get_parameter_by_name(from_index));
    }

    #[inline]
    pub fn get_parameter_by_name(&self, index: i32) -> f32 {
        self.transfer.get_parameter(index as usize)
    }

    pub fn new(host: HostCallback) -> Self {
        NoteGeneratorPluginParameters {
            host: Mutex::new(HostCallbackLock { host }),
            ..Default::default()
        }
    }
}

impl PluginParameters for NoteGeneratorPluginParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match index {
            Self::CHANNEL => format!("{}", self.get_displayable_channel()),
            Self::PITCH => self.get_pitch_label(),
            Self::VELOCITY => format!("{}", self.get_velocity()),
            Self::NOTE_OFF_VELOCITY => format!("{}", self.get_note_off_velocity()),
            Self::PRESSURE => format!("{}", self.get_pressure()),
            Self::TRIGGER => format!("{}", self.get_trigger()),
            _ => "".to_string(),
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index {
            Self::CHANNEL => "Channel",
            Self::PITCH => "Pitch",
            Self::VELOCITY => "Velocity",
            Self::NOTE_OFF_VELOCITY => "Note off velocity",
            Self::PRESSURE => "Pressure",
            Self::TRIGGER => "Ttrigger generated note",
            _ => "",
        }
        .to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.transfer.get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        match index {
            Self::TRIGGER => {
                // boolean case: in order to ignore intermediary changes,
                // don't just pass the unchanged f32
                let new_value = f32_to_bool(value);
                let old_value = self.get_bool_parameter(Self::TRIGGER);

                if new_value != old_value {
                    self.set_bool_parameter(Self::TRIGGER, new_value)
                }
            }
            _ => {
                // reduce to a byte and compare, so modulators don't generate tons of
                // irrelevant changes
                let new_value = f32_to_byte(value);
                let old_value = self.get_byte_parameter(index);
                if new_value != old_value {
                    self.set_byte_parameter(index, new_value)
                }
            }
        }
    }

    fn string_to_parameter(&self, index: i32, text: String) -> bool {
        // actually never called in bitwig
        match index {
            Self::CHANNEL => match text.parse::<u8>() {
                Ok(n) => {
                    if n > 0 && n <= 16 {
                        self.set_byte_parameter(Self::CHANNEL, n);
                        true
                    } else {
                        false
                    }
                }
                Err(_) => false,
            },
            Self::VELOCITY | Self::NOTE_OFF_VELOCITY | Self::PRESSURE => match text.parse::<u8>() {
                Ok(n) => {
                    if n < 128 {
                        self.set_byte_parameter(Self::VELOCITY, n);
                        true
                    } else {
                        false
                    }
                }
                Err(_) => false,
            },
            Self::PITCH => match NOTE_NAMES.iter().position(|&s| text.starts_with(s)) {
                None => false,
                Some(position) => {
                    match text[NOTE_NAMES[position].len()..text.len()].parse::<i8>() {
                        Ok(octave) => {
                            if octave >= -2 && octave <= 8 {
                                let pitch = octave as i16 * 12 + C0 as i16 + position as i16;
                                if pitch < 128 {
                                    self.set_byte_parameter(Self::PITCH, pitch as u8);
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        }
                        Err(_) => false,
                    }
                }
            },
            Self::TRIGGER => match text.to_ascii_lowercase().as_ref() {
                "0" | "off" | "" => {
                    self.set_bool_parameter(Self::TRIGGER, false);
                    true
                }
                "1" | "on" => {
                    self.set_bool_parameter(Self::TRIGGER, true);
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn get_preset_data(&self) -> Vec<u8> {
        (0..8).map(|i| self.get_byte_parameter(i)).collect()
    }

    fn get_bank_data(&self) -> Vec<u8> {
        (0..8).map(|i| self.get_byte_parameter(i)).collect()
    }

    fn load_preset_data(&self, data: &[u8]) {
        for (i, item) in data.iter().enumerate() {
            self.set_byte_parameter(i as i32, *item);
        }
    }

    fn load_bank_data(&self, data: &[u8]) {
        for (i, item) in data.iter().enumerate() {
            self.set_byte_parameter(i as i32, *item);
        }
    }
}

impl Default for NoteGeneratorPluginParameters {
    fn default() -> Self {
        let parameters = NoteGeneratorPluginParameters {
            host: Default::default(),
            transfer: ParameterTransfer::new(8),
        };
        parameters.set_byte_parameter(Self::PITCH, C0 as u8);
        parameters.set_byte_parameter(Self::VELOCITY, 64);
        parameters
    }
}
