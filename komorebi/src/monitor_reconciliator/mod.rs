#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::border_manager;
use crate::config_generation::WorkspaceMatchingRule;
use crate::core::Rect;
use crate::monitor;
use crate::monitor::Monitor;
use crate::monitor_reconciliator::hidden::Hidden;
use crate::notify_subscribers;
use crate::runtime;
use crate::windows_api::WinApi;
use crate::Notification;
use crate::NotificationEvent;
use crate::State;
use crate::WindowManager;
use crate::WindowsApi;
use crate::DISPLAY_INDEX_PREFERENCES;
use crate::DUPLICATE_MONITOR_SERIAL_IDS;
use crate::WORKSPACE_MATCHING_RULES;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

pub mod hidden;

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", content = "content")]
pub enum MonitorNotification {
    ResolutionScalingChanged,
    WorkAreaChanged,
    DisplayConnectionChange,
    EnteringSuspendedState,
    ResumingFromSuspendedState,
    SessionLocked,
    SessionUnlocked,
}

impl From<MonitorNotification> for runtime::Control {
    fn from(value: MonitorNotification) -> Self {
        runtime::Control::Monitor(value)
    }
}

/// Responsible for scanning and transmitting all the monitor events to the runtime
#[derive(Debug, Clone)]
pub struct MonitorReconciliator {
    /// Defines if the computer is active or suspended
    pub active: bool,
    /// Holds a cache of disconnected monitors with their id (usually the `serial_number_id` or
    /// `device_id` if the serial is not available)
    pub monitor_cache: HashMap<String, Monitor>,
    /// Holds the hidden window hwnd responsible to get and transmit any monitor related event. If
    /// it fails to spawn this window this will be `None`
    _hidden: Option<Hidden>,
}

impl Default for MonitorReconciliator {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorReconciliator {
    /// Checks if there are any previous hidden windows, if there are it closes them. Then it
    /// spawns a new hidden window to relay the monitor events and returns a new
    /// `MonitorReconciliator`.
    pub fn new() -> Self {
        let mut remaining_hidden_hwnds: Vec<Hidden> = vec![];

        if let Err(error) = WindowsApi::enum_windows(
            Some(hidden::hidden_hwnds),
            &mut remaining_hidden_hwnds as *mut Vec<Hidden> as isize,
        ) {
            tracing::error!("failed to enum hidden windows: {}", error);
        }

        if !remaining_hidden_hwnds.is_empty() {
            tracing::info!(
                "purging existing hidden windows: {:?}",
                remaining_hidden_hwnds
            );

            for hidden in remaining_hidden_hwnds {
                let _ = WindowsApi::close_window(hidden.hwnd);
            }
        }

        let hidden = match Hidden::create("komorebi-hidden") {
            Ok(hidden) => Some(hidden),
            Err(error) => {
                tracing::error!(
                    "failed to create hidden window to listen for monitor-related events: {}",
                    error
                );
                None
            }
        };

        tracing::info!("created hidden window to listen for monitor-related events");

        Self {
            active: true,
            monitor_cache: HashMap::new(),
            _hidden: hidden,
        }
    }

