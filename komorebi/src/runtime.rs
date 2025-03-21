use crate::border_manager;
use crate::listen_for_commands;
use crate::listen_for_commands_tcp;
use crate::monitor_reconciliator;
use crate::reaper;
use crate::stackbar_manager;
use crate::transparency_manager;
use crate::winevent_listener;
use crate::RuleDebug;
use crate::SocketMessage;
use crate::Window;
use crate::WindowManager;
use crate::WindowManagerEvent;
use crate::WindowsApi;

use std::net::TcpStream;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::OnceLock;
use std::time::Duration;

use crossbeam_channel::Receiver;
use crossbeam_channel::Sender;
use parking_lot::RwLock;
use uds_windows::UnixStream;

/// Handles windows events
static EVENTS_CHANNEL: OnceLock<(Sender<Message>, Receiver<Message>)> = OnceLock::new();

/// Handles commands
static COMMANDS_CHANNEL: OnceLock<(Sender<Message>, Receiver<Message>)> = OnceLock::new();

/// Handles all the control actions requested by the managers to perform something on the `WindowManager`
static CONTROL_CHANNEL: OnceLock<(Sender<Message>, Receiver<Message>)> = OnceLock::new();

static RUNTIME_STOPPED: LazyLock<Arc<RwLock<bool>>> =
    LazyLock::new(|| Arc::new(RwLock::new(false)));

fn events_channel() -> Option<&'static (Sender<Message>, Receiver<Message>)> {
    EVENTS_CHANNEL.get()
}

fn commands_channel() -> Option<&'static (Sender<Message>, Receiver<Message>)> {
    COMMANDS_CHANNEL.get()
}

fn control_channel() -> Option<&'static (Sender<Message>, Receiver<Message>)> {
    CONTROL_CHANNEL.get()
}

fn event_tx() -> Option<&'static Sender<Message>> {
    events_channel().map(|(s, _)| s)
}

fn command_tx() -> Option<&'static Sender<Message>> {
    commands_channel().map(|(s, _)| s)
}

fn control_tx() -> Option<&'static Sender<Message>> {
    control_channel().map(|(s, _)| s)
}

pub fn send_message(message: impl Into<Message>) {
    if *RUNTIME_STOPPED.read() {
        tracing::debug!("runtime is stopped; dropping message");
        return;
    }

    let message = message.into();
    let tx = match message {
        Message::Event(_) => event_tx(),
        Message::Command(_, _) | Message::TcpCommand(_, _) => command_tx(),
        Message::Control(_) => control_tx(),
    };
    if let Some(sender) = tx {
        if sender.try_send(message).is_err() {
            tracing::warn!("channel is full; dropping message");
        }
    } else {
        tracing::debug!("runtime isn't initialized yet; dropping message");
    }
}

pub fn batch_messages(messages: Vec<impl Into<Message>>) {
    if *RUNTIME_STOPPED.read() {
        tracing::debug!("runtime is stopped; dropping messages");
        return;
    }

    for message in messages {
        let message = message.into();
        let tx = match message {
            Message::Event(_) => event_tx(),
            Message::Command(_, _) | Message::TcpCommand(_, _) => command_tx(),
            Message::Control(_) => control_tx(),
        };
        if let Some(sender) = tx {
            if sender.try_send(message).is_err() {
                tracing::warn!("channel is full; dropping message");
            }
        } else {
            tracing::debug!("runtime isn't initialized yet; dropping message");
        }
    }
}

#[derive(Debug)]
pub enum Message {
    Event(WindowManagerEvent),
    Command(Vec<SocketMessage>, UnixStream),
    TcpCommand(SocketMessage, TcpStream),
    Control(Control),
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::Event(window_manager_event) => write!(f, "{}", window_manager_event),
            Message::Command(messages, _) => write!(f, "{:?}", messages),
            Message::TcpCommand(message, _) => write!(f, "{}", message),
            Message::Control(control) => write!(f, "{}", control),
        }
    }
}

#[derive(Debug)]
pub enum Control {
    Border(border_manager::BorderMessage),
    Reaper(reaper::ReaperNotification),
    Monitor(monitor_reconciliator::MonitorNotification),
    Stackbar(stackbar_manager::StackbarMessage),
    Transparency(transparency_manager::TransparencyMessage),
    WindowWithBorder(WindowWithBorderAction),
}

