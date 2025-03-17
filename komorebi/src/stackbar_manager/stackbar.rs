use super::StackbarGlobals;
use super::StackbarMessage;
use crate::container::Container;
use crate::core::BorderStyle;
use crate::core::Rect;
use crate::core::StackbarLabel;
use crate::windows_api;
use crate::WindowsApi;
use crate::DEFAULT_CONTAINER_PADDING;
use crate::WINDOWS_11;
use crossbeam_utils::atomic::AtomicConsume;
use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc;
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::COLORREF;
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::Foundation::LRESULT;
use windows::Win32::Foundation::WPARAM;
use windows::Win32::Graphics::Gdi::CreateFontIndirectW;
use windows::Win32::Graphics::Gdi::CreatePen;
use windows::Win32::Graphics::Gdi::CreateSolidBrush;
use windows::Win32::Graphics::Gdi::DeleteObject;
use windows::Win32::Graphics::Gdi::DrawTextW;
use windows::Win32::Graphics::Gdi::GetDC;
use windows::Win32::Graphics::Gdi::GetDeviceCaps;
use windows::Win32::Graphics::Gdi::Rectangle;
use windows::Win32::Graphics::Gdi::ReleaseDC;
use windows::Win32::Graphics::Gdi::RoundRect;
use windows::Win32::Graphics::Gdi::SelectObject;
use windows::Win32::Graphics::Gdi::SetBkColor;
use windows::Win32::Graphics::Gdi::SetTextColor;
use windows::Win32::Graphics::Gdi::DT_CENTER;
use windows::Win32::Graphics::Gdi::DT_END_ELLIPSIS;
use windows::Win32::Graphics::Gdi::DT_SINGLELINE;
use windows::Win32::Graphics::Gdi::DT_VCENTER;
use windows::Win32::Graphics::Gdi::FONT_QUALITY;
use windows::Win32::Graphics::Gdi::FW_BOLD;
use windows::Win32::Graphics::Gdi::LOGFONTW;
use windows::Win32::Graphics::Gdi::LOGPIXELSY;
use windows::Win32::Graphics::Gdi::PROOF_QUALITY;
use windows::Win32::Graphics::Gdi::PS_SOLID;
use windows::Win32::System::WindowsProgramming::MulDiv;
use windows::Win32::UI::WindowsAndMessaging::CreateWindowExW;
use windows::Win32::UI::WindowsAndMessaging::DefWindowProcW;
use windows::Win32::UI::WindowsAndMessaging::DispatchMessageW;
use windows::Win32::UI::WindowsAndMessaging::GetMessageW;
use windows::Win32::UI::WindowsAndMessaging::PostQuitMessage;
use windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes;
use windows::Win32::UI::WindowsAndMessaging::TranslateMessage;
use windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW;
use windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW;
use windows::Win32::UI::WindowsAndMessaging::LWA_COLORKEY;
use windows::Win32::UI::WindowsAndMessaging::MSG;
use windows::Win32::UI::WindowsAndMessaging::WM_DESTROY;
use windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDOWN;
use windows::Win32::UI::WindowsAndMessaging::WNDCLASSW;
use windows::Win32::UI::WindowsAndMessaging::WS_EX_LAYERED;
use windows::Win32::UI::WindowsAndMessaging::WS_EX_TOOLWINDOW;
use windows::Win32::UI::WindowsAndMessaging::WS_POPUP;
use windows::Win32::UI::WindowsAndMessaging::WS_VISIBLE;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Stackbar {
    pub hwnd: isize,
}

impl From<isize> for Stackbar {
    fn from(value: isize) -> Self {
        Self { hwnd: value }
    }
}

impl Stackbar {
    pub const fn hwnd(&self) -> HWND {
        HWND(windows_api::as_ptr!(self.hwnd))
    }

    pub fn create(id: &str) -> color_eyre::Result<Self> {
        let name: Vec<u16> = format!("komostackbar-{id}\0").encode_utf16().collect();
        let class_name = PCWSTR(name.as_ptr());

        let h_module = WindowsApi::module_handle_w()?;

        let window_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(Self::callback),
            hInstance: h_module.into(),
            lpszClassName: class_name,
            hbrBackground: WindowsApi::create_solid_brush(0),
            ..Default::default()
        };

        let _ = WindowsApi::register_class_w(&window_class);

        let (hwnd_sender, hwnd_receiver) = mpsc::channel();

        let name_cl = name.clone();
        let instance = h_module.0 as isize;
        std::thread::spawn(move || -> color_eyre::Result<()> {
            unsafe {
                let hwnd = CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_LAYERED,
                    PCWSTR(name_cl.as_ptr()),
                    PCWSTR(name_cl.as_ptr()),
                    WS_POPUP | WS_VISIBLE,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Option::from(HINSTANCE(windows_api::as_ptr!(instance))),
                    None,
                )?;

                SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_COLORKEY)?;
                hwnd_sender.send(hwnd.0 as isize)?;

                let mut msg: MSG = MSG::default();

                loop {
                    if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        tracing::debug!("stackbar window event processing thread shutdown");
                        break;
                    };
                    // TODO: error handling
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);

