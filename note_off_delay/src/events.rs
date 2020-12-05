use vst::buffer::{PlaceholderEvent, WriteIntoPlaceholder};
use vst::event::Event::Midi;
use vst::event::{Event, MidiEvent};

pub struct AbsoluteTimeEvent {
    pub event: MidiEvent,
    pub play_time_in_samples: usize,
}

impl Clone for AbsoluteTimeEvent {
    fn clone(&self) -> Self {
        AbsoluteTimeEvent {
            event: self.event.clone(),
            play_time_in_samples: self.play_time_in_samples,
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.event = source.event.clone();
        self.play_time_in_samples = source.play_time_in_samples
    }
}

pub type AbsoluteTimeEventVector = Vec<AbsoluteTimeEvent>;

pub fn format_midi_event(e: &MidiEvent) -> String {
    format!(
        "[{:#04X} {:#04X} {:#04X}] delta_frames={}",
        e.data[0], e.data[1], e.data[2], e.delta_frames
    )
}

pub fn format_event(e: &Event) -> String {
    // notice bitwig only gives midi events anyway
    match e {
        Midi(e) => format_midi_event(e),
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

/*
    this contains midi events that have a play time not relative to the current buffer,
    but to the amount of samples since the plugin was active
*/

pub trait AbsoluteTimeEventVectorMethods {
    fn insert_event(&mut self, event: AbsoluteTimeEvent);
    fn merge_notes_off(&mut self, notes_off: &mut AbsoluteTimeEventVector, note_off_delay: usize);
}

impl AbsoluteTimeEventVectorMethods for AbsoluteTimeEventVector {
    // called when receiving events ; caller takes care of not pushing note offs in a first phase
    fn insert_event(&mut self, event: AbsoluteTimeEvent) {
        if let Some(insert_point) = self.iter().position(|event_at_position| {
            event.play_time_in_samples < event_at_position.play_time_in_samples
        }) {
            self.insert(insert_point, event);
        } else {
            self.push(event);
        }
    }

    // caller sends the notes off after inserting other events, so we know which notes are planned,
    // and insert notes off with the configured delay while making sure that between a note off
    // initial position and its final position, no note of same pitch and channel is triggered,
    // otherwise we will interrupt this second instance
    fn merge_notes_off(&mut self, notes_off: &mut AbsoluteTimeEventVector, note_off_delay: usize) {
        for mut note_off_event in notes_off {
            let mut iterator = self.iter();
            let mut position = 0;

            // find original position
            let mut current_event: Option<&AbsoluteTimeEvent> = loop {
                match iterator.next() {
                    None => {
                        break None;
                    }
                    Some(event_at_position) => {
                        if note_off_event.play_time_in_samples
                            > event_at_position.play_time_in_samples
                        {
                            position += 1;
                            continue;
                        } else {
                            break Some(event_at_position);
                        }
                    }
                }
            };

            // add delay
            note_off_event.play_time_in_samples += note_off_delay;

            loop {
                match current_event {
                    None => {
                        self.push(note_off_event.clone());
                        break;
                    }
                    Some(event_at_position) => {
                        if event_at_position.play_time_in_samples
                            <= note_off_event.play_time_in_samples
                        {
                            if (event_at_position.event.data[0] & 0x0F)
                                == (note_off_event.event.data[0] & 0x0F)
                                && event_at_position.event.data[1] == note_off_event.event.data[1]
                            {
                                // same note on or off already happen between its original position and its final position, so skip it to prevent interrupting a new note
                                break;
                            }

                            position += 1;
                            current_event = iterator.next();
                            continue;
                        }

                        self.insert(position, note_off_event.clone());
                        break;
                    }
                }
            }
        }
    }
}

impl WriteIntoPlaceholder for AbsoluteTimeEvent {
    fn write_into(&self, out: &mut PlaceholderEvent) {
        self.event.write_into(out)
    }
}


// Mission: redo enums to have midi notes that are specialized:
// note on, note off, cc, pressure, pitch

pub struct AbsoluteTimeMidiMessage {
    data: [u8;3],
    play_time_in_samples: usize,
}


impl AbsoluteTimeMidiMessage {
    pub fn from(event: &Event, current_time_in_samples: usize) -> AbsoluteTimeMidiMessageType {
        match event {
            Midi(e) => {
                let absolute_time_midi_message = AbsoluteTimeMidiMessage {
                    data: e.data.clone(),
                    play_time_in_samples: current_time_in_samples + e.delta_frames as usize
                };

                // a bit bold but we need different concrete types that happen to all hold the same data
                match e.data[0] & 0xF0 {
                    0x80 => {
                        // note off
                        AbsoluteTimeMidiMessageType::NoteOffMessage(NoteOff {
                            absolute_time_midi_message
                        })
                    }
                    0x90 => {
                        // note on
                        AbsoluteTimeMidiMessageType::NoteOnMessage(NoteOn {
                            absolute_time_midi_message
                        })
                    }
                    0xB0 => {
                        // CC
                        AbsoluteTimeMidiMessageType::CCMessage(CC {
                            absolute_time_midi_message
                        })
                    }
                    0xD0 => {
                        // pressure
                        AbsoluteTimeMidiMessageType::PressureMessage(Pressure {
                            absolute_time_midi_message
                        })
                    }
                    0xE0 => {
                        // pitch bend
                        AbsoluteTimeMidiMessageType::PitchBendMessage(PitchBend {
                            absolute_time_midi_message
                        })
                    }
                    0xA0 | 0xC0 | 0xF0 => {
                        AbsoluteTimeMidiMessageType::UnsupportedChannelMessage(GenericChannelMessage {
                            absolute_time_midi_message
                        })
                    }
                    _ => { AbsoluteTimeMidiMessageType::Unsupported }
                }
            }
            Event::SysEx(_) => { AbsoluteTimeMidiMessageType::Unsupported }
            Event::Deprecated(_) => { AbsoluteTimeMidiMessageType::Unsupported }
        }
    }
}


pub trait AbsoluteTimeMidiMessageMethods {
    fn get_absolute_time_midi_message(&self) -> &AbsoluteTimeMidiMessage;

    fn channel(&self) -> u8 {
        self.get_absolute_time_midi_message().data[0] & 0x0F
    }

    fn into(&self, current_time_in_samples: usize) -> Event {
        Event::Midi(MidiEvent {
            data: self.get_absolute_time_midi_message().data,
            delta_frames: (self.get_absolute_time_midi_message().play_time_in_samples - current_time_in_samples) as i32,
            live: true,
            note_length: None,
            note_offset: None,
            detune: 0,
            note_off_velocity: 0 // TODO to test, certainly redundant
        })
    }
}

pub struct NoteOn {
    absolute_time_midi_message: AbsoluteTimeMidiMessage,
}


impl AbsoluteTimeMidiMessageMethods for NoteOn {
    #[inline]
    fn get_absolute_time_midi_message(&self) -> &AbsoluteTimeMidiMessage {
        &self.absolute_time_midi_message
    }
}

pub struct NoteOff {
    absolute_time_midi_message: AbsoluteTimeMidiMessage,
}

impl AbsoluteTimeMidiMessageMethods for NoteOff {
    #[inline]
    fn get_absolute_time_midi_message(&self) -> &AbsoluteTimeMidiMessage {
        &self.absolute_time_midi_message
    }
}

pub struct Pressure {
    absolute_time_midi_message: AbsoluteTimeMidiMessage
}

impl AbsoluteTimeMidiMessageMethods for Pressure {
    #[inline]
    fn get_absolute_time_midi_message(&self) -> &AbsoluteTimeMidiMessage {
        &self.absolute_time_midi_message
    }
}

pub struct PitchBend {
    absolute_time_midi_message: AbsoluteTimeMidiMessage
}

impl AbsoluteTimeMidiMessageMethods for PitchBend {
    #[inline]
    fn get_absolute_time_midi_message(&self) -> &AbsoluteTimeMidiMessage {
        &self.absolute_time_midi_message
    }
}

pub struct CC {
    absolute_time_midi_message: AbsoluteTimeMidiMessage
}

impl AbsoluteTimeMidiMessageMethods for CC {
    #[inline]
    fn get_absolute_time_midi_message(&self) -> &AbsoluteTimeMidiMessage {
        &self.absolute_time_midi_message
    }
}

pub struct GenericChannelMessage {
    absolute_time_midi_message: AbsoluteTimeMidiMessage
}

impl AbsoluteTimeMidiMessageMethods for GenericChannelMessage {
    #[inline]
    fn get_absolute_time_midi_message(&self) -> &AbsoluteTimeMidiMessage {
        &self.absolute_time_midi_message
    }
}

pub enum AbsoluteTimeMidiMessageType {
    NoteOnMessage(NoteOn),
    NoteOffMessage(NoteOff),
    CCMessage(CC),
    PressureMessage(Pressure),
    PitchBendMessage(PitchBend),
    UnsupportedChannelMessage(GenericChannelMessage),
    Unsupported
}

trait Note where Self: AbsoluteTimeMidiMessageMethods {
    fn pitch(&self) -> u8;
    fn velocity(&self) -> u8;

    fn same_note(&self, note: &dyn Note) -> bool {
        self.channel() == note.channel() && self.pitch() == note.pitch()
    }
}

impl Note for NoteOn {
    fn pitch(&self) -> u8 {
        self.get_absolute_time_midi_message().data[1]
    }

    fn velocity(&self) -> u8 {
        self.get_absolute_time_midi_message().data[2]
    }
}


impl CC {
    pub fn cc(&self) -> u8 {
        self.get_absolute_time_midi_message().data[1]
    }

    pub fn value(&self) -> u8 {
        self.get_absolute_time_midi_message().data[2]
    }
}

impl Pressure {
    pub fn value(&self) -> u8 {
        self.get_absolute_time_midi_message().data[1]
    }
}

impl PitchBend {
    pub fn semitones(&self) -> f32 {
        // we assume pitchbend is over -48/+48 semitones
        // data is stored on 2x 7 bits and represents 96 semitones
        (((self.get_absolute_time_midi_message().data[2] as i32) << 7) as f32 + self.get_absolute_time_midi_message().data[1] as f32) / 16384. * 96. - 48.
    }
}
