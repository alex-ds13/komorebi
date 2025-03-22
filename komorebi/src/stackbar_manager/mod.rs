mod stackbar;

use crate::container::Container;
use crate::core::StackbarLabel;
use crate::core::StackbarMode;
use crate::monitor::Monitor;
use crate::ring::Ring;
use crate::runtime;
use crate::stackbar_manager::stackbar::Stackbar;
use crate::BorderStyle;
use crate::WindowManager;
use crate::WindowsApi;
use crate::DEFAULT_CONTAINER_PADDING;
use crossbeam_utils::atomic::AtomicConsume;
use std::collections::hash_map::Entry;
use std::collections::HashMap;

/// Responsible for handling all stackbar related logic and control
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StackbarManager {
    pub stackbars: HashMap<String, Stackbar>,
    pub stackbars_containers: HashMap<isize, Container>,
    pub stackbars_monitors: HashMap<String, usize>,
    pub globals: StackbarGlobals,
    pub temporarely_disabled: bool,
}

/// Contains all the global data related to the stackbars
#[derive(Debug, Clone, PartialEq)]
pub struct StackbarGlobals {
    pub tab_width: i32,
    pub tab_height: i32,
    pub label: StackbarLabel,
    pub mode: StackbarMode,
    pub focused_text_colour: u32,
    pub unfocused_text_colour: u32,
    pub tab_background_colour: u32,
    pub font_family: Option<String>,
    pub font_size: i32,
}

