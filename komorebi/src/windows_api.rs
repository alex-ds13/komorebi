use color_eyre::eyre::anyhow;
use color_eyre::eyre::bail;
use color_eyre::eyre::Error;
use color_eyre::Result;
use core::ffi::c_void;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::mem::size_of;
use std::path::Path;
use std::sync::Arc;
use windows::core::Result as WindowsCrateResult;
use windows::core::PCWSTR;
use windows::core::PWSTR;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::Foundation::POINT;
use windows::Win32::Foundation::RECT;
use windows::Win32::Foundation::WPARAM;
use windows::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows::Win32::Graphics::Dwm::DWMWA_BORDER_COLOR;
use windows::Win32::Graphics::Dwm::DWMWA_CLOAKED;
use windows::Win32::Graphics::Dwm::DWMWA_COLOR_NONE;
use windows::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS;
use windows::Win32::Graphics::Dwm::DWMWA_WINDOW_CORNER_PREFERENCE;
use windows::Win32::Graphics::Dwm::DWMWCP_ROUND;
use windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE;
use windows::Win32::Graphics::Dwm::DWM_CLOAKED_APP;
use windows::Win32::Graphics::Dwm::DWM_CLOAKED_INHERITED;
use windows::Win32::Graphics::Dwm::DWM_CLOAKED_SHELL;
use windows::Win32::Graphics::Gdi::CreateSolidBrush;
use windows::Win32::Graphics::Gdi::EnumDisplayMonitors;
use windows::Win32::Graphics::Gdi::GetMonitorInfoW;
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::Graphics::Gdi::MonitorFromPoint;
use windows::Win32::Graphics::Gdi::MonitorFromWindow;
use windows::Win32::Graphics::Gdi::Rectangle;
use windows::Win32::Graphics::Gdi::RoundRect;
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::Graphics::Gdi::MONITORENUMPROC;
use windows::Win32::Graphics::Gdi::MONITORINFOEXW;
use windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST;
use windows::Win32::System::Com::CoCreateInstance;
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::RegisterPowerSettingNotification;
use windows::Win32::System::Power::HPOWERNOTIFY;
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::RemoteDesktop::WTSRegisterSessionNotification;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::System::Threading::OpenProcess;
use windows::Win32::System::Threading::QueryFullProcessImageNameW;
use windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS;
use windows::Win32::System::Threading::PROCESS_NAME_WIN32;
use windows::Win32::System::Threading::PROCESS_QUERY_INFORMATION;
use windows::Win32::UI::HiDpi::GetDpiForMonitor;
use windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext;
use windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
use windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows::Win32::UI::Input::KeyboardAndMouse::SendInput;
use windows::Win32::UI::Input::KeyboardAndMouse::INPUT;
use windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0;
use windows::Win32::UI::Input::KeyboardAndMouse::INPUT_MOUSE;
use windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN;
use windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP;
use windows::Win32::UI::Input::KeyboardAndMouse::MOUSEINPUT;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_LBUTTON;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
use windows::Win32::UI::Shell::DesktopWallpaper;
use windows::Win32::UI::Shell::IDesktopWallpaper;
use windows::Win32::UI::Shell::DWPOS_FILL;
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
use windows::Win32::UI::WindowsAndMessaging::BringWindowToTop;
use windows::Win32::UI::WindowsAndMessaging::CreateWindowExW;
use windows::Win32::UI::WindowsAndMessaging::EnumWindows;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::Win32::UI::WindowsAndMessaging::GetLayeredWindowAttributes;
use windows::Win32::UI::WindowsAndMessaging::GetTopWindow;
use windows::Win32::UI::WindowsAndMessaging::GetWindow;
use windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW;
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
use windows::Win32::UI::WindowsAndMessaging::IsIconic;
use windows::Win32::UI::WindowsAndMessaging::IsWindow;
use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;
use windows::Win32::UI::WindowsAndMessaging::IsZoomed;
use windows::Win32::UI::WindowsAndMessaging::MoveWindow;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::Win32::UI::WindowsAndMessaging::RealGetWindowClassW;
use windows::Win32::UI::WindowsAndMessaging::RegisterClassW;
use windows::Win32::UI::WindowsAndMessaging::RegisterDeviceNotificationW;
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;
use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
use windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes;
use windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW;
use windows::Win32::UI::WindowsAndMessaging::SetWindowPos;
use windows::Win32::UI::WindowsAndMessaging::ShowWindow;
use windows::Win32::UI::WindowsAndMessaging::ShowWindowAsync;
use windows::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;
use windows::Win32::UI::WindowsAndMessaging::WindowFromPoint;
use windows::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT;
use windows::Win32::UI::WindowsAndMessaging::DEV_BROADCAST_DEVICEINTERFACE_W;
use windows::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE;
use windows::Win32::UI::WindowsAndMessaging::GWL_STYLE;
use windows::Win32::UI::WindowsAndMessaging::GW_HWNDNEXT;
use windows::Win32::UI::WindowsAndMessaging::HDEVNOTIFY;
use windows::Win32::UI::WindowsAndMessaging::HWND_BOTTOM;
use windows::Win32::UI::WindowsAndMessaging::HWND_TOP;
use windows::Win32::UI::WindowsAndMessaging::LWA_ALPHA;
use windows::Win32::UI::WindowsAndMessaging::REGISTER_NOTIFICATION_FLAGS;
use windows::Win32::UI::WindowsAndMessaging::SET_WINDOW_POS_FLAGS;
use windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD;
use windows::Win32::UI::WindowsAndMessaging::SPIF_SENDCHANGE;
use windows::Win32::UI::WindowsAndMessaging::SPI_GETACTIVEWINDOWTRACKING;
use windows::Win32::UI::WindowsAndMessaging::SPI_GETFOREGROUNDLOCKTIMEOUT;
use windows::Win32::UI::WindowsAndMessaging::SPI_SETACTIVEWINDOWTRACKING;
use windows::Win32::UI::WindowsAndMessaging::SPI_SETFOREGROUNDLOCKTIMEOUT;
use windows::Win32::UI::WindowsAndMessaging::SWP_ASYNCWINDOWPOS;
use windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE;
use windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE;
use windows::Win32::UI::WindowsAndMessaging::SWP_SHOWWINDOW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::Win32::UI::WindowsAndMessaging::SW_MAXIMIZE;
use windows::Win32::UI::WindowsAndMessaging::SW_MINIMIZE;
use windows::Win32::UI::WindowsAndMessaging::SW_NORMAL;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE;
use windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_ACTION;
use windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS;
use windows::Win32::UI::WindowsAndMessaging::WINDOW_LONG_PTR_INDEX;
use windows::Win32::UI::WindowsAndMessaging::WM_CLOSE;
use windows::Win32::UI::WindowsAndMessaging::WNDCLASSW;
use windows::Win32::UI::WindowsAndMessaging::WNDENUMPROC;
use windows::Win32::UI::WindowsAndMessaging::WS_DISABLED;
use windows::Win32::UI::WindowsAndMessaging::WS_EX_NOACTIVATE;
use windows::Win32::UI::WindowsAndMessaging::WS_EX_TOOLWINDOW;
use windows::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST;
use windows::Win32::UI::WindowsAndMessaging::WS_POPUP;
use windows::Win32::UI::WindowsAndMessaging::WS_SYSMENU;
use windows_core::BOOL;
use windows_core::HSTRING;

use crate::core::Rect;

use crate::container::Container;
use crate::monitor;
use crate::monitor::Monitor;
use crate::ring::Ring;
use crate::set_window_position::SetWindowPosition;
use crate::windows_callbacks;
use crate::RuleDebug;
use crate::Window;
use crate::WindowHandlingBehaviour;
use crate::WindowManager;
use crate::DISPLAY_INDEX_PREFERENCES;
use crate::DUPLICATE_MONITOR_SERIAL_IDS;
use crate::MONITOR_INDEX_PREFERENCES;
use crate::WINDOW_HANDLING_BEHAVIOUR;

macro_rules! as_ptr {
    ($value:expr) => {
        $value as *mut core::ffi::c_void
    };
}

use crate::border_manager::Border;
pub(crate) use as_ptr;

pub enum WindowsResult<T, E> {
    Err(E),
    Ok(T),
}

macro_rules! impl_from_integer_for_windows_result {
    ( $( $integer_type:ty ),+ ) => {
        $(
            impl From<$integer_type> for WindowsResult<$integer_type, Error> {
                fn from(return_value: $integer_type) -> Self {
                    match return_value {
                        0 => Self::Err(std::io::Error::last_os_error().into()),
                        _ => Self::Ok(return_value),
                    }
                }
            }
        )+
    };
}

impl_from_integer_for_windows_result!(usize, isize, u16, u32, i32);