    pub fn insert_in_monitor_cache(&mut self, serial_or_device_id: String, monitor: Monitor) {
        let dip = DISPLAY_INDEX_PREFERENCES.read();
        let mut dip_ids = dip.values();
        let preferred_id = if dip_ids.any(|id| id == monitor.device_id()) {
            monitor.device_id().clone()
        } else if dip_ids.any(|id| Some(id) == monitor.serial_number_id().as_ref()) {
            monitor.serial_number_id().clone().unwrap_or_default()
        } else {
            serial_or_device_id.to_string()
        };
        self.monitor_cache.insert(preferred_id, monitor);
    }
}

/// Forwards a monitor event to the runtime
pub fn send_notification(notification: MonitorNotification) {
    runtime::send_message(notification);
}

pub fn attached_display_devices<F>(display_provider: F) -> color_eyre::Result<Vec<Monitor>>
where
    F: Fn() -> Vec<win32_display_data::Device>,
{
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

    Ok(all_displays
        .into_iter()
        .map(|mut display| {
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

            monitor::new(
                display.hmonitor,
                display.size.into(),
                display.work_area_size.into(),
                name,
                device,
                device_id,
                display.serial_number_id,
            )
        })
        .collect::<Vec<_>>())
}

impl WindowManager {
    pub fn handle_monitor_notification<F>(
        &mut self,
        notification: MonitorNotification,
        display_provider: F,
    ) -> color_eyre::Result<()>
    where
        F: Fn() -> Vec<win32_display_data::Device> + Clone,
    {
        if !self.monitor_reconciliator.active
            && matches!(
                notification,
                MonitorNotification::ResumingFromSuspendedState
                    | MonitorNotification::SessionUnlocked
            )
        {
            tracing::debug!(
                "reactivating reconciliator - system has resumed from suspended state or session has been unlocked"
            );

            self.monitor_reconciliator.active = true;
            border_manager::send_notification(None);
        }

        let initial_state = State::from(&*self);

        match notification {
            MonitorNotification::EnteringSuspendedState | MonitorNotification::SessionLocked => {
                tracing::debug!(
                    "deactivating reconciliator until system resumes from suspended state or session is unlocked"
                );
                self.monitor_reconciliator.active = false;
            }
            MonitorNotification::WorkAreaChanged => {
                tracing::debug!("handling work area changed notification");
                let mut to_update = vec![];
                for (monitor_idx, monitor) in self.monitors_mut().iter_mut().enumerate() {
                    let mut should_update = false;

                    // Update work areas as necessary
                    if let Ok(reference) = WindowsApi::monitor(monitor.id()) {
                        if reference.work_area_size() != monitor.work_area_size() {
                            monitor.set_work_area_size(Rect {
                                left: reference.work_area_size().left,
                                top: reference.work_area_size().top,
                                right: reference.work_area_size().right,
                                bottom: reference.work_area_size().bottom,
                            });

                            should_update = true;
                            to_update.push((monitor_idx, monitor.focused_workspace_idx()));
                        }
                    }

                    if !should_update {
                        tracing::debug!(
                            "work areas match, reconciliation not required for {}",
                            monitor.device_id()
                        );
                    }
                }

                let should_update_borders = !to_update.is_empty();
                for (m_idx, ws_idx) in to_update {
                    self.update_workspace_globals(m_idx, ws_idx);
                    if let Some(monitor) = self.monitors_mut().get_mut(m_idx) {
                        tracing::info!("updated work area for {}", monitor.device_id());
                        monitor.update_focused_workspace()?;
                    }
                }
                if should_update_borders {
                    border_manager::send_notification(None);
                }
            }
            MonitorNotification::ResolutionScalingChanged => {
                tracing::debug!("handling resolution/scaling changed notification");
                let mut to_update = vec![];
                for (monitor_idx, monitor) in self.monitors_mut().iter_mut().enumerate() {
                    let mut should_update = false;

                    // Update sizes and work areas as necessary
                    if let Ok(reference) = WindowsApi::monitor(monitor.id()) {
                        if reference.work_area_size() != monitor.work_area_size() {
                            monitor.set_work_area_size(Rect {
                                left: reference.work_area_size().left,
                                top: reference.work_area_size().top,
                                right: reference.work_area_size().right,
                                bottom: reference.work_area_size().bottom,
                            });

                            should_update = true;
                            to_update.push((monitor_idx, monitor.focused_workspace_idx()));
                        }

                        if reference.size() != monitor.size() {
                            monitor.set_size(Rect {
                                left: reference.size().left,
                                top: reference.size().top,
                                right: reference.size().right,
                                bottom: reference.size().bottom,
                            });

                            should_update = true;
                        }
                    }

                    if !should_update {
                        tracing::debug!(
                            "resolutions match, reconciliation not required for {}",
                            monitor.device_id()
                        );
                    }
                }

                let should_update_borders = !to_update.is_empty();
                for (m_idx, ws_idx) in to_update {
                    self.update_workspace_globals(m_idx, ws_idx);
                    if let Some(monitor) = self.monitors_mut().get_mut(m_idx) {
                        tracing::info!(
                            "updated monitor resolution/scaling for {}",
                            monitor.device_id()
                        );
                        monitor.update_focused_workspace()?;
                    }
                }
                if should_update_borders {
                    border_manager::send_notification(None);
                }
            }
            // this is handled above if the reconciliator is paused but we should still check if
            // there were any changes to the connected monitors while the system was
            // suspended/locked.
            MonitorNotification::ResumingFromSuspendedState
            | MonitorNotification::SessionUnlocked
            | MonitorNotification::DisplayConnectionChange => {
                tracing::debug!("handling display connection change notification");
                let initial_monitor_count = self.monitors().len();

                // Get the currently attached display devices
                let attached_devices = attached_display_devices(display_provider.clone())?;

                // Make sure that in our state any attached displays have the latest Win32 data
                for monitor in self.monitors_mut() {
                    for attached in &attached_devices {
                        let serial_number_ids_match = if let (Some(attached_snid), Some(m_snid)) =
                            (attached.serial_number_id(), monitor.serial_number_id())
                        {
                            attached_snid.eq(m_snid)
                        } else {
                            false
                        };

                        if serial_number_ids_match || attached.device_id().eq(monitor.device_id()) {
                            monitor.set_id(attached.id());
                            monitor.set_device(attached.device().clone());
                            monitor.set_device_id(attached.device_id().clone());
                            monitor.set_serial_number_id(attached.serial_number_id().clone());
                            monitor.set_name(attached.name().clone());
                            monitor.set_size(*attached.size());
                            monitor.set_work_area_size(*attached.work_area_size());
                        }
                    }
                }

                if initial_monitor_count == attached_devices.len() {
                    tracing::debug!("monitor counts match, reconciliation not required");
                    return Ok(());
                }

                if attached_devices.is_empty() {
                    tracing::debug!(
                        "no devices found, skipping reconciliation to avoid breaking state"
                    );
                    return Ok(());
                }

                if initial_monitor_count > attached_devices.len() {
                    tracing::info!(
                        "monitor count mismatch ({initial_monitor_count} vs {}), removing disconnected monitors",
                        attached_devices.len()
                    );

                    // Windows to remove from `known_hwnds`
                    let mut windows_to_remove = Vec::new();

                    // Collect the ids in our state which aren't in the current attached display ids
                    // These are monitors that have been removed
                    let mut newly_removed_displays = vec![];

                    let mut to_cache = None;

                    for (m_idx, m) in self.monitors().iter().enumerate() {
                        if !attached_devices.iter().any(|attached| {
                            attached.serial_number_id().eq(m.serial_number_id())
                                || attached.device_id().eq(m.device_id())
                        }) {
                            let id = m
                                .serial_number_id()
                                .as_ref()
                                .map_or(m.device_id().clone(), |sn| sn.clone());

                            newly_removed_displays.push(id.clone());

                            let focused_workspace_idx = m.focused_workspace_idx();

                            for (idx, workspace) in m.workspaces().iter().enumerate() {
                                let is_focused_workspace = idx == focused_workspace_idx;
                                let focused_container_idx = workspace.focused_container_idx();
                                for (c_idx, container) in workspace.containers().iter().enumerate()
                                {
                                    let focused_window_idx = container.focused_window_idx();
                                    for (w_idx, window) in container.windows().iter().enumerate() {
                                        windows_to_remove.push(window.hwnd);
                                        if is_focused_workspace
                                            && c_idx == focused_container_idx
                                            && w_idx == focused_window_idx
                                        {
                                            // Minimize the focused window since Windows might try
                                            // to move it to another monitor if it was focused.
                                            if window.is_focused() {
                                                window.minimize();
                                            }
                                        }
                                    }
                                }

                                if let Some(maximized) = workspace.maximized_window() {
                                    windows_to_remove.push(maximized.hwnd);
                                    // Minimize the focused window since Windows might try
                                    // to move it to another monitor if it was focused.
                                    if maximized.is_focused() {
                                        maximized.minimize();
                                    }
                                }

                                if let Some(container) = workspace.monocle_container() {
                                    for window in container.windows() {
                                        windows_to_remove.push(window.hwnd);
                                    }
                                    if let Some(window) = container.focused_window() {
                                        // Minimize the focused window since Windows might try
                                        // to move it to another monitor if it was focused.
                                        if window.is_focused() {
                                            window.minimize();
                                        }
                                    }
                                }

                                for window in workspace.floating_windows() {
                                    windows_to_remove.push(window.hwnd);
                                    // Minimize the focused window since Windows might try
                                    // to move it to another monitor if it was focused.
                                    if window.is_focused() {
                                        window.minimize();
                                    }
                                }
                            }

                            // Remove any workspace_rules for this specific monitor
                            let mut workspace_rules = WORKSPACE_MATCHING_RULES.lock();
                            let mut rules_to_remove = Vec::new();
                            for (i, rule) in workspace_rules.iter().enumerate().rev() {
                                if rule.monitor_index == m_idx {
                                    rules_to_remove.push(i);
                                }
                            }
                            for i in rules_to_remove {
                                workspace_rules.remove(i);
                            }

                            // Let's add their state to the cache for later, make sure to use what
                            // the user set as preference as the id.
                            let dip = DISPLAY_INDEX_PREFERENCES.read();
                            let mut dip_ids = dip.values();
                            let preferred_id = if dip_ids.any(|id| id == m.device_id()) {
                                m.device_id().clone()
                            } else if dip_ids.any(|id| Some(id) == m.serial_number_id().as_ref()) {
                                m.serial_number_id().clone().unwrap_or_default()
                            } else {
                                id
                            };
                            to_cache = Some((preferred_id, m.clone()));
                        }
                    }

                    // Let's add their state to the cache for later
                    if let Some((id, cache)) = to_cache {
                        self.monitor_reconciliator.monitor_cache.insert(id, cache);
                    }

                    // Update known_hwnds
                    self.known_hwnds
                        .retain(|i, _| !windows_to_remove.contains(i));

                    if !newly_removed_displays.is_empty() {
                        // After we have cached them, remove them from our state
                        self.monitors_mut().retain(|m| {
                            !newly_removed_displays.iter().any(|id| {
                                m.serial_number_id().as_ref().is_some_and(|sn| sn == id)
                                    || m.device_id() == id
                            })
                        });
                    }

                    let post_removal_monitor_count = self.monitors().len();
                    let focused_monitor_idx = self.focused_monitor_idx();
                    if focused_monitor_idx >= post_removal_monitor_count {
                        self.focus_monitor(0)?;
                    }

                    for monitor in self.monitors_mut() {
                        // If we have lost a monitor, update everything to filter out any jank
                        if initial_monitor_count != post_removal_monitor_count {
                            monitor.update_focused_workspace()?;
                        }
                    }
                }

                let post_removal_monitor_count = self.monitors().len();

                // This is the list of device ids after we have removed detached displays. We can
                // keep this with just the device_ids without the serial numbers since this is used
                // only to check which one is the newly added monitor below if there is a new
                // monitor. Everything done after with said new monitor will again consider both
                // serial number and device ids.
                let post_removal_device_ids = self
                    .monitors()
                    .iter()
                    .map(Monitor::device_id)
                    .cloned()
                    .collect::<Vec<_>>();

                // Check for and add any new monitors that may have been plugged in
                // Monitor and display index preferences get applied in this function
                WindowsApi::load_monitor_information(self, display_provider)?;

                let post_addition_monitor_count = self.monitors().len();

                if post_addition_monitor_count > post_removal_monitor_count {
                    tracing::info!(
                        "monitor count mismatch ({post_removal_monitor_count} vs {post_addition_monitor_count}), adding connected monitors",
                    );

                    let known_hwnds = self.known_hwnds.clone();
                    let mouse_follows_focus = self.mouse_follows_focus;
                    let focused_monitor_idx = self.focused_monitor_idx();
                    let focused_workspace_idx = self.focused_workspace_idx()?;

                    // Update the globals of the focused workspaces on all monitors so that the new
                    // monitors' workspaces have the updated values
                    self.update_all_workspace_globals();

                    // Get the monitor cache temporarily out of `self.monitor_reconciliator` so we can edit it
                    // while also editing `self`.
                    // NOTE: We need to make sure we don't exit early on error (don't use `?`) between this call
                    // and the one that moves the monitor cache back to `self.monitor_reconciliator`, so that we
                    // don't lose the cache, that is why we deny the `question_mark_used` below for the entire
                    // `for` loop on `self.monitors_mut()`.
                    let mut monitor_cache: HashMap<String, Monitor> =
                        self.monitor_reconciliator.monitor_cache.drain().collect();

                    // Look in the updated state for new monitors
                    #[deny(clippy::question_mark_used)]
                    for (i, m) in self.monitors_mut().iter_mut().enumerate() {
                        let device_id = m.device_id();
                        // We identify a new monitor when we encounter a new device id
                        if !post_removal_device_ids.contains(device_id) {
                            let mut cache_hit = false;
                            let mut cached_id = String::new();
                            // Check if that device id exists in the cache for this session
                            if let Some((id, cached)) = monitor_cache.get_key_value(device_id).or(m
                                .serial_number_id()
                                .as_ref()
                                .and_then(|sn| monitor_cache.get_key_value(sn)))
                            {
                                cache_hit = true;
                                cached_id = id.clone();

                                tracing::info!("found monitor and workspace configuration for {id} in the monitor cache, applying");

                                // If it does, update the cached monitor info with the new one and
                                // load the cached monitor removing any window that has since been
                                // closed or moved to another workspace
                                *m = Monitor {
                                    // Data that should be the one just read from `win32-display-data`
                                    id: m.id,
                                    name: m.name.clone(),
                                    device: m.device.clone(),
                                    device_id: m.device_id.clone(),
                                    serial_number_id: m.serial_number_id.clone(),
                                    size: m.size,
                                    work_area_size: m.work_area_size,

                                    // The rest should come from the cached monitor
                                    work_area_offset: cached.work_area_offset,
                                    window_based_work_area_offset: cached
                                        .window_based_work_area_offset,
                                    window_based_work_area_offset_limit: cached
                                        .window_based_work_area_offset_limit,
                                    workspaces: cached.workspaces.clone(),
                                    last_focused_workspace: cached.last_focused_workspace,
                                    workspace_names: cached.workspace_names.clone(),
                                    container_padding: cached.container_padding,
                                    workspace_padding: cached.workspace_padding,
                                    wallpaper: cached.wallpaper.clone(),
                                    floating_layer_behaviour: cached.floating_layer_behaviour,
                                };

                                let focused_workspace_idx = m.focused_workspace_idx();

                                for (j, workspace) in m.workspaces_mut().iter_mut().enumerate() {
                                    // If this is the focused workspace we need to show (restore) all
                                    // windows that were visible since they were probably minimized by
                                    // Windows.
                                    let is_focused_workspace = j == focused_workspace_idx;
                                    let focused_container_idx = workspace.focused_container_idx();

                                    let mut empty_containers = Vec::new();
                                    for (idx, container) in
                                        workspace.containers_mut().iter_mut().enumerate()
                                    {
                                        container.windows_mut().retain(|window| {
                                            window.exe().is_ok()
                                                && !known_hwnds.contains_key(&window.hwnd)
                                        });

                                        if container.windows().is_empty() {
                                            empty_containers.push(idx);
                                        }

                                        if is_focused_workspace {
                                            if let Some(window) = container.focused_window() {
                                                tracing::debug!(
                                                    "restoring window: {}",
                                                    window.hwnd
                                                );
                                                WindowsApi::restore_window(window.hwnd);
                                            } else {
                                                // If the focused window was moved or removed by
                                                // the user after the disconnect then focus the
                                                // first window and show that one
                                                container.focus_window(0);

                                                if let Some(window) = container.focused_window() {
                                                    WindowsApi::restore_window(window.hwnd);
                                                }
                                            }
                                        }
                                    }

                                    // Remove empty containers
                                    for empty_idx in empty_containers {
                                        if empty_idx == focused_container_idx {
                                            workspace.remove_container(empty_idx);
                                        } else {
                                            workspace.remove_container_by_idx(empty_idx);
                                        }
                                    }

                                    if let Some(window) = workspace.maximized_window() {
                                        if window.exe().is_err()
                                            || known_hwnds.contains_key(&window.hwnd)
                                        {
                                            workspace.set_maximized_window(None);
                                        } else if is_focused_workspace {
                                            WindowsApi::restore_window(window.hwnd);
                                        }
                                    }

                                    if let Some(container) = workspace.monocle_container_mut() {
                                        container.windows_mut().retain(|window| {
                                            window.exe().is_ok()
                                                && !known_hwnds.contains_key(&window.hwnd)
                                        });

                                        if container.windows().is_empty() {
                                            workspace.set_monocle_container(None);
                                        } else if is_focused_workspace {
                                            if let Some(window) = container.focused_window() {
                                                WindowsApi::restore_window(window.hwnd);
                                            } else {
                                                // If the focused window was moved or removed by
                                                // the user after the disconnect then focus the
                                                // first window and show that one
                                                container.focus_window(0);

                                                if let Some(window) = container.focused_window() {
                                                    WindowsApi::restore_window(window.hwnd);
                                                }
                                            }
                                        }
                                    }

                                    workspace.floating_windows_mut().retain(|window| {
                                        window.exe().is_ok()
                                            && !known_hwnds.contains_key(&window.hwnd)
                                    });

                                    if is_focused_workspace {
                                        for window in workspace.floating_windows() {
                                            WindowsApi::restore_window(window.hwnd);
                                        }
                                    }

                                    // Apply workspace rules
                                    let mut workspace_matching_rules =
                                        WORKSPACE_MATCHING_RULES.lock();
                                    if let Some(rules) = workspace
                                        .workspace_config()
                                        .as_ref()
                                        .and_then(|c| c.workspace_rules.as_ref())
                                    {
                                        for r in rules {
                                            workspace_matching_rules.push(WorkspaceMatchingRule {
                                                monitor_index: i,
                                                workspace_index: j,
                                                matching_rule: r.clone(),
                                                initial_only: false,
                                            });
                                        }
                                    }

                                    if let Some(rules) = workspace
                                        .workspace_config()
                                        .as_ref()
                                        .and_then(|c| c.initial_workspace_rules.as_ref())
                                    {
                                        for r in rules {
                                            workspace_matching_rules.push(WorkspaceMatchingRule {
                                                monitor_index: i,
                                                workspace_index: j,
                                                matching_rule: r.clone(),
                                                initial_only: true,
                                            });
                                        }
                                    }
                                }

                                // Restore windows from new monitor and update the focused
                                // workspace
                                let _ = m.load_focused_workspace(mouse_follows_focus);
                                let _ = m.update_focused_workspace();
                            }

                            // Entries in the cache should only be used once; remove the entry there was a cache hit
                            if cache_hit && !cached_id.is_empty() {
                                monitor_cache.remove(&cached_id);
                            }
                        }
                    }

                    // Finnally move the `monitor_cache` back to the `monitor_reconciliator` again
                    self.monitor_reconciliator.monitor_cache = monitor_cache;

                    // Refocus the previously focused monitor since the code above might
                    // steal the focus away.
                    self.focus_monitor(focused_monitor_idx)?;
                    self.focus_workspace(focused_workspace_idx)?;
                }

                let final_count = self.monitors().len();

                if post_removal_monitor_count != final_count {
                    self.retile_all(true)?;
                    // Second retile to fix DPI/resolution related jank
                    self.retile_all(true)?;
                    // Border updates to fix DPI/resolution related jank
                    border_manager::send_notification(None);
                }
            }
        }

        notify_subscribers(
            Notification {
                event: NotificationEvent::Monitor(notification),
                state: self.as_ref().into(),
            },
            initial_state.has_been_modified(self),
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::tests::no_display_provider;
    use runtime::tests::setup_window_manager;
    use windows::Win32::Devices::Display::DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY;
    // NOTE: Using RECT instead of RECT since I get a mismatched type error. Can be updated if
    // needed.
    use windows::Win32::Foundation::RECT;

    // Creating a Mock Display Provider
    #[derive(Clone)]
    struct MockDevice {
        hmonitor: isize,
        device_path: String,
        device_name: String,
        device_description: String,
        serial_number_id: Option<String>,
        size: RECT,
        work_area_size: RECT,
        device_key: String,
        output_technology: Option<DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY>,
    }

    impl From<MockDevice> for win32_display_data::Device {
        fn from(mock: MockDevice) -> Self {
            win32_display_data::Device {
                hmonitor: mock.hmonitor,
                device_path: mock.device_path,
                device_name: mock.device_name,
                device_description: mock.device_description,
                serial_number_id: mock.serial_number_id,
                size: mock.size,
                work_area_size: mock.work_area_size,
                device_key: mock.device_key,
                output_technology: mock.output_technology,
            }
        }
    }

    #[test]
    #[ignore]
    fn test_send_notification() {
        // Create a WindowManager instance for testing
        let (_wm, _test_context, mut runtime) = setup_window_manager(no_display_provider);

        // Test sending a notification
        send_notification(MonitorNotification::ResolutionScalingChanged);

        // Get the messages
        let messages = runtime.get_messages();

        // Check that there is 1 message
        assert_eq!(messages.len(), 1);

        // Check if we received the notification and if it matches what we sent
        assert!(matches!(
            messages[0],
            runtime::Message::Control(runtime::Control::Monitor(
                MonitorNotification::ResolutionScalingChanged
            ))
        ));
    }

    #[test]
    #[ignore]
    fn test_channel_bounded_capacity() {
        // Create a WindowManager instance for testing
        let (_wm, _test_context, mut runtime) = setup_window_manager(no_display_provider);

        assert!(runtime.get_messages().is_empty());

        // Fill the channel to its capacity (50 messages)
        for _ in 0..50 {
            send_notification(MonitorNotification::WorkAreaChanged);
        }

        // Attempt to send another message (should be dropped)
        send_notification(MonitorNotification::ResolutionScalingChanged);

        // Get the messages
        let messages = runtime.get_messages();

        // Check that there are 50 message
        assert_eq!(messages.len(), 50);

        // Verify the channel contains only the first 50 messages
        for (i, message) in messages.into_iter().enumerate() {
            println!("message {}: {}", i, &message);
            assert!(
                matches!(
                    message,
                    runtime::Message::Control(runtime::Control::Monitor(
                        MonitorNotification::WorkAreaChanged
                    ))
                ),
                "Unexpected notification in the channel: {}",
                message
            );
        }

        // Verify that no additional messages are in the channel
        assert!(
            runtime.get_messages().is_empty(),
            "Channel should be empty after consuming all messages"
        );
    }

    #[test]
    fn test_insert_in_monitor_cache() {
        // Create a WindowManager instance for testing
        let (mut wm, _test_context, _runtime) = setup_window_manager(no_display_provider);

        let m = monitor::new(
            0,
            Rect::default(),
            Rect::default(),
            "Test Monitor".to_string(),
            "Test Device".to_string(),
            "Test Device ID".to_string(),
            Some("TestMonitorID".to_string()),
        );

        // Insert the monitor into the cache
        wm.monitor_reconciliator
            .insert_in_monitor_cache("TestMonitorID".into(), m.clone());

        // Retrieve the monitor from the cache
        let retrieved_monitor = wm.monitor_reconciliator.monitor_cache.get("TestMonitorID");

        // Check that the monitor was inserted correctly and matches the expected value
        assert_eq!(retrieved_monitor, Some(&m));
    }

    #[test]
    fn test_insert_two_monitors_cache() {
        // Create a WindowManager instance for testing
        let (mut wm, _test_context, _runtime) = setup_window_manager(no_display_provider);

        let m1 = monitor::new(
            0,
            Rect::default(),
            Rect::default(),
            "Test Monitor".to_string(),
            "Test Device".to_string(),
            "Test Device ID".to_string(),
            Some("TestMonitorID".to_string()),
        );

        let m2 = monitor::new(
            0,
            Rect::default(),
            Rect::default(),
            "Test Monitor 2".to_string(),
            "Test Device 2".to_string(),
            "Test Device ID 2".to_string(),
            Some("TestMonitorID2".to_string()),
        );

        // Insert the first monitor into the cache
        wm.monitor_reconciliator
            .insert_in_monitor_cache("TestMonitorID".into(), m1.clone());

        // Insert the second monitor into the cache
        wm.monitor_reconciliator
            .insert_in_monitor_cache("TestMonitorID2".into(), m2.clone());

        // Retrieve the cache to check if the first and second monitors are present
        let cache = wm.monitor_reconciliator.monitor_cache;

        // Check if Monitor 1 was found in the cache
        assert_eq!(
            cache.get("TestMonitorID"),
            Some(&m1),
            "Monitor cache should contain monitor 1"
        );

        // Check if Monitor 2 was found in the cache
        assert_eq!(
            cache.get("TestMonitorID2"),
            Some(&m2),
            "Monitor cache should contain monitor 2"
        );
    }

    #[test]
    fn test_attached_display_devices() {
        // Define mock display data
        let mock_monitor = MockDevice {
            hmonitor: 1,
            device_path: String::from(
                "\\\\?\\DISPLAY#ABC123#4&123456&0&UID0#{saucepackets-4321-5678-2468-abc123456789}",
            ),
            device_name: String::from("\\\\.\\DISPLAY1"),
            device_description: String::from("Display description"),
            serial_number_id: Some(String::from("SaucePackets123")),
            device_key: String::from("Mock Key"),
            size: RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work_area_size: RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            output_technology: Some(DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY(0)),
        };

        // Create a closure to simulate the display provider
        let display_provider = || vec![win32_display_data::Device::from(mock_monitor.clone())];

        // Should contain the mock monitor
        let result = attached_display_devices(display_provider).ok();
        if let Some(monitors) = result {
            // Check Number of monitors
            assert_eq!(monitors.len(), 1, "Expected one monitor");

            // hmonitor
            assert_eq!(monitors[0].id(), 1);

            // device name
            assert_eq!(monitors[0].name(), &String::from("DISPLAY1"));

            // Device
            assert_eq!(monitors[0].device(), &String::from("ABC123"));

            // Device ID
            assert_eq!(
                monitors[0].device_id(),
                &String::from("ABC123-4&123456&0&UID0")
            );

            // Check monitor serial number id
            assert_eq!(
                monitors[0].serial_number_id,
                Some(String::from("SaucePackets123")),
            );
        } else {
            panic!("No monitors found");
        }
    }

    #[test]
    fn test_monitor_disconnect_to_cache() {
        // Define mock display data
        let mock_devices = {
            let mock_monitor = MockDevice {
                hmonitor: 1,
                device_path: String::from(
                    "\\\\?\\DISPLAY#ABC123#4&123456&0&UID0#{saucepackets-4321-5678-2468-abc123456789}",
                ),
                device_name: String::from("\\\\.\\DISPLAY1"),
                device_description: String::from("Display description"),
                serial_number_id: Some(String::from("SaucePackets123")),
                device_key: String::from("Mock Key"),
                size: RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                work_area_size: RECT {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                output_technology: Some(DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY(0)),
            };

            let mock_monitor2 = MockDevice {
                hmonitor: 2,
                device_path: String::from(
                    "\\\\?\\DISPLAY#ABC123#4&123456&0&UID1#{saucepackets2-4321-5678-2468-abc123456789}",
                ),
                device_name: String::from("\\\\.\\DISPLAY2"),
                device_description: String::from("Display description2"),
                serial_number_id: Some(String::from("SaucePackets1234")),
                device_key: String::from("Mock Key2"),
                size: RECT {
                    left: 1920,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                work_area_size: RECT {
                    left: 1920,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
                output_technology: Some(DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY(0)),
            };

            vec![mock_monitor, mock_monitor2]
        };
        let mock_devices_2 =
            std::sync::LazyLock::new(|| parking_lot::Mutex::new(mock_devices.clone()));

        // Create a closure to simulate the display provider with 2 monitors
        let display_provider = || {
            mock_devices_2
                .lock()
                .clone()
                .into_iter()
                .map(win32_display_data::Device::from)
                .collect()
        };

        // Create a WindowManager instance for testing
        let (mut wm, _test_context, runtime) = setup_window_manager(display_provider);

        // Should contain the mock monitors
        assert_eq!(wm.monitors().len(), 2, "Expected two monitors");

        // hmonitor
        assert_eq!(wm.monitors()[0].id(), 1);

        // device name
        assert_eq!(wm.monitors()[0].name(), &String::from("DISPLAY1"));

        // Device
        assert_eq!(wm.monitors()[0].device(), &String::from("ABC123"));

        // Device ID
        assert_eq!(
            wm.monitors()[0].device_id(),
            &String::from("ABC123-4&123456&0&UID0")
        );

        // Check monitor serial number id
        assert_eq!(
            wm.monitors()[0].serial_number_id,
            Some(String::from("SaucePackets123")),
        );

        // hmonitor
        assert_eq!(wm.monitors()[1].id(), 2);

        // device name
        assert_eq!(wm.monitors()[1].name(), &String::from("DISPLAY2"));

        // Device
        assert_eq!(wm.monitors()[1].device(), &String::from("ABC123"));

        // Device ID
        assert_eq!(
            wm.monitors()[1].device_id(),
            &String::from("ABC123-4&123456&0&UID1")
        );

        // Check monitor serial number id
        assert_eq!(
            wm.monitors()[1].serial_number_id,
            Some(String::from("SaucePackets1234")),
        );

        // Change the display provider to now have only one monitor
        {
            let mut md = mock_devices_2.lock();
            *md = mock_devices.clone().into_iter().take(1).collect();
        }

        send_notification(MonitorNotification::DisplayConnectionChange);

        wm.test_run(runtime);

        // Should contain only 1 mock monitor
        assert_eq!(wm.monitors().len(), 1, "Expected two monitors");

        // hmonitor
        assert_eq!(wm.monitors()[0].id(), 1);

        // device name
        assert_eq!(wm.monitors()[0].name(), &String::from("DISPLAY1"));

        // Device
        assert_eq!(wm.monitors()[0].device(), &String::from("ABC123"));

        // Device ID
        assert_eq!(
            wm.monitors()[0].device_id(),
            &String::from("ABC123-4&123456&0&UID0")
        );

        // Check monitor serial number id
        assert_eq!(
            wm.monitors()[0].serial_number_id,
            Some(String::from("SaucePackets123")),
        );
    }
}
