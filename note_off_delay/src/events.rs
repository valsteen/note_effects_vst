// this redefines the events in a way that they can outlive process_events ; e.g. SysExEvent
// contains the payload by itself and is no more a reference to a non-owned array of u8

use vst::api;
use vst::buffer::{PlaceholderEvent, WriteIntoPlaceholder};
use vst::event::{MidiEvent, SysExEvent, Event};
use vst::event::Event::{Midi, Deprecated};
use crate::events::OwnedEvent::{OwnMidi, OwnSysEx, OwnDeprecated};

pub struct OwnSysExEvent {
    pub payload: Vec<u8>,
    pub delta_frames: i32,
}

pub enum OwnedEvent {
    OwnMidi(MidiEvent),
    OwnSysEx(OwnSysExEvent),
    OwnDeprecated(api::Event),
}

// by implementing this trait we may just give a OwnedEvent iterator to send_buffer.send_events,
// without having to hold a SysExEvent and its lifetime issues due to the &[u8] ref
// longer than needed

impl WriteIntoPlaceholder for OwnedEvent {
    fn write_into(&self, out: &mut PlaceholderEvent) {
        match self {
            OwnedEvent::OwnMidi(e) => {
                e.write_into(out);
            }
            OwnedEvent::OwnSysEx(e) => SysExEvent {
                payload: &e.payload,
                delta_frames: e.delta_frames,
            }
            .write_into(out),
            _ => {}
        }
    }
}

pub struct AbsoluteTimeEvent {
    pub event: OwnedEvent,
    pub play_time_in_samples: usize,
}

pub type AbsoluteTimeEventVector = Vec<AbsoluteTimeEvent>;

pub fn format_event(e: &Event) -> String {
    match e {
        Midi(e) => {
            format!(
                "[{:#04X} {:#04X} {:#04X}] delta_frames={}",
                e.data[0], e.data[1], e.data[2], e.delta_frames
            )
        }
        Event::SysEx(e) => {
            format!(
                "SysEx [{}] delta_frames={}",
                e.payload
                    .iter()
                    .fold(String::new(), |x, u| x + &*format!(" {:#04X}", u)),
                e.delta_frames
            )
        }
        Event::Deprecated(e) => {
            format!(
                "Deprecated [{}] delta_frames={}",
                e._reserved
                    .iter()
                    .fold(String::new(), |x, u| x + &*format!(" {:#04X}", u)),
                e.delta_frames
            )
        }
    }
}

pub fn format_own_event(event: &OwnedEvent) -> String {
    match event {
        OwnMidi(e) => format_event(&Midi(*e)),
        OwnSysEx(e) => {
            format!(
                "SysEx [{}] delta_frames={}",
                e.payload
                    .iter()
                    .fold(String::new(), |x, u| x + &*format!(" {:#04X}", u)),
                e.delta_frames
            )
        }
        OwnDeprecated(e) => format_event(&Deprecated(*e)),
    }
}
