use alloc::{
    collections::btree_map::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use core::{
    cell::RefCell,
    sync::atomic::{AtomicUsize, Ordering},
};

use esp_println::println;
use owo_colors::OwoColorize;
use tracing::{field::Visit, span};

type Mutex<T> = embassy_sync::blocking_mutex::CriticalSectionMutex<T>;

struct SpanData {
    name: String,
    location: String,
    values: BTreeMap<String, String>,
}

struct ValueVisitor {
    values: BTreeMap<String, String>,
}

impl ValueVisitor {
    pub fn new() -> ValueVisitor {
        ValueVisitor {
            values: BTreeMap::new(),
        }
    }

    pub fn into_map(self) -> BTreeMap<String, String> {
        self.values
    }
}

impl Visit for ValueVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn alloc::fmt::Debug) {
        self.values
            .insert(field.to_string(), alloc::format!("{:?}", value));
    }
}

pub struct SimpleSubscriber {
    spans: Mutex<RefCell<BTreeMap<usize, SpanData>>>,
    stack: Mutex<RefCell<Vec<u64>>>,
    count: AtomicUsize,
}

impl SimpleSubscriber {
    pub fn new() -> SimpleSubscriber {
        SimpleSubscriber {
            spans: Mutex::new(RefCell::new(BTreeMap::new())),
            stack: Mutex::new(RefCell::new(Vec::new())),
            count: AtomicUsize::new(1),
        }
    }
}

impl tracing::Subscriber for SimpleSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, span: &span::Attributes<'_>) -> span::Id {
        let id = self.count.fetch_add(1, Ordering::Relaxed);
        let mut vv = ValueVisitor::new();
        span.values().record(&mut vv);
        let data = SpanData {
            name: span
                .fields()
                .field("name")
                .map(|f| f.to_string())
                .unwrap_or_else(|| String::from("[no name]")),
            location: span
                .fields()
                .field("location")
                .map(|f| f.to_string())
                .unwrap_or_else(|| String::from("[unknown]")),
            values: vv.into_map(),
        };
        self.spans.lock(|spans| spans.borrow_mut().insert(id, data));
        span::Id::from_u64(id as u64)
    }

    fn record(&self, _span: &span::Id, values: &span::Record<'_>) {
        log::info!("TRACING record {values:?}");
    }

    fn record_follows_from(&self, span: &span::Id, follows: &span::Id) {
        log::info!("TRACING {follows:?} follows {span:?}");
    }

    fn event(&self, event: &tracing::Event<'_>) {
        self.stack.lock(|stack| {
            let stack = stack.borrow();
            for i in stack.iter() {
                let id = (*i).try_into().unwrap();
                self.spans.lock(|spans| {
                    if let Some(span) = spans.borrow().get(&id) {
                        println!(
                            "{} - {} ({:?})",
                            span.name.blue(),
                            span.location,
                            span.values
                        );
                    } else {
                        println!("span unknown");
                    }
                })
            }
        });
        // Ugh, just whatever man
        println!("EVENT {:?}", event.blue());
    }

    fn enter(&self, span: &span::Id) {
        self.stack
            .lock(|stack| stack.borrow_mut().push(span.into_u64()));
    }

    fn exit(&self, span: &span::Id) {
        let i = self.stack.lock(|stack| stack.borrow_mut().pop());
        if let Some(i) = i {
            if i != span.into_u64() {
                log::error!("Exited span we didn't enter!? {i} != {}", span.into_u64());
            }
        } else {
            log::error!("Exited span when we never entered one");
        }
    }
}

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
#[macro_export]
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

/// SliceCursor is kind of the read-side companion to [`BorrowedCursor`](core::io::BorrowedCursor).
///
/// It will panic if you attempt to read beyond the end of the slice.
pub(crate) struct SliceCursor<'a> {
    slice: &'a [u8],
    pos: usize,
}

impl<'a> SliceCursor<'a> {
    pub fn new(slice: &'a [u8]) -> SliceCursor<'a> {
        SliceCursor { slice, pos: 0 }
    }

    /// Return the number of bytes remaining in the cursor
    pub fn remaining(&self) -> usize {
        self.slice.len() - self.pos
    }

    /// Get a subslice for the next `n` bytes of the slice.
    pub fn next(&mut self, n: usize) -> &[u8] {
        assert!(self.pos + n <= self.slice.len());
        let slice = &self.slice[self.pos..self.pos + n];
        self.pos += n;
        slice
    }

    /// Grab the next byte and return it as a `u8`.
    pub fn next_u8(&mut self) -> u8 {
        self.next(1)[0]
    }

    /// Grab the next two bytes and interpret them as a big-endian `u16`.
    pub fn next_u16_be(&mut self) -> u16 {
        u16::from_be_bytes(self.next(2).try_into().unwrap())
    }
}
