//! Settings window: tab bar and routing to the pages.
//!
//! Each page has its own `Message`, `view` and `update`. Pages edit [`Config`] directly;
//! the app compares the config before and after the update, saves it and applies the
//! effects (orchestrator restart, OCR rebuild, HUD style). Actions that need app state
//! beyond the config are returned as a [`Request`].

pub mod capture;
pub mod display;
pub mod engine;
pub mod glossary;
pub mod history;

use cosmic::iced::{Alignment, Length, Task};
use cosmic::widget::{self, button, column, row};
use cosmic::Element;

use crate::config::Config;
use crate::fl;
use crate::history::DialogueBacklog;
use crate::services::{HudService, RegionKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Capture,
    Engine,
    Display,
    Glossary,
    History,
}

#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    pub active_tab: SettingsTab,
    pub engine: engine::State,
    pub glossary: glossary::State,
    pub history: history::State,
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    TabSelected(SettingsTab),
    Capture(capture::Message),
    Engine(engine::Message),
    Display(display::Message),
    Glossary(glossary::Message),
    History(history::Message),
    Close,
}

/// Actions that need app state beyond the config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Run the region selector for the given region.
    SelectRegion(RegionKind),
    /// Close the settings window.
    Close,
}

/// Data a page update may read or change.
pub struct Context<'a> {
    pub config: &'a mut Config,
    pub backlog: &'a mut DialogueBacklog,
    pub hud: &'a HudService,
}

/// Result of a page update: a task producing further page messages, and an optional
/// request to the app.
pub struct Update<M> {
    pub task: Task<M>,
    pub request: Option<Request>,
}

impl<M: Send + 'static> Update<M> {
    #[must_use]
    pub fn none() -> Self {
        Self {
            task: Task::none(),
            request: None,
        }
    }

    #[must_use]
    pub fn task(task: Task<M>) -> Self {
        Self {
            task,
            request: None,
        }
    }

    #[must_use]
    pub fn request(request: Request) -> Self {
        Self {
            task: Task::none(),
            request: Some(request),
        }
    }

    fn map<N: Send + 'static>(self, f: fn(M) -> N) -> Update<N> {
        Update {
            task: self.task.map(f),
            request: self.request,
        }
    }
}

impl SettingsState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(
        &mut self,
        message: SettingsMessage,
        ctx: Context<'_>,
    ) -> Update<SettingsMessage> {
        match message {
            SettingsMessage::TabSelected(tab) => {
                self.active_tab = tab;
                Update::none()
            }
            SettingsMessage::Capture(msg) => {
                capture::update(ctx.config, msg).map(SettingsMessage::Capture)
            }
            SettingsMessage::Engine(msg) => {
                engine::update(&mut self.engine, ctx.config, msg).map(SettingsMessage::Engine)
            }
            SettingsMessage::Display(msg) => {
                display::update(ctx.config, ctx.hud, msg).map(SettingsMessage::Display)
            }
            SettingsMessage::Glossary(msg) => {
                glossary::update(&mut self.glossary, ctx.config, msg).map(SettingsMessage::Glossary)
            }
            SettingsMessage::History(msg) => {
                history::update(&mut self.history, ctx.backlog, msg).map(SettingsMessage::History)
            }
            SettingsMessage::Close => Update::request(Request::Close),
        }
    }

    pub fn view<'a>(
        &'a self,
        config: &'a Config,
        backlog: &'a DialogueBacklog,
    ) -> Element<'a, SettingsMessage> {
        let nav_bar = row![
            tab_button(fl!("tab-capture"), SettingsTab::Capture, self.active_tab),
            tab_button(fl!("tab-engine"), SettingsTab::Engine, self.active_tab),
            tab_button(fl!("tab-display"), SettingsTab::Display, self.active_tab),
            tab_button(fl!("tab-glossary"), SettingsTab::Glossary, self.active_tab),
            tab_button(fl!("tab-history"), SettingsTab::History, self.active_tab),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let content: Element<'a, SettingsMessage> = match self.active_tab {
            SettingsTab::Capture => capture::view(config).map(SettingsMessage::Capture),
            SettingsTab::Engine => engine::view(&self.engine, config).map(SettingsMessage::Engine),
            SettingsTab::Display => display::view(config).map(SettingsMessage::Display),
            SettingsTab::Glossary => {
                glossary::view(&self.glossary, config).map(SettingsMessage::Glossary)
            }
            SettingsTab::History => {
                history::view(&self.history, backlog).map(SettingsMessage::History)
            }
        };

        let footer = row![
            cosmic::iced::widget::space::horizontal(),
            button::standard(fl!("close-window")).on_press(SettingsMessage::Close),
        ]
        .width(Length::Fill);

        widget::container(
            column![
                nav_bar,
                widget::divider::horizontal::default(),
                widget::scrollable(content).height(Length::Fill),
                widget::divider::horizontal::default(),
                footer,
            ]
            .spacing(12),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

fn tab_button<'a>(
    title: String,
    tab: SettingsTab,
    active: SettingsTab,
) -> Element<'a, SettingsMessage> {
    choice_button(title, tab == active, SettingsMessage::TabSelected(tab))
}

/// Button for one option of a group: highlighted when `selected`.
fn choice_button<'a, M: Clone + 'static>(
    label: String,
    selected: bool,
    on_press: M,
) -> Element<'a, M> {
    let button = if selected {
        button::suggested(label)
    } else {
        button::standard(label)
    };
    button.on_press(on_press).into()
}

/// A control with its current value shown next to it.
fn with_value_label<'a, M: 'a>(
    control: impl Into<Element<'a, M>>,
    label: String,
) -> Element<'a, M> {
    row![control.into(), widget::text(label).size(14)]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}
