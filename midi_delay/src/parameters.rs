use std::sync::Mutex;
use util::{HostCallbackLock, duration_display};
use vst::plugin::{HostCallback, PluginParameters};
use vst::util::ParameterTransfer;
use util::parameters::ParameterConversion;

pub struct MidiDelayParameters {
    pub host: Mutex<HostCallbackLock>,
    pub transfer: ParameterTransfer,
}

#[repr(i32)]
pub enum Parameter {
    Delay = 0,
}


impl From<i32> for Parameter {
    fn from(i: i32) -> Self {
        match i {
            0 => Parameter::Delay,
            _ => panic!(format!("No such Parameter {}", i)),
        }
    }
}

impl Into<i32> for Parameter {
    fn into(self) -> i32 {
        self as i32
    }
}


impl ParameterConversion<Parameter> for MidiDelayParameters {
    fn get_parameter_transfer(&self) -> &ParameterTransfer {
        &self.transfer
    }

    fn get_parameter_count() -> usize {
        1
    }
}

impl MidiDelayParameters {
    pub fn new(host: HostCallback) -> Self {
        MidiDelayParameters {
            host: Mutex::new(HostCallbackLock { host }),
            transfer: ParameterTransfer::new(1),
        }
    }
}


impl PluginParameters for MidiDelayParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match index.into() {
            Parameter::Delay => {
                let value = self.get_exponential_scale_parameter(Parameter::Delay, 1., 80.);
                if value > 0. {
                    duration_display(value)
                } else {
                    "Off".to_string()
                }
            }
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match Parameter::from(index as i32) {
            Parameter::Delay => "Delay"
        }.to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.transfer.get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        match index.into() {
            Parameter::Delay => {
                let old_value = self.get_parameter(index);
                if (value - old_value).abs() > 0.00001 {
                    self.transfer.set_parameter(index as usize, value)
                }
            }
        }
    }

    fn get_preset_data(&self) -> Vec<u8> {
        self.serialize_state()
    }

    fn get_bank_data(&self) -> Vec<u8> {
        self.serialize_state()
    }

    fn load_preset_data(&self, data: &[u8]) {
        self.deserialize_state(data)
    }

    fn load_bank_data(&self, data: &[u8]) {
        self.deserialize_state(data)
    }
}

impl Default for MidiDelayParameters {
    fn default() -> Self {
        let parameters = MidiDelayParameters {
            host: Default::default(),
            transfer: ParameterTransfer::new(3),
        };
        parameters.set_byte_parameter(Parameter::Delay, 0);
        parameters
    }
}