impl Default for StackbarGlobals {
    fn default() -> Self {
        Self {
            tab_width: 200,
            tab_height: 40,
            label: Default::default(),
            mode: Default::default(),
            focused_text_colour: 16777215,   // white
            unfocused_text_colour: 11776947, // gray text
            tab_background_colour: 3355443,  // gray
            font_family: None,
            font_size: 0, // 0 will produce the system default
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StackbarMessage {
    Update,
    ButtonDown(ButtonDownInfo),
    Enable,
    Disable,
}

impl From<StackbarMessage> for runtime::Control {
    fn from(value: StackbarMessage) -> Self {
        runtime::Control::Stackbar(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ButtonDownInfo {
    pub hwnd: isize,
    pub x: i32,
    pub y: i32,
}

impl From<(isize, i32, i32)> for ButtonDownInfo {
    fn from(value: (isize, i32, i32)) -> Self {
        ButtonDownInfo {
            hwnd: value.0,
            x: value.1,
            y: value.2,
        }
    }
}

/// Represents the info from the `WindowManager` that is needed by the `StackbarManager`
#[derive(Debug, Clone, PartialEq)]
pub struct WindowManagerInfo {
    pub is_paused: bool,
    pub monitors: Ring<Monitor>,
    pub border_width: i32,
    pub border_offset: i32,
    pub border_style: BorderStyle,
}

impl From<&WindowManager> for WindowManagerInfo {
    fn from(value: &WindowManager) -> Self {
        let is_paused = value.is_paused;
        let monitors = value.monitors.clone();
        let border_width = value.border_manager.border_width;
        let border_offset = value.border_manager.border_offset;
        let border_style = value.border_manager.border_style;

        WindowManagerInfo {
            is_paused,
            monitors,
            border_width,
            border_offset,
            border_style,
        }
    }
}

impl WindowManager {
    pub fn to_stackbar_info(&self) -> WindowManagerInfo {
        self.into()
    }
}

pub fn send_update() {
    runtime::send_message(StackbarMessage::Update)
}

pub fn button_down(info: ButtonDownInfo) {
    runtime::send_message(StackbarMessage::ButtonDown(info))
}

pub fn disable() {
    runtime::send_message(StackbarMessage::Disable)
}

pub fn enable() {
    runtime::send_message(StackbarMessage::Enable)
}

impl StackbarManager {
    pub fn update(
        &mut self,
        message: StackbarMessage,
        wm_info: WindowManagerInfo,
    ) -> color_eyre::Result<()> {
        match message {
            StackbarMessage::Update => {
                self.update_stackbars(wm_info)?;
            }
            StackbarMessage::ButtonDown(info) => self.button_down(info),
            StackbarMessage::Enable => {
                self.temporarely_disabled = false;
                self.update_stackbars(wm_info)?;
            },
            StackbarMessage::Disable => {
                self.temporarely_disabled = true;
                self.update_stackbars(wm_info)?;
            },
        }
        Ok(())
    }

    pub fn update_stackbars(&mut self, wm_info: WindowManagerInfo) -> color_eyre::Result<()> {
        let stackbars = &mut self.stackbars;
        let stackbars_monitors = &mut self.stackbars_monitors;

        // If stackbars are disabled
        if self.temporarely_disabled || matches!(self.globals.mode, StackbarMode::Never) {
            for (_, stackbar) in stackbars.iter() {
                stackbar.destroy()?;
            }

            stackbars.clear();
            return Ok(());
        }

        let border_width = wm_info.border_width;
        let border_offset = wm_info.border_offset;
        let border_style = wm_info.border_style;

        for (monitor_idx, m) in wm_info.monitors.elements().iter().enumerate() {
            // Only operate on the focused workspace of each monitor
            if let Some(ws) = m.focused_workspace() {
                // Workspaces with tiling disabled don't have stackbars
                if !ws.tile() {
                    let mut to_remove = vec![];
                    for (id, border) in stackbars.iter() {
                        if stackbars_monitors.get(id).copied().unwrap_or_default() == monitor_idx {
                            border.destroy()?;
                            to_remove.push(id.clone());
                        }
                    }

                    for id in &to_remove {
                        stackbars.remove(id);
                    }

                    return Ok(());
                }

                let is_maximized =
                    WindowsApi::is_zoomed(WindowsApi::foreground_window().unwrap_or_default());

                // Handle the monocle container separately
                if ws.monocle_container().is_some() || is_maximized {
                    // Destroy any stackbars associated with the focused workspace
                    let mut to_remove = vec![];
                    for (id, stackbar) in stackbars.iter() {
                        if stackbars_monitors.get(id).copied().unwrap_or_default() == monitor_idx {
                            stackbar.destroy()?;
                            to_remove.push(id.clone());
                        }
                    }

                    for id in &to_remove {
                        stackbars.remove(id);
                    }

                    return Ok(());
                }

                // Destroy any stackbars not associated with the focused workspace
                let container_ids = ws
                    .containers()
                    .iter()
                    .map(|c| c.id().clone())
                    .collect::<Vec<_>>();

                let mut to_remove = vec![];
                for (id, stackbar) in stackbars.iter() {
                    if stackbars_monitors.get(id).copied().unwrap_or_default() == monitor_idx
                        && !container_ids.contains(id)
                    {
                        stackbar.destroy()?;
                        to_remove.push(id.clone());
                    }
                }

                for id in &to_remove {
                    stackbars.remove(id);
                }

                let container_padding = ws
                    .container_padding()
                    .unwrap_or_else(|| DEFAULT_CONTAINER_PADDING.load_consume());

                'containers: for container in ws.containers() {
                    let should_add_stackbar =
                        should_have_stackbar(&self.globals.mode, container.windows().len());

                    if !should_add_stackbar {
                        if let Some(stackbar) = stackbars.get(container.id()) {
                            stackbar.destroy()?
                        }

                        stackbars.remove(container.id());
                        stackbars_monitors.remove(container.id());
                        continue 'containers;
                    }

                    // Get the stackbar entry for this container from the map or create one
                    let stackbar = match stackbars.entry(container.id().clone()) {
                        Entry::Occupied(entry) => entry.into_mut(),
                        Entry::Vacant(entry) => {
                            if let Ok(stackbar) = Stackbar::create(container.id()) {
                                entry.insert(stackbar)
                            } else {
                                return Ok(());
                            }
                        }
                    };

                    stackbars_monitors.insert(container.id().clone(), monitor_idx);

                    let rect = WindowsApi::window_rect(
                        container.focused_window().copied().unwrap_or_default().hwnd,
                    )?;

                    stackbar.update(
                        self.globals.clone(),
                        container_padding,
                        container,
                        &mut self.stackbars_containers,
                        &rect,
                        border_width,
                        border_offset,
                        border_style,
                    )?;
                }
            }
        }

        Ok(())
    }

    fn button_down(&mut self, info: ButtonDownInfo) {
        let ButtonDownInfo { hwnd, x, y } = info;
        let stackbars_containers = &mut self.stackbars_containers;
        if let Some(container) = stackbars_containers.get(&hwnd) {
            let width = self.globals.tab_width;
            let height = self.globals.tab_height;
            let gap = DEFAULT_CONTAINER_PADDING.load_consume();

            let focused_window_idx = container.focused_window_idx();
            let focused_window_rect = WindowsApi::window_rect(
                container.focused_window().cloned().unwrap_or_default().hwnd,
            )
            .unwrap_or_default();

            for (index, window) in container.windows().iter().enumerate() {
                let left = gap + (index as i32 * (width + gap));
                let right = left + width;
                let top = 0;
                let bottom = height;

                if x >= left && x <= right && y >= top && y <= bottom {
                    // If we are focusing a window that isn't currently focused in the
                    // stackbar, make sure we update its location so that it doesn't render
                    // on top of other tiles before eventually ending up in the correct
                    // tile
                    if index != focused_window_idx {
                        if let Err(err) = window.set_position(&focused_window_rect, false) {
                            tracing::error!(
                                "stackbar WM_LBUTTONDOWN repositioning error: hwnd {} ({})",
                                *window,
                                err
                            );
                        }
                    }

                    // Restore the window corresponding to the tab we have clicked
                    window.restore_with_border(false);
                    if let Err(err) = window.focus(false) {
                        tracing::error!(
                            "stackbar WMLBUTTONDOWN focus error: hwnd {} ({})",
                            *window,
                            err
                        );
                    }
                } else {
                    // Hide any windows in the stack that don't correspond to the window
                    // we have clicked
                    window.hide_with_border(false);
                }
            }
        }
    }
}

pub fn should_have_stackbar(mode: &StackbarMode, window_count: usize) -> bool {
    match mode {
        StackbarMode::Always => true,
        StackbarMode::OnStack => window_count > 1,
        StackbarMode::Never => false,
    }
}
