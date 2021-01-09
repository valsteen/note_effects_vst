use core::cmp::{Ordering, PartialEq, PartialOrd};
use core::option::Option;
use crate::midi_messages::device::DeviceChange;
use crate::midi_messages::pattern_device::PatternDeviceChange;
use crate::midi_messages::timed_event::TimedEvent;

pub enum SourceChange {
    NoteChange(DeviceChange),
    PatternChange(PatternDeviceChange)
}


impl PartialOrd for SourceChange {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // note come first, in order to start the pattern with the intended note
        match self {
            SourceChange::PatternChange(pattern) => {
                match other {
                    SourceChange::PatternChange(other_pattern) => {
                        pattern.partial_cmp(other_pattern)
                    }
                    SourceChange::NoteChange(_) => Option::Some(Ordering::Greater)
                }
            }
            SourceChange::NoteChange(note) => {
                match other {
                    SourceChange::PatternChange(_) => Option::Some(Ordering::Less),
                    SourceChange::NoteChange(other_note) => {
                        note.partial_cmp(other_note)
                    }
                }
            }
        }
    }
}


impl PartialEq for SourceChange {
    fn eq(&self, other: &Self) -> bool {
        match self {
            SourceChange::PatternChange(pattern) => {
                match other {
                    SourceChange::PatternChange(other_pattern) => pattern.eq(other_pattern),
                    SourceChange::NoteChange(_) => false
                }
            }
            SourceChange::NoteChange(note) => {
                match other {
                    SourceChange::PatternChange(_) => false,
                    SourceChange::NoteChange(other_note) => note.eq(other_note)
                }
            }
        }
    }
}


impl Ord for SourceChange {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Eq for SourceChange {

}

impl TimedEvent for SourceChange {
    fn timestamp(&self) -> usize {
        match self {
            SourceChange::NoteChange(note) => note.timestamp(),
            SourceChange::PatternChange(pattern) => pattern.timestamp()
        }
    }

    fn id(&self) -> usize {
        match self {
            SourceChange::NoteChange(note) => note.id(),
            SourceChange::PatternChange(pattern) => pattern.id()
        }
    }
}
