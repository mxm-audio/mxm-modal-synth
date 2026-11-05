//! Lock-free audio-to-editor observations and the developer channel's requests: atomics only,
//! written once per internal block. The editor reads them; the audio never reads the editor.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct Telemetry {
    /// The output's peak and its clip latch: the collection's one meter policy
    /// (`mxm_telemetry::PeakMeter`), max-combined until read.
    pub meter: mxm_telemetry::PeakMeter,
    /// The developer channel's requests of the editor, each taken once
    /// (`mxm_telemetry::DevRequests`).
    pub dev: mxm_telemetry::DevRequests,
    /// The host tempo in force, so a synced LFO rate reads its division.
    pub tempo: mxm_tempo::TempoCell,
    voices_sounding: AtomicU32,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            meter: mxm_telemetry::PeakMeter::new(),
            dev: mxm_telemetry::DevRequests::new(),
            tempo: mxm_tempo::TempoCell::new(),
            voices_sounding: AtomicU32::new(0),
        }
    }

    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// What one render left: the instrument's own telemetry.
    pub fn publish(&self, telemetry: mxm_modal_synth_dsp::Telemetry) {
        self.meter.publish(telemetry.peak);
        self.voices_sounding.store(
            u32::try_from(telemetry.voices_sounding).unwrap_or(u32::MAX),
            Ordering::Relaxed,
        );
    }

    /// How many voices sounded in the last block.
    #[must_use]
    pub fn voices_sounding(&self) -> u32 {
        self.voices_sounding.load(Ordering::Relaxed)
    }
}
