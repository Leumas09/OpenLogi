//! Compact transient key indicator: "Caps Lock On" and the like when a
//! keyboard reports one of its lock keys turning on or off, and, on Windows,
//! the screen brightness or media key it reports pressed.
//!
//! The agent decides what shows and for how long
//! ([`openlogi_ipc::LockKeyObservation`]); this module only draws it, with the
//! shared indicator card ([`crate::indicator`]). The window opens with the
//! first change, shows each later change in place, and closes when the agent
//! reports nothing showing. A brightness key also shows the built-in panel's
//! brightness, read here once the key has had time to change it.

use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Hsla, IntoElement, Render, SharedString,
    Window, WindowHandle,
};
use openlogi_core::device::LockKey;
use openlogi_ipc::LockKeyChange;
use openlogi_ui::action_icons::{
    BRIGHTNESS_DOWN_ICON, BRIGHTNESS_UP_ICON, CAPS_LOCK_ICON, FN_LOCK_OFF_ICON, FN_LOCK_ON_ICON,
    NEXT_TRACK_ICON, NUM_LOCK_ICON, PLAY_PAUSE_ICON, PREVIOUS_TRACK_ICON, SCROLL_LOCK_ICON,
};
use openlogi_ui::color::{self, neutral};
use tracing::warn;

use crate::indicator::{self, Level, TEXT};

/// The glyph of a key that is now off.
const RELEASED: Hsla = neutral(0.60, 1.0);

/// The catalog key for the text that announces `change`.
pub(crate) const fn title_key(change: LockKeyChange) -> &'static str {
    match (change.key, change.engaged) {
        (LockKey::CapsLock, true) => "keyboard.caps_lock_on",
        (LockKey::CapsLock, false) => "keyboard.caps_lock_off",
        (LockKey::NumLock, true) => "keyboard.num_lock_on",
        (LockKey::NumLock, false) => "keyboard.num_lock_off",
        (LockKey::ScrollLock, true) => "keyboard.scroll_lock_on",
        (LockKey::ScrollLock, false) => "keyboard.scroll_lock_off",
        (LockKey::FnLock, true) => "keyboard.fn_lock_on",
        (LockKey::FnLock, false) => "keyboard.fn_lock_off",
        (LockKey::BrightnessDown | LockKey::BrightnessUp, _) => "keyboard.indicator_brightness",
        (LockKey::PreviousTrack, _) => "actions.previous_track",
        (LockKey::PlayPause, _) => "actions.play_pause",
        (LockKey::NextTrack, _) => "actions.next_track",
    }
}

/// The glyph that stands for `change`.
pub(crate) const fn icon(change: LockKeyChange) -> &'static str {
    match (change.key, change.engaged) {
        (LockKey::CapsLock, _) => CAPS_LOCK_ICON,
        (LockKey::NumLock, _) => NUM_LOCK_ICON,
        (LockKey::ScrollLock, _) => SCROLL_LOCK_ICON,
        (LockKey::FnLock, true) => FN_LOCK_ON_ICON,
        (LockKey::FnLock, false) => FN_LOCK_OFF_ICON,
        (LockKey::BrightnessDown, _) => BRIGHTNESS_DOWN_ICON,
        (LockKey::BrightnessUp, _) => BRIGHTNESS_UP_ICON,
        (LockKey::PreviousTrack, _) => PREVIOUS_TRACK_ICON,
        (LockKey::PlayPause, _) => PLAY_PAUSE_ICON,
        (LockKey::NextTrack, _) => NEXT_TRACK_ICON,
    }
}

/// The colour of the glyph for `change`. A lock key that is on takes the
/// brand accent, the same blue the level bars and the settings app paint
/// with, and one that is off is grey; a pressed key carries no state.
fn glyph_colour(change: LockKeyChange) -> Hsla {
    if change.key.is_press() {
        TEXT
    } else if change.engaged {
        color::accent()
    } else {
        RELEASED
    }
}

/// Whether `key` changes the screen brightness, so its indicator shows it.
const fn shows_brightness(key: LockKey) -> bool {
    matches!(key, LockKey::BrightnessDown | LockKey::BrightnessUp)
}

