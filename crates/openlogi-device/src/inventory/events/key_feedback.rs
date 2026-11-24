//! Function-row keys whose presses the on-screen key indicator acknowledges:
//! screen brightness and the media keys.
//!
//! The keys keep their own action; the keyboard is only asked, once per
//! connection, to also report their presses over HID++ `0x1b04` analytics
//! (`setCidReporting` with the analytics flag alone, which leaves diversion
//! and remapping untouched). [`FeedbackArming`] owns when that request is due.

use hidpp::feature::reprog_controls::{AnalyticsKeyEvent, ControlId};
use openlogi_core::device::LockKey;

/// The controls asked to report their presses, and the key each one is.
/// Control IDs are Logitech's own (see `openlogi_core::binding::control`).
pub(super) const FEEDBACK_CONTROLS: [(u16, LockKey); 6] = [
    (0x00c7, LockKey::BrightnessDown),
    (0x00c8, LockKey::BrightnessUp),
    (0x00e4, LockKey::PreviousTrack),
    (0x00e5, LockKey::PlayPause),
    (0x0141, LockKey::PlayPause),
    (0x00e6, LockKey::NextTrack),
];

/// The key a control's analytics report is about, when it is one of
/// [`FEEDBACK_CONTROLS`].
fn feedback_key(cid: ControlId) -> Option<LockKey> {
    FEEDBACK_CONTROLS
        .iter()
        .find_map(|&(control, key)| (control == cid.0).then_some(key))
}

/// The keys an analytics batch reports pressed, in report order.
///
/// A batch lists up to five events; an unused slot carries control `0`. The
/// event byte is a code the feature leaves to the device; a key that sends
/// more than one per press is acknowledged again, which the indicator, which
/// replaces what it shows in place, absorbs.
pub(super) fn pressed(events: [AnalyticsKeyEvent; 5]) -> impl Iterator<Item = LockKey> {
    events
        .into_iter()
        .filter(|event| event.cid.0 != 0)
        .filter_map(|event| feedback_key(event.cid))
}

/// One device's analytics request, and which of its connections it is about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct FeedbackArming {
    /// Bumped each time the device comes back: a request started before a
    /// comeback does not mark the new connection armed.
    connection: u64,
    phase: Phase,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    /// The next scan asks the keyboard to report the keys.
    #[default]
    Due,
    /// This connection's request is in flight, or done.
    Asked,
}

/// The connection an analytics request started on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ArmConnection(u64);

impl FeedbackArming {
    /// The device was away: its reporting settings reset with the link, so
    /// the next scan asks again.
    pub(super) fn forget(&mut self) {
        self.connection = self.connection.wrapping_add(1);
        self.phase = Phase::Due;
    }

    /// Start this connection's one request, unless it was started already.
    pub(super) fn start(&mut self) -> Option<ArmConnection> {
        if self.phase != Phase::Due {
            return None;
        }
        self.phase = Phase::Asked;
        Some(ArmConnection(self.connection))
    }

    /// The request started on `connection` failed before any key accepted
    /// it: ask again on the next scan, unless the device came back meanwhile
    /// (which already made the request due).
    pub(super) fn retry(&mut self, connection: ArmConnection) {
        if connection.0 == self.connection {
            self.phase = Phase::Due;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(cid: u16, code: u8) -> AnalyticsKeyEvent {
        AnalyticsKeyEvent {
            cid: ControlId(cid),
            event: code,
        }
    }

    #[test]
    fn a_batch_names_the_feedback_keys_it_reports_in_order() {
        let mut batch = [AnalyticsKeyEvent::default(); 5];
        batch[0] = event(0x00c8, 1);
        // Easy-Switch keys report through the same feature; not ours.
        batch[1] = event(0x00d1, 1);
        batch[2] = event(0x00e5, 0);
        let keys: Vec<_> = pressed(batch).collect();
        assert_eq!(keys, [LockKey::BrightnessUp, LockKey::PlayPause]);
        assert_eq!(pressed([AnalyticsKeyEvent::default(); 5]).count(), 0);
    }

    #[test]
    fn every_feedback_key_is_a_press_the_indicator_acknowledges() {
        for (_, key) in FEEDBACK_CONTROLS {
            assert!(key.is_press(), "{key:?}");
        }
    }

    #[test]
    fn arming_is_asked_once_per_connection() {
        let mut arming = FeedbackArming::default();
        let first = arming.start().expect("due on a new connection");
        assert_eq!(arming.start(), None, "asked once");

        // A failed request is retried on the same connection.
        arming.retry(first);
        let again = arming.start().expect("due again after a failure");

        // A comeback makes it due; the old request's failure changes nothing.
        arming.forget();
        arming.retry(again);
        assert!(arming.start().is_some());
        assert_eq!(arming.start(), None);
    }
}
