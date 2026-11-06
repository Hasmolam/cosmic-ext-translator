//! Glossary page: fixed translations for names and terms.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, button, column, container, row, text};
use cosmic::Element;

use super::Update;
use crate::config::Config;
use crate::fl;

/// Text typed into the "add term" inputs.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub new_source: String,
    pub new_target: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    SourceChanged(String),
    TargetChanged(String),
    Add,
    Remove(String),
}

pub fn update(state: &mut State, config: &mut Config, message: Message) -> Update<Message> {
    match message {
        Message::SourceChanged(value) => state.new_source = value,
        Message::TargetChanged(value) => state.new_target = value,
        Message::Add => {
            let source = state.new_source.trim();
            let target = state.new_target.trim();
            if !source.is_empty() && !target.is_empty() {
                config
                    .glossary
                    .insert(source.to_string(), target.to_string());
                state.new_source.clear();
                state.new_target.clear();
            }
        }
        Message::Remove(term) => {
            config.glossary.remove(&term);
        }
    }
    Update::none()
}

pub fn view<'a>(state: &'a State, config: &'a Config) -> Element<'a, Message> {
    let add_row = row![
        widget::text_input(fl!("glossary-source-placeholder"), &state.new_source)
            .on_input(Message::SourceChanged)
            .width(180),
        widget::text_input(fl!("glossary-target-placeholder"), &state.new_target)
            .on_input(Message::TargetChanged)
            .width(180),
        button::suggested(fl!("add-button")).on_press(Message::Add),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut terms: Vec<_> = config.glossary.iter().collect();
    terms.sort_unstable();
    let mut list = column![].spacing(8);
    for (term, translation) in terms {
        let item = row![
            text(format!("\"{term}\" ➔ \"{translation}\"")).size(14),
            cosmic::iced::widget::space::horizontal(),
            button::destructive(fl!("delete-button")).on_press(Message::Remove(term.clone())),
        ]
        .width(Length::Fill)
        .align_y(Alignment::Center);
        list = list.push(container(item).padding(6));
    }

    let section = widget::settings::section()
        .title(fl!("glossary-title"))
        .add(widget::settings::item(fl!("add-new-term"), add_row));

    column![
        section,
        text(fl!("saved-terms")).size(14),
        if config.glossary.is_empty() {
            Element::from(text(fl!("no-saved-terms")).size(13))
        } else {
            Element::from(list)
        }
    ]
    .spacing(12)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_trims_and_clears_inputs() {
        let mut state = State {
            new_source: "  Phoenix Down  ".into(),
            new_target: "  Anka Kuşu Tüyü  ".into(),
        };
        let mut config = Config::default();
        let _ = update(&mut state, &mut config, Message::Add);
        assert_eq!(
            config.glossary.get("Phoenix Down").map(String::as_str),
            Some("Anka Kuşu Tüyü")
        );
        assert!(state.new_source.is_empty() && state.new_target.is_empty());

        let _ = update(
            &mut state,
            &mut config,
            Message::Remove("Phoenix Down".into()),
        );
        assert!(config.glossary.is_empty());
    }

    #[test]
    fn add_ignores_incomplete_term() {
        let mut state = State {
            new_source: "Sword".into(),
            new_target: "   ".into(),
        };
        let mut config = Config::default();
        let _ = update(&mut state, &mut config, Message::Add);
        assert!(config.glossary.is_empty());
        assert_eq!(state.new_source, "Sword");
    }
}
