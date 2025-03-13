#![deny(clippy::unwrap_used, clippy::expect_used)]

mod border;
use crate::core::BorderImplementation;
use crate::core::BorderStyle;
use crate::core::WindowKind;
use crate::monitor::Monitor;
use crate::ring::Ring;
use crate::runtime;
use crate::windows_api;
use crate::workspace::WorkspaceLayer;
use crate::Colour;
use crate::Rgb;
use crate::WindowManager;
use crate::WindowsApi;
use border::border_hwnds;
pub use border::Border;
use serde::Deserialize;
use serde::Serialize;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::ops::Deref;
use strum::Display;
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::Foundation::WPARAM;
use windows::Win32::Graphics::Direct2D::ID2D1HwndRenderTarget;
use windows::Win32::UI::WindowsAndMessaging::SendNotifyMessageW;

/// Responsible for handling all border related logic and control
#[derive(Debug, Default, Clone, PartialEq)]
pub struct BorderManager {
    pub enabled: bool,
    pub borders: HashMap<String, Box<Border>>,
    pub windows_borders: HashMap<isize, String>,
    pub tracking_hwnd: Option<isize>,
    pub wm_info: WindowManagerInfo,
    pub border_width: i32,
    pub border_offset: i32,
    pub border_style: BorderStyle,
    pub border_implementation: BorderImplementation,
    pub kind_colours: WindowKindColours,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowKindColours {
    pub single_colour: u32,
    pub unfocused_colour: u32,
    pub monocle_colour: u32,
    pub stack_colour: u32,
    pub floating_colour: u32,
}

impl Default for WindowKindColours {
    fn default() -> Self {
        Self {
            single_colour: u32::from(Colour::Rgb(Rgb::new(66, 165, 245))),
            unfocused_colour: u32::from(Colour::Rgb(Rgb::new(128, 128, 128))),
            monocle_colour: u32::from(Colour::Rgb(Rgb::new(255, 51, 153))),
            stack_colour: u32::from(Colour::Rgb(Rgb::new(0, 165, 66))),
            floating_colour: u32::from(Colour::Rgb(Rgb::new(245, 245, 165))),
        }
    }
}

impl WindowKindColours {
    /// Gets the colour as a `u32` from the `WindowKind`
    pub fn from_kind(&self, window_kind: WindowKind) -> u32 {
        match window_kind {
            WindowKind::Unfocused => self.unfocused_colour,
            WindowKind::Single => self.single_colour,
            WindowKind::Stack => self.stack_colour,
            WindowKind::Monocle => self.monocle_colour,
            WindowKind::Floating => self.floating_colour,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderTarget(pub ID2D1HwndRenderTarget);
unsafe impl Send for RenderTarget {}

impl Deref for RenderTarget {
    type Target = ID2D1HwndRenderTarget;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BorderMessage {
    Update(Option<isize>),
    PassEvent(isize, u32),
    Delete(isize),
    Show(isize),
    Hide(isize),
    Raise(isize),
    Lower(isize),
    DestroyAll,
}

impl From<BorderMessage> for runtime::Control {
    fn from(value: BorderMessage) -> Self {
        runtime::Control::Border(value)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct BorderInfo {
    pub border_hwnd: isize,
    pub window_kind: WindowKind,
}

impl BorderInfo {
    pub fn hwnd(&self) -> HWND {
        HWND(windows_api::as_ptr!(self.border_hwnd))
    }
}

impl BorderManager {
    pub fn update(
        &mut self,
        message: BorderMessage,
        wm_info: WindowManagerInfo,
    ) -> color_eyre::Result<()> {
        match message {
            BorderMessage::Update(tracking_hwnd) => {
                self.handle_border_update(wm_info, tracking_hwnd)
            }
            BorderMessage::PassEvent(tracking_hwnd, event) => {
                let border_info = self.window_border(tracking_hwnd);

                if let Some(border_info) = border_info {
                    notify_border(border_info.hwnd(), event, tracking_hwnd);
                }

                Ok(())
            }
            BorderMessage::Delete(tracking_hwnd) => {
                let id = self
                    .windows_borders
                    .get(&tracking_hwnd)
                    .cloned()
                    .unwrap_or_default();

                if let Err(error) = self.remove_border(&id) {
                    tracing::error!("Failed to delete border: {}", error);
                }
                Ok(())
            }
            BorderMessage::Show(tracking_hwnd) => {
                if let Some(border_info) = self.window_border(tracking_hwnd) {
                    WindowsApi::restore_window(border_info.border_hwnd);
                }
                Ok(())
            }
            BorderMessage::Hide(tracking_hwnd) => {
                if let Some(border_info) = self.window_border(tracking_hwnd) {
                    WindowsApi::hide_window(border_info.border_hwnd);
                }
                Ok(())
            }
            BorderMessage::Raise(tracking_hwnd) => {
                if let Some(border_info) = self.window_border(tracking_hwnd) {
                    WindowsApi::raise_window(border_info.border_hwnd)
                } else {
                    Ok(())
                }
            }
            BorderMessage::Lower(tracking_hwnd) => {
                if let Some(border_info) = self.window_border(tracking_hwnd) {
                    WindowsApi::lower_window(border_info.border_hwnd)
                } else {
                    Ok(())
                }
            }
            BorderMessage::DestroyAll => self.destroy_all_borders(),
        }
    }

    fn handle_border_update(
        &mut self,
        wm_info: WindowManagerInfo,
        tracking_hwnd: Option<isize>,
    ) -> color_eyre::Result<()> {
        // Check the wm info every time we receive an update
        let is_paused = wm_info.is_paused;
        let focused_monitor_idx = wm_info.focused_monitor_idx;
        let pending_move_op = wm_info.pending_move_op;
        let workspace_layer = wm_info.workspace_layer;

        let foreground_window = WindowsApi::foreground_window().unwrap_or_default();

        let previous_monitors = &self.wm_info.monitors;
        let previous_pending_move_op = &self.wm_info.pending_move_op;
        let previous_is_paused = &self.wm_info.is_paused;
        let previous_tracking_hwnd = &self.tracking_hwnd;
        let previous_layer = &self.wm_info.workspace_layer;
        let layer_changed = *previous_layer != workspace_layer;

        match self.border_implementation {
            BorderImplementation::Windows => {
                'monitors: for (monitor_idx, m) in wm_info.monitors.elements().iter().enumerate() {
                    // Only operate on the focused workspace of each monitor
                    if let Some(ws) = m.focused_workspace() {
                        // Handle the monocle container separately
                        if let Some(monocle) = ws.monocle_container() {
                            let window_kind = if monitor_idx != focused_monitor_idx {
                                WindowKind::Unfocused
                            } else {
                                WindowKind::Monocle
                            };

                            monocle
                                .focused_window()
                                .copied()
                                .unwrap_or_default()
                                .set_accent(self.window_kind_colour(window_kind))?;

                            continue 'monitors;
                        }

                        for (idx, c) in ws.containers().iter().enumerate() {
                            let window_kind = if idx != ws.focused_container_idx()
                                || monitor_idx != focused_monitor_idx
                            {
                                WindowKind::Unfocused
                            } else if c.windows().len() > 1 {
                                WindowKind::Stack
                            } else {
                                WindowKind::Single
                            };

                            c.focused_window()
                                .copied()
                                .unwrap_or_default()
                                .set_accent(self.window_kind_colour(window_kind))?;
                        }

                        for window in ws.floating_windows() {
                            let mut window_kind = WindowKind::Unfocused;

                            if foreground_window == window.hwnd {
                                window_kind = WindowKind::Floating;
                            }

                            window.set_accent(self.window_kind_colour(window_kind))?;
                        }
                    }
                }
            }
            BorderImplementation::Komorebi => {
                let mut should_process_notification = true;

                if wm_info.monitors == *previous_monitors
                    // handle the window dragging edge case
                    && pending_move_op == *previous_pending_move_op
                {
                    should_process_notification = false;
                }

                // handle the pause edge case
                if is_paused && !*previous_is_paused {
                    should_process_notification = true;
                }

                // handle the unpause edge case
                if *previous_is_paused && !is_paused {
                    should_process_notification = true;
                }

                // handle the retile edge case
                if !should_process_notification && self.borders.is_empty() {
                    should_process_notification = true;
                }

                // when we switch focus to/from a floating window
                let switch_focus_to_from_floating_window =
                    wm_info.floating_window_hwnds.iter().any(|fw| {
                        // if we switch focus to a floating window
                        fw == &tracking_hwnd.unwrap_or_default() ||
                        // if there is any floating window with a `WindowKind::Floating` border
                        // that no longer is the foreground window then we need to update that
                        // border.
                        (fw != &foreground_window
                            && self.window_border(*fw)
                            .is_some_and(|b| b.window_kind == WindowKind::Floating))
                    });

                // when the focused window has an `Unfocused` border kind, usually this happens if
                // we focus an admin window and then refocus the previously focused window. For
                // komorebi it will have the same state has before, however the previously focused
                // window changed its border to unfocused so now we need to update it again.
                if !should_process_notification
                    && self
                        .window_border(tracking_hwnd.unwrap_or_default())
                        .is_some_and(|b| b.window_kind == WindowKind::Unfocused)
                {
                    should_process_notification = true;
                }

                if !should_process_notification && switch_focus_to_from_floating_window {
                    should_process_notification = true;
                }

                if !should_process_notification {
                    if let Some(previous) = previous_tracking_hwnd {
                        if *previous != tracking_hwnd.unwrap_or_default() {
                            should_process_notification = true;
                        }
                    }
                }

                if !should_process_notification {
                    tracing::trace!("monitor state matches latest snapshot, skipping notification");
                    return Ok(());
                }

                // If borders are disabled
                if !self.enabled
                    // Or if the wm is paused
                    || is_paused
                {
                    // Destroy the borders we know about
                    for (_, border) in self.borders.drain() {
                        destroy_border(border)?;
                    }

                    self.windows_borders.clear();

                    self.wm_info.is_paused = is_paused;
                    return Ok(());
                }

                let style = self.border_style;
                let width = self.border_width;
                let offset = self.border_offset;
                let kind_colours = self.kind_colours;

                'monitors: for (monitor_idx, m) in wm_info.monitors.elements().iter().enumerate() {
                    // Only operate on the focused workspace of each monitor
                    if let Some(ws) = m.focused_workspace() {
                        // Workspaces with tiling disabled don't have borders
                        if !ws.tile() {
                            // Remove all borders on this monitor
                            self.remove_borders(monitor_idx, |_, _| true)?;

                            continue 'monitors;
                        }

                        // Handle the monocle container separately
                        if let Some(monocle) = ws.monocle_container() {
                            let mut new_border = false;
                            let focused_window_hwnd =
                                monocle.focused_window().map(|w| w.hwnd).unwrap_or_default();
                            let id = monocle.id().clone();
                            let border = match self.borders.entry(id.clone()) {
                                Entry::Occupied(entry) => entry.into_mut(),
                                Entry::Vacant(entry) => {
                                    if let Ok(border) = Border::create(
                                        monocle.id(),
                                        focused_window_hwnd,
                                        monitor_idx,
                                        style,
                                        width,
                                        offset,
                                        kind_colours,
                                    ) {
                                        new_border = true;
                                        entry.insert(border)
                                    } else {
                                        continue 'monitors;
                                    }
                                }
                            };

                            // Update border globals
                            border.style = style;
                            border.width = width;
                            border.offset = offset;

                            let new_focus_state = if monitor_idx != focused_monitor_idx {
                                WindowKind::Unfocused
                            } else {
                                WindowKind::Monocle
                            };
                            border.window_kind = new_focus_state;

                            // Update the borders tracking_hwnd in case it changed and remove the
                            // old `tracking_hwnd` from `WINDOWS_BORDERS` if needed.
                            if border.tracking_hwnd != focused_window_hwnd {
                                if let Some(previous) =
                                    self.windows_borders.get(&border.tracking_hwnd)
                                {
                                    // Only remove the border from `windows_borders` if it
                                    // still corresponds to the same border, if doesn't then
                                    // it means it was already updated by another border for
                                    // that window and in that case we don't want to remove it.
                                    if previous == &id {
                                        self.windows_borders.remove(&border.tracking_hwnd);
                                    }
                                }
                                border.tracking_hwnd = focused_window_hwnd;
                                if !WindowsApi::is_window_visible(border.hwnd) {
                                    WindowsApi::restore_window(border.hwnd);
                                }
                            }

                            // Update the border's monitor idx in case it changed
                            border.monitor_idx = Some(monitor_idx);

                            let rect = WindowsApi::window_rect(focused_window_hwnd)?;
                            border.window_rect = rect;

                            if new_border {
                                border.set_position(&rect, focused_window_hwnd)?;
                            }

                            border.invalidate();

                            self.windows_borders.insert(focused_window_hwnd, id);

                            let border_hwnd = border.hwnd;
                            // Remove all borders on this monitor except monocle
                            self.remove_borders(monitor_idx, |_, b| border_hwnd != b.hwnd)?;

                            continue 'monitors;
                        }

                        let foreground_hwnd = WindowsApi::foreground_window().unwrap_or_default();
                        let foreground_monitor_id =
                            WindowsApi::monitor_from_window(foreground_hwnd);
                        let is_maximized = foreground_monitor_id == m.id()
                            && WindowsApi::is_zoomed(foreground_hwnd);

                        if is_maximized {
                            // Remove all borders on this monitor
                            self.remove_borders(monitor_idx, |_, _| true)?;

                            continue 'monitors;
                        }

                        // Collect focused workspace container and floating windows ID's
                        let mut container_and_floating_window_ids = ws
                            .containers()
                            .iter()
                            .map(|c| c.id().clone())
                            .collect::<Vec<_>>();

                        for w in ws.floating_windows() {
                            container_and_floating_window_ids.push(w.hwnd.to_string());
                        }

                        // Remove any borders not associated with the focused workspace
                        self.remove_borders(monitor_idx, |id, _| {
                            !container_and_floating_window_ids.contains(id)
                        })?;

                        'containers: for (idx, c) in ws.containers().iter().enumerate() {
                            let focused_window_hwnd =
                                c.focused_window().map(|w| w.hwnd).unwrap_or_default();
                            let id = c.id().clone();

                            // Get the border entry for this container from the map or create one
                            let mut new_border = false;
                            let border = match self.borders.entry(id.clone()) {
                                Entry::Occupied(entry) => entry.into_mut(),
                                Entry::Vacant(entry) => {
                                    if let Ok(border) = Border::create(
                                        c.id(),
                                        focused_window_hwnd,
                                        monitor_idx,
                                        style,
                                        width,
                                        offset,
                                        kind_colours,
                                    ) {
                                        new_border = true;
                                        entry.insert(border)
                                    } else {
                                        continue 'monitors;
                                    }
                                }
                            };

                            // Update border globals
                            border.style = style;
                            border.width = width;
                            border.offset = offset;

                            let last_focus_state = border.window_kind;

                            let new_focus_state = if idx != ws.focused_container_idx()
                                || monitor_idx != focused_monitor_idx
                                || focused_window_hwnd != foreground_window
                            {
                                WindowKind::Unfocused
                            } else if c.windows().len() > 1 {
                                WindowKind::Stack
                            } else {
                                WindowKind::Single
                            };

                            border.window_kind = new_focus_state;

                            // Update the borders `tracking_hwnd` in case it changed and remove the
                            // old `tracking_hwnd` from `WINDOWS_BORDERS` if needed.
                            if border.tracking_hwnd != focused_window_hwnd {
                                if let Some(previous) =
                                    self.windows_borders.get(&border.tracking_hwnd)
                                {
                                    // Only remove the border from `windows_borders` if it
                                    // still corresponds to the same border, if doesn't then
                                    // it means it was already updated by another border for
                                    // that window and in that case we don't want to remove it.
                                    if previous == &id {
                                        self.windows_borders.remove(&border.tracking_hwnd);
                                    }
                                }
                                border.tracking_hwnd = focused_window_hwnd;
                                if !WindowsApi::is_window_visible(border.hwnd) {
                                    WindowsApi::restore_window(border.hwnd);
                                }
                            }

                            // Update the border's monitor idx in case it changed
                            border.monitor_idx = Some(monitor_idx);

                            // avoid getting into a thread restart loop if we try to look up
                            // rect info for a window that has been destroyed by the time
                            // we get here
                            let rect = match WindowsApi::window_rect(focused_window_hwnd) {
                                Ok(rect) => rect,
                                Err(_) => {
                                    self.remove_border(c.id())?;
                                    continue 'containers;
                                }
                            };
                            border.window_rect = rect;

                            let should_invalidate = new_border
                                || (last_focus_state != new_focus_state)
                                || layer_changed;

                            if should_invalidate {
                                border.set_position(&rect, focused_window_hwnd)?;
                                border.invalidate();
                            }

                            self.windows_borders.insert(focused_window_hwnd, id);
                        }

                        for window in ws.floating_windows() {
                            let mut new_border = false;
                            let id = window.hwnd.to_string();
                            let border = match self.borders.entry(id.clone()) {
                                Entry::Occupied(entry) => entry.into_mut(),
                                Entry::Vacant(entry) => {
                                    if let Ok(border) = Border::create(
                                        &window.hwnd.to_string(),
                                        window.hwnd,
                                        monitor_idx,
                                        style,
                                        width,
                                        offset,
                                        kind_colours,
                                    ) {
                                        new_border = true;
                                        entry.insert(border)
                                    } else {
                                        continue 'monitors;
                                    }
                                }
                            };

                            // Update border globals
                            border.style = style;
                            border.width = width;
                            border.offset = offset;

                            let last_focus_state = border.window_kind;

                            let new_focus_state = if foreground_window == window.hwnd {
                                WindowKind::Floating
                            } else {
                                WindowKind::Unfocused
                            };

                            border.window_kind = new_focus_state;

                            // Update the border's monitor idx in case it changed
                            border.monitor_idx = Some(monitor_idx);

                            let rect = WindowsApi::window_rect(window.hwnd)?;
                            border.window_rect = rect;

                            let should_invalidate = new_border
                                || (last_focus_state != new_focus_state)
                                || layer_changed;

                            if should_invalidate {
                                border.set_position(&rect, window.hwnd)?;
                                border.invalidate();
                            }

                            self.windows_borders.insert(window.hwnd, id);
                        }
                    }
                }
            }
        }

        self.wm_info = wm_info;
        self.tracking_hwnd = tracking_hwnd;

        Ok(())
    }

    /// Check if some window with `hwnd` has a border attached to it, if it does returns the
    /// `BorderInfo` related to it's border.
    pub fn window_border(&self, hwnd: isize) -> Option<BorderInfo> {
        self.windows_borders.get(&hwnd).and_then(|id| {
            self.borders.get(id).map(|b| BorderInfo {
                border_hwnd: b.hwnd,
                window_kind: b.window_kind,
            })
        })
    }

    /// Destroys all known and unknown borders
    fn destroy_all_borders(&mut self) -> color_eyre::Result<()> {
        tracing::info!(
            "purging known borders: {:?}",
            self.borders.iter().map(|b| b.1.hwnd).collect::<Vec<_>>()
        );

        for (_, border) in self.borders.drain() {
            let _ = destroy_border(border);
        }

        self.windows_borders.clear();

        let mut remaining_borders = vec![];

        WindowsApi::enum_windows(
            Some(border_hwnds),
            &mut remaining_borders as *mut Vec<Border> as isize,
        )?;

        if !remaining_borders.is_empty() {
            tracing::info!("purging unknown borders: {:?}", remaining_borders);

            for border in remaining_borders {
                let _ = destroy_border(Box::new(border));
            }
        }

        Ok(())
    }

    /// Removes all borders from monitor with index `monitor_idx` filtered by
    /// `condition`. This condition is a function that will take a reference to
    /// the container id and the border and returns a bool, if true that border
    /// will be removed.
    fn remove_borders(
        &mut self,
        monitor_idx: usize,
        condition: impl Fn(&String, &Border) -> bool,
    ) -> color_eyre::Result<()> {
        let mut to_remove = vec![];
        for (id, border) in self.borders.iter() {
            // if border is on this monitor
            if border.monitor_idx.is_some_and(|idx| idx == monitor_idx)
                // and the condition applies
                && condition(id, border)
                    // and the border is visible (we don't remove hidden borders)
                    && WindowsApi::is_window_visible(border.hwnd)
            {
                // we mark it to be removed
                to_remove.push(id.clone());
            }
        }

        for id in &to_remove {
            self.remove_border(id)?;
        }

        Ok(())
    }

    /// Removes the border with `id` and all its related info from all maps
    fn remove_border(&mut self, id: &str) -> color_eyre::Result<()> {
        if let Some(removed_border) = self.borders.remove(id) {
            self.windows_borders.remove(&removed_border.tracking_hwnd);
            destroy_border(removed_border)?;
        }

        Ok(())
    }

    /// Gets the colour as a `u32` from the `WindowKind`
    fn window_kind_colour(&self, focus_kind: WindowKind) -> u32 {
        self.kind_colours.from_kind(focus_kind)
    }
}

/// IMPORTANT: BEWARE when changing this function. We need to make sure that we don't let the
/// `Box<Border>` be dropped normally. We need to turn the `Box` into the raw pointer and use that
/// pointer to call the `.destroy()` funtion of the border so it closes the window. This way the
/// `Box` is consumed and the pointer is dropped like a normal `Copy` number instead of trying to
/// drop the struct it points to. The actual border is owned by the thread that created the window
/// and once the window closes that thread gets out of its loop, finishes and properly disposes of
/// the border.
fn destroy_border(border: Box<Border>) -> color_eyre::Result<()> {
    let raw_pointer = Box::into_raw(border);
    unsafe {
        (*raw_pointer).destroy()?;
    }
    Ok(())
}

/// Removes the border around window with `tracking_hwnd` if it exists
pub fn delete_border(tracking_hwnd: isize) {
    runtime::send_message(BorderMessage::Delete(tracking_hwnd));
}

/// Destroys all known and unknown borders
pub fn destroy_all_borders() {
    runtime::send_message(BorderMessage::DestroyAll);
}

/// Shows the border around window with `tracking_hwnd` if it exists
pub fn show_border(tracking_hwnd: isize) {
    runtime::send_message(BorderMessage::Show(tracking_hwnd));
}

/// Hides the border around window with `tracking_hwnd` if it exists
pub fn hide_border(tracking_hwnd: isize) {
    runtime::send_message(BorderMessage::Hide(tracking_hwnd));
}

/// Raises the border around window with `tracking_hwnd` if it exists
pub fn raise_border(tracking_hwnd: isize) {
    runtime::send_message(BorderMessage::Raise(tracking_hwnd));
}

/// Lowers the border around window with `tracking_hwnd` if it exists
pub fn lower_border(tracking_hwnd: isize) {
    runtime::send_message(BorderMessage::Lower(tracking_hwnd));
}

/// Sends an `BorderMessage::Update` to the runtime to update all the borders using the optional
/// `hwnd` as the tracking_hwnd that might have triggered the update
pub fn send_notification(hwnd: Option<isize>) {
    runtime::send_message(BorderMessage::Update(hwnd));
}

/// Send a notify message with `event` to the border window with handle `border_hwnd`. It also
/// passes the tracking window `hwnd` as the `LPARAM`.
fn notify_border(border_hwnd: HWND, event: u32, hwnd: isize) {
    unsafe {
        let _ = SendNotifyMessageW(border_hwnd, event, WPARAM(0), LPARAM(hwnd));
    }
}

/// Represents the info from the `WindowManager` that is needed by the `BorderManager`
#[derive(Debug, Default, Clone, PartialEq)]
pub struct WindowManagerInfo {
    pub is_paused: bool,
    pub focused_monitor_idx: usize,
    pub monitors: Ring<Monitor>,
    pub pending_move_op: Option<(usize, usize, isize)>,
    pub floating_window_hwnds: Vec<isize>,
    pub workspace_layer: WorkspaceLayer,
}

impl From<&WindowManager> for WindowManagerInfo {
    fn from(value: &WindowManager) -> Self {
        let is_paused = value.is_paused;
        let focused_monitor_idx = value.focused_monitor_idx();
        let focused_workspace_idx =
            value.monitors.elements()[focused_monitor_idx].focused_workspace_idx();
        let monitors = value.monitors.clone();
        let pending_move_op = *value.pending_move_op;
        let floating_window_hwnds = value.monitors.elements()[focused_monitor_idx].workspaces()
            [focused_workspace_idx]
            .floating_windows()
            .iter()
            .map(|w| w.hwnd)
            .collect::<Vec<_>>();
        let workspace_layer = *value.monitors.elements()[focused_monitor_idx].workspaces()
            [focused_workspace_idx]
            .layer();

        WindowManagerInfo {
            is_paused,
            focused_monitor_idx,
            monitors,
            pending_move_op,
            floating_window_hwnds,
            workspace_layer,
        }
    }
}

impl WindowManager {
    /// Returns the info from the `WindowManager` that is needed by the `BorderManager`
    pub fn to_border_info(&self) -> WindowManagerInfo {
        self.into()
    }
}

#[derive(Debug, Copy, Clone, Display, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ZOrder {
    Top,
    NoTopMost,
    Bottom,
    TopMost,
}

impl From<ZOrder> for isize {
    fn from(val: ZOrder) -> Self {
        match val {
            ZOrder::Top => 0,
            ZOrder::NoTopMost => -2,
            ZOrder::Bottom => 1,
            ZOrder::TopMost => -1,
        }
    }
}
