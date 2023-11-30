#![feature(assert_matches)]
#![feature(try_trait_v2)]
#![feature(async_closure)]

mod child;
mod config;
mod data;
mod parser;

use config::Config;
use data::DataComponent;
use iced::event::{Event, Status};
use iced::futures::channel::mpsc::Sender;
use iced::widget::{scrollable, text};
use iced::{executor, keyboard, Length};
use iced::{subscription, window};
use iced::{Application, Command, Element, Settings, Subscription, Theme};
use log::{debug, info};
use parser::{Node, NodeParseResult};
use std::path::Path;
use unicode_segmentation::UnicodeSegmentation;

struct Firn {
    data: DataComponent,
    text_buffer: String,
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
                data: DataComponent::new(),
                text_buffer: String::new(),
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
        scrollable(text(self.data.render()).width(Length::Fill))
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
                self.add_text(&text);
                scrollable::snap_to(self.scrollable_id.clone(), scrollable::RelativeOffset::END)
            }
            Message::ChildEvent(child::OutputEvent::Stderr(text)) => {
                self.add_text(&text);
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
            child::subscribe_to_pty(self.config.clone()).map(Message::ChildEvent),
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

impl Firn {
    fn add_text(&mut self, text: &str) {
        self.text_buffer += text;
        let mut chars = self.text_buffer.chars();
        while let NodeParseResult::Match(remaining_chars, node) = Node::parse(chars.clone()) {
            chars = remaining_chars;
            write_node(&mut self.data, &node);
        }
        self.text_buffer = chars.collect();
    }
}

fn write_node(data: &mut DataComponent, node: &Node) {
    match node {
        Node::Text(text) => write_text(data, text),
        Node::C0Control('\x08') => data.activate_prev_cell(),
        Node::C0Control('\x0A') => data.activate_next_line(),
        Node::C0Control('\x0D') => data.activate_first_cell(),
        Node::C1Control('\x45') => data.activate_first_cell(),
        Node::C1Control('\x4D') => data.activate_prev_line(),
        node => info!("Ignoring node {node:?}"),
    };
}

fn write_text(data: &mut DataComponent, text: &str) {
    let combined_text = data
        .get_active_cell()
        .grapheme
        .to_owned()
        .unwrap_or_default()
        + text;
    let mut graphemes = combined_text.graphemes(true);

    if let Some(grapheme) = graphemes.next() {
        data.get_active_cell_mut().grapheme = Some(grapheme.to_string());
    }
    for grapheme in graphemes {
        data.activate_next_cell();
        data.get_active_cell_mut().grapheme = Some(grapheme.to_string());
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let config = Config::from_path(Path::new("config.json")).unwrap_or_default();

    Firn::run(Settings::with_flags(config))?;
    Ok(())
}
