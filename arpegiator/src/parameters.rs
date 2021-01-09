#[allow(unused_imports)]
use log::{error, info};

use vst::plugin::PluginParameters;
use vst::util::ParameterTransfer;

use util::parameters::ParameterConversion;
use util::parameter_value_conversion::{f32_to_byte, f32_to_bool};
use crate::workers::main_worker::WorkerCommand;
use std::sync::Mutex;
use async_channel::Sender;
use util::duration_display;
use util::system::Uuid;


pub const PARAMETER_COUNT: usize = 4;
const BASE_PORT: u16 = 6000;

// 1 = Immediate - TODO : must be default
// 0 = Off
// highest = faster
// lowest = slower
// choose or configurable:
// fixed time between start/end pitch
// fixed time per semitone
/*
think about those cases:
    large difference / small interval
    small difference / large interval => weirdest

    thus fixed time sounds weird, but the player can keep changing the target note, thus has the possibility to
    influence the speed.
    but then we need to reset the time at each change

    also : velocity would influence pressure

 */

// note: if pitchbend is not off, it makes sense to consume note pitchbend as is
// ideally while a note eposes its pitch and a pitchbend value, a method should directly tell the pitch in
// millisemitones relative to 0 ( C-2 )
enum PitchBendValues {
    Off,  // no pitchbend, means same pitch until pattern ends
    DurationToReachTarget(f32),
    Immediate
}

pub(crate) struct ArpegiatorParameters {
    pub transfer: ParameterTransfer,
    pub worker_commands: Mutex<Option<Sender<WorkerCommand>>>,
}

impl ArpegiatorParameters {
    pub fn get_port(&self) -> u16 {
        BASE_PORT + self.get_byte_parameter(Parameter::PortIndex) as u16
    }

    pub fn update_port(&self, event_id: Uuid) {
        let port = self.get_byte_parameter(Parameter::PortIndex) as u16 + BASE_PORT;
        info!("Applying parameter change: port={}", port);
        if let Err(error) = self.worker_commands.lock().unwrap().as_ref().unwrap().try_send(
            WorkerCommand::SetPort(port, event_id)
        ) {
            info!("[{}] main worker is shutdown - ignoring port change ({})", event_id, error);
        }
    }
}


#[repr(i32)]
pub enum Parameter {
    PortIndex = 0,
    HoldNotes,  // a started pattern will find a note to play, even if no note is playing for that index
    PatternLegato,  // pattern is not restarted if start/end match, and note is thus held
    Pitchbend
}

impl From<i32> for Parameter {
    fn from(i: i32) -> Self {
        match i {
            0 => Parameter::PortIndex,
            1 => Parameter::HoldNotes,
            2 => Parameter::PatternLegato,
            3 => Parameter::Pitchbend,
            _ => panic!("no such parameter {}", i),
        }
    }
}


impl Into<i32> for Parameter {
    fn into(self) -> i32 {
        self as i32
    }
}

impl ParameterConversion<Parameter> for ArpegiatorParameters {
    fn get_parameter_transfer(&self) -> &ParameterTransfer {
        &self.transfer
    }

    fn get_parameter_count() -> usize {
        PARAMETER_COUNT
    }
}


impl ArpegiatorParameters {
    pub fn new() -> Self {
        let parameters = ArpegiatorParameters {
            transfer: ParameterTransfer::new(PARAMETER_COUNT),
            worker_commands: Mutex::new(None),
        };
        parameters.set_parameter(Parameter::PatternLegato.into(), 1.);
        parameters
    }
}


impl PluginParameters for ArpegiatorParameters {
    fn get_parameter_text(&self, index: i32) -> String {
        let parameter = index.into();
        match parameter {
            Parameter::PortIndex => {
                self.get_port().to_string()
            }
            Parameter::HoldNotes | Parameter::PatternLegato => {
                match self.get_bool_parameter(parameter) {
                    true => "On",
                    false => "Off"
                }.to_string()
            }
            Parameter::Pitchbend => {  // TODO set default to 1
                match self.get_parameter(index) {
                    x if x <= 0. => {
                        "Immediate".into()
                    }
                    x if x >= 1. => {
                        "Off".into()
                    }
                    _ => {
                        let value = self.get_exponential_scale_parameter(Parameter::Pitchbend, 1., 80.);
                        duration_display(value)
                    }
                }
            }
        }
    }

    fn get_parameter_name(&self, index: i32) -> String {
        match index.into() {
            Parameter::PortIndex => "Port",
            Parameter::HoldNotes => "Hold notes",
            Parameter::PatternLegato => "Pattern Legato",
            Parameter::Pitchbend => "Use pitchbend"
        }.to_string()
    }

    fn get_parameter(&self, index: i32) -> f32 {
        self.get_parameter_transfer().get_parameter(index as usize)
    }

    fn set_parameter(&self, index: i32, value: f32) {
        let parameter = index.into();
        match parameter {
            Parameter::PortIndex => {
                let new_value = f32_to_byte(value);
                let old_value = self.get_byte_parameter(Parameter::PortIndex);
                if old_value != new_value {
                    let event_id = Uuid::new_v4();
                    info!("[{}] set parameter port {}", event_id, BASE_PORT + new_value as u16);
                    self.transfer.set_parameter(index as usize, value);
                    self.update_port(event_id)
                }
            }
            Parameter::HoldNotes | Parameter::PatternLegato => {
                let new_value = f32_to_bool(value);
                let old_value = self.get_bool_parameter(parameter);
                if old_value != new_value {
                    self.transfer.set_parameter(index as usize, value);
                }
            }
            Parameter::Pitchbend => self.transfer.set_parameter(index as usize, value)
        }
    }

    fn get_preset_data(&self) -> Vec<u8> {
        self.serialize_state()
    }

    fn get_bank_data(&self) -> Vec<u8> {
        self.serialize_state()
    }

    fn load_preset_data(&self, data: &[u8]) {
        let event_id = Uuid::new_v4();
        info!("[{}] Load present data", event_id);
        self.deserialize_state(data);
        self.update_port(event_id)
    }

    fn load_bank_data(&self, data: &[u8]) {
        let event_id = Uuid::new_v4();
        info!("[{}] Load bank data", event_id);
        self.deserialize_state(data);
        self.update_port(event_id)
    }
}
