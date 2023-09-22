mod child;
mod config;

use std::path::Path;

use config::Config;
use iced::event::{Event, Status};
use iced::futures::channel::mpsc::Sender;
use iced::widget::{scrollable, text};
use iced::{executor, keyboard};
use iced::{subscription, window};
use iced::{Application, Command, Element, Settings, Subscription, Theme};
use log::debug;

struct Firn {
    text: String,
    scrollable_id: scrollable::Id,
    child_sender: Option<Sender<child::InputEvent>>,
    theme: Theme,
    config: Config,
}

#[derive(Debug, Clone)]
pub enum Message {
    ApplicationEvent(Event),
    ChildEvent(child::OutputEvent),
}

impl Application for Firn {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = Config;

    fn new(config: Config) -> (Self, Command<Message>) {
        (
            Self {
                text: "".into(),
                scrollable_id: scrollable::Id::unique(),
                child_sender: None,
                theme: Theme::Dark,
                config,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Firn Terminal")
    }

    fn view(&self) -> Element<Message> {
        scrollable(text(self.text.clone()))
            .id(self.scrollable_id.clone())
            .into()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        debug!("Recv message: {message:?}");
        match message {
            Message::ChildEvent(child::OutputEvent::Connected(sender)) => {
                self.child_sender = Some(sender);
                Command::none()
            }
            Message::ChildEvent(child::OutputEvent::Disconnected) => window::close(),
            Message::ChildEvent(child::OutputEvent::Stdout(text)) => {
                self.text += &text;
                scrollable::snap_to(self.scrollable_id.clone(), scrollable::RelativeOffset::END)
            }
            Message::ChildEvent(child::OutputEvent::Stderr(text)) => {
                self.text += &text;
                scrollable::snap_to(self.scrollable_id.clone(), scrollable::RelativeOffset::END)
            }
            Message::ApplicationEvent(Event::Keyboard(keyboard::Event::CharacterReceived(ch))) => {
                if let Some(child_sender) = self.child_sender.as_mut() {
                    debug!("Send character to shell: {ch}");
                    child_sender
                        .try_send(child::InputEvent::Stdin(ch.into()))
                        .unwrap();
                }
                Command::none()
            }
            _ => Command::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            child::connect(self.config.clone()).map(|event| Message::ChildEvent(event)),
            subscription::events_with(|event, status| match (&event, status) {
                (Event::Keyboard(_), Status::Ignored) => Some(Message::ApplicationEvent(event)),
                _ => None,
            }),
        ])
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let config = Config::from_path(Path::new("config.json")).unwrap_or(Config::default());

    Firn::run(Settings::with_flags(config))?;
    Ok(())
}
