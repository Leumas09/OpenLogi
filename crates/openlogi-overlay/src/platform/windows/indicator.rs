//! Win32 for the key indicators: Windows 11's rounded flyout corners, and the
//! built-in panel's brightness the brightness indicator shows.

use windows_sys::Win32::Devices::Display::{
    DISPLAY_BRIGHTNESS, IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS,
};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

#[cfg(not(test))]
pub(crate) use corners::round_corners;

#[cfg(not(test))]
mod corners {
    use anyhow::Result;
    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DwmSetWindowAttribute,
    };

    /// Give a placed indicator window Windows 11's rounded corners and hairline
    /// border, which the shared overlay placement turns off for the ring.
    /// Windows 10 has neither attribute and keeps the window square.
    #[expect(
        unsafe_code,
        reason = "DWM attribute writes on a borrowed HWND, with typed values"
    )]
    pub(crate) fn round_corners(window: &Window) -> Result<()> {
        // The trait's handle, not GPUI's own `Window::window_handle`.
        let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window)
            .map_err(|error| anyhow::anyhow!("get indicator HWND: {error}"))?
            .as_raw()
        else {
            anyhow::bail!("indicator window has no Win32 handle");
        };
        let hwnd = handle.hwnd.get() as HWND;
        let size = u32::try_from(std::mem::size_of::<u32>())?;
        for (attribute, value) in [
            (DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND.cast_unsigned()),
            (DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT),
        ] {
            // SAFETY: hwnd belongs to the still-borrowed window, `value` outlives
            // this synchronous call, and `size` is the DWORD both attributes take.
            let hr = unsafe {
                DwmSetWindowAttribute(
                    hwnd,
                    attribute.cast_unsigned(),
                    std::ptr::from_ref(&value).cast(),
                    size,
                )
            };
            anyhow::ensure!(
                hr >= 0,
                "DWM attribute {attribute} refused (HRESULT {hr:#x})"
            );
        }
        Ok(())
    }
}

/// The built-in panel's brightness, from 0 to 100, under the current power
/// source, as the video port reports it to `\\.\LCD`. `None` without a
/// built-in panel, or when the driver does not answer.
#[expect(
    unsafe_code,
    reason = "CreateFileW, DeviceIoControl and GetSystemPowerStatus with owned out-buffers"
)]
pub(crate) fn screen_brightness() -> Option<u8> {
    let name: Vec<u16> = r"\\.\LCD".encode_utf16().chain([0]).collect();
    // SAFETY: `name` is NUL-terminated and outlives the call. No access right
    // is asked for: the brightness query needs none.
    let device = unsafe {
        CreateFileW(
            name.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if device == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut brightness = DISPLAY_BRIGHTNESS {
        ucDisplayPolicy: 0,
        ucACBrightness: 0,
        ucDCBrightness: 0,
    };
    let mut returned = 0u32;
    let out_size = u32::try_from(std::mem::size_of::<DISPLAY_BRIGHTNESS>()).ok()?;
    // SAFETY: `device` is the handle opened above; the out-buffer is a live
    // DISPLAY_BRIGHTNESS of `out_size` bytes, and the call is synchronous.
    let answered = unsafe {
        DeviceIoControl(
            device,
            IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS,
            std::ptr::null(),
            0,
            std::ptr::from_mut(&mut brightness).cast(),
            out_size,
            &raw mut returned,
            std::ptr::null_mut(),
        )
    } != 0;
    // SAFETY: closes the handle this function opened, once.
    unsafe { CloseHandle(device) };
    if !answered || returned < out_size {
        return None;
    }
    let mut power = SYSTEM_POWER_STATUS {
        ACLineStatus: 0,
        BatteryFlag: 0,
        BatteryLifePercent: 0,
        SystemStatusFlag: 0,
        BatteryLifeTime: 0,
        BatteryFullLifeTime: 0,
    };
    // SAFETY: `power` is a live out-buffer for this synchronous call.
    let on_battery =
        unsafe { GetSystemPowerStatus(&raw mut power) } != 0 && power.ACLineStatus == 0;
    let level = if on_battery {
        brightness.ucDCBrightness
    } else {
        brightness.ucACBrightness
    };
    (level <= 100).then_some(level)
}
