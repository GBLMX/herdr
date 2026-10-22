//! Native iced frontend: one Herdr pane at a time through `iced_term`.
//!
//! The pane list is a core snapshot. The terminal widget is renderer-local
//! because it owns a PTY and an alacritty grid; recreating the window must not
//! recreate that state. Switching panes drops the old `herdr terminal attach`
//! process and starts the new one.

use std::collections::HashMap;

use ferriso_gui::iced::{
    self, Alignment, Color, Element, Length, Subscription, Task, Theme,
    widget::{Space, button, column, container, row, scrollable, text},
};
use ferriso_gui::{Message as GuiMessage, Renderer};

use crate::app::{Message, Pane, View};

type Msg = GuiMessage<View, Message, Local>;

const BG: Color = Color::from_rgb(0.07, 0.08, 0.10);
const SURFACE: Color = Color::from_rgb(0.11, 0.13, 0.16);
const TEXT: Color = Color::from_rgb(0.88, 0.90, 0.94);
const MUTED: Color = Color::from_rgb(0.55, 0.58, 0.65);
const ACCENT: Color = Color::from_rgb(0.47, 0.64, 0.97);
const ERROR: Color = Color::from_rgb(0.96, 0.35, 0.42);

#[derive(Clone, Debug)]
pub enum Local {
    Terminal(iced_term::Event),
    Reconnect,
}

struct TerminalSlot {
    terminal: iced_term::Terminal,
    exited: bool,
}

pub struct HerdrGui {
    slot: Option<TerminalSlot>,
    current: Option<String>,
    next_id: u64,
    error: Option<String>,
}

impl HerdrGui {
    pub fn new() -> Self {
        Self {
            slot: None,
            current: None,
            next_id: 0,
            error: None,
        }
    }

    fn attach(&mut self, pane: &Pane) -> Result<iced_term::Terminal, String> {
        let id = self.next_id;
        self.next_id += 1;
        let herdr = std::env::var("HERDR_BIN").unwrap_or_else(|_| "herdr".to_owned());
        let env_program =
            std::env::var("HERDR_GUI_ENV").unwrap_or_else(|_| "/usr/bin/env".to_owned());
        let mut env = HashMap::new();
        env.insert("TERM".to_owned(), "xterm-256color".to_owned());
        env.insert("COLORTERM".to_owned(), "truecolor".to_owned());

        let settings = iced_term::settings::Settings {
            font: iced_term::settings::FontSettings {
                size: 14.0,
                scale_factor: 1.2,
                font_type: iced::Font::MONOSPACE,
            },
            theme: iced_term::settings::ThemeSettings::default(),
            backend: iced_term::settings::BackendSettings {
                program: env_program,
                args: vec![
                    "-u".to_owned(),
                    "HERDR_ENV".to_owned(),
                    "-u".to_owned(),
                    "HERDR_CLIENT_SOCKET_PATH".to_owned(),
                    herdr,
                    "terminal".to_owned(),
                    "attach".to_owned(),
                    pane.terminal_id.clone(),
                    "--takeover".to_owned(),
                ],
                env,
                working_directory: None,
            },
        };

        iced_term::Terminal::new(id, settings)
            .map_err(|error| format!("could not attach to {}: {error}", pane.pane_id))
    }

    fn show(&mut self, view: &View) {
        let wanted = view.current().map(|pane| pane.terminal_id.clone());
        if wanted == self.current {
            return;
        }
        self.current = wanted;
        self.slot = None;
        self.error = None;

        let Some(pane) = view.current() else {
            return;
        };
        match self.attach(pane) {
            Ok(terminal) => {
                self.slot = Some(TerminalSlot {
                    terminal,
                    exited: false,
                });
            }
            Err(error) => self.error = Some(error),
        }
    }
}

