pub use vst::util::AtomicFloat as FloatParameter;
use std::sync::atomic::{AtomicBool, Ordering, AtomicU8};


pub struct BoolParameter {
    atomic: AtomicBool
}

impl BoolParameter {
    pub fn new(value: bool) -> BoolParameter {
        BoolParameter {
            atomic: AtomicBool::new(value),
        }
    }

    pub fn get(&self) -> bool {
        self.atomic.load(Ordering::Relaxed)
    }

    pub fn get_as_f32(&self) -> f32 {
        if self.get() { 1.0 } else { 0.0 }
    }

    pub fn set(&self, value: bool) {
        self.atomic.store(value, Ordering::Relaxed)
    }

    pub fn set_from_f32(&self, value: f32) {
        self.set(value > 0.5)
    }
}

impl Default for BoolParameter {
    fn default() -> Self {
        BoolParameter::new(false)
    }
}


pub struct ByteParameter {
    atomic: AtomicU8
}

impl ByteParameter {
    pub fn new(value: u8) -> ByteParameter {
        ByteParameter {
            atomic: AtomicU8::new(value),
        }
    }

    pub fn get(&self) -> u8 {
        self.atomic.load(Ordering::Relaxed)
    }

    pub fn get_as_f32(&self) -> f32 {
        self.get() as f32 / 127.
    }

    pub fn set(&self, value: u8) {
        self.atomic.store(value, Ordering::Relaxed)
    }

    pub fn set_from_f32(&self, value: f32) {
        self.set((value * 127.) as u8)
    }
}


impl Default for ByteParameter {
    fn default() -> Self {
        ByteParameter::new(64)
    }
}