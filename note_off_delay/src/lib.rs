mod events;
mod parameters;

#[macro_use]
extern crate vst;

use std::sync::Arc;

use vst::api;
use vst::buffer::{AudioBuffer, SendEventBuffer};
use vst::plugin::{CanDo, Category, HostCallback, Info, Plugin};

use crate::events::format_midi_event;
use crate::parameters::Parameter;
use events::{AbsoluteTimeEvent, AbsoluteTimeEventVector, AbsoluteTimeEventVectorMethods};
use parameters::NoteOffDelayPluginParameters;
use std::collections::HashMap;
use util::constants::{NOTE_OFF, NOTE_ON};
use util::debug::DebugSocket;
use util::make_midi_event;
use vst::event::Event::Midi;

plugin_main!(NoteOffDelayPlugin);

pub struct NoteOffDelayPlugin {
    send_buffer: SendEventBuffer,
    parameters: Arc<NoteOffDelayPluginParameters>,
    sample_rate: f32,
    current_time_in_samples: usize,
    events_queue: AbsoluteTimeEventVector,
    current_playing_notes: CurrentPlayingNotes,
}

impl Default for NoteOffDelayPlugin {
    fn default() -> Self {
        NoteOffDelayPlugin {
            send_buffer: Default::default(),
            parameters: Arc::new(Default::default()),
            sample_rate: 44100.0,
            current_time_in_samples: 0,
            events_queue: Vec::new(),
            current_playing_notes: Default::default(),
        }
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

            let mut events: Vec<AbsoluteTimeEvent> = event_consumer.collect();
            let notes_off = self
                .current_playing_notes
                .update(&events, self.parameters.get_max_notes());

            for note_off in notes_off {
                events.push(note_off);
            }

            self.send_buffer
                .send_events(events, &mut host_callback_lock.host);
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
            DebugSocket::send(
                &*(events::format_event(&e)
                    + &*format!(" current time={}", self.current_time_in_samples)),
            );
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
            parameters: 2,
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
            current_playing_notes: Default::default(),
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
            .get_exponential_scale_parameter(Parameter::Delay)
        {
            Some(value) => self.seconds_to_samples(value),
            _ => 0,
        };

        let mut notes_off = AbsoluteTimeEventVector::new();

        for event in events.events() {
            // TODO: minimum time, maximum time ( with delay )
            match event {
                Midi(e) => {
                    match e.data[0] {
                        x if x >= NOTE_OFF && x < NOTE_OFF + 0x10 => {
                            notes_off.insert_event(AbsoluteTimeEvent {
                                event: e,
                                play_time_in_samples: e.delta_frames as usize
                                    + self.current_time_in_samples,
                            })
                        }
                        x if x >= NOTE_ON && x < NOTE_ON + 0x10 => {
                            // drop any note off that was planned already
                            if let Some(delayed_note_off_position) =
                                self.events_queue.iter().position(|delayed_note_off| {
                                    (delayed_note_off.event.data[0] & 0x0F) == (e.data[0] & 0x0F)
                                        && e.data[1] == delayed_note_off.event.data[1]
                                        && (delayed_note_off.event.data[0] & 0xF0 == 0x80)
                                })
                            {
                                let note_off = self.events_queue.remove(delayed_note_off_position);
                                DebugSocket::send(&*format!(
                                    "removing delayed note off {}",
                                    format_midi_event(&note_off.event)
                                ));
                            }

                            self.events_queue.insert_event(AbsoluteTimeEvent {
                                event: e,
                                play_time_in_samples: e.delta_frames as usize
                                    + self.current_time_in_samples,
                            })
                        }
                        // ignore everything else for now ( CC, expressions, ... )
                        _ => {}
                    }
                }
                _ => {}
            };
        }

        self.events_queue.merge_notes_off(notes_off, note_off_delay);
    }

    fn get_parameter_object(&mut self) -> Arc<dyn vst::plugin::PluginParameters> {
        Arc::clone(&self.parameters) as Arc<dyn vst::plugin::PluginParameters>
    }
}

type CurrentPlayingNotes = HashMap<[u8; 2], AbsoluteTimeEvent>;