impl std::fmt::Display for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Control::Border(border_message) => write!(f, "Border({:?})", border_message),
            Control::Reaper(reaper_notification) => {
                write!(f, "Reaper({:?})", reaper_notification.0)
            }
            Control::Monitor(monitor_notification) => {
                write!(f, "Monitor({:?})", monitor_notification)
            }
            Control::Stackbar(stackbar_message) => write!(f, "Stackbar({:?})", stackbar_message),
            Control::Transparency(transparency_message) => {
                write!(f, "Transparency({:?})", transparency_message)
            }
            Control::WindowWithBorder(action) => match action {
                WindowWithBorderAction::Show(hwnd) => write!(f, "ShowWindowWithBorder({})", hwnd),
                WindowWithBorderAction::Hide(hwnd) => write!(f, "HideWindowWithBorder({})", hwnd),
                WindowWithBorderAction::Raise(hwnd) => write!(f, "RaiseWindowWithBorder({})", hwnd),
                WindowWithBorderAction::Lower(hwnd) => write!(f, "LowerWindowWithBorder({})", hwnd),
            },
        }
    }
}

#[derive(Debug)]
pub enum WindowWithBorderAction {
    Show(isize),
    Hide(isize),
    Raise(isize),
    Lower(isize),
}

impl From<WindowManagerEvent> for Message {
    fn from(value: WindowManagerEvent) -> Self {
        Message::Event(value)
    }
}

impl From<(Vec<SocketMessage>, UnixStream)> for Message {
    fn from(value: (Vec<SocketMessage>, UnixStream)) -> Self {
        Message::Command(value.0, value.1)
    }
}

impl From<(SocketMessage, TcpStream)> for Message {
    fn from(value: (SocketMessage, TcpStream)) -> Self {
        Message::TcpCommand(value.0, value.1)
    }
}

impl<T: Into<Control>> From<T> for Message {
    fn from(value: T) -> Self {
        let control = value.into();
        Message::Control(control)
    }
}

impl From<WindowWithBorderAction> for Control {
    fn from(value: WindowWithBorderAction) -> Self {
        Control::WindowWithBorder(value)
    }
}

