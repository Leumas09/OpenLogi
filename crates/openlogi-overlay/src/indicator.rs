//! Where the transient keyboard indicators — lock keys and the backlight —
//! open.
//!
//! On Windows they sit where the notices do: at the bottom centre of the
//! display under the cursor, above the taskbar, out of the way of what the
//! user is working on. Each opens above the indicators and notices already
//! showing, so two never cover each other. Elsewhere they keep the window
//! their own options centre on the main display.

use gpui::{App, Entity, Render, Window, WindowHandle, WindowOptions};

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

#[cfg(all(windows, not(test)))]
pub(crate) use windows::stacked_height;

#[cfg(all(windows, not(test)))]
mod windows {
    use anyhow::anyhow;
    use gpui::{
        AnyWindowHandle, App, Entity, Pixels, Render, Window, WindowBounds, WindowHandle,
        WindowOptions, px,
    };

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
                placement.show(window).map(|()| window.bounds_changed(cx))
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
