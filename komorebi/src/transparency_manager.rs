#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::monitor::Monitor;
use crate::ring::Ring;
use crate::runtime;
use crate::should_act;
use crate::Window;
use crate::WindowManager;
use crate::WindowsApi;
use crate::REGEX_IDENTIFIERS;
use crate::TRANSPARENCY_BLACKLIST;

/// Responsible for handling all transparency related logic and control
#[derive(Debug, Clone, PartialEq)]
pub struct TransparencyManager {
    pub enabled: bool,
    pub alpha: u8,
    pub known_transparent_hwnds: Vec<isize>,
}

impl Default for TransparencyManager {
    fn default() -> Self {
        Self {
            enabled: false,
            alpha: 200,
            known_transparent_hwnds: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransparencyMessage {
    Update,
}

impl From<TransparencyMessage> for runtime::Control {
    fn from(value: TransparencyMessage) -> Self {
        runtime::Control::Transparency(value)
    }
}

/// Represents the info from the `WindowManager` that is needed by the `StackbarManager`
#[derive(Debug, Clone, PartialEq)]
pub struct WindowManagerInfo {
    pub monitors: Ring<Monitor>,
    pub focused_monitor_idx: usize,
}

impl From<&WindowManager> for WindowManagerInfo {
    fn from(value: &WindowManager) -> Self {
        let monitors = value.monitors.clone();
        let focused_monitor_idx = value.focused_monitor_idx();

        WindowManagerInfo {
            monitors,
            focused_monitor_idx,
        }
    }
}

impl WindowManager {
    pub fn to_transparency_info(&self) -> WindowManagerInfo {
        self.into()
    }
}

impl TransparencyManager {
    pub fn update(&mut self, message: TransparencyMessage, wm_info: WindowManagerInfo) {
        match message {
            TransparencyMessage::Update => self.update_transparent_hwnds(wm_info),
        }
    }

    fn update_transparent_hwnds(&mut self, wm_info: WindowManagerInfo) {
        let known_hwnds = &mut self.known_transparent_hwnds;
        if !self.enabled {
            for hwnd in known_hwnds.iter() {
                if let Err(error) = Window::from(*hwnd).opaque(self.alpha) {
                    tracing::error!("failed to make window {hwnd} opaque: {error}")
                }
            }

            return;
        }

        known_hwnds.clear();

        let WindowManagerInfo {
            monitors,
            focused_monitor_idx,
        } = wm_info;

        'monitors: for (monitor_idx, m) in monitors.elements().iter().enumerate() {
            let focused_workspace_idx = m.focused_workspace_idx();

            'workspaces: for (workspace_idx, ws) in m.workspaces().iter().enumerate() {
                // Only operate on the focused workspace of each monitor
                // Workspaces with tiling disabled don't have transparent windows
                if !ws.tile() || workspace_idx != focused_workspace_idx {
                    for window in ws.visible_windows().iter().flatten() {
                        if let Err(error) = window.opaque(self.alpha) {
                            let hwnd = window.hwnd;
                            tracing::error!("failed to make window {hwnd} opaque: {error}")
                        }
                    }

                    continue 'workspaces;
                }

                // Monocle container is never transparent
                if let Some(monocle) = ws.monocle_container() {
                    if let Some(window) = monocle.focused_window() {
                        if monitor_idx == focused_monitor_idx {
                            if let Err(error) = window.opaque(self.alpha) {
                                let hwnd = window.hwnd;
                                tracing::error!(
                                    "failed to make monocle window {hwnd} opaque: {error}"
                                )
                            }
                        } else if let Err(error) = window.transparent(self.alpha) {
                            let hwnd = window.hwnd;
                            tracing::error!(
                                "failed to make monocle window {hwnd} transparent: {error}"
                            )
                        }
                    }

                    continue 'monitors;
                }

                let foreground_hwnd = WindowsApi::foreground_window().unwrap_or_default();
                let is_maximized = WindowsApi::is_zoomed(foreground_hwnd);

                if is_maximized {
                    if let Err(error) = Window::from(foreground_hwnd).opaque(self.alpha) {
                        let hwnd = foreground_hwnd;
                        tracing::error!("failed to make maximized window {hwnd} opaque: {error}")
                    }

                    continue 'monitors;
                }

                let transparency_blacklist = TRANSPARENCY_BLACKLIST.lock();
                let regex_identifiers = REGEX_IDENTIFIERS.lock();

                for (idx, c) in ws.containers().iter().enumerate() {
                    // Update the transparency for all containers on this workspace

                    // If the window is not focused on the current workspace, or isn't on the focused monitor
                    // make it transparent
                    #[allow(clippy::collapsible_else_if)]
                    if idx != ws.focused_container_idx() || monitor_idx != focused_monitor_idx {
                        let focused_window_idx = c.focused_window_idx();
                        for (window_idx, window) in c.windows().iter().enumerate() {
                            if window_idx == focused_window_idx {
                                let mut should_make_transparent = true;
                                if !transparency_blacklist.is_empty() {
                                    if let (Ok(title), Ok(exe_name), Ok(class), Ok(path)) = (
                                        window.title(),
                                        window.exe(),
                                        window.class(),
                                        window.path(),
                                    ) {
                                        let is_blacklisted = should_act(
                                            &title,
                                            &exe_name,
                                            &class,
                                            &path,
                                            &transparency_blacklist,
                                            &regex_identifiers,
                                        )
                                        .is_some();

                                        should_make_transparent = !is_blacklisted;
                                    }
                                }

                                if should_make_transparent {
                                    match window.transparent(self.alpha) {
                                        Err(error) => {
                                            let hwnd = foreground_hwnd;
                                            tracing::error!("failed to make unfocused window {hwnd} transparent: {error}" )
                                        }
                                        Ok(..) => {
                                            known_hwnds.push(window.hwnd);
                                        }
                                    }
                                }
                            } else {
                                // just in case, this is useful when people are clicking around
                                // on unfocused stackbar tabs
                                known_hwnds.push(window.hwnd);
                            }
                        }
                    // Otherwise, make it opaque
                    } else {
                        let focused_window_idx = c.focused_window_idx();
                        for (window_idx, window) in c.windows().iter().enumerate() {
                            if window_idx != focused_window_idx {
                                known_hwnds.push(window.hwnd);
                            } else {
                                if let Err(error) = c
                                    .focused_window()
                                    .copied()
                                    .unwrap_or_default()
                                    .opaque(self.alpha)
                                {
                                    let hwnd = foreground_hwnd;
                                    tracing::error!(
                                        "failed to make focused window {hwnd} opaque: {error}"
                                    )
                                }
                            }
                        }
                    };
                }
            }
        }
    }
}

pub fn send_update() {
    runtime::send_message(TransparencyMessage::Update);
}