/// The bar for a screen brightness of `percent`.
fn brightness_level(percent: u8) -> Level {
    Level {
        fraction: f32::from(percent) / 100.0,
        label: SharedString::from(percent.to_string()),
    }
}

/// The indicator window's view: the latest change the agent is showing, and
/// for a brightness key the brightness read since.
pub(crate) struct LockKeyView {
    change: LockKeyChange,
    brightness: Option<u8>,
}

impl Render for LockKeyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let level = shows_brightness(self.change.key)
            .then_some(self.brightness)
            .flatten()
            .map(brightness_level);
        indicator::card(
            icon(self.change),
            glyph_colour(self.change),
            rust_i18n::t!(title_key(self.change)).into_owned(),
            level,
        )
    }
}

/// Show `shown`, or close the indicator when it is `None`.
///
/// A change while the window is open replaces what it shows in place, so
/// presses in quick succession do not flash the window closed and open again.
pub(crate) fn present(cx: &mut App, shown: Option<LockKeyChange>) {
    let open: Vec<WindowHandle<LockKeyView>> = cx
        .windows()
        .iter()
        .filter_map(AnyWindowHandle::downcast::<LockKeyView>)
        .collect();
    let Some(change) = shown else {
        for view in open {
            let _ = view.update(cx, |_, window, _| window.remove_window());
        }
        return;
    };
    let brightness = if shows_brightness(change.key) {
        indicator::screen_brightness()
    } else {
        None
    };
    if open.is_empty() {
        let options = indicator::window_options(cx, "openlogi-lock-key-osd");
        if let Err(error) = indicator::open(cx, options, |_, cx| {
            cx.new(|_| LockKeyView { change, brightness })
        }) {
            warn!(%error, "could not open the lock-key indicator window");
        }
    } else {
        for view in open {
            let _ = view.update(cx, |view, _, cx| {
                view.change = change;
                view.brightness = brightness;
                cx.notify();
            });
        }
    }
    if shows_brightness(change.key) {
        follow_brightness(cx);
    }
}

/// Re-read the panel's brightness a few times after a brightness key, as
/// Windows eases it to its new level, and show each reading.
#[cfg(all(windows, not(test)))]
fn follow_brightness(cx: &mut App) {
    use std::time::Duration;

    cx.spawn(async move |cx| {
        for delay in [120, 250, 450] {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let brightness = indicator::screen_brightness();
            cx.update(|cx| {
                for handle in cx.windows() {
                    if let Some(view) = handle.downcast::<LockKeyView>() {
                        let _ = view.update(cx, |view, _, cx| {
                            if shows_brightness(view.change.key) {
                                view.brightness = brightness;
                                cx.notify();
                            }
                        });
                    }
                }
            });
        }
    })
    .detach();
}

/// Elsewhere no panel reports a brightness, and tests read none.
#[cfg(any(not(windows), test))]
const fn follow_brightness(_cx: &mut App) {}

#[cfg(test)]
mod tests {
    use super::*;

    const fn change(key: LockKey, engaged: bool) -> LockKeyChange {
        LockKeyChange { key, engaged }
    }

    #[test]
    fn each_change_announces_its_own_key_and_state() {
        let expected = [
            (change(LockKey::CapsLock, true), "keyboard.caps_lock_on"),
            (change(LockKey::CapsLock, false), "keyboard.caps_lock_off"),
            (change(LockKey::NumLock, true), "keyboard.num_lock_on"),
            (change(LockKey::NumLock, false), "keyboard.num_lock_off"),
            (change(LockKey::ScrollLock, true), "keyboard.scroll_lock_on"),
            (
                change(LockKey::ScrollLock, false),
                "keyboard.scroll_lock_off",
            ),
            (change(LockKey::FnLock, true), "keyboard.fn_lock_on"),
            (change(LockKey::FnLock, false), "keyboard.fn_lock_off"),
            (
                change(LockKey::BrightnessDown, true),
                "keyboard.indicator_brightness",
            ),
            (
                change(LockKey::BrightnessUp, true),
                "keyboard.indicator_brightness",
            ),
            (
                change(LockKey::PreviousTrack, true),
                "actions.previous_track",
            ),
            (change(LockKey::PlayPause, true), "actions.play_pause"),
            (change(LockKey::NextTrack, true), "actions.next_track"),
        ];
        // Exhaustive on purpose: a new lock key stops this compiling until it
        // is listed above.
        let _: fn(LockKey) = |key| match key {
            LockKey::CapsLock
            | LockKey::NumLock
            | LockKey::ScrollLock
            | LockKey::FnLock
            | LockKey::BrightnessDown
            | LockKey::BrightnessUp
            | LockKey::PreviousTrack
            | LockKey::PlayPause
            | LockKey::NextTrack => {}
        };
        for (change, key) in expected {
            assert_eq!(title_key(change), key, "{change:?}");
        }
    }