                    std::thread::sleep(Duration::from_millis(10))
                }
            }

            Ok(())
        });

        Ok(Self {
            hwnd: hwnd_receiver.recv()?,
        })
    }

    pub fn destroy(&self) -> color_eyre::Result<()> {
        WindowsApi::close_window(self.hwnd)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        globals: StackbarGlobals,
        container_padding: i32,
        container: &Container,
        stackbars_containers: &mut HashMap<isize, Container>,
        layout: &Rect,
        border_width: i32,
        border_offset: i32,
        border_style: BorderStyle,
    ) -> color_eyre::Result<()> {
        let width = globals.tab_width;
        let height = globals.tab_height;
        let gap = DEFAULT_CONTAINER_PADDING.load_consume();
        let background = globals.tab_background_colour;
        let focused_text_colour = globals.focused_text_colour;
        let unfocused_text_colour = globals.unfocused_text_colour;
        let font_family = &globals.font_family;
        let font_size = globals.font_size;
        let stackbar_label = globals.label;

        stackbars_containers.insert(self.hwnd, container.clone());

        let mut layout = *layout;
        let workspace_specific_offset = border_width + border_offset + container_padding;

        layout.top -= workspace_specific_offset + height;
        layout.left -= workspace_specific_offset;

        // Async causes the stackbar to disappear or flicker because we modify it right after,
        // so we have to do a synchronous call
        WindowsApi::position_window(self.hwnd, &layout, false, false)?;

        unsafe {
            let hdc = GetDC(Option::from(self.hwnd()));

            let hpen = CreatePen(PS_SOLID, 0, COLORREF(background));
            let hbrush = CreateSolidBrush(COLORREF(background));

            SelectObject(hdc, hpen.into());
            SelectObject(hdc, hbrush.into());
            SetBkColor(hdc, COLORREF(background));

            let mut logfont = LOGFONTW {
                lfWeight: FW_BOLD.0 as i32,
                lfQuality: FONT_QUALITY(PROOF_QUALITY.0),
                lfFaceName: [0; 32],
                ..Default::default()
            };

            if let Some(font_name) = font_family {
                let font = wide_string(font_name);
                for (i, &c) in font.iter().enumerate() {
                    logfont.lfFaceName[i] = c;
                }
            }

            let logical_height =
                -MulDiv(font_size, 72, GetDeviceCaps(Option::from(hdc), LOGPIXELSY));

            logfont.lfHeight = logical_height;

            let hfont = CreateFontIndirectW(&logfont);

            SelectObject(hdc, hfont.into());

            for (i, window) in container.windows().iter().enumerate() {
                if window.hwnd == container.focused_window().copied().unwrap_or_default().hwnd {
                    SetTextColor(hdc, COLORREF(focused_text_colour));
                } else {
                    SetTextColor(hdc, COLORREF(unfocused_text_colour));
                }

                let left = gap + (i as i32 * (width + gap));
                let mut rect = Rect {
                    top: 0,
                    left,
                    right: left + width,
                    bottom: height,
                };

                match border_style {
                    BorderStyle::System => {
                        if *WINDOWS_11 {
                            // TODO: error handling
                            let _ = RoundRect(
                                hdc,
                                rect.left,
                                rect.top,
                                rect.right,
                                rect.bottom,
                                20,
                                20,
                            );
                        } else {
                            // TODO: error handling
                            let _ = Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
                        }
                    }
                    BorderStyle::Rounded => {
                        // TODO: error handling
                        let _ =
                            RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, 20, 20);
                    }
                    BorderStyle::Square => {
                        // TODO: error handling
                        let _ = Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
                    }
                }

                let label = match stackbar_label {
                    StackbarLabel::Process => {
                        let exe = window.exe()?;
                        exe.trim_end_matches(".exe").to_string()
                    }
                    StackbarLabel::Title => window.title()?,
                };

                let mut tab_title: Vec<u16> = label.encode_utf16().collect();

                rect.left_padding(10);
                rect.right_padding(10);

                DrawTextW(
                    hdc,
                    &mut tab_title,
                    &mut rect.into(),
                    DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_END_ELLIPSIS,
                );
            }

            ReleaseDC(Option::from(self.hwnd()), hdc);
            // TODO: error handling
            let _ = DeleteObject(hpen.into());
            // TODO: error handling
            let _ = DeleteObject(hbrush.into());
            // TODO: error handling
            let _ = DeleteObject(hfont.into());
        }

        Ok(())
    }

    unsafe extern "system" fn callback(
        hwnd: HWND,
        msg: u32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        unsafe {
            match msg {
                WM_LBUTTONDOWN => {
                    let x = l_param.0 as i32 & 0xFFFF;
                    let y = (l_param.0 as i32 >> 16) & 0xFFFF;
                    super::send_notification(StackbarMessage::ButtonDown(
                        (hwnd.0 as isize, x, y).into(),
                    ));

                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, w_param, l_param),
            }
        }
    }
}

fn wide_string(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
