#[allow(unused_imports)]
use {
    log::{error, info},
    std::error,
    util::parameter_value_conversion::f32_to_byte,
};

use vst::plugin::PluginParameters;
use vst::util::ParameterTransfer;

use util::parameters::ParameterConversion;
#[cfg(not(feature = "midi_hack_transmission"))]
use {crate::ipc_worker::IPCWorkerCommand, async_channel::Sender, std::sync::Mutex};

#[cfg(feature = "midi_hack_transmission")]
pub const PARAMETER_COUNT: usize = 0;
#[cfg(not(feature = "midi_hack_transmission"))]
pub const PARAMETER_COUNT: usize = 1;

const BASE_PORT: u16 = 6000;

pub(crate) struct ArpegiatorPatternReceiverParameters {
    pub transfer: ParameterTransfer,
    #[cfg(not(feature = "midi_hack_transmission"))]
    pub ipc_worker_sender: Mutex<Option<Sender<IPCWorkerCommand>>>,
}

impl ArpegiatorPatternReceiverParameters {
    pub fn get_port(&self) -> u16 {
        BASE_PORT + self.get_byte_parameter(Parameter::PortIndex) as u16
    }

    #[cfg(not(feature = "midi_hack_transmission"))]
    fn update_port(&self) -> Result<(), Box<dyn error::Error + '_>> {
        let port = self.get_byte_parameter(Parameter::PortIndex);
        self.ipc_worker_sender
            .lock()?
            .as_ref()
            .ok_or("cannot unlock ipc worker sender mutex")?
            .try_send(IPCWorkerCommand::SetPort(BASE_PORT + port as u16))?;
        Ok(())
    }
}

#[repr(i32)]
pub enum Parameter {
    PortIndex = 0,
}

impl From<i32> for Parameter {
    fn from(i: i32) -> Self {
        match i {
            0 => Parameter::PortIndex,
            _ => panic!("no such parameter {}", i),
        }
    }
}

impl From<Parameter> for i32 {
    fn from(p: Parameter) -> Self {
        p as i32
    }
}

impl ParameterConversion<Parameter> for ArpegiatorPatternReceiverParameters {
    fn get_parameter_transfer(&self) -> &ParameterTransfer {
        &self.transfer
    }

    fn get_parameter_count() -> usize {
        PARAMETER_COUNT
    }
}

impl ArpegiatorPatternReceiverParameters {
    pub fn new() -> Self {
        ArpegiatorPatternReceiverParameters {
            transfer: ParameterTransfer::new(PARAMETER_COUNT),
            #[cfg(not(feature = "midi_hack_transmission"))]
            ipc_worker_sender: Mutex::new(None),
        }
    }
}

impl PluginParameters for ArpegiatorPatternReceiverParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        match index.into() {
            Parameter::PortIndex => self.get_port().to_string(),
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index.into() {
            Parameter::PortIndex => "Port",
        }
        .to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.get_parameter_transfer().get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        match index.into() {
            Parameter::PortIndex => {
                #[cfg(not(feature = "midi_hack_transmission"))] {
                    let new_value = f32_to_byte(value);
                    let old_value = self.get_byte_parameter(Parameter::PortIndex);
                    if old_value != new_value {
                        self.transfer.set_parameter(index as usize, value);
                        self.update_port().unwrap_or_else(|err| {
                            error!("Could not update port: {}", err);
                        });
                    }
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
        self.deserialize_state(data);
        #[cfg(not(feature = "midi_hack_transmission"))]
        {
            self.update_port().unwrap_or_else(|err| {
                error!("Could not update port: {}", err);
            });
        }
    }

    fn load_bank_data(&self, data: &[u8]) {
        self.deserialize_state(data);
        #[cfg(not(feature = "midi_hack_transmission"))]
        {
            self.update_port().unwrap_or_else(|err| {
                error!("Could not update port: {}", err);
            });
        }
    }
}
