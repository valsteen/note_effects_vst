use std::sync::Mutex;

use vst::plugin::HostCallback;
use vst::util::ParameterTransfer;

use util::debug::DebugSocket;
use util::parameter_value_conversion::{byte_to_f32, f32_to_byte};
use util::HostCallbackLock;

const PARAMETER_COUNT: usize = 2;

pub struct NoteOffDelayPluginParameters {
    pub host_mutex: Mutex<HostCallbackLock>,
    pub transfer: ParameterTransfer,
}

#[repr(i32)]
pub enum Parameter {
    Delay = 0,
    MaxNotes,
}

impl From<i32> for Parameter {
    fn from(i: i32) -> Self {
        match i {
            0 => Parameter::Delay,
            1 => Parameter::MaxNotes,
            _ => panic!("no such parameter {}", i),
        }
    }
}

impl NoteOffDelayPluginParameters {
    pub fn new(host: HostCallback) -> Self {
        return NoteOffDelayPluginParameters {
            host_mutex: Mutex::new(HostCallbackLock { host }),
            ..Default::default()
        };
    }

    #[inline]
    pub fn get_byte_parameter(&self, index: Parameter) -> u8 {
        f32_to_byte(self.transfer.get_parameter(index as usize))
    }

    #[inline]
    fn set_byte_parameter(&self, index: Parameter, value: u8) {
        self.transfer
            .set_parameter(index as usize, byte_to_f32(value))
    }

    #[inline]
    pub fn get_exponential_scale_parameter(&self, index: Parameter) -> Option<f32> {
        let x = self.transfer.get_parameter(index as usize);
        const FACTOR: f32 = 20.0;
        if x == 0.0 {
            None
        } else {
            Some((FACTOR.powf(x) - 1.) * 5. / (FACTOR - 1.0))
        }
    }

    pub fn get_max_notes(&self) -> u8 {
        self.get_byte_parameter(Parameter::MaxNotes) / 4
    }

    pub fn set_max_notes(&self, value: u8) {
        self.set_byte_parameter(Parameter::MaxNotes, value * 4)
    }
}

impl Default for NoteOffDelayPluginParameters {
    fn default() -> Self {
        let parameters = NoteOffDelayPluginParameters {
            host_mutex: Default::default(),
            transfer: ParameterTransfer::new(PARAMETER_COUNT),
        };
        parameters
    }
}

impl vst::plugin::PluginParameters for NoteOffDelayPluginParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match Parameter::from(index) {
            Parameter::Delay => {
                if let Some(mut value) = self.get_exponential_scale_parameter(Parameter::Delay) {
                    let mut out = String::new();
                    if value >= 1.0 {
                        out += &*format!("{:.0}s ", value);
                        value -= value.trunc();
                    }
                    if value > 0.0 {
                        out += &*format!("{:3.0}ms", value * 1000.0);
                    }
                    out
                } else {
                    "Off".to_string()
                }
            }
            Parameter::MaxNotes => {
                if self.get_parameter(Parameter::MaxNotes as i32) == 0.0 {
                    "Off".to_string()
                } else {
                    format!("{}", self.get_max_notes())
                }
            }
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match Parameter::from(index) {
            Parameter::Delay => "Delay",
            Parameter::MaxNotes => "Max Notes",
        }
        .to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.transfer.get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        match Parameter::from(index) {
            Parameter::Delay => {
                DebugSocket::send(&*format!("Parameter {} set to {}", index, value));
                let old_value = self.get_parameter(index);
                if value != old_value {
                    self.transfer.set_parameter(index as usize, value)
                }
            }
            Parameter::MaxNotes => {
                let old_value = self.get_max_notes();
                let max_notes = f32_to_byte(value) / 4;
                if max_notes != old_value {
                    self.set_max_notes(max_notes)
                }
            }
        }
    }

    fn get_preset_data(&self) -> Vec<u8> {
        (0..PARAMETER_COUNT)
            .map(|i| self.get_byte_parameter(Parameter::from(i as i32)))
            .collect()
    }

    fn get_bank_data(&self) -> Vec<u8> {
        (0..PARAMETER_COUNT)
            .map(|i| self.get_byte_parameter(Parameter::from(i as i32)))
            .collect()
    }

    fn load_preset_data(&self, data: &[u8]) {
        for (i, item) in data.iter().enumerate() {
            self.set_byte_parameter(Parameter::from(i as i32), *item);
        }
    }

    fn load_bank_data(&self, data: &[u8]) {
        for (i, item) in data.iter().enumerate() {
            self.set_byte_parameter(Parameter::from(i as i32), *item);
        }
    }
}