impl WindowManager {
    pub fn run(&mut self) {
        tracing::info!("Starting runtime...");

        let mut stop_runtime = false;

        let (_, events_rx) = EVENTS_CHANNEL.get_or_init(|| crossbeam_channel::bounded(50));
        let (_, commands_rx) = COMMANDS_CHANNEL.get_or_init(|| crossbeam_channel::bounded(50));
        let (_, control_rx) = CONTROL_CHANNEL.get_or_init(|| crossbeam_channel::bounded(50));

        let (ctrlc_sender, ctrlc_receiver) = crossbeam_channel::bounded(1);
        if let Err(error) = ctrlc::set_handler(move || {
            ctrlc_sender
                .send(())
                .expect("could not send signal on ctrl-c channel");
        }) {
            tracing::error!("failed to set ctrl-c handler: {error}");
        }

        self.start_listeners();

        loop {
            std::thread::sleep(Duration::from_millis(1));

            // Check for ctrl-c before getting the messages
            if ctrlc_receiver.try_recv().is_ok() {
                tracing::error!(
                    "received ctrl-c, restoring all hidden windows and terminating process"
                );
                break;
            }

            // Messages buffer
            let mut messages = vec![];

            while let Ok(message) = control_rx.try_recv() {
                tracing::trace!("Control received: {}", &message);
                messages.push(message);
            }
            while let Ok(message) = commands_rx.try_recv() {
                tracing::trace!("Commands received: {}", &message);
                messages.push(message);
            }
            while let Ok(message) = events_rx.try_recv() {
                tracing::trace!("Event received: {}", &message);
                messages.push(message);
            }

            // Filter messages
            self.filter_messages(&mut messages);

            if !messages.is_empty() {
                //TODO: turn to trace
                tracing::debug!("Got {} messages! Processing...", messages.len());
            } else {
                continue;
            }

            // Check for ctrl-c before handling the messages
            if ctrlc_receiver.try_recv().is_ok() {
                tracing::error!(
                    "received ctrl-c, restoring all hidden windows and terminating process"
                );
                break;
            }

            for message in messages {
                tracing::info!("processing message: {}", &message);
                match message {
                    Message::Event(event) => {
                        if let Err(error) = self.process_event(event) {
                            tracing::error!("Error from 'process_event': {}", error);
                        }
                    }
                    Message::TcpCommand(message, mut stream) => {
                        self.handle_command(message, &mut stream, &mut stop_runtime);
                    }
                    Message::Command(messages, mut stream) => {
                        for message in messages {
                            self.handle_command(message, &mut stream, &mut stop_runtime);
                        }
                    }
                    Message::Control(control) => match control {
                        Control::Border(message) => {
                            self.update_border(message);
                        }
                        Control::Reaper(notification) => {
                            if let Err(error) = self.handle_reaper_notification(notification) {
                                tracing::error!(
                                    "Error from 'handle_reaper_notification': {}",
                                    error
                                );
                            }
                        }
                        Control::Monitor(notification) => {
                            if let Err(error) = self.handle_monitor_notification(notification) {
                                tracing::error!(
                                    "Error from 'handle_monitor_notification': {}",
                                    error
                                );
                            }
                        }
                        Control::Stackbar(message) => {
                            if let Err(error) = self
                                .stackbar_manager
                                .update(message, self.to_stackbar_info())
                            {
                                tracing::error!(
                                    "Error from 'stackbar_manager.update()': {}",
                                    error
                                );
                            }
                        }
                        Control::Transparency(message) => {
                            self.transparency_manager
                                .update(message, self.to_transparency_info());
                        }
                        Control::WindowWithBorder(action) => match action {
                            WindowWithBorderAction::Show(hwnd) => {
                                let window = Window::from(hwnd);
                                let message = border_manager::BorderMessage::Show(hwnd);
                                window.internal_restore();
                                self.update_border(message);
                            }
                            WindowWithBorderAction::Hide(hwnd) => {
                                let window = Window::from(hwnd);
                                let message = border_manager::BorderMessage::Hide(hwnd);
                                window.internal_hide();
                                self.update_border(message);
                            }
                            WindowWithBorderAction::Raise(hwnd) => {
                                let window = Window::from(hwnd);
                                let message = border_manager::BorderMessage::Raise(hwnd);
                                if let Err(error) = window.internal_raise() {
                                    tracing::error!("Error raising window: {}", error);
                                }
                                self.update_border(message);
                            }
                            WindowWithBorderAction::Lower(hwnd) => {
                                let window = Window::from(hwnd);
                                let message = border_manager::BorderMessage::Lower(hwnd);
                                if let Err(error) = window.internal_lower() {
                                    tracing::error!("Error lowering window: {}", error);
                                }
                                self.update_border(message);
                            }
                        },
                    },
                }

                // Check for ctrl-c between messages
                if ctrlc_receiver.try_recv().is_ok() {
                    tracing::error!(
                        "received ctrl-c, restoring all hidden windows and terminating process"
                    );
                    stop_runtime = true;
                    break;
                }

                if stop_runtime {
                    tracing::debug!(
                        "Received a 'Stop' command, ignoring the remainder messages..."
                    );
                    break;
                }
            }

            // Check for ctrl-c (we check this multiple times to reduce the wait for the user)
            if ctrlc_receiver.try_recv().is_ok() {
                tracing::error!(
                    "received ctrl-c, restoring all hidden windows and terminating process"
                );
                break;
            }

            if stop_runtime {
                tracing::info!("Stopping the runtime...");
                break;
            }
        }
        let mut stopped = RUNTIME_STOPPED.write();
        *stopped = true;
        drop(stopped);
        self.dump_state();
    }

    /// Starts all the listeners and watchers threads
    fn start_listeners(&mut self) {
        // Start window events listener
        winevent_listener::start();

        // Start the reaper watcher
        let known_hwnds = self.known_hwnds.clone();
        reaper::watch_for_orphans(known_hwnds);

        // Start the commands listeners
        let command_listener = self
            .command_listener
            .try_clone()
            .expect("could not clone unix listener");

        listen_for_commands(command_listener);

        if let Some(port) = self.tcp_port {
            listen_for_commands_tcp(port);
        }
    }

