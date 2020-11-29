// this redefines the events in a way that they can outlive process_events ; e.g. SysExEvent
// contains the payload by itself and is no more a reference to a non-owned array of u8

use vst::api;
use vst::buffer::{PlaceholderEvent, WriteIntoPlaceholder};
use vst::event::{MidiEvent, SysExEvent};

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
// longer that needed

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