impl<T, E> From<WindowsResult<T, E>> for Result<T, E> {
    fn from(result: WindowsResult<T, E>) -> Self {
        match result {
            WindowsResult::Err(error) => Err(error),
            WindowsResult::Ok(ok) => Ok(ok),
        }
    }
}

pub trait ProcessWindowsCrateResult<T> {
    fn process(self) -> Result<T>;
}

macro_rules! impl_process_windows_crate_integer_wrapper_result {
    ( $($input:ty => $deref:ty),+ $(,)? ) => (
        paste::paste! {
            $(
                impl ProcessWindowsCrateResult<$deref> for $input {
                    fn process(self) -> Result<$deref> {
                        if self == $input(std::ptr::null_mut()) {
                            Err(std::io::Error::last_os_error().into())
                        } else {
                            Ok(self.0 as $deref)
                        }
                    }
                }
            )+
        }
    );
}

impl_process_windows_crate_integer_wrapper_result!(
    HWND => isize,
);

impl<T> ProcessWindowsCrateResult<T> for WindowsCrateResult<T> {
    fn process(self) -> Result<T> {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(error.into()),
        }
    }
}

pub struct WindowsApiInternal<'a> {
    pub display_provider: Arc<dyn Fn() -> Vec<win32_display_data::Device> + Send + Sync + 'a>,
}

// impl
//     WindowsApiInternal<
//         fn() -> dyn Iterator<Item = Result<win32_display_data::Device, win32_display_data::Error>>,
//         dyn Iterator<Item = Result<win32_display_data::Device, win32_display_data::Error>>,
//     >
// {
//     fn new_default() -> Self {
//         Self {
//             display_provider: win32_display_data::connected_displays_all,
//         }
//     }
// }

impl Default for WindowsApiInternal<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> WindowsApiInternal<'a> {
    pub fn new() -> WindowsApiInternal<'a> {
        let display_provider = Arc::new(win32_display_data::connected_displays_all);
        WindowsApiInternal { display_provider }
    }

    pub fn new_with_provider(
        display_provider: impl Fn() -> Vec<win32_display_data::Device> + Send + Sync + 'a,
    ) -> WindowsApiInternal<'a> {
        let display_provider = Arc::new(display_provider);
        WindowsApiInternal { display_provider }
    }
}
// pub static WindowsApi = WindowsApiInternal {
//     display_provider: win32_display_data::connected_displays_all,
// };

// pub struct WindowsApi<F, I>
// where
//     F: Fn() -> I + Copy,
//     I: Iterator<Item = Result<win32_display_data::Device, win32_display_data::Error>>,
// {
//     pub display_provider: F,
// }

pub struct WindowsApi;

// impl WinApi for WindowsApi {}

// pub trait WinApi: std::marker::Sync + std::marker::Send {
pub trait WinApi {
    // impl WindowsApi {
    fn enum_display_monitors(
        &self,
        callback: MONITORENUMPROC,
        callback_data_address: isize,
    ) -> Result<()>;

    fn valid_hmonitors(&self) -> Result<Vec<(String, isize)>>;

    fn load_monitor_information(&self, wm: &mut WindowManager) -> Result<()>;

    fn enum_windows(&self, callback: WNDENUMPROC, callback_data_address: isize) -> Result<()>;

    fn load_workspace_information(
        &self,
        monitors: &mut Ring<Monitor>,
        known_hwnds: &HashMap<isize, (usize, usize)>,
        known_layered_hwnds: &[isize],
    ) -> Result<()>;

    fn allow_set_foreground_window(&self, process_id: u32) -> Result<()>;

    fn monitor_from_window(&self, hwnd: isize) -> isize;

    fn monitor_name_from_window(&self, hwnd: isize) -> Result<String>;

    fn monitor_from_point(&self, point: POINT) -> isize;

    /// position window resizes the target window to the given layout, adjusting
    /// the layout to account for any window shadow borders (the window painted
    /// region will match layout on completion).
    fn position_window(&self, hwnd: isize, layout: &Rect, top: bool) -> Result<()>;

    fn bring_window_to_top(&self, hwnd: isize) -> Result<()>;

    /// Raise the window to the top of the Z order, but do not activate or focus
    /// it. Use raise_and_focus_window to activate and focus a window.
    fn raise_window(&self, hwnd: isize) -> Result<()>;

    /// Lower the window to the bottom of the Z order, but do not activate or focus
    /// it.
    fn lower_window(&self, hwnd: isize) -> Result<()>;

    fn set_border_pos(&self, hwnd: isize, layout: &Rect, position: isize) -> Result<()>;

    /// set_window_pos calls SetWindowPos without any accounting for Window decorations.
    fn set_window_pos(&self, hwnd: HWND, layout: &Rect, position: HWND, flags: u32) -> Result<()>;

    fn move_window(&self, hwnd: isize, layout: &Rect, repaint: bool) -> Result<()>;

    fn show_window(&self, hwnd: isize, command: SHOW_WINDOW_CMD);

    fn minimize_window(&self, hwnd: isize);

