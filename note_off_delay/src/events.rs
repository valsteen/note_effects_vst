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
    fn merge_notes_off(&mut self, notes_off: AbsoluteTimeEventVector, note_off_delay: usize);
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
    fn merge_notes_off(&mut self, notes_off: AbsoluteTimeEventVector, note_off_delay: usize) {
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
                        self.push(note_off_event);
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

                        self.insert(position, note_off_event);
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
