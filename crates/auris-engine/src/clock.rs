//! What the listener is hearing, and when.
//!
//! The playhead says which frame the engine renders next. That is not what anybody hears: the
//! block being rendered reaches the speaker after the device's output latency, and every source in
//! it was held back by the graph's delay compensation on the way. A take that lines itself up
//! against the playhead therefore lands late by all of that, plus whatever the input device took
//! to deliver the performance. Recording needs the other number — which timeline frame was coming
//! out of the speaker at the instant a sample went into the microphone.
//!
//! The output callback knows it. cpal tells it when the block it is filling will be played, and it
//! knows which frame that block starts with. It publishes the pair here after every block, and the
//! input callback reads it against the instant cpal says its own block was captured. The two
//! streams' instants share a clock on the hosts Auris ships for (WASAPI, CoreAudio and ASIO all
//! read one system counter for both directions). Where they might not, the answer is checked for
//! plausibility and dropped rather than trusted.
//!
//! Positions here are *virtual* frames: the playhead minus whatever count-in is still to play. A
//! count-in holds the playhead still while time passes, and the virtual frame is what keeps moving
//! through it, so a performance that starts during the count is placed as correctly as one that
//! starts after it.
//!
//! A sequence lock rather than a mutex, because both readers and the writer are realtime callbacks.
//! The writer never waits; the reader retries a bounded number of times and gives up rather than
//! spin.

use std::sync::atomic::{AtomicU64, Ordering};

/// How many times a reader retries a publication it caught half written.
const READ_ATTEMPTS: usize = 4;

/// The heard timeline position at one instant of the output stream's clock.
#[derive(Debug, Default)]
pub struct OutputClock {
    /// Odd while a publication is being written.
    sequence: AtomicU64,
    /// The virtual frame heard at `nanos`, as `i64` bits: compensation can put it before zero.
    frame: AtomicU64,
    /// The stream instant that frame is heard, in nanoseconds; zero when nothing is moving.
    nanos: AtomicU64,
    /// The output rate, as `f64` bits.
    rate: AtomicU64,
}

/// One consistent reading of an [`OutputClock`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeardAt {
    /// The virtual frame being heard.
    pub frame: i64,
    /// When, on the output stream's clock, in nanoseconds.
    pub nanos: u64,
    /// Frames per second of the output.
    pub rate: f64,
}

impl HeardAt {
    /// The virtual frame being heard at `nanos`, extrapolated at the output rate.
    pub fn frame_at(&self, nanos: u64) -> f64 {
        let seconds = (nanos as f64 - self.nanos as f64) / 1e9;
        self.frame as f64 + seconds * self.rate
    }
}

impl OutputClock {
    /// Records that `frame` will be heard at `nanos`. Called by the output callback, which is the
    /// only writer.
    pub fn publish(&self, frame: i64, nanos: u64, rate: f64) {
        let before = self.sequence.fetch_add(1, Ordering::Relaxed);
        // The odd sequence must be visible before any of the fields it guards change.
        std::sync::atomic::fence(Ordering::Release);
        self.frame.store(frame as u64, Ordering::Relaxed);
        self.nanos.store(nanos, Ordering::Relaxed);
        self.rate.store(rate.to_bits(), Ordering::Relaxed);
        self.sequence
            .store(before.wrapping_add(2), Ordering::Release);
    }

    /// Records that the timeline is not moving, so there is nothing to line a take up against.
    pub fn stop(&self) {
        self.publish(0, 0, 0.0);
    }

    /// The latest consistent publication, if the timeline was moving when it was made.
    pub fn read(&self) -> Option<HeardAt> {
        for _ in 0..READ_ATTEMPTS {
            let before = self.sequence.load(Ordering::Acquire);
            if before % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let frame = self.frame.load(Ordering::Relaxed) as i64;
            let nanos = self.nanos.load(Ordering::Relaxed);
            let rate = f64::from_bits(self.rate.load(Ordering::Relaxed));
            std::sync::atomic::fence(Ordering::Acquire);
            if self.sequence.load(Ordering::Relaxed) != before {
                continue;
            }
            return (nanos != 0 && rate.is_finite() && rate > 0.0).then_some(HeardAt {
                frame,
                nanos,
                rate,
            });
        }
        None
    }
}

/// How many output frames a take stamped at `stamped` lags behind what was heard when its first
/// sample was captured, or `None` when the clocks give no believable answer.
///
/// `stamped` is the virtual frame the capture would otherwise claim its first sample sits at. The
/// lag is what has to come off it. Anything negative or longer than `ceiling_seconds` is not a
/// latency any device has — it is two clocks that do not share an origin — and is refused.
pub fn capture_lag(
    stamped: i64,
    heard: HeardAt,
    captured_nanos: u64,
    ceiling_seconds: f64,
) -> Option<u64> {
    let lag = stamped as f64 - heard.frame_at(captured_nanos);
    (lag.is_finite() && (0.0..=ceiling_seconds * heard.rate).contains(&lag))
        .then(|| lag.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    #[test]
    fn a_reading_extrapolates_along_the_output_clock() {
        let clock = OutputClock::default();
        clock.publish(96_000, 2_000_000_000, RATE);
        let heard = clock.read().unwrap();
        assert_eq!(heard.frame_at(2_000_000_000), 96_000.0);
        assert_eq!(heard.frame_at(2_500_000_000), 120_000.0);
        assert_eq!(heard.frame_at(1_500_000_000), 72_000.0);
    }

    #[test]
    fn a_stopped_timeline_gives_nothing_to_line_up_against() {
        let clock = OutputClock::default();
        assert_eq!(clock.read(), None, "never published");
        clock.publish(10, 5, RATE);
        clock.stop();
        assert_eq!(clock.read(), None);
    }

    #[test]
    fn the_lag_is_the_distance_from_what_was_heard_to_the_stamp() {
        // Heard frame 48 000 at t = 1 s. A sample captured 10 ms later was played against frame
        // 48 480; a take stamped at 50 000 is 1 520 frames late.
        let heard = HeardAt {
            frame: 48_000,
            nanos: 1_000_000_000,
            rate: RATE,
        };
        assert_eq!(capture_lag(50_000, heard, 1_010_000_000, 1.0), Some(1_520));
        assert_eq!(capture_lag(48_480, heard, 1_010_000_000, 1.0), Some(0));
    }

    #[test]
    fn clocks_without_a_shared_origin_are_refused_rather_than_believed() {
        let heard = HeardAt {
            frame: 48_000,
            nanos: 1_000_000_000,
            rate: RATE,
        };
        // A stamp *before* what was heard, or a second and more behind it.
        assert_eq!(capture_lag(40_000, heard, 1_000_000_000, 1.0), None);
        assert_eq!(capture_lag(200_000, heard, 1_000_000_000, 1.0), None);
    }
}