    fn post_message(&self, hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Result<()>;

    fn close_window(&self, hwnd: isize) -> Result<()>;

    fn hide_window(&self, hwnd: isize);

    fn restore_window(&self, hwnd: isize);

    fn unmaximize_window(&self, hwnd: isize);

    fn maximize_window(&self, hwnd: isize);

    fn foreground_window(&self) -> Result<isize>;

    fn raise_and_focus_window(&self, hwnd: isize) -> Result<()>;

    #[allow(dead_code)]
    fn top_window(&self) -> Result<isize>;

    fn desktop_window(&self) -> Result<isize>;

    #[allow(dead_code)]
    fn next_window(&self, hwnd: isize) -> Result<isize>;

    #[allow(dead_code)]
    fn top_visible_window(&self) -> Result<isize>;

    fn window_rect(&self, hwnd: isize) -> Result<Rect>;

    /// shadow_rect computes the offset of the shadow position of the window to
    /// the window painted region. The four values in the returned Rect can be
    /// added to a position rect to compute a size for set_window_pos that will
    /// fill the target area, ignoring shadows.
    fn shadow_rect(&self, hwnd: HWND) -> Result<Rect>;

    fn round_rect(&self, hdc: HDC, rect: &Rect, border_radius: i32);
    fn rectangle(&self, hdc: HDC, rect: &Rect);
    fn set_cursor_pos(&self, x: i32, y: i32) -> Result<()>;

    fn cursor_pos(&self) -> Result<POINT>;

    fn window_from_point(&self, point: POINT) -> Result<isize>;

    fn window_at_cursor_pos(&self) -> Result<isize>;

    fn center_cursor_in_rect(&self, rect: &Rect) -> Result<()>;

    fn window_thread_process_id(&self, hwnd: isize) -> (u32, u32);

    fn current_process_id(&self) -> u32;

    fn process_id_to_session_id(&self) -> Result<u32>;

    #[cfg(target_pointer_width = "64")]
    fn set_window_long_ptr_w(
        &self,
        hwnd: HWND,
        index: WINDOW_LONG_PTR_INDEX,
        new_value: isize,
    ) -> Result<()>;

    #[cfg(target_pointer_width = "32")]
    fn set_window_long_ptr_w(
        &self,
        hwnd: HWND,
        index: WINDOW_LONG_PTR_INDEX,
        new_value: i32,
    ) -> Result<()>;

    #[cfg(target_pointer_width = "64")]
    fn gwl_style(&self, hwnd: isize) -> Result<isize>;

    #[cfg(target_pointer_width = "32")]
    fn gwl_style(&self, hwnd: isize) -> Result<i32>;

    #[cfg(target_pointer_width = "64")]
    fn gwl_ex_style(&self, hwnd: isize) -> Result<isize>;

    #[cfg(target_pointer_width = "32")]
    fn gwl_ex_style(&self, hwnd: isize) -> Result<i32>;

    #[cfg(target_pointer_width = "64")]
    fn window_long_ptr_w(&self, hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> Result<isize>;

    #[cfg(target_pointer_width = "32")]
    fn window_long_ptr_w(&self, hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> Result<i32>;

    #[cfg(target_pointer_width = "64")]
    fn update_style(&self, hwnd: isize, new_value: isize) -> Result<()>;

    #[cfg(target_pointer_width = "32")]
    fn update_style(&self, hwnd: isize, new_value: i32) -> Result<()>;

    #[cfg(target_pointer_width = "64")]
    fn update_ex_style(&self, hwnd: isize, new_value: isize) -> Result<()>;

    #[cfg(target_pointer_width = "32")]
    fn update_ex_style(&self, hwnd: isize, new_value: i32) -> Result<()>;

    fn window_text_w(&self, hwnd: isize) -> Result<String>;

    fn open_process(
        &self,
        access_rights: PROCESS_ACCESS_RIGHTS,
        inherit_handle: bool,
        process_id: u32,
    ) -> Result<HANDLE>;

    fn close_process(&self, handle: HANDLE) -> Result<()>;

    fn process_handle(&self, process_id: u32) -> Result<HANDLE>;

    fn exe_path(&self, handle: HANDLE) -> Result<String>;

    fn exe(&self, handle: HANDLE) -> Result<String>;

    fn real_window_class_w(&self, hwnd: isize) -> Result<String>;

    fn dwm_get_window_attribute<T>(
        &self,
        hwnd: isize,
        attribute: DWMWINDOWATTRIBUTE,
        value: &mut T,
    ) -> Result<()>;

    fn is_window_cloaked(&self, hwnd: isize) -> Result<bool>;

    fn is_window(&self, hwnd: isize) -> bool;

    fn is_window_visible(&self, hwnd: isize) -> bool;

    fn is_iconic(&self, hwnd: isize) -> bool;

    fn is_zoomed(&self, hwnd: isize) -> bool;

    fn monitor_info_w(&self, hmonitor: HMONITOR) -> Result<MONITORINFOEXW>;

    fn monitor(&self, hmonitor: isize) -> Result<Monitor>;

    fn set_process_dpi_awareness_context(&self) -> Result<()>;

    #[allow(dead_code)]
    fn system_parameters_info_w(
        &self,
        action: SYSTEM_PARAMETERS_INFO_ACTION,
        ui_param: u32,
        pv_param: *mut c_void,
        update_flags: SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    ) -> Result<()>;

    // #[tracing::instrument]
    fn foreground_lock_timeout(&self) -> Result<()>;

    #[allow(dead_code)]
    fn focus_follows_mouse(&self) -> Result<bool>;

    #[allow(dead_code)]
    fn enable_focus_follows_mouse(&self) -> Result<()>;

    #[allow(dead_code)]
    fn disable_focus_follows_mouse(&self) -> Result<()>;

    fn module_handle_w(&self) -> Result<HMODULE>;

    fn create_solid_brush(&self, colour: u32) -> HBRUSH;

    fn register_class_w(&self, window_class: &WNDCLASSW) -> Result<u16>;

    fn dpi_for_monitor(&self, hmonitor: isize) -> Result<f32>;

    fn monitors_have_same_dpi(&self, hmonitor_a: isize, hmonitor_b: isize) -> Result<bool>;

    fn round_corners(&self, hwnd: isize) -> Result<()>;

    fn set_window_accent(&self, hwnd: isize, color: Option<u32>) -> Result<()>;

    fn create_border_window(
        &self,
        name: PCWSTR,
        instance: isize,
        border: *mut Border,
    ) -> Result<isize>;

    fn set_transparent(&self, hwnd: isize, alpha: u8) -> Result<()>;

    fn get_transparent(&self, hwnd: isize) -> Result<u8>;

    fn create_hidden_window(&self, name: PCWSTR, instance: isize) -> Result<isize>;

    fn register_power_setting_notification(
        &self,
        hwnd: isize,
        guid: &windows_core::GUID,
        flags: REGISTER_NOTIFICATION_FLAGS,
    ) -> WindowsCrateResult<HPOWERNOTIFY>;

    fn register_device_notification(
        &self,
        hwnd: isize,
        filter: DEV_BROADCAST_DEVICEINTERFACE_W,
        flags: REGISTER_NOTIFICATION_FLAGS,
    ) -> WindowsCrateResult<HDEVNOTIFY>;

    fn invalidate_rect(&self, hwnd: isize, rect: Option<&Rect>, erase: bool) -> bool;

    fn alt_is_pressed(&self) -> bool;

    fn lbutton_is_pressed(&self) -> bool;

    fn left_click(&self) -> u32;

    fn wts_register_session_notification(&self, hwnd: isize) -> Result<()>;
}

impl WinApi for WindowsApiInternal<'_> {
    fn enum_display_monitors(
        &self,
        callback: MONITORENUMPROC,
        callback_data_address: isize,
    ) -> Result<()> {
        todo!()
    }

    fn valid_hmonitors(&self) -> Result<Vec<(String, isize)>> {
        todo!()
    }

    fn load_monitor_information(&self, wm: &mut WindowManager) -> Result<()> {
        todo!()
    }

    fn enum_windows(&self, callback: WNDENUMPROC, callback_data_address: isize) -> Result<()> {
        todo!()
    }

    fn load_workspace_information(
        &self,
        monitors: &mut Ring<Monitor>,
        known_hwnds: &HashMap<isize, (usize, usize)>,
        known_layered_hwnds: &[isize],
    ) -> Result<()> {
        todo!()
    }

    fn allow_set_foreground_window(&self, process_id: u32) -> Result<()> {
        todo!()
    }

    fn monitor_from_window(&self, hwnd: isize) -> isize {
        todo!()
    }

    fn monitor_name_from_window(&self, hwnd: isize) -> Result<String> {
        todo!()
    }

    fn monitor_from_point(&self, point: POINT) -> isize {
        todo!()
    }

    fn position_window(&self, hwnd: isize, layout: &Rect, top: bool) -> Result<()> {
        todo!()
    }

    fn bring_window_to_top(&self, hwnd: isize) -> Result<()> {
        todo!()
    }

    fn raise_window(&self, hwnd: isize) -> Result<()> {
        todo!()
    }

    fn lower_window(&self, hwnd: isize) -> Result<()> {
        todo!()
    }

    fn set_border_pos(&self, hwnd: isize, layout: &Rect, position: isize) -> Result<()> {
        todo!()
    }

    fn set_window_pos(&self, hwnd: HWND, layout: &Rect, position: HWND, flags: u32) -> Result<()> {
        todo!()
    }

    fn move_window(&self, hwnd: isize, layout: &Rect, repaint: bool) -> Result<()> {
        todo!()
    }

    fn show_window(&self, hwnd: isize, command: SHOW_WINDOW_CMD) {
        todo!()
    }

    fn minimize_window(&self, hwnd: isize) {
        todo!()
    }

    fn post_message(&self, hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Result<()> {
        todo!()
    }

    fn close_window(&self, hwnd: isize) -> Result<()> {
        todo!()
    }

    fn hide_window(&self, hwnd: isize) {
        todo!()
    }

    fn restore_window(&self, hwnd: isize) {
        todo!()
    }

    fn unmaximize_window(&self, hwnd: isize) {
        todo!()
    }

    fn maximize_window(&self, hwnd: isize) {
        todo!()
    }

    fn foreground_window(&self) -> Result<isize> {
        todo!()
    }

    fn raise_and_focus_window(&self, hwnd: isize) -> Result<()> {
        todo!()
    }

    fn top_window(&self) -> Result<isize> {
        todo!()
    }

    fn desktop_window(&self) -> Result<isize> {
        todo!()
    }

    fn next_window(&self, hwnd: isize) -> Result<isize> {
        todo!()
    }

    fn top_visible_window(&self) -> Result<isize> {
        todo!()
    }

    fn window_rect(&self, hwnd: isize) -> Result<Rect> {
        todo!()
    }

    fn shadow_rect(&self, hwnd: HWND) -> Result<Rect> {
        todo!()
    }

    fn round_rect(&self, hdc: HDC, rect: &Rect, border_radius: i32) {
        todo!()
    }

    fn rectangle(&self, hdc: HDC, rect: &Rect) {
        todo!()
    }

    fn set_cursor_pos(&self, x: i32, y: i32) -> Result<()> {
        todo!()
    }

    fn cursor_pos(&self) -> Result<POINT> {
        todo!()
    }

    fn window_from_point(&self, point: POINT) -> Result<isize> {
        todo!()
    }

    fn window_at_cursor_pos(&self) -> Result<isize> {
        todo!()
    }

    fn center_cursor_in_rect(&self, rect: &Rect) -> Result<()> {
        todo!()
    }

    fn window_thread_process_id(&self, hwnd: isize) -> (u32, u32) {
        todo!()
    }

    fn current_process_id(&self) -> u32 {
        todo!()
    }

    fn process_id_to_session_id(&self) -> Result<u32> {
        todo!()
    }

    fn set_window_long_ptr_w(
        &self,
        hwnd: HWND,
        index: WINDOW_LONG_PTR_INDEX,
        new_value: isize,
    ) -> Result<()> {
        todo!()
    }

    fn gwl_style(&self, hwnd: isize) -> Result<isize> {
        todo!()
    }

    fn gwl_ex_style(&self, hwnd: isize) -> Result<isize> {
        todo!()
    }

    fn window_long_ptr_w(&self, hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> Result<isize> {
        todo!()
    }

    fn update_style(&self, hwnd: isize, new_value: isize) -> Result<()> {
        todo!()
    }

    fn update_ex_style(&self, hwnd: isize, new_value: isize) -> Result<()> {
        todo!()
    }

    fn window_text_w(&self, hwnd: isize) -> Result<String> {
        todo!()
    }

    fn open_process(
        &self,
        access_rights: PROCESS_ACCESS_RIGHTS,
        inherit_handle: bool,
        process_id: u32,
    ) -> Result<HANDLE> {
        todo!()
    }

    fn close_process(&self, handle: HANDLE) -> Result<()> {
        todo!()
    }

    fn process_handle(&self, process_id: u32) -> Result<HANDLE> {
        todo!()
    }

    fn exe_path(&self, handle: HANDLE) -> Result<String> {
        todo!()
    }

    fn exe(&self, handle: HANDLE) -> Result<String> {
        todo!()
    }

    fn real_window_class_w(&self, hwnd: isize) -> Result<String> {
        todo!()
    }

    fn dwm_get_window_attribute<T>(
        &self,
        hwnd: isize,
        attribute: DWMWINDOWATTRIBUTE,
        value: &mut T,
    ) -> Result<()> {
        todo!()
    }

    fn is_window_cloaked(&self, hwnd: isize) -> Result<bool> {
        todo!()
    }

    fn is_window(&self, hwnd: isize) -> bool {
        todo!()
    }

    fn is_window_visible(&self, hwnd: isize) -> bool {
        todo!()
    }

    fn is_iconic(&self, hwnd: isize) -> bool {
        todo!()
    }

    fn is_zoomed(&self, hwnd: isize) -> bool {
        todo!()
    }

    fn monitor_info_w(&self, hmonitor: HMONITOR) -> Result<MONITORINFOEXW> {
        todo!()
    }

    fn monitor(&self, hmonitor: isize) -> Result<Monitor> {
        todo!()
    }

    fn set_process_dpi_awareness_context(&self) -> Result<()> {
        todo!()
    }

    fn system_parameters_info_w(
        &self,
        action: SYSTEM_PARAMETERS_INFO_ACTION,
        ui_param: u32,
        pv_param: *mut c_void,
        update_flags: SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    ) -> Result<()> {
        todo!()
    }

    fn foreground_lock_timeout(&self) -> Result<()> {
        todo!()
    }

    fn focus_follows_mouse(&self) -> Result<bool> {
        todo!()
    }

    fn enable_focus_follows_mouse(&self) -> Result<()> {
        todo!()
    }

    fn disable_focus_follows_mouse(&self) -> Result<()> {
        todo!()
    }

    fn module_handle_w(&self) -> Result<HMODULE> {
        todo!()
    }

    fn create_solid_brush(&self, colour: u32) -> HBRUSH {
        todo!()
    }

    fn register_class_w(&self, window_class: &WNDCLASSW) -> Result<u16> {
        todo!()
    }

    fn dpi_for_monitor(&self, hmonitor: isize) -> Result<f32> {
        todo!()
    }

    fn monitors_have_same_dpi(&self, hmonitor_a: isize, hmonitor_b: isize) -> Result<bool> {
        todo!()
    }

    fn round_corners(&self, hwnd: isize) -> Result<()> {
        todo!()
    }

    fn set_window_accent(&self, hwnd: isize, color: Option<u32>) -> Result<()> {
        todo!()
    }

    fn create_border_window(
        &self,
        name: PCWSTR,
        instance: isize,
        border: *mut Border,
    ) -> Result<isize> {
        todo!()
    }

    fn set_transparent(&self, hwnd: isize, alpha: u8) -> Result<()> {
        todo!()
    }

    fn get_transparent(&self, hwnd: isize) -> Result<u8> {
        todo!()
    }

    fn create_hidden_window(&self, name: PCWSTR, instance: isize) -> Result<isize> {
        todo!()
    }

    fn register_power_setting_notification(
        &self,
        hwnd: isize,
        guid: &windows_core::GUID,
        flags: REGISTER_NOTIFICATION_FLAGS,
    ) -> WindowsCrateResult<HPOWERNOTIFY> {
        todo!()
    }

    fn register_device_notification(
        &self,
        hwnd: isize,
        filter: DEV_BROADCAST_DEVICEINTERFACE_W,
        flags: REGISTER_NOTIFICATION_FLAGS,
    ) -> WindowsCrateResult<HDEVNOTIFY> {
        todo!()
    }

    fn invalidate_rect(&self, hwnd: isize, rect: Option<&Rect>, erase: bool) -> bool {
        todo!()
    }

    fn alt_is_pressed(&self) -> bool {
        todo!()
    }

    fn lbutton_is_pressed(&self) -> bool {
        todo!()
    }

    fn left_click(&self) -> u32 {
        todo!()
    }

    fn wts_register_session_notification(&self, hwnd: isize) -> Result<()> {
        todo!()
    }
}

impl WindowsApi {
    pub fn enum_display_monitors(
        callback: MONITORENUMPROC,
        callback_data_address: isize,
    ) -> Result<()> {
        unsafe { EnumDisplayMonitors(None, None, callback, LPARAM(callback_data_address)) }
            .ok()
            .process()
    }

    pub fn valid_hmonitors() -> Result<Vec<(String, isize)>> {
        Ok(win32_display_data::connected_displays_all()
            .into_iter()
            .map(|d| {
                let name = d.device_name.trim_start_matches(r"\\.\").to_string();
                let name = name.split('\\').collect::<Vec<_>>()[0].to_string();

                (name, d.hmonitor)
            })
            .collect::<Vec<_>>())
    }

    pub fn load_monitor_information<F>(wm: &mut WindowManager, display_provider: F) -> Result<()>
    where
        F: Fn() -> Vec<win32_display_data::Device>,
    {
        let monitors = &mut wm.monitors;
        let monitor_usr_idx_map = &mut wm.monitor_usr_idx_map;

        let all_displays = display_provider();

        let mut serial_id_map = HashMap::new();

        for d in &all_displays {
            if let Some(id) = &d.serial_number_id {
                *serial_id_map.entry(id.clone()).or_insert(0) += 1;
            }
        }

        for d in &all_displays {
            if let Some(id) = &d.serial_number_id {
                if serial_id_map.get(id).copied().unwrap_or_default() > 1 {
                    let mut dupes = DUPLICATE_MONITOR_SERIAL_IDS.write();
                    if !dupes.contains(id) {
                        (*dupes).push(id.clone());
                    }
                }
            }
        }

        'read: for mut display in all_displays {
            let path = display.device_path.clone();

            let (device, device_id) = if path.is_empty() {
                (String::from("UNKNOWN"), String::from("UNKNOWN"))
            } else {
                let mut split: Vec<_> = path.split('#').collect();
                split.remove(0);
                split.remove(split.len() - 1);
                let device = split[0].to_string();
                let device_id = split.join("-");
                (device, device_id)
            };

            let name = display.device_name.trim_start_matches(r"\\.\").to_string();
            let name = name.split('\\').collect::<Vec<_>>()[0].to_string();

            for monitor in monitors.elements() {
                if device_id.eq(monitor.device_id()) {
                    continue 'read;
                }
            }

            if let Some(id) = &display.serial_number_id {
                let dupes = DUPLICATE_MONITOR_SERIAL_IDS.read();
                if dupes.contains(id) {
                    display.serial_number_id = None;
                }
            }

            let m = monitor::new(
                display.hmonitor,
                display.size.into(),
                display.work_area_size.into(),
                name,
                device,
                device_id,
                display.serial_number_id,
            );

            let mut index_preference = None;
            let monitor_index_preferences = MONITOR_INDEX_PREFERENCES.lock();
            for (index, monitor_size) in &*monitor_index_preferences {
                if m.size() == monitor_size {
                    index_preference = Option::from(index);
                }
            }

            let display_index_preferences = DISPLAY_INDEX_PREFERENCES.read();
            for (index, id) in &*display_index_preferences {
                if m.serial_number_id().as_ref().is_some_and(|sn| sn == id) || id.eq(m.device_id())
                {
                    index_preference = Option::from(index);
                }
            }

            if let Some(preference) = index_preference {
                while *preference >= monitors.elements().len() {
                    monitors.elements_mut().push_back(Monitor::placeholder());
                }

                let current_name = monitors
                    .elements_mut()
                    .get(*preference)
                    .map_or("", |m| m.name());
                if current_name == "PLACEHOLDER" {
                    let _ = monitors.elements_mut().remove(*preference);
                    monitors.elements_mut().insert(*preference, m);
                } else {
                    monitors.elements_mut().insert(*preference, m);
                }
            } else {
                monitors.elements_mut().push_back(m);
            }
        }

        monitors
            .elements_mut()
            .retain(|m| m.name().ne("PLACEHOLDER"));

        // Rebuild monitor index map
        *monitor_usr_idx_map = HashMap::new();
        let mut added_monitor_idxs = Vec::new();
        for (index, id) in &*DISPLAY_INDEX_PREFERENCES.read() {
            if let Some(m_idx) = monitors.elements().iter().position(|m| {
                m.serial_number_id().as_ref().is_some_and(|sn| sn == id) || m.device_id() == id
            }) {
                monitor_usr_idx_map.insert(*index, m_idx);
                added_monitor_idxs.push(m_idx);
            }
        }

        let max_usr_idx = monitors
            .elements()
            .len()
            .max(monitor_usr_idx_map.keys().max().map_or(0, |v| *v));

        let mut available_usr_idxs = (0..max_usr_idx)
            .filter(|i| !monitor_usr_idx_map.contains_key(i))
            .collect::<Vec<_>>();

        let not_added_monitor_idxs = (0..monitors.elements().len())
            .filter(|i| !added_monitor_idxs.contains(i))
            .collect::<Vec<_>>();

        for i in not_added_monitor_idxs {
            if let Some(next_usr_idx) = available_usr_idxs.first() {
                monitor_usr_idx_map.insert(*next_usr_idx, i);
                available_usr_idxs.remove(0);
            } else if let Some(idx) = monitor_usr_idx_map.keys().max() {
                monitor_usr_idx_map.insert(*idx, i);
            }
        }

        Ok(())
    }

    pub fn enum_windows(callback: WNDENUMPROC, callback_data_address: isize) -> Result<()> {
        unsafe { EnumWindows(callback, LPARAM(callback_data_address)) }.process()
    }

    pub fn load_workspace_information(
        monitors: &mut Ring<Monitor>,
        known_hwnds: &HashMap<isize, (usize, usize)>,
        known_layered_hwnds: &Vec<isize>,
    ) -> Result<()> {
        for monitor in monitors.elements_mut() {
            let monitor_name = monitor.name().clone();
            if let Some(workspace) = monitor.workspaces_mut().front_mut() {
                let mut windows: Vec<Window> = Vec::new();
                // EnumWindows will enumerate through windows on all monitors
                Self::enum_windows(
                    Some(windows_callbacks::enum_window),
                    &mut windows as *mut Vec<Window> as isize,
                )?;

                // Keep only the windows that should be managed and that are on this monitor and
                // that are not already managed from an existing state
                windows.retain(|window| {
                    !known_hwnds.contains_key(&window.hwnd)
                        && window
                            .should_manage(None, &mut RuleDebug::default(), known_layered_hwnds)
                            .is_ok_and(|should_manage| {
                                should_manage
                                    && Self::monitor_name_from_window(window.hwnd)
                                        .is_ok_and(|name| name == monitor_name)
                            })
                });

                let containers = windows.into_iter().map(|window| {
                    let mut container = Container::default();
                    container.windows_mut().push_back(window);
                    container
                });
                workspace.containers_mut().extend(containers);

                // Ensure that the resize_dimensions Vec length matches the number of containers for
                // the potential later calls to workspace.remove_window later in this fn
                let len = workspace.containers().len();
                workspace.resize_dimensions_mut().resize(len, None);
            }
        }

        Ok(())
    }

    pub fn allow_set_foreground_window(process_id: u32) -> Result<()> {
        unsafe { AllowSetForegroundWindow(process_id) }.process()
    }

    pub fn monitor_from_window(hwnd: isize) -> isize {
        // MONITOR_DEFAULTTONEAREST ensures that the return value will never be NULL
        // https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-monitorfromwindow
        unsafe { MonitorFromWindow(HWND(as_ptr!(hwnd)), MONITOR_DEFAULTTONEAREST) }.0 as isize
    }

    pub fn monitor_name_from_window(hwnd: isize) -> Result<String> {
        // MONITOR_DEFAULTTONEAREST ensures that the return value will never be NULL
        // https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-monitorfromwindow
        Ok(Self::monitor(
            unsafe { MonitorFromWindow(HWND(as_ptr!(hwnd)), MONITOR_DEFAULTTONEAREST) }.0 as isize,
        )?
        .name()
        .to_string())
    }

    pub fn monitor_from_point(point: POINT) -> isize {
        // MONITOR_DEFAULTTONEAREST ensures that the return value will never be NULL
        // https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-monitorfromwindow
        unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) }.0 as isize
    }

    /// position window resizes the target window to the given layout, adjusting
    /// the layout to account for any window shadow borders (the window painted
    /// region will match layout on completion).
    pub fn position_window(
        hwnd: isize,
        layout: &Rect,
        top: bool,
        with_async_window_pos: bool,
    ) -> Result<()> {
        let hwnd = HWND(as_ptr!(hwnd));

        let mut flags = SetWindowPosition::NO_ACTIVATE
            | SetWindowPosition::NO_SEND_CHANGING
            | SetWindowPosition::NO_COPY_BITS
            | SetWindowPosition::FRAME_CHANGED;

        // If the request is to place the window on top, then HWND_TOP will take
        // effect, otherwise pass NO_Z_ORDER that will cause set_window_pos to
        // ignore the z-order paramter.

        // By default SetWindowPos waits for target window's WindowProc thread
        // to process the message, so we have to use ASYNC_WINDOW_POS to avoid
        // blocking our thread in case the target window is not responding.
        if with_async_window_pos
            && matches!(
                WINDOW_HANDLING_BEHAVIOUR.load(),
                WindowHandlingBehaviour::Async
            )
        {
            flags |= SetWindowPosition::ASYNC_WINDOW_POS;
        }

        if !top {
            flags |= SetWindowPosition::NO_Z_ORDER;
        }

        let shadow_rect = Self::shadow_rect(hwnd).unwrap_or_default();
        let rect = Rect {
            left: layout.left + shadow_rect.left,
            top: layout.top + shadow_rect.top,
            right: layout.right + shadow_rect.right,
            bottom: layout.bottom + shadow_rect.bottom,
        };

        // Note: earlier code had set HWND_TOPMOST here, but we should not do
        // that. HWND_TOPMOST is a sticky z-order change, rather than a regular
        // z-order reordering. Programs will use TOPMOST themselves to do things
        // such as making sure that their tool windows or dialog pop-ups are
        // above their main window. If any such windows are unmanaged, they must
        // still remian topmost, so we set HWND_TOP here, which will cause the
        // managed window to come to the front, but if the managed window has a
        // child that is TOPMOST it will still be rendered above, in the proper
        // order expected by the application. It's also important to understand
        // that TOPMOST is somewhat viral, in that when you set a window to
        // TOPMOST all of its owned windows are also made TOPMOST.
        // See https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos#remarks
        Self::set_window_pos(hwnd, &rect, HWND_TOP, flags.bits())
    }

    pub fn bring_window_to_top(hwnd: isize) -> Result<()> {
        unsafe { BringWindowToTop(HWND(as_ptr!(hwnd))) }.process()
    }

    /// Raise the window to the top of the Z order, but do not activate or focus
    /// it. Use raise_and_focus_window to activate and focus a window.
    pub fn raise_window(hwnd: isize) -> Result<()> {
        let mut flags = SetWindowPosition::NO_MOVE
            | SetWindowPosition::NO_SIZE
            | SetWindowPosition::NO_ACTIVATE
            | SetWindowPosition::SHOW_WINDOW;

        if matches!(
            WINDOW_HANDLING_BEHAVIOUR.load(),
            WindowHandlingBehaviour::Async
        ) {
            flags |= SetWindowPosition::ASYNC_WINDOW_POS;
        }

        let position = HWND_TOP;
        Self::set_window_pos(
            HWND(as_ptr!(hwnd)),
            &Rect::default(),
            position,
            flags.bits(),
        )
    }

    /// Lower the window to the bottom of the Z order, but do not activate or focus
    /// it.
    pub fn lower_window(hwnd: isize) -> Result<()> {
        let mut flags = SetWindowPosition::NO_MOVE
            | SetWindowPosition::NO_SIZE
            | SetWindowPosition::NO_ACTIVATE
            | SetWindowPosition::SHOW_WINDOW;

        if matches!(
            WINDOW_HANDLING_BEHAVIOUR.load(),
            WindowHandlingBehaviour::Async
        ) {
            flags |= SetWindowPosition::ASYNC_WINDOW_POS;
        }

        let position = HWND_BOTTOM;
        Self::set_window_pos(
            HWND(as_ptr!(hwnd)),
            &Rect::default(),
            position,
            flags.bits(),
        )
    }

    pub fn set_border_pos(hwnd: isize, layout: &Rect, position: isize) -> Result<()> {
        let mut flags = SetWindowPosition::NO_SEND_CHANGING
            | SetWindowPosition::NO_ACTIVATE
            | SetWindowPosition::NO_REDRAW
            | SetWindowPosition::SHOW_WINDOW;

        if matches!(
            WINDOW_HANDLING_BEHAVIOUR.load(),
            WindowHandlingBehaviour::Async
        ) {
            flags |= SetWindowPosition::ASYNC_WINDOW_POS;
        }

        Self::set_window_pos(
            HWND(as_ptr!(hwnd)),
            layout,
            HWND(as_ptr!(position)),
            flags.bits(),
        )
    }

    /// set_window_pos calls SetWindowPos without any accounting for Window decorations.
    fn set_window_pos(hwnd: HWND, layout: &Rect, position: HWND, flags: u32) -> Result<()> {
        unsafe {
            SetWindowPos(
                hwnd,
                Option::from(position),
                layout.left,
                layout.top,
                layout.right,
                layout.bottom,
                SET_WINDOW_POS_FLAGS(flags),
            )
        }
        .process()
    }

    /// move_windows calls MoveWindow, but cannot be called with async window pos, so it might hang
    pub fn move_window(hwnd: isize, layout: &Rect, repaint: bool) -> Result<()> {
        let hwnd = HWND(as_ptr!(hwnd));

        let shadow_rect = Self::shadow_rect(hwnd).unwrap_or_default();
        let rect = Rect {
            left: layout.left + shadow_rect.left,
            top: layout.top + shadow_rect.top,
            right: layout.right + shadow_rect.right,
            bottom: layout.bottom + shadow_rect.bottom,
        };
        unsafe { MoveWindow(hwnd, rect.left, rect.top, rect.right, rect.bottom, repaint) }.process()
    }

    pub fn show_window(hwnd: isize, command: SHOW_WINDOW_CMD) {
        // BOOL is returned but does not signify whether or not the operation was succesful
        // https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow
        // TODO: error handling
        if matches!(
            WINDOW_HANDLING_BEHAVIOUR.load(),
            WindowHandlingBehaviour::Async
        ) {
            unsafe {
                let _ = ShowWindowAsync(HWND(as_ptr!(hwnd)), command);
            };
        } else {
            unsafe {
                let _ = ShowWindow(HWND(as_ptr!(hwnd)), command);
            };
        }
    }

    pub fn minimize_window(hwnd: isize) {
        Self::show_window(hwnd, SW_MINIMIZE);
    }

    fn post_message(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Result<()> {
        unsafe { PostMessageW(Option::from(hwnd), message, wparam, lparam) }.process()
    }

    pub fn close_window(hwnd: isize) -> Result<()> {
        match Self::post_message(HWND(as_ptr!(hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0)) {
            Ok(()) => Ok(()),
            Err(_) => Err(anyhow!("could not close window")),
        }
    }

    pub fn hide_window(hwnd: isize) {
        Self::show_window(hwnd, SW_HIDE);
    }

    pub fn restore_window(hwnd: isize) {
        Self::show_window(hwnd, SW_SHOWNOACTIVATE);
    }

    pub fn unmaximize_window(hwnd: isize) {
        Self::show_window(hwnd, SW_NORMAL);
    }

    pub fn maximize_window(hwnd: isize) {
        Self::show_window(hwnd, SW_MAXIMIZE);
    }

    pub fn foreground_window() -> Result<isize> {
        unsafe { GetForegroundWindow() }.process()
    }

    pub fn raise_and_focus_window(hwnd: isize) -> Result<()> {
        let event = [INPUT {
            r#type: INPUT_MOUSE,
            ..Default::default()
        }];

        unsafe {
            // Send an input event to our own process first so that we pass the
            // foreground lock check
            SendInput(&event, size_of::<INPUT>() as i32);
            // Error ignored, as the operation is not always necessary.
            let _ = SetWindowPos(
                HWND(as_ptr!(hwnd)),
                Option::from(HWND_TOP),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS,
            )
            .process();
            SetForegroundWindow(HWND(as_ptr!(hwnd)))
        }
        .ok()
        .process()
    }

    #[allow(dead_code)]
    pub fn top_window() -> Result<isize> {
        unsafe { GetTopWindow(None)? }.process()
    }

    pub fn desktop_window() -> Result<isize> {
        unsafe { GetDesktopWindow() }.process()
    }

    #[allow(dead_code)]
    pub fn next_window(hwnd: isize) -> Result<isize> {
        unsafe { GetWindow(HWND(as_ptr!(hwnd)), GW_HWNDNEXT)? }.process()
    }

    #[allow(dead_code)]
    pub fn top_visible_window() -> Result<isize> {
        let hwnd = Self::top_window()?;
        let mut next_hwnd = hwnd;

        while next_hwnd != 0 {
            if Self::is_window_visible(next_hwnd) {
                return Ok(next_hwnd);
            }

            next_hwnd = Self::next_window(next_hwnd)?;
        }

        Err(anyhow!("could not find next window"))
    }

    pub fn window_rect(hwnd: isize) -> Result<Rect> {
        let mut rect = unsafe { std::mem::zeroed() };

        if Self::dwm_get_window_attribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut rect).is_ok() {
            // TODO(raggi): once we declare DPI awareness, we will need to scale the rect.
            // let window_scale = unsafe { GetDpiForWindow(hwnd) };
            // let system_scale = unsafe { GetDpiForSystem() };
            // Ok(Rect::from(rect).scale(system_scale.try_into()?, window_scale.try_into()?))
            Ok(Rect::from(rect))
        } else {
            unsafe { GetWindowRect(HWND(as_ptr!(hwnd)), &mut rect) }.process()?;
            Ok(Rect::from(rect))
        }
    }

    /// shadow_rect computes the offset of the shadow position of the window to
    /// the window painted region. The four values in the returned Rect can be
    /// added to a position rect to compute a size for set_window_pos that will
    /// fill the target area, ignoring shadows.
    fn shadow_rect(hwnd: HWND) -> Result<Rect> {
        let window_rect = Self::window_rect(hwnd.0 as isize)?;

        let mut srect = Default::default();
        unsafe { GetWindowRect(hwnd, &mut srect) }.process()?;
        let shadow_rect = Rect::from(srect);

        Ok(Rect {
            left: shadow_rect.left - window_rect.left,
            top: shadow_rect.top - window_rect.top,
            right: shadow_rect.right - window_rect.right,
            bottom: shadow_rect.bottom - window_rect.bottom,
        })
    }

    pub fn round_rect(hdc: HDC, rect: &Rect, border_radius: i32) {
        unsafe {
            // TODO: error handling
            let _ = RoundRect(
                hdc,
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                border_radius,
                border_radius,
            );
        }
    }
    pub fn rectangle(hdc: HDC, rect: &Rect) {
        unsafe {
            // TODO: error handling
            let _ = Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
        }
    }
    fn set_cursor_pos(x: i32, y: i32) -> Result<()> {
        unsafe { SetCursorPos(x, y) }.process()
    }

    pub fn cursor_pos() -> Result<POINT> {
        let mut cursor_pos = POINT::default();
        unsafe { GetCursorPos(&mut cursor_pos) }.process()?;

        Ok(cursor_pos)
    }

    pub fn window_from_point(point: POINT) -> Result<isize> {
        unsafe { WindowFromPoint(point) }.process()
    }

    pub fn window_at_cursor_pos() -> Result<isize> {
        Self::window_from_point(Self::cursor_pos()?)
    }

    pub fn center_cursor_in_rect(rect: &Rect) -> Result<()> {
        Self::set_cursor_pos(rect.left + (rect.right / 2), rect.top + (rect.bottom / 2))
    }

    pub fn window_thread_process_id(hwnd: isize) -> (u32, u32) {
        let mut process_id: u32 = 0;

        // Behaviour is undefined if an invalid HWND is given
        // https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid
        let thread_id = unsafe {
            GetWindowThreadProcessId(
                HWND(as_ptr!(hwnd)),
                Option::from(std::ptr::addr_of_mut!(process_id)),
            )
        };

        (process_id, thread_id)
    }

    pub fn current_process_id() -> u32 {
        unsafe { GetCurrentProcessId() }
    }

    pub fn process_id_to_session_id() -> Result<u32> {
        let process_id = Self::current_process_id();
        let mut session_id = 0;

        unsafe {
            if ProcessIdToSessionId(process_id, &mut session_id).is_ok() {
                Ok(session_id)
            } else {
                Err(anyhow!("could not determine current session id"))
            }
        }
    }

    #[cfg(target_pointer_width = "64")]
    fn set_window_long_ptr_w(
        hwnd: HWND,
        index: WINDOW_LONG_PTR_INDEX,
        new_value: isize,
    ) -> Result<()> {
        Result::from(WindowsResult::from(unsafe {
            SetWindowLongPtrW(hwnd, index, new_value)
        }))
        .map(|_| {})
    }

    #[cfg(target_pointer_width = "32")]
    fn set_window_long_ptr_w(
        hwnd: HWND,
        index: WINDOW_LONG_PTR_INDEX,
        new_value: i32,
    ) -> Result<()> {
        Result::from(WindowsResult::from(unsafe {
            SetWindowLongPtrW(hwnd, index, new_value)
        }))
        .map(|_| {})
    }

    #[cfg(target_pointer_width = "64")]
    pub fn gwl_style(hwnd: isize) -> Result<isize> {
        Self::window_long_ptr_w(HWND(as_ptr!(hwnd)), GWL_STYLE)
    }

    #[cfg(target_pointer_width = "32")]
    pub fn gwl_style(hwnd: isize) -> Result<i32> {
        Self::window_long_ptr_w(HWND(as_ptr!(hwnd)), GWL_STYLE)
    }

    #[cfg(target_pointer_width = "64")]
    pub fn gwl_ex_style(hwnd: isize) -> Result<isize> {
        Self::window_long_ptr_w(HWND(as_ptr!(hwnd)), GWL_EXSTYLE)
    }

    #[cfg(target_pointer_width = "32")]
    pub fn gwl_ex_style(hwnd: isize) -> Result<i32> {
        Self::window_long_ptr_w(HWND(as_ptr!(hwnd)), GWL_EXSTYLE)
    }

    #[cfg(target_pointer_width = "64")]
    fn window_long_ptr_w(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> Result<isize> {
        // Can return 0, which does not always mean that an error has occurred
        // https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowlongptrw
        Result::from(WindowsResult::from(unsafe {
            GetWindowLongPtrW(hwnd, index)
        }))
    }

    #[cfg(target_pointer_width = "32")]
    fn window_long_ptr_w(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX) -> Result<i32> {
        // Can return 0, which does not always mean that an error has occurred
        // https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowlongptrw
        Result::from(WindowsResult::from(unsafe {
            GetWindowLongPtrW(hwnd, index)
        }))
    }

    #[cfg(target_pointer_width = "64")]
    pub fn update_style(hwnd: isize, new_value: isize) -> Result<()> {
        Self::set_window_long_ptr_w(HWND(as_ptr!(hwnd)), GWL_STYLE, new_value)
    }

    #[cfg(target_pointer_width = "32")]
    pub fn update_style(hwnd: isize, new_value: i32) -> Result<()> {
        Self::set_window_long_ptr_w(HWND(as_ptr!(hwnd)), GWL_STYLE, new_value)
    }

    #[cfg(target_pointer_width = "64")]
    pub fn update_ex_style(hwnd: isize, new_value: isize) -> Result<()> {
        Self::set_window_long_ptr_w(HWND(as_ptr!(hwnd)), GWL_EXSTYLE, new_value)
    }

    #[cfg(target_pointer_width = "32")]
    pub fn update_ex_style(hwnd: isize, new_value: i32) -> Result<()> {
        Self::set_window_long_ptr_w(HWND(as_ptr!(hwnd)), GWL_EXSTYLE, new_value)
    }

    pub fn window_text_w(hwnd: isize) -> Result<String> {
        let mut text: [u16; 512] = [0; 512];
        match WindowsResult::from(unsafe { GetWindowTextW(HWND(as_ptr!(hwnd)), &mut text) }) {
            WindowsResult::Ok(len) => {
                let length = usize::try_from(len)?;
                Ok(String::from_utf16(&text[..length])?)
            }
            WindowsResult::Err(error) => Err(error),
        }
    }

    fn open_process(
        access_rights: PROCESS_ACCESS_RIGHTS,
        inherit_handle: bool,
        process_id: u32,
    ) -> Result<HANDLE> {
        unsafe { OpenProcess(access_rights, inherit_handle, process_id) }.process()
    }

    pub fn close_process(handle: HANDLE) -> Result<()> {
        unsafe { CloseHandle(handle) }.process()
    }

    pub fn process_handle(process_id: u32) -> Result<HANDLE> {
        Self::open_process(PROCESS_QUERY_INFORMATION, false, process_id)
    }

    pub fn exe_path(handle: HANDLE) -> Result<String> {
        let mut len = 260_u32;
        let mut path: Vec<u16> = vec![0; len as usize];
        let text_ptr = path.as_mut_ptr();

        unsafe {
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(text_ptr), &mut len)
        }
        .process()?;

        Ok(String::from_utf16(&path[..len as usize])?)
    }

    pub fn exe(handle: HANDLE) -> Result<String> {
        Ok(Self::exe_path(handle)?
            .split('\\')
            .next_back()
            .ok_or_else(|| anyhow!("there is no last element"))?
            .to_string())
    }

    pub fn real_window_class_w(hwnd: isize) -> Result<String> {
        const BUF_SIZE: usize = 512;
        let mut class: [u16; BUF_SIZE] = [0; BUF_SIZE];

        let len = Result::from(WindowsResult::from(unsafe {
            RealGetWindowClassW(HWND(as_ptr!(hwnd)), &mut class)
        }))?;

        Ok(String::from_utf16(&class[0..len as usize])?)
    }

    pub fn dwm_get_window_attribute<T>(
        hwnd: isize,
        attribute: DWMWINDOWATTRIBUTE,
        value: &mut T,
    ) -> Result<()> {
        unsafe {
            DwmGetWindowAttribute(
                HWND(as_ptr!(hwnd)),
                attribute,
                (value as *mut T).cast(),
                u32::try_from(std::mem::size_of::<T>())?,
            )?;
        }

        Ok(())
    }

    pub fn is_window_cloaked(hwnd: isize) -> Result<bool> {
        let mut cloaked: u32 = 0;
        Self::dwm_get_window_attribute(hwnd, DWMWA_CLOAKED, &mut cloaked)?;

        Ok(matches!(
            cloaked,
            DWM_CLOAKED_APP | DWM_CLOAKED_SHELL | DWM_CLOAKED_INHERITED
        ))
    }

    pub fn is_window(hwnd: isize) -> bool {
        unsafe { IsWindow(Option::from(HWND(as_ptr!(hwnd)))) }.into()
    }

    pub fn is_window_visible(hwnd: isize) -> bool {
        unsafe { IsWindowVisible(HWND(as_ptr!(hwnd))) }.into()
    }

    pub fn is_iconic(hwnd: isize) -> bool {
        unsafe { IsIconic(HWND(as_ptr!(hwnd))) }.into()
    }

    pub fn is_zoomed(hwnd: isize) -> bool {
        unsafe { IsZoomed(HWND(as_ptr!(hwnd))) }.into()
    }

    pub fn monitor_info_w(hmonitor: HMONITOR) -> Result<MONITORINFOEXW> {
        let mut ex_info = MONITORINFOEXW::default();
        ex_info.monitorInfo.cbSize = u32::try_from(std::mem::size_of::<MONITORINFOEXW>())?;
        unsafe { GetMonitorInfoW(hmonitor, &mut ex_info.monitorInfo) }
            .ok()
            .process()?;

        Ok(ex_info)
    }

    pub fn monitor_device_path(hmonitor: isize) -> Option<String> {
        for display in win32_display_data::connected_displays_all() {
            if display.hmonitor == hmonitor {
                return Some(display.device_path.clone());
            }
        }

        None
    }

    pub fn monitor(hmonitor: isize) -> Result<Monitor> {
        for mut display in win32_display_data::connected_displays_all() {
            if display.hmonitor == hmonitor {
                let path = display.device_path;

                let (device, device_id) = if path.is_empty() {
                    (String::from("UNKNOWN"), String::from("UNKNOWN"))
                } else {
                    let mut split: Vec<_> = path.split('#').collect();
                    split.remove(0);
                    split.remove(split.len() - 1);
                    let device = split[0].to_string();
                    let device_id = split.join("-");
                    (device, device_id)
                };

                let name = display.device_name.trim_start_matches(r"\\.\").to_string();
                let name = name.split('\\').collect::<Vec<_>>()[0].to_string();

                if let Some(id) = &display.serial_number_id {
                    let dupes = DUPLICATE_MONITOR_SERIAL_IDS.read();
                    if dupes.contains(id) {
                        display.serial_number_id = None;
                    }
                }

                let monitor = monitor::new(
                    hmonitor,
                    display.size.into(),
                    display.work_area_size.into(),
                    name,
                    device,
                    device_id,
                    display.serial_number_id,
                );

                return Ok(monitor);
            }
        }

        bail!("could not find device_id for hmonitor: {hmonitor}");
    }

    pub fn set_process_dpi_awareness_context() -> Result<()> {
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }
            .process()
    }

    #[allow(dead_code)]
    pub fn system_parameters_info_w(
        action: SYSTEM_PARAMETERS_INFO_ACTION,
        ui_param: u32,
        pv_param: *mut c_void,
        update_flags: SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    ) -> Result<()> {
        unsafe { SystemParametersInfoW(action, ui_param, Option::from(pv_param), update_flags) }
            .process()
    }

    #[tracing::instrument]
    pub fn foreground_lock_timeout() -> Result<()> {
        let mut value: u32 = 0;

        Self::system_parameters_info_w(
            SPI_GETFOREGROUNDLOCKTIMEOUT,
            0,
            std::ptr::addr_of_mut!(value).cast(),
            SPIF_SENDCHANGE,
        )?;

        tracing::info!("current value of ForegroundLockTimeout is {value}");

        if value != 0 {
            tracing::info!("updating value of ForegroundLockTimeout to {value} in order to enable keyboard-driven focus updating");

            Self::system_parameters_info_w(
                SPI_SETFOREGROUNDLOCKTIMEOUT,
                0,
                std::ptr::null_mut::<c_void>(),
                SPIF_SENDCHANGE,
            )?;

            Self::system_parameters_info_w(
                SPI_GETFOREGROUNDLOCKTIMEOUT,
                0,
                std::ptr::addr_of_mut!(value).cast(),
                SPIF_SENDCHANGE,
            )?;

            tracing::info!("updated value of ForegroundLockTimeout is now {value}");
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn focus_follows_mouse() -> Result<bool> {
        let mut is_enabled: BOOL = unsafe { std::mem::zeroed() };

        Self::system_parameters_info_w(
            SPI_GETACTIVEWINDOWTRACKING,
            0,
            std::ptr::addr_of_mut!(is_enabled).cast(),
            SPIF_SENDCHANGE,
        )?;

        Ok(is_enabled.into())
    }

    #[allow(dead_code)]
    pub fn enable_focus_follows_mouse() -> Result<()> {
        Self::system_parameters_info_w(
            SPI_SETACTIVEWINDOWTRACKING,
            0,
            1 as *mut c_void,
            SPIF_SENDCHANGE,
        )
    }

    #[allow(dead_code)]
    pub fn disable_focus_follows_mouse() -> Result<()> {
        Self::system_parameters_info_w(
            SPI_SETACTIVEWINDOWTRACKING,
            0,
            std::ptr::null_mut::<c_void>(),
            SPIF_SENDCHANGE,
        )
    }

    pub fn module_handle_w() -> Result<HMODULE> {
        unsafe { GetModuleHandleW(None) }.process()
    }

    pub fn create_solid_brush(colour: u32) -> HBRUSH {
        unsafe { CreateSolidBrush(COLORREF(colour)) }
    }

    pub fn register_class_w(window_class: &WNDCLASSW) -> Result<u16> {
        Result::from(WindowsResult::from(unsafe { RegisterClassW(window_class) }))
    }

    pub fn dpi_for_monitor(hmonitor: isize) -> Result<f32> {
        let mut dpi_x = u32::default();
        let mut dpi_y = u32::default();

        unsafe {
            GetDpiForMonitor(
                HMONITOR(as_ptr!(hmonitor)),
                MDT_EFFECTIVE_DPI,
                std::ptr::addr_of_mut!(dpi_x),
                std::ptr::addr_of_mut!(dpi_y),
            )
        }
        .process()?;

        #[allow(clippy::cast_precision_loss)]
        Ok(dpi_y as f32 / 96.0)
    }

    pub fn monitors_have_same_dpi(hmonitor_a: isize, hmonitor_b: isize) -> Result<bool> {
        let dpi_a = Self::dpi_for_monitor(hmonitor_a)?;
        let dpi_b = Self::dpi_for_monitor(hmonitor_b)?;

        Ok((dpi_a - dpi_b).abs() < f32::EPSILON)
    }

    pub fn round_corners(hwnd: isize) -> Result<()> {
        let round = DWMWCP_ROUND;

        unsafe {
            DwmSetWindowAttribute(
                HWND(as_ptr!(hwnd)),
                DWMWA_WINDOW_CORNER_PREFERENCE,
                std::ptr::addr_of!(round).cast(),
                4,
            )
        }
        .process()
    }

    pub fn set_window_accent(hwnd: isize, color: Option<u32>) -> Result<()> {
        let col_ref = COLORREF(color.unwrap_or(DWMWA_COLOR_NONE));
        unsafe {
            DwmSetWindowAttribute(
                HWND(as_ptr!(hwnd)),
                DWMWA_BORDER_COLOR,
                std::ptr::addr_of!(col_ref).cast(),
                4,
            )
        }
        .process()
    }

    pub fn create_border_window(
        name: PCWSTR,
        instance: isize,
        border: *mut Border,
    ) -> Result<isize> {
        unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                name,
                name,
                WS_POPUP | WS_SYSMENU,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Option::from(HINSTANCE(as_ptr!(instance))),
                Some(border as _),
            )?
        }
        .process()
    }

    pub fn set_transparent(hwnd: isize, alpha: u8) -> Result<()> {
        unsafe {
            #[allow(clippy::cast_sign_loss)]
            SetLayeredWindowAttributes(
                HWND(as_ptr!(hwnd)),
                COLORREF(-1i32 as u32),
                alpha,
                LWA_ALPHA,
            )?;
        }

        Ok(())
    }

    pub fn get_transparent(hwnd: isize) -> Result<u8> {
        unsafe {
            let mut alpha: u8 = u8::default();
            let mut color_ref = COLORREF(-1i32 as u32);
            let mut flags = LWA_ALPHA;
            GetLayeredWindowAttributes(
                HWND(as_ptr!(hwnd)),
                Some(std::ptr::addr_of_mut!(color_ref)),
                Some(std::ptr::addr_of_mut!(alpha)),
                Some(std::ptr::addr_of_mut!(flags)),
            )?;
            Ok(alpha)
        }
    }

    pub fn create_hidden_window(name: PCWSTR, instance: isize) -> Result<isize> {
        unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                name,
                name,
                WS_DISABLED,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Option::from(HINSTANCE(as_ptr!(instance))),
                None,
            )?
        }
        .process()
    }

    pub fn register_power_setting_notification(
        hwnd: isize,
        guid: &windows_core::GUID,
        flags: REGISTER_NOTIFICATION_FLAGS,
    ) -> WindowsCrateResult<HPOWERNOTIFY> {
        unsafe { RegisterPowerSettingNotification(HANDLE::from(HWND(as_ptr!(hwnd))), guid, flags) }
    }

    pub fn register_device_notification(
        hwnd: isize,
        mut filter: DEV_BROADCAST_DEVICEINTERFACE_W,
        flags: REGISTER_NOTIFICATION_FLAGS,
    ) -> WindowsCrateResult<HDEVNOTIFY> {
        unsafe {
            let state_ptr: *const c_void = &mut filter as *mut _ as *const c_void;
            RegisterDeviceNotificationW(HANDLE::from(HWND(as_ptr!(hwnd))), state_ptr, flags)
        }
    }

    pub fn invalidate_rect(hwnd: isize, rect: Option<&Rect>, erase: bool) -> bool {
        let rect = rect.map(|rect| &rect.rect() as *const RECT);
        unsafe { InvalidateRect(Option::from(HWND(as_ptr!(hwnd))), rect, erase) }.as_bool()
    }

    pub fn alt_is_pressed() -> bool {
        let state = unsafe { GetKeyState(i32::from(VK_MENU.0)) };
        #[allow(clippy::cast_sign_loss)]
        let actual = (state as u16) & 0x8000;
        actual != 0
    }

    pub fn lbutton_is_pressed() -> bool {
        let state = unsafe { GetKeyState(i32::from(VK_LBUTTON.0)) };
        #[allow(clippy::cast_sign_loss)]
        let actual = (state as u16) & 0x8000;
        actual != 0
    }

    pub fn left_click() -> u32 {
        let inputs = [
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_LEFTDOWN,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_LEFTUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
        unsafe {
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32)
        }
    }

    pub fn wts_register_session_notification(hwnd: isize) -> Result<()> {
        unsafe { WTSRegisterSessionNotification(HWND(as_ptr!(hwnd)), 1) }.process()
    }

    pub fn set_wallpaper(path: &Path, hmonitor: isize) -> Result<()> {
        let path = path.canonicalize()?;

        let wallpaper: IDesktopWallpaper =
            unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)? };

        let wallpaper_path = HSTRING::from(path.to_str().unwrap_or_default());
        unsafe {
            wallpaper.SetPosition(DWPOS_FILL)?;
        }

        let monitor_id = if let Some(path) = Self::monitor_device_path(hmonitor) {
            PCWSTR::from_raw(HSTRING::from(path).as_ptr())
        } else {
            PCWSTR::null()
        };

        // Set the wallpaper
        unsafe {
            wallpaper.SetWallpaper(monitor_id, PCWSTR::from_raw(wallpaper_path.as_ptr()))?;
        }
        Ok(())
    }

    pub fn get_wallpaper(hmonitor: isize) -> Result<String> {
        let wallpaper: IDesktopWallpaper =
            unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL)? };

        let monitor_id = if let Some(path) = Self::monitor_device_path(hmonitor) {
            PCWSTR::from_raw(HSTRING::from(path).as_ptr())
        } else {
            PCWSTR::null()
        };

        // Set the wallpaper
        unsafe {
            wallpaper
                .GetWallpaper(monitor_id)
                .and_then(|pwstr| pwstr.to_string().map_err(|e| e.into()))
        }
        .process()
    }
}
