//! The transient keyboard indicators — lock keys, function-row keys and the
//! backlight: how they look, and where they open.
//!
//! Every indicator is the same compact card: a glyph, a title, and for a
//! level (screen brightness, backlight) a bar with its value.
//!
//! On Windows the card is frosted glass: the window takes the system's
//! acrylic blur behind a translucent dark tint, with the rounded corners and
//! hairline border Windows 11 gives its own flyouts. It sits where the
//! notices do — at the bottom centre of the display under the cursor, above
//! the taskbar, out of the way of what the user is working on — and opens
//! above the indicators and notices already showing, so two never cover each
//! other. Elsewhere it keeps an opaque card in a window centred on the main
//! display.

use gpui::{
    App, Bounds, Div, Entity, FontWeight, Hsla, ParentElement, Render, SharedString, Styled,
    Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div,
    prelude::FluentBuilder as _, px, relative, size, svg,
};
use openlogi_ui::color::{self, neutral};

/// Every indicator window's size, in logical pixels: wide enough for the
/// longest translated title beside its glyph and value.
pub(crate) const CARD_WIDTH: f32 = 320.0;
pub(crate) const CARD_HEIGHT: f32 = 56.0;

/// The card's corner radius: Windows 11's own for small flyouts, which DWM
/// clips the window to.
const CORNER: f32 = 8.0;

/// The card's fill. Over the acrylic blur a translucent tint is enough to
/// keep white text legible on any wallpaper; without the blur the card has to
/// be nearly opaque.
#[cfg(windows)]
const CARD_TINT: Hsla = neutral(0.12, 0.78);
#[cfg(not(windows))]
const CARD_TINT: Hsla = neutral(0.08, 0.94);

#[cfg(windows)]
const BACKGROUND: WindowBackgroundAppearance = WindowBackgroundAppearance::Blurred;
#[cfg(not(windows))]
const BACKGROUND: WindowBackgroundAppearance = WindowBackgroundAppearance::Transparent;

/// The title's colour, and a glyph's when it carries no state.
pub(crate) const TEXT: Hsla = neutral(0.97, 1.0);
/// The value beside a level bar.
const SECONDARY: Hsla = neutral(0.82, 1.0);
/// The part of a level bar above the level.
const TRACK: Hsla = neutral(1.0, 0.22);

/// A level an indicator shows as a bar, with its value written beside it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Level {
    /// How full the bar is, from 0 to 1.
    pub(crate) fraction: f32,
    /// The value as written beside the bar, such as `68` or `3/4`.
    pub(crate) label: SharedString,
}

/// The indicator card: `icon` in `icon_colour`, `title`, and `level` when
/// there is one.
pub(crate) fn card(
    icon: &'static str,
    icon_colour: Hsla,
    title: impl Into<SharedString>,
    level: Option<Level>,
) -> Div {
    let bar = level.as_ref().map(|level| {
        div().w_full().h(px(4.0)).rounded_full().bg(TRACK).child(
            div()
                .h_full()
                .w(relative(level.fraction.clamp(0.0, 1.0)))
                .rounded_full()
                .bg(color::accent()),
        )
    });
    div()
        .size_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .px(px(16.0))
        .rounded(px(CORNER))
        .bg(CARD_TINT)
        .text_color(TEXT)
        .text_size(px(14.0))
        .child(
            svg()
                .path(icon)
                .size(px(22.0))
                .flex_none()
                .text_color(icon_colour),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(7.0))
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.into()),
                )
                .when_some(bar, ParentElement::child),
        )
        .when_some(level, |row, level| {
            row.child(
                div()
                    .flex_none()
                    .min_w(px(34.0))
                    .text_right()
                    .text_size(px(13.0))
                    .text_color(SECONDARY)
                    .child(level.label),
            )
        })
}

