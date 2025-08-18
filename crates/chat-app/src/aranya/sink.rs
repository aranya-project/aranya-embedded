use alloc::vec::Vec;

use aranya_runtime::{Sink, VmEffect};

use crate::aranya::daemon::EFFECT_OUT_CHANNEL;

/// Holds a collection of effect data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VecSink<E> {
    /// Effects from executing a policy action.
    pub(crate) effects: Vec<E>,
}

impl<E> VecSink<E> {
    /// Creates a new `VecSink`.
    pub const fn new() -> Self {
        Self {
            effects: Vec::new(),
        }
    }

    /// Returns the collected effects.
    pub fn collect<T>(self) -> Result<Vec<T>, <T as TryFrom<E>>::Error>
    where
        T: TryFrom<E>,
    {
        self.effects.into_iter().map(T::try_from).collect()
    }
}

impl<E> Sink<E> for VecSink<E> {
    fn begin(&mut self) {}

    fn consume(&mut self, effect: E) {
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

pub struct PubSubSink<'s> {
    publisher: crate::aranya::daemon::Publisher<'s, VmEffect>,
}

impl<'s> PubSubSink<'s> {
    pub fn new() -> PubSubSink<'s> {
        PubSubSink {
            publisher: EFFECT_OUT_CHANNEL
                .publisher()
                .expect("could not get publisher slot in PubSubSink"),
        }
    }
}

impl<'s> Sink<VmEffect> for PubSubSink<'s> {
    fn begin(&mut self) {}

    fn consume(&mut self, effect: VmEffect) {
        self.publisher
            .try_publish(effect)
            .inspect_err(|_| log::error!("dropping effect - no space in publish queue"))
            .ok();
    }

    fn rollback(&mut self) {}

    fn commit(&mut self) {}
}