pub trait CurrentPlayingNotesMethods {
    fn oldest(&self) -> Option<AbsoluteTimeEvent>;
    fn add_event(&mut self, event: AbsoluteTimeEvent, max_notes: u8) -> Option<AbsoluteTimeEvent>;
    fn update(&mut self, events: &[AbsoluteTimeEvent], max_notes: u8) -> Vec<AbsoluteTimeEvent>;
}

fn format_playing_notes(v : &CurrentPlayingNotes) -> String {
    v.keys().fold( String::new(), |acc, x| format!("{}, {} {}", acc, x[0], x[1].to_string()))
}


impl CurrentPlayingNotesMethods for CurrentPlayingNotes {
    fn oldest(&self) -> Option<AbsoluteTimeEvent> {
        let oldest_option = self
            .values()
            .min_by( |a, b| a.play_time_in_samples.cmp(&b.play_time_in_samples) );

        if let Some(oldest) = oldest_option {
            Some(oldest.clone())
        } else {
            None
        }
    }

    fn add_event(&mut self, event: AbsoluteTimeEvent, max_notes: u8) -> Option<AbsoluteTimeEvent> {
        self.insert([event.event.data[0] & 0x0F, event.event.data[1]], event);
        if max_notes > 0 && self.len() > max_notes as usize {
            if let Some(event_to_remove) = self.oldest() {
                let note_off = AbsoluteTimeEvent {
                    event: make_midi_event(
                        [
                            NOTE_OFF + (event_to_remove.event.data[0] & 0x0F),
                            event_to_remove.event.data[1],
                            event_to_remove.event.data[2],
                        ],
                        event_to_remove.event.delta_frames,
                    ),
                    play_time_in_samples: event_to_remove.play_time_in_samples,
                };
                self.remove_entry(&[event_to_remove.event.data[0] & 0x0F, event_to_remove.event.data[1]]);
                return Some(note_off);
            }
        }
        return None;
    }

    fn update(&mut self, events: &[AbsoluteTimeEvent], max_notes: u8) -> Vec<AbsoluteTimeEvent> {
        let mut notes_off: Vec<AbsoluteTimeEvent> = Vec::new();

        for event in events {
            match event.event.data[0] {
                x if x > NOTE_OFF && x < NOTE_OFF + 0x10 => {
                    self.remove(&[event.event.data[0] & 0x0F, event.event.data[1]]);
                }
                x if x > NOTE_ON && x < NOTE_ON + 0x10 => {
                    // TODO ideally the corresponding note off should be also eliminated. maybe after refactoring to states
                    let note_off_option = self.add_event((*event).clone(), max_notes);
                    if let Some(note_off) = note_off_option {
                        notes_off.push(note_off);
                    }
                }
                _ => {}
            }
        }
        notes_off
    }
}

struct DelayedEventConsumer<'a> {
    samples_in_buffer: usize,
    events: &'a mut AbsoluteTimeEventVector,
    current_time_in_samples: usize,
}

impl<'a> Iterator for DelayedEventConsumer<'a> {
    type Item = AbsoluteTimeEvent;

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
                    format_midi_event(&delayed_event.event),
                    delayed_event.play_time_in_samples,
                    self.current_time_in_samples,
                    self.current_time_in_samples + self.samples_in_buffer
                ));
                self.events.remove(0);
                continue;
            };

            if play_time_in_samples > self.current_time_in_samples + self.samples_in_buffer {
                // DebugSocket::send(&*format!(
                //     "too soon for {} ( planned: {} , current buffer: {} - {}",
                //     &delayed_event.event,
                //     delayed_event.play_time_in_samples,
                //     self.current_time_in_samples,
                //     self.current_time_in_samples + self.samples_in_buffer
                // ));
                return None;
            }

            let mut delayed_event: AbsoluteTimeEvent = self.events.remove(0);

            // until we know that the notes can be played in the current buffer, we don't know the final
            // delta frame
            delayed_event.event.delta_frames =
                (delayed_event.play_time_in_samples - self.current_time_in_samples) as i32;

            DebugSocket::send(&*format!(
                "will do {} current_time={} ( play_time_in_samples={} )",
                format_midi_event(&delayed_event.event),
                self.current_time_in_samples,
                delayed_event.play_time_in_samples
            ));

            return Some(delayed_event);
        }
    }
}