    /// Filter messages, we only care for window events from known hwnds or from unknown hwnds if
    /// the event was `Show`, `Uncloak` or `Manage`. Also the border `Update` and `PassEvent`
    /// messages can also be sent from the events listener so we also need to check those to make
    /// sure the hwnd is known.
    fn filter_messages(&self, messages: &mut Vec<Message>) {
        messages.retain(|message| match message {
            Message::Event(event) => match event {
                WindowManagerEvent::Destroy(_, window)
                | WindowManagerEvent::FocusChange(_, window)
                | WindowManagerEvent::Hide(_, window)
                | WindowManagerEvent::Cloak(_, window)
                | WindowManagerEvent::Minimize(_, window)
                | WindowManagerEvent::MoveResizeStart(_, window)
                | WindowManagerEvent::MoveResizeEnd(_, window)
                | WindowManagerEvent::MouseCapture(_, window)
                | WindowManagerEvent::Unmanage(window)
                | WindowManagerEvent::Raise(window)
                | WindowManagerEvent::TitleUpdate(_, window) => {
                    self.known_hwnds.contains_key(&window.hwnd)
                }
                WindowManagerEvent::Show(_, window)
                | WindowManagerEvent::Uncloak(_, window)
                | WindowManagerEvent::Manage(window) => {
                    let is_window = WindowsApi::is_window(window.hwnd);

                    if is_window {
                        let window = Window::from(window.hwnd);

                        window
                            .should_manage(
                                None,
                                &mut RuleDebug::default(),
                                &self.transparency_manager.known_transparent_hwnds,
                            )
                            .is_ok_and(|v| v)
                    } else {
                        false
                    }
                }
            },
            Message::Command(_, _) | Message::TcpCommand(_, _) => true,
            Message::Control(control) => match control {
                Control::Border(border_message) => match border_message {
                    // Only `Update` and `PassEvent` are sent from the events listener callback so
                    // we only need to check these two
                    border_manager::BorderMessage::Update(hwnd) => {
                        if let Some(hwnd) = hwnd {
                            self.known_hwnds.contains_key(hwnd)
                        } else {
                            // If an update was requested with `None` we want to handle that since
                            // that was sent by the `WindowManager`
                            true
                        }
                    }
                    border_manager::BorderMessage::PassEvent(hwnd, _) => {
                        self.known_hwnds.contains_key(hwnd)
                    }
                    border_manager::BorderMessage::Delete(_)
                    | border_manager::BorderMessage::Show(_)
                    | border_manager::BorderMessage::Hide(_)
                    | border_manager::BorderMessage::Raise(_)
                    | border_manager::BorderMessage::Lower(_)
                    | border_manager::BorderMessage::DestroyAll => true,
                },
                Control::Reaper(_)
                | Control::Monitor(_)
                | Control::Stackbar(_)
                | Control::Transparency(_)
                | Control::WindowWithBorder(_) => true,
            },
        });
    }

    /// Helper function to call the `border_manager` update function and handle errors
    fn update_border(&mut self, message: border_manager::BorderMessage) {
        if let Err(error) = self.border_manager.update(message, self.to_border_info()) {
            tracing::error!("Error from 'border_manager.update()': {}", error);
        }
    }

    /// Helper function to call the `process_command` and handle the pause and stop situations and
    /// any errors that might occur
    fn handle_command(
        &mut self,
        message: SocketMessage,
        reply: &mut impl std::io::Write,
        stop_runtime: &mut bool,
    ) {
        if self.is_paused
            && !matches!(
                message,
                SocketMessage::TogglePause
                    | SocketMessage::State
                    | SocketMessage::GlobalState
                    | SocketMessage::Stop
            )
        {
            tracing::trace!("ignoring while paused");
        } else if let Err(error) = self.process_command(message.clone(), reply) {
            tracing::error!("Error from 'process_command': {}", error);
        }

        if matches!(
            message,
            SocketMessage::Stop | SocketMessage::StopIgnoreRestore
        ) {
            *stop_runtime = true;
        }
    }

    /// Dumps the state and restores all windows
    fn dump_state(&self) {
        use crate::State;
        use std::env::temp_dir;

        tracing::info!("dumping state...");
        let dumped_state = temp_dir().join("komorebi.state.json");
        let state = State::from(self);
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            if let Err(error) = std::fs::write(dumped_state, json) {
                tracing::error!("failed to write state dump: {}", error);
            }
            if let Err(error) = self.restore_all_windows(false) {
                tracing::error!("failed to restore all windows: {}", error);
            }
        }
    }
}
