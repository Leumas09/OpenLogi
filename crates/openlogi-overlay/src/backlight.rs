//! Compact transient keyboard-backlight indicator, on the shared indicator
//! card ([`crate::indicator`]): a bulb, the settings app's "Backlight" label,
//! and the level as a bar with "level/maximum" beside it.

use gpui::{App, Context, IntoElement, Render, SharedString, Window, WindowOptions};
use openlogi_core::hid::backlight::max_backlight_level;
use openlogi_ipc::BacklightObservation;
use openlogi_ui::action_icons::BACKLIGHT_ICON;

use crate::indicator::{self, Level, TEXT};

pub(crate) struct BacklightView {
    pub(crate) observation: BacklightObservation,
}

impl BacklightView {
    pub(crate) fn new(observation: BacklightObservation) -> Self {
        Self { observation }
    }
}

impl Render for BacklightView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let max_level = max_backlight_level(self.observation.levels);
        let current = self.observation.current_level;
        // The settings app's own "Backlight" label, already translated in
        // every catalog; the level and its maximum are plain numbers.
        indicator::card(
            BACKLIGHT_ICON,
            TEXT,
            rust_i18n::t!("device.backlight").into_owned(),
            Some(Level {
                fraction: relative_level(current, max_level),
                label: SharedString::from(format!("{}/{max_level}", current.min(max_level))),
            }),
        )
    }
}

fn relative_level(level: u8, max_level: u8) -> f32 {
    if max_level == 0 {
        0.0
    } else {
        f32::from(level.min(max_level)) / f32::from(max_level)
    }
}

pub(crate) fn window_options(cx: &mut App) -> WindowOptions {
    indicator::window_options(cx, "openlogi-backlight-osd")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highest_reported_level_fills_the_bar() {
        assert_eq!(relative_level(7, max_backlight_level(8)), 1.0);
        assert_eq!(relative_level(0, max_backlight_level(8)), 0.0);
    }
}
