mod events;
mod parameters;

#[macro_use]
extern crate vst;

use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::event::Event;
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use crate::events::OwnedEvent::{OwnDeprecated, OwnMidi, OwnSysEx};
use crate::events::{OwnSysExEvent, OwnedEvent};
use parameters::NoteOffDelayPluginParameters;
use util::constants::NOTE_OFF;
use util::debug::DebugSocket;
use vst::event::Event::{Deprecated, Midi};

plugin_main!(NoteOffDelayPlugin);

/*
    this contains midi events that have a play time not relative to the current buffer,
    but to the amount of samples since the plugin was active
*/

struct AbsoluteTimeEvent {
    event: OwnedEvent,
    play_time_in_samples: usize,
}

type AbsoluteTimeEventVector = Vec<AbsoluteTimeEvent>;

trait AbsoluteTimeEventVectorMethods {
    fn insert_event(&mut self, event: AbsoluteTimeEvent);
    fn merge_notes_off(&mut self, notes_off: AbsoluteTimeEventVector, note_off_delay: usize);
}

impl AbsoluteTimeEventVectorMethods for AbsoluteTimeEventVector {
    // called when receiving events ; caller takes care of not pushing note offs in a first phase
    fn insert_event(&mut self, event: AbsoluteTimeEvent) {
        if let Some(insert_point) = self
            .iter()
            .position(
                |event_at_position|
                    event.play_time_in_samples < event_at_position.play_time_in_samples
            )
        {
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
            let note_off_midi_event = match &note_off_event.event {
                OwnMidi(e) => e,
                _ => {
                    panic!("we're supposed to only have note off events in that list")
                }
            };

            let mut iterator = self.iter();
            let mut position = 0;

            // find original position
            let mut current_event: Option<&AbsoluteTimeEvent> = loop {
                match iterator.next() {
                    None => {
                        break None;
                    }
                    Some(event_at_position) => {
                        if note_off_event.play_time_in_samples >= event_at_position.play_time_in_samples {
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
                        if event_at_position.play_time_in_samples <= note_off_event.play_time_in_samples {
                            if let OwnMidi(midi_event) = event_at_position.event {
                                if (midi_event.data[0] & 0x0F)
                                    == (note_off_midi_event.data[0] & 0x0F)
                                    && midi_event.data[1] == note_off_midi_event.data[1]
                                {
                                    // same note on or off already happen between its original position and its final position, so skip it to prevent interrupting a new note
                                    break;
                                }
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

pub struct NoteOffDelayPlugin {
    send_buffer: SendEventBuffer,
    parameters: Arc<NoteOffDelayPluginParameters>,
    sample_rate: f32,
    current_time_in_samples: usize,
    events_queue: AbsoluteTimeEventVector,
}

struct DelayedEventConsumer<'a> {
    samples_in_buffer: usize,
    events: &'a mut AbsoluteTimeEventVector,
    current_time_in_samples: usize,
}

impl<'a> Iterator for DelayedEventConsumer<'a> {
    type Item = OwnedEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.events.is_empty() {
                return None;
            }

            let delayed_event = &self.events[0];
            let play_time_in_samples = delayed_event.play_time_in_samples;

            if play_time_in_samples < self.current_time_in_samples {
                DebugSocket::send(&*format!(
                    "too late for {} ( planned: {} , current buffer: {} - {}, removing",
                    format_own_event(&delayed_event.event),
                    delayed_event.play_time_in_samples,
                    self.current_time_in_samples,
                    self.current_time_in_samples + self.samples_in_buffer
                ));
                self.events.remove(0);
                continue;
            };

            if play_time_in_samples > self.current_time_in_samples + self.samples_in_buffer{
                // DebugSocket::send(&*format!(
                //     "too soon for {} ( planned: {} , current buffer: {} - {}",
                //     format_own_event(&delayed_event.event),
                //     delayed_event.play_time_in_samples,
                //     self.current_time_in_samples,
                //     self.current_time_in_samples + self.samples_in_buffer
                // ));
                return None;
            }

            let mut delayed_event: AbsoluteTimeEvent = self.events.remove(0);

            // until we know that the notes can be played in the current buffer, we don't know the final
            // delta frame
            let delta_frames =
                (delayed_event.play_time_in_samples - self.current_time_in_samples) as i32;

            match &mut delayed_event.event {
                OwnMidi(e) => e.delta_frames = delta_frames,
                OwnSysEx(e) => e.delta_frames = delta_frames,
                OwnDeprecated(e) => e.delta_frames = delta_frames,
            }

            DebugSocket::send(&*format!(
                "will do {} current_time={} ( play_time_in_samples={} )",
                format_own_event(&delayed_event.event),
                self.current_time_in_samples,
                delayed_event.play_time_in_samples
            ));

            return Some(delayed_event.event);
        }
    }
}

impl Default for NoteOffDelayPlugin {
    fn default() -> Self {
        NoteOffDelayPlugin {
            send_buffer: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            events_queue: Vec::new(),
        }
    }
}

fn format_event(e: &Event) -> String {
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

impl NoteOffDelayPlugin {
    fn send_events(&mut self, samples: usize) {
        if let Ok(mut host_callback_lock) = self.parameters.host_mutex.lock() {
            let event_consumer: DelayedEventConsumer = DelayedEventConsumer {
                samples_in_buffer: samples,
                events: &mut self.events_queue,
                current_time_in_samples: self.current_time_in_samples,
            };

            self.send_buffer
                .send_events(event_consumer, &mut host_callback_lock.host);
        }
    }

    #[allow(dead_code)]
    fn seconds_per_sample(&self) -> f32 {
        1.0 / self.sample_rate
    }

    fn seconds_to_samples(&self, seconds: f32) -> usize {
        (seconds * self.sample_rate) as usize
    }

    fn debug_events_in(&mut self, events: &api::Events) {
        for e in events.events() {
            DebugSocket::send(&*(format_event(&e) + &*format!(" current time={}", self.current_time_in_samples)));
        }
    }

    fn increase_time_in_samples(&mut self, samples: usize) {
        let new_time_in_samples = self.current_time_in_samples + samples;

        // tick every second in the debug socket
        // let old_time_in_seconds = self.seconds_per_sample() * self.current_time_in_samples as f32;
        // let new_time_in_seconds = self.seconds_per_sample() * new_time_in_samples as f32;
        //
        // if old_time_in_seconds.trunc() != new_time_in_seconds.trunc() {
        //     self.parameters.debug(&*format!("{}s", new_time_in_seconds));
        // }
        self.current_time_in_samples = new_time_in_samples;
    }
}

impl Plugin for NoteOffDelayPlugin {
    fn get_info(&self) -> Info {
        Info {
            name: "Note Off Delay".to_string(),
            vendor: "DJ Crontab".to_string(),
            unique_id: 234213173,
            parameters: 1,
            category: Category::Effect,
            initial_delay: 0,
            version: 1,
            inputs: 0,
            outputs: 0,
            midi_inputs: 1,
            f64_precision: false,
            presets: 1,
            midi_outputs: 1,
            preset_chunks: true,
            silent_when_stopped: true,
        }
    }

    fn new(host: HostCallback) -> Self {
        let parameters = NoteOffDelayPluginParameters::new(host);
        DebugSocket::send(
            build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version, $.compiler, $.timestamp),
        );
        NoteOffDelayPlugin {
            send_buffer: Default::default(),
            parameters: Arc::new(parameters),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            events_queue: Vec::new(),
        }
    }

    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate
    }

    fn can_do(&self, can_do: CanDo) -> vst::api::Supported {
        use vst::api::Supported::*;
        use vst::plugin::CanDo::*;

        match can_do {
            SendEvents
            | SendMidiEvent
            | ReceiveEvents
            | ReceiveMidiEvent
            | Offline
            | ReceiveTimeInfo
            | MidiKeyBasedInstrumentControl
            | Bypass => Yes,
            MidiProgramNames => No,
            ReceiveSysExEvent => Yes,
            MidiSingleNoteTuningChange => No,
            Other(s) => {
                if s == "MPE" {
                    Yes
                } else {
                    DebugSocket::send(&*s);
                    No
                }
            }
        }
    }

    fn process(&mut self, audio_buffer: &mut AudioBuffer<f32>) {
        self.send_events(audio_buffer.samples());
        self.increase_time_in_samples(audio_buffer.samples());
    }

    fn process_events(&mut self, events: &api::Events) {
        self.debug_events_in(events);

        let note_off_delay = match self
            .parameters
            .get_exponential_scale_parameter(NoteOffDelayPluginParameters::DELAY)
        {
            Some(value) => self.seconds_to_samples(value),
            _ => 0,
        };

        let mut notes_off = AbsoluteTimeEventVector::new();

        for event in events.events() {
            // TODO: minimum time, maximum time ( with delay )
            match event {
                Midi(e) => {
                    if e.data[0] >= NOTE_OFF && e.data[0] < NOTE_OFF + 0x10 {
                        notes_off.insert_event(AbsoluteTimeEvent {
                            event: OwnMidi(e),
                            play_time_in_samples: e.delta_frames as usize
                                + self.current_time_in_samples,
                        })
                    } else {
                        // drop any note off that was planned already
                        if let Some(delayed_note_off_position) = self.events_queue.iter().position(|delayed_note_off|
                            if let OwnMidi(note_off) = delayed_note_off.event {
                                (note_off.data[0] & 0x0F) == (e.data[0] & 0x0F) && e.data[1] == note_off.data[1]
                            } else {
                                false
                            }
                        ) {
                            let note_off = self.events_queue.remove(delayed_note_off_position);
                            DebugSocket::send(&*format!("removing delayed note off {}", format_own_event(&note_off.event)));
                        }

                        self.events_queue.insert_event(AbsoluteTimeEvent {
                            event: OwnMidi(e),
                            play_time_in_samples: e.delta_frames as usize
                                + self.current_time_in_samples,
                        })
                    }
                }
                Event::SysEx(e) => self.events_queue.insert_event(AbsoluteTimeEvent {
                    event: OwnSysEx(OwnSysExEvent {
                        payload: Vec::from(e.payload),
                        delta_frames: e.delta_frames,
                    }),
                    play_time_in_samples: e.delta_frames as usize + self.current_time_in_samples,
                }),
                Event::Deprecated(e) => self.events_queue.insert_event(AbsoluteTimeEvent {
                    event: OwnDeprecated(e),
                    play_time_in_samples: e.delta_frames as usize + self.current_time_in_samples,
                }),
            };
        }

        self.events_queue.merge_notes_off(notes_off, note_off_delay);
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}
