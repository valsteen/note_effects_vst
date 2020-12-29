use std::sync::Mutex;

use vst::plugin::HostCallback ;
use vst::util::ParameterTransfer;

use util::{HostCallbackLock, duration_display};
use util::parameters::ParameterConversion;

const PARAMETER_COUNT: usize = 1;

pub struct MaxNoteDurationPluginParameters {
    pub host_mutex: Mutex<HostCallbackLock>,
    pub transfer: ParameterTransfer,
}

#[repr(i32)]
pub enum Parameter {
    MaxDuration = 0,
}

impl From<i32> for Parameter {
    fn from(i: i32) -> Self {
        match i {
            0 => Parameter::MaxDuration,
            _ => panic!("no such parameter {}", i),
        }
    }
}


impl Into<i32> for Parameter {
    fn into(self) -> i32 {
        self as i32
    }
}


impl ParameterConversion<Parameter> for MaxNoteDurationPluginParameters {
    fn get_parameter_transfer(&self) -> &ParameterTransfer {
        &self.transfer
    }

    fn get_parameter_count() -> usize {
        PARAMETER_COUNT
    }
}


impl MaxNoteDurationPluginParameters {
    pub fn new(host: HostCallback) -> Self {
        MaxNoteDurationPluginParameters {
            host_mutex: Mutex::new(HostCallbackLock { host }),
            ..Default::default()
        }
    }
}


impl Default for MaxNoteDurationPluginParameters {
    fn default() -> Self {
        MaxNoteDurationPluginParameters {
            host_mutex: Default::default(),
            transfer: ParameterTransfer::new(PARAMETER_COUNT),
        }
    }
}


impl vst::plugin::PluginParameters for MaxNoteDurationPluginParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match index.into() {
            Parameter::MaxDuration => {
                let value = self.get_exponential_scale_parameter(Parameter::MaxDuration, 10., 20.);

                if value > 0.0 {
                    duration_display(value)
                } else {
                    "Off".to_string()
                }
            }
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index.into() {
            Parameter::MaxDuration => "Maximum duration",
        }
        .to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.get_parameter_transfer().get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        match index.into() {
            Parameter::MaxDuration => {
                let old_value = self.get_parameter(index);
                if (value - old_value).abs() > 0.0001 {
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
