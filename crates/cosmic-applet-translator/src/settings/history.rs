//! History page: searchable list of recent translations.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, button, column, row, text};
use cosmic::Element;

use super::Update;
use crate::fl;
use crate::history::{DialogueBacklog, HistoryEntry};

#[derive(Debug, Clone, Default)]
pub struct State {
    pub query: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    SearchChanged(String),
    Clear,
    Copy(String),
}

pub fn update(
    state: &mut State,
    backlog: &mut DialogueBacklog,
    message: Message,
) -> Update<Message> {
    match message {
        Message::SearchChanged(query) => state.query = query,
        Message::Clear => backlog.clear(),
        Message::Copy(text) => return Update::task(cosmic::iced::clipboard::write(text)),
    }
    Update::none()
}

pub fn view<'a>(state: &'a State, backlog: &'a DialogueBacklog) -> Element<'a, Message> {
    let search_bar = row![
        widget::text_input(fl!("history-search-placeholder"), &state.query)
            .on_input(Message::SearchChanged)
            .width(Length::Fill),
        button::destructive(fl!("clear-history")).on_press(Message::Clear),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let entries = backlog.search(&state.query);
    let entries_list: Element<'a, Message> = if entries.is_empty() {
        widget::container(text(fl!("no-history-entries")).size(14))
            .padding(24)
            .center_x(Length::Fill)
            .into()
    } else {
        let mut col = column![].spacing(8);
        for entry in entries {
            col = col.push(entry_card(entry));
        }
        col.into()
    };

    column![
        search_bar,
        widget::divider::horizontal::default(),
        entries_list
    ]
    .spacing(12)
    .width(Length::Fill)
    .into()
}

fn entry_card(entry: &HistoryEntry) -> Element<'_, Message> {
    let mut header = row![text(&entry.timestamp_str).size(12)]
        .spacing(8)
        .align_y(Alignment::Center);

    let copy_payload = if let Some(speaker) = &entry.speaker {
        header = header.push(speaker_badge(speaker));
        format!("{speaker}: {}\n{}", entry.source, entry.target)
    } else {
        format!("{}\n{}", entry.source, entry.target)
    };
    let header = header
        .push(cosmic::iced::widget::space::horizontal())
        .push(button::standard(fl!("copy")).on_press(Message::Copy(copy_payload)));

    let content = column![
        header,
        widget::divider::horizontal::default(),
        text(&entry.source)
            .size(13)
            .wrapping(cosmic::iced::widget::text::Wrapping::Word),
        text(&entry.target)
            .size(14)
            .wrapping(cosmic::iced::widget::text::Wrapping::Word),
    ]
    .spacing(6);

    widget::container(content)
        .padding(10)
        .width(Length::Fill)
        .class(cosmic::theme::Container::Card)
        .into()
}

fn speaker_badge(speaker: &str) -> Element<'_, Message> {
    widget::container(text(speaker).size(12))
        .padding(cosmic::iced::Padding::from([2, 8]))
        .style(|_theme| widget::container::Style {
            background: Some(cosmic::iced::Background::Color(
                cosmic::iced::Color::from_rgba(0.2, 0.5, 0.8, 0.25),
            )),
            border: cosmic::iced::Border {
                color: cosmic::iced::Color::from_rgba(0.3, 0.6, 0.9, 0.5),
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_empties_backlog() {
        let mut backlog = DialogueBacklog::new(10);
        backlog.push(None, "a".into(), "b".into());
        let mut state = State::default();
        let _ = update(&mut state, &mut backlog, Message::Clear);
        assert!(backlog.entries().is_empty());
    }
}
