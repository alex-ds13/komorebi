use crate::border_manager;
use crate::reaper;
use crate::SocketMessage;
use crate::Window;
use crate::WindowManager;
use crate::WindowManagerEvent;

use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::OnceLock;

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
        Message::Command(_, _) => command_tx(),
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
            Message::Command(_, _) => command_tx(),
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
    Control(Control),
}

#[derive(Debug)]
pub enum Control {
    Border(border_manager::BorderMessage),
    Reaper(reaper::ReaperNotification),
    WindowWithBorder(WindowWithBorderAction),
}

#[derive(Debug)]
pub enum WindowWithBorderAction {
    Show(isize),
    Hide(isize),
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

        loop {
            // Check for ctrl-c before getting the messages
            if ctrlc_receiver.try_recv().is_ok() {
                tracing::error!(
                    "received ctrl-c, restoring all hidden windows and terminating process"
                );
                break;
            }

            // Messages buffer
            let mut messages = vec![];

            // while let Ok(message) = control_rx.recv_timeout(Duration::from_millis(20)) {
            while let Ok(message) = control_rx.try_recv() {
                //TODO: turn to trace
                tracing::debug!("Control received: {:?}", &message);
                messages.push(message);
            }
            while let Ok(message) = commands_rx.try_recv() {
                //TODO: turn to trace
                tracing::debug!("Command received: {:?}", &message);
                messages.push(message);
            }
            while let Ok(message) = events_rx.try_recv() {
                //TODO: turn to trace
                tracing::debug!("Event received: {:?}", &message);
                messages.push(message);
            }

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
                tracing::info!("processing message: {:?}", &message);
                match message {
                    Message::Event(event) => {
                        if let Err(error) = self.process_event(event) {
                            tracing::error!("Error from 'process_event': {}", error);
                        }
                    }
                    Message::Command(messages, stream) => {
                        for message in messages {
                            if let Ok(reply) = stream.try_clone() {
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
                                } else if let Err(error) =
                                    self.process_command(message.clone(), reply)
                                {
                                    tracing::error!("Error from 'process_command': {}", error);
                                }

                                if matches!(
                                    message,
                                    SocketMessage::Stop | SocketMessage::StopIgnoreRestore
                                ) {
                                    stop_runtime = true;
                                }
                            } else {
                                tracing::error!("Failed to clone UnixStream on 'process_command'");
                            }
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

    fn update_border(&mut self, message: border_manager::BorderMessage) {
        if let Err(error) = self.border_manager.update(message, self.to_border_info()) {
            tracing::error!("Error from 'border_manager.update()': {}", error);
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
