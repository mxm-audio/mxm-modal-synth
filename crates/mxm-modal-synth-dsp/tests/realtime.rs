//! Nothing allocates or frees after `Instrument::new` and `set_sample_rate`: not `render`, the
//! note calls, the expressions or `reset`, at 32 kHz and through the resampler, with every slot in
//! use (the crate's `AGENTS.md`, *Realtime*).

use mxm_modal_synth_dsp::{
    EnvelopeMode, Instrument, LfoShape, Mode, SLOTS, Settings, Slot, Source, Target,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct TrackingAllocator;

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static OPERATIONS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TRACKING.with(|tracking| {
            if tracking.get() {
                OPERATIONS.with(|operations| operations.set(operations.get() + 1));
            }
        });
        // SAFETY: this allocator delegates the exact layout to the process allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        TRACKING.with(|tracking| {
            if tracking.get() {
                OPERATIONS.with(|operations| operations.set(operations.get() + 1));
            }
        });
        // SAFETY: `pointer` and `layout` came from this delegate.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn tracked(run: impl FnOnce()) -> usize {
    OPERATIONS.with(|operations| operations.set(0));
    TRACKING.with(|tracking| tracking.set(true));
    run();
    TRACKING.with(|tracking| tracking.set(false));
    OPERATIONS.with(Cell::get)
}

#[test]
fn nothing_allocates_after_new_and_set_sample_rate() {
    for rate in [32_000.0, 44_100.0, 96_000.0] {
        let mut settings = Settings::default();
        for (i, slot) in settings.slots.iter_mut().enumerate() {
            *slot = Slot {
                source: Some(Source::ALL[(i * 5) % Source::ALL.len()]),
                target: Some(Target::ALL[(i * 3) % Target::ALL.len()]),
                amount: 0.3,
            };
        }
        settings.envelopes[1].mode = EnvelopeMode::Gate;
        settings.envelopes[2].mode = EnvelopeMode::Follow;
        settings.lfos[0].retrigger = true;
        settings.lfos[1].shape = LfoShape::SmoothRandom;
        let mut instrument = Instrument::new();
        instrument.set_sample_rate(rate);
        let (mut left, mut right) = (vec![0.0f32; 1024], vec![0.0f32; 1024]);
        let operations = tracked(|| {
            for k in 0..10u8 {
                instrument.note_on(50 + 2 * k, 0.8, Some(i32::from(k)));
                instrument.set_tuning(50 + 2 * k, Some(i32::from(k)), 0.3);
                instrument.render(&settings, &mut left[..333], &mut right[..333]);
            }
            instrument.set_bend(0.5);
            instrument.set_wheel(0.7);
            instrument.set_pressure(0.2);
            instrument.note_off(52, Some(1));
            instrument.note_off_id(3);
            instrument.choke_id(4);
            instrument.set_tuning_id(5, -1.0);
            instrument.choke(Some(60));
            for slot in &mut settings.slots[..SLOTS / 2] {
                slot.amount = -slot.amount;
            }
            settings.model = 3;
            settings.mode = Mode::Legato;
            instrument.render(&settings, &mut left, &mut right);
            instrument.note_on(70, 1.0, None);
            instrument.note_on(74, 1.0, None);
            instrument.render(&settings, &mut left, &mut right);
            instrument.release_all();
            instrument.render(&settings, &mut left, &mut right);
            let _ = (instrument.tail_samples(), instrument.telemetry());
            instrument.reset();
            instrument.note_on(60, 1.0, None);
            instrument.render(&settings, &mut left, &mut right);
        });
        assert_eq!(
            operations, 0,
            "{rate} Hz: {operations} allocations or frees"
        );
    }
}