    /// A key missing from the catalog would show the raw key on screen. The
    /// other locales are held to the same keys by `catalog_parity`.
    #[test]
    fn every_announcement_is_in_the_catalog() {
        for key in ALL_KEYS {
            for engaged in [true, false] {
                let title = title_key(change(key, engaged));
                assert_ne!(rust_i18n::t!(title, locale = "en"), title);
            }
        }
    }

    const ALL_KEYS: [LockKey; 9] = [
        LockKey::CapsLock,
        LockKey::NumLock,
        LockKey::ScrollLock,
        LockKey::FnLock,
        LockKey::BrightnessDown,
        LockKey::BrightnessUp,
        LockKey::PreviousTrack,
        LockKey::PlayPause,
        LockKey::NextTrack,
    ];

    /// A glyph path missing from the embedded icons would draw nothing.
    #[test]
    fn every_glyph_is_an_embedded_icon() {
        for key in ALL_KEYS {
            for engaged in [true, false] {
                let path = icon(change(key, engaged));
                assert!(
                    openlogi_ui::action_icons::embedded_icon(path).is_some(),
                    "{path} for {key:?}"
                );
            }
        }
    }

    #[test]
    fn fn_lock_shows_a_padlock_shut_or_open() {
        assert_ne!(
            icon(change(LockKey::FnLock, true)),
            icon(change(LockKey::FnLock, false))
        );
    }

    #[test]
    fn only_a_brightness_key_shows_a_level() {
        for key in ALL_KEYS {
            assert_eq!(
                shows_brightness(key),
                matches!(key, LockKey::BrightnessDown | LockKey::BrightnessUp),
                "{key:?}"
            );
        }
        let level = brightness_level(68);
        assert!((level.fraction - 0.68).abs() < 1e-6);
        assert_eq!(level.label.as_ref(), "68");
    }

    #[test]
    fn a_key_turning_on_is_drawn_apart_from_one_turning_off() {
        let on = change(LockKey::CapsLock, true);
        let off = change(LockKey::CapsLock, false);
        assert_eq!(glyph_colour(on), color::accent());
        assert_eq!(glyph_colour(off), RELEASED);
        // A pressed key carries no state to colour.
        assert_eq!(glyph_colour(change(LockKey::PlayPause, true)), TEXT);
    }

    /// The change each open indicator window shows.
    fn shown(cx: &mut gpui::TestAppContext) -> Vec<LockKeyChange> {
        cx.update(|cx| {
            cx.windows()
                .iter()
                .filter_map(AnyWindowHandle::downcast::<LockKeyView>)
                .filter_map(|window| window.read(cx).ok().map(|view| view.change))
                .collect()
        })
    }

    #[gpui::test]
    fn one_window_shows_each_change_in_place_until_nothing_shows(cx: &mut gpui::TestAppContext) {
        let on = change(LockKey::CapsLock, true);
        let off = change(LockKey::CapsLock, false);

        cx.update(|cx| present(cx, None));
        assert!(shown(cx).is_empty(), "nothing showing opens nothing");

        cx.update(|cx| present(cx, Some(on)));
        assert_eq!(shown(cx), [on]);

        cx.update(|cx| present(cx, Some(off)));
        cx.run_until_parked();
        assert_eq!(shown(cx), [off], "the open window shows the newer change");

        cx.update(|cx| present(cx, None));
        cx.run_until_parked();
        assert!(shown(cx).is_empty(), "nothing showing closes the window");
    }
}