/// The options every indicator window opens with; `app_id` names which one
/// it is to a compositor or window rule.
pub(crate) fn window_options(cx: &mut App, app_id: &str) -> WindowOptions {
    let bounds = Bounds::centered(None, size(px(CARD_WIDTH), px(CARD_HEIGHT)), cx);
    WindowOptions {
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: BACKGROUND,
        app_id: Some(app_id.to_string()),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        ..WindowOptions::default()
    }
}

/// Open an indicator window with `options`, placed as the module doc says.
pub(crate) fn open<V: Render + 'static>(
    cx: &mut App,
    options: WindowOptions,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> anyhow::Result<WindowHandle<V>> {
    #[cfg(all(windows, not(test)))]
    {
        windows::open_at_bottom(cx, options, build)
    }
    #[cfg(any(not(windows), test))]
    {
        cx.open_window(options, build)
    }
}

/// The screen brightness, from 0 to 100, where the platform reports it: on
/// Windows, the built-in panel's. External monitors report none.
pub(crate) fn screen_brightness() -> Option<u8> {
    #[cfg(windows)]
    {
        crate::platform::screen_brightness()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(all(windows, not(test)))]
pub(crate) use windows::stacked_height;

#[cfg(all(windows, not(test)))]
mod windows {
    use anyhow::anyhow;
    use gpui::{
        AnyWindowHandle, App, Entity, Pixels, Render, Window, WindowBounds, WindowHandle,
        WindowOptions, px,
    };
    use tracing::debug;

    use crate::backlight::BacklightView;
    use crate::lock_key::LockKeyView;
    use crate::notice::NoticeView;
    use crate::platform::NoticePlacement;

    /// Space between two stacked windows. Each already carries its own clear
    /// margin, so this only keeps them visibly apart.
    const STACK_GAP: f32 = 8.0;

    pub(super) fn open_at_bottom<V: Render + 'static>(
        cx: &mut App,
        options: WindowOptions,
        build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
    ) -> anyhow::Result<WindowHandle<V>> {
        let Some(WindowBounds::Windowed(requested)) = options.window_bounds else {
            return cx.open_window(options, build);
        };
        let lift = stacked_height::<V>(cx);
        let placement = NoticePlacement::capture(cx, requested.size)?.lifted(lift);
        let bootstrap = placement
            .openings()
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("the bottom placement offers no window"))?;
        let options = WindowOptions {
            display_id: bootstrap.display_id,
            window_bounds: bootstrap.window_bounds,
            show: bootstrap.show,
            ..options
        };
        let handle = cx.open_window(options, build)?;
        let placed = handle
            .update(cx, |_, window, cx| {
                // Native placement resizes the window inside this update, so
                // resync GPUI's viewport and scale, as the notice does.
                placement.show(window)?;
                window.bounds_changed(cx);
                // Cosmetic: a square card is still a working indicator.
                if let Err(error) = crate::platform::round_corners(window) {
                    debug!(%error, "indicator keeps square corners");
                }
                anyhow::Ok(())
            })
            .and_then(std::convert::identity);
        if let Err(error) = placed {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            return Err(error);
        }
        Ok(handle)
    }

    /// The height of the bottom windows already showing other than `V`'s
    /// own — notices and indicators — each with the gap above it: how far a
    /// new `V` window rises to clear them.
    pub(crate) fn stacked_height<V: 'static>(cx: &mut App) -> Pixels {
        let below: Vec<AnyWindowHandle> = cx
            .windows()
            .into_iter()
            .filter(|handle| handle.downcast::<V>().is_none() && stacks_at_bottom(*handle))
            .collect();
        below
            .into_iter()
            .filter_map(|handle| {
                handle
                    .update(cx, |_, window, _| window.bounds().size.height)
                    .ok()
            })
            .fold(px(0.0), |total, height| total + height + px(STACK_GAP))
    }

    fn stacks_at_bottom(handle: AnyWindowHandle) -> bool {
        handle.downcast::<NoticeView>().is_some()
            || handle.downcast::<LockKeyView>().is_some()
            || handle.downcast::<BacklightView>().is_some()
    }
}
