use log::error;

use core::iter::Filter;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::hash_map::Values;

use util::messages::CC;
use crate::midi_messages::device::{DeviceChange, Expression};
use crate::midi_messages::pattern::Pattern;
use crate::midi_messages::timed_event::TimedEvent;



#[derive(Default)]
pub struct PatternDevice {
    patterns: HashMap<usize, Pattern>
}


pub enum PatternDeviceChange {
    AddPattern { time: usize, pattern: Pattern },
    PatternExpressionChange { time: usize, expression: Expression, pattern: Pattern },
    RemovePattern { time: usize, pattern: Pattern },
    ReplacePattern { time: usize, old_pattern: Pattern, new_pattern: Pattern },
    CC { cc: CC, time: usize },
    None { time: usize },
}


impl PartialOrd for PatternDeviceChange {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let timestamp_cmp = self.timestamp().cmp(&other.timestamp());
        if timestamp_cmp == Ordering::Equal {
            Some(self.id().cmp(&other.id()))
        } else {
            Some(timestamp_cmp)
        }
    }
}

impl PartialEq for PatternDeviceChange {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl TimedEvent for PatternDeviceChange {
    fn timestamp(&self) -> usize {
        match self {
            PatternDeviceChange::AddPattern { time, .. } => *time,
            PatternDeviceChange::PatternExpressionChange { time, .. } => *time,
            PatternDeviceChange::RemovePattern { time, .. } => *time,
            PatternDeviceChange::ReplacePattern { time, .. } => *time,
            PatternDeviceChange::None { time, .. } => *time,
            PatternDeviceChange::CC { time, .. } => *time
        }
    }

    fn id(&self) -> usize {
        match self {
            PatternDeviceChange::AddPattern { pattern, .. } => pattern.id,
            PatternDeviceChange::PatternExpressionChange { pattern, .. } => pattern.id,
            PatternDeviceChange::RemovePattern { pattern, .. } => pattern.id,
            PatternDeviceChange::ReplacePattern { new_pattern: pattern, .. } => pattern.id,
            PatternDeviceChange::CC { .. } => 0,
            PatternDeviceChange::None { .. } => 0,
        }
    }
}


impl PatternDevice {
    pub fn update(&mut self, change: DeviceChange) -> PatternDeviceChange {
        match change {
            DeviceChange::AddNote { time, note } => {
                let new_pattern = Pattern::from(note);
                let pattern = self.patterns.insert(note.id, new_pattern);

                match pattern {
                    None => PatternDeviceChange::AddPattern { time, pattern: new_pattern },
                    Some(old_pattern) => {
                        PatternDeviceChange::ReplacePattern { time, old_pattern, new_pattern }
                    }
                }
            }
            DeviceChange::RemoveNote { time, note } => {
                match self.patterns.remove(&note.id) {
                    None => PatternDeviceChange::None { time },
                    Some(mut pattern) => {
                        pattern.velocity_off = note.velocity_off;
                        pattern.released_at = note.released_at;
                        PatternDeviceChange::RemovePattern { time, pattern }
                    }
                }
            }
            DeviceChange::NoteExpressionChange { time, expression, note } => {
                match self.patterns.get_mut(&note.id) {
                    None => PatternDeviceChange::None { time },
                    Some(pattern) => {
                        pattern.pressure = note.pressure;
                        pattern.timbre = note.timbre;
                        pattern.pitchbend = note.pitchbend;
                        PatternDeviceChange::PatternExpressionChange { time, expression, pattern: *pattern }
                    }
                }
            }
            DeviceChange::ReplaceNote { time, old_note, new_note } => {
                let new_pattern = Pattern::from(new_note);
                match self.patterns.insert(new_pattern.id, new_pattern) {
                    None => {
                        error!("Expected to replace a pattern matching, found nothing {:?}", old_note);
                        PatternDeviceChange::None { time }
                    }
                    Some(old_pattern) => {
                        PatternDeviceChange::ReplacePattern { time, old_pattern, new_pattern }
                    }
                }
            }
            DeviceChange::CCChange { time, cc } => PatternDeviceChange::CC { cc, time },
            DeviceChange::Ignored { time } => PatternDeviceChange::None { time }
        }
    }

    pub fn at(&self, index: u8) -> Filter<Values<'_, usize, Pattern>, PatternIteratorClosure> {
        self.patterns.values().filter(Box::new(move |pattern| pattern.index == index))
    }
}

type PatternIteratorClosure = Box<dyn Fn(&&Pattern) -> bool>;