impl Default for HerdrGui {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer<View, Message, Local> for HerdrGui {
    fn view<'a>(&'a self, view: &'a View) -> Element<'a, Msg> {
        let selected = view.current().map(|pane| pane.title.as_str());
        let status = if view.loading {
            "loading panes…".to_owned()
        } else if let Some(error) = view.error.as_deref() {
            error.to_owned()
        } else {
            format!("{} pane(s) · {}", view.panes.len(), view.herdr_bin)
        };

        let mut list = column![].spacing(6);
        for (index, pane) in view.panes.iter().enumerate() {
            list = list.push(pane_button(pane, index, index == view.selected));
        }

        let sidebar = container(
            column![
                text("herdr · gui").size(18).color(ACCENT),
                text(status)
                    .size(11)
                    .color(if view.error.is_some() { ERROR } else { MUTED }),
                scrollable(list).height(Length::Fill),
                button(text("Refresh").size(12).color(TEXT))
                    .width(Length::Fill)
                    .padding([7, 10])
                    .on_press(GuiMessage::User(Message::Refresh)),
                text("Select a pane, then click the terminal to focus it.")
                    .size(10)
                    .color(MUTED),
            ]
            .spacing(10),
        )
        .width(Length::Fixed(280.0))
        .height(Length::Fill)
        .padding(12)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(SURFACE)),
            ..Default::default()
        });

        let terminal: Element<'a, Msg> = match &self.slot {
            Some(slot) => iced_term::TerminalView::show(&slot.terminal)
                .map(|event| GuiMessage::Local(Local::Terminal(event))),
            None => container(
                column![
                    text(selected.unwrap_or("No pane selected"))
                        .size(18)
                        .color(TEXT),
                    text(
                        view.error
                            .as_deref()
                            .or(self.error.as_deref())
                            .unwrap_or("Start herdr, open a pane, then press Refresh.")
                    )
                    .size(13)
                    .color(MUTED),
                ]
                .spacing(8),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into(),
        };

        let title = selected.unwrap_or("herdr");
        let mut main = column![
            row![
                text(title).size(16).color(TEXT),
                Space::new().width(Length::Fill),
                text("native panes · iced_term").size(11).color(MUTED),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(8);

        if self.slot.as_ref().is_some_and(|slot| slot.exited) {
            main = main.push(
                row![
                    text("The attach process exited.").size(12).color(ERROR),
                    Space::new().width(Length::Fixed(8.0)),
                    button(text("Reconnect").size(11).color(TEXT))
                        .padding([3, 8])
                        .on_press(GuiMessage::Local(Local::Reconnect)),
                ]
                .align_y(Alignment::Center),
            );
        }
        if let Some(error) = self.error.as_deref() {
            main = main.push(
                row![
                    text(error).size(12).color(ERROR),
                    Space::new().width(Length::Fixed(8.0)),
                    button(text("Reconnect").size(11).color(TEXT))
                        .padding([3, 8])
                        .on_press(GuiMessage::Local(Local::Reconnect)),
                ]
                .align_y(Alignment::Center),
            );
        }
        main = main.push(terminal);

        container(row![sidebar, container(main).padding(10)].spacing(10))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(10)
            .style(|_| container::Style {
                background: Some(iced::Background::Color(BG)),
                text_color: Some(TEXT),
                ..Default::default()
            })
            .into()
    }

    fn subscription(&self, _view: &View) -> Subscription<Msg> {
        match &self.slot {
            Some(slot) => slot
                .terminal
                .subscription()
                .map(|event| GuiMessage::Local(Local::Terminal(event))),
            None => Subscription::none(),
        }
    }

    fn update(&mut self, local: Local, view: &View) -> Task<Msg> {
        match local {
            Local::Terminal(iced_term::Event::BackendCall(_, command)) => {
                if let Some(slot) = self.slot.as_mut() {
                    let action = slot
                        .terminal
                        .handle(iced_term::Command::ProxyToBackend(command));
                    if matches!(action, iced_term::actions::Action::Shutdown) {
                        slot.exited = true;
                    }
                }
            }
            Local::Reconnect => {
                self.current = None;
                self.slot = None;
                self.error = None;
                self.show(view);
            }
        }
        Task::none()
    }

    fn on_snapshot(&mut self, view: &View) {
        self.show(view);
    }

    fn theme(&self, _view: &View) -> Theme {
        Theme::Dark
    }
}

fn pane_button(pane: &Pane, index: usize, selected: bool) -> Element<'_, Msg> {
    let status = match pane.status.as_str() {
        "working" => "● working",
        "blocked" => "● blocked",
        "done" => "● done",
        "idle" => "● idle",
        other => other,
    };
    let title = if pane.focused {
        format!("★ {}", pane.title)
    } else {
        pane.title.clone()
    };
    let meta = if let Some(agent) = &pane.agent {
        format!("{agent} · {status} · {}", pane.workspace_id)
    } else {
        format!("{status} · {}", pane.workspace_id)
    };
    button(
        column![
            text(title)
                .size(13)
                .color(if selected { ACCENT } else { TEXT }),
            text(meta).size(10).color(MUTED),
        ]
        .spacing(2),
    )
    .width(Length::Fill)
    .padding([6, 8])
    .style(move |_, _| button::Style {
        background: Some(iced::Background::Color(if selected {
            Color::from_rgb(0.16, 0.24, 0.40)
        } else {
            SURFACE
        })),
        text_color: TEXT,
        ..Default::default()
    })
    .on_press(GuiMessage::User(Message::Select(index)))
    .into()
}
