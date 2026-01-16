use alloc::vec::Vec;

use aranya_runtime::{Sink, VmEffect};

/// Holds a collection of effect data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VecSink<Eff> {
    /// Effects from executing a policy action.
    pub(crate) effects: Vec<Eff>,
}

impl<Eff> VecSink<Eff> {
    /// Creates a new `VecSink`.
    pub const fn new() -> Self {
        Self {
            effects: Vec::new(),
        }
    }

    /// Returns the collected effects.
    pub fn collect<T>(self) -> Result<Vec<T>, <T as TryFrom<Eff>>::Error>
    where
        T: TryFrom<Eff>,
    {
        self.effects.into_iter().map(T::try_from).collect()
    }
}

impl<Eff> Sink<Eff> for VecSink<Eff> {
    fn begin(&mut self) {}

    fn consume(&mut self, effect: Eff) {
        self.effects.push(effect);
    }

    fn rollback(&mut self) {}

    fn commit(&mut self) {}
}

pub struct DebugSink {}

impl Sink<VmEffect> for DebugSink {
    fn begin(&mut self) {
        log::info!("DebugSink begin");
    }

    fn consume(&mut self, effect: VmEffect) {
        log::info!("DebugSink consume {effect}");
    }

    fn rollback(&mut self) {
        log::info!("DebugSink rollback");
    }

    fn commit(&mut self) {
        log::info!("DebugSink commit");
    }
}
