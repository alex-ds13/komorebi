use crate::border_manager;
use crate::reaper;
use crate::SocketMessage;
use crate::WindowManager;
use crate::WindowManagerEvent;

use std::sync::OnceLock;

use crossbeam_channel::Receiver;
use crossbeam_channel::Sender;
use uds_windows::UnixStream;

static CHANNEL: OnceLock<(Sender<Message>, Receiver<Message>)> = OnceLock::new();

pub fn channel() -> &'static (Sender<Message>, Receiver<Message>) {
    CHANNEL.get_or_init(crossbeam_channel::unbounded)
}

fn event_tx() -> Sender<Message> {
    channel().0.clone()
}

fn event_rx() -> Receiver<Message> {
    channel().1.clone()
}

pub fn send_message(message: impl Into<Message>) {
    if event_tx().try_send(message.into()).is_err() {
        tracing::warn!("channel is full; dropping notification")
    }
}

pub fn batch_messages(messages: Vec<impl Into<Message>>) {
    for message in messages {
        if event_tx().try_send(message.into()).is_err() {
            tracing::warn!("channel is full; dropping notification")
        }
    }
}

#[derive(Debug)]
pub enum Message {
    Event(WindowManagerEvent),
    Command(Vec<SocketMessage>, UnixStream),
    Border(border_manager::BorderMessage),
    Reaper(reaper::ReaperNotification),
}

impl WindowManager {
    pub fn run(&mut self) {
        let receiver = event_rx();

        tracing::info!("Starting runtime...");
        loop {
            if let Ok(message) = receiver.try_recv() {
                tracing::info!("Runtime message received: {:?}", &message);
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
                            } else {
                                tracing::error!("Failed to clone UnixStream on 'process_command'");
                            }
                        }
                    }
                    Message::Border(message) => {
                        if let Err(error) =
                            self.border_manager.update(message, self.to_border_info())
                        {
                            tracing::error!("Error from 'border_manager.update()': {}", error);
                        }
                    }
                    Message::Reaper(notification) => {
                        if let Err(error) = self.handle_reaper_notification(notification) {
                            tracing::error!("Error from 'handle_reaper_notification': {}", error);
                        }
                    }
                }
            }
        }
    }
}
