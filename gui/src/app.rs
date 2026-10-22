//! Frontend-neutral Herdr pane catalog.
//!
//! The GUI does not link Herdr. It asks the installed CLI for the pane list and
//! lets one `herdr terminal attach` process own each visible terminal. The
//! actor here owns only the pane snapshot and selection; PTY state stays in the
//! iced renderer.

use std::time::Duration;

use ferriso_core::{App, Command};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Pane {
    pub terminal_id: String,
    pub pane_id: String,
    pub title: String,
    pub workspace_id: String,
    pub agent: Option<String>,
    pub status: String,
    pub focused: bool,
}

#[derive(Clone, Debug, Default)]
pub struct View {
    pub panes: Vec<Pane>,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub herdr_bin: String,
}

impl View {
    pub fn current(&self) -> Option<&Pane> {
        self.panes.get(self.selected)
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    Loaded(Result<Vec<Pane>, String>),
    Refresh,
    Select(usize),
}

pub struct Dashboard {
    panes: Vec<Pane>,
    selected: usize,
    loading: bool,
    error: Option<String>,
    herdr_bin: String,
}

impl Dashboard {
    pub fn new() -> Self {
        let herdr_bin = std::env::var("HERDR_BIN").unwrap_or_else(|_| "herdr".to_owned());
        Self {
            panes: Vec::new(),
            selected: 0,
            loading: true,
            error: None,
            herdr_bin,
        }
    }

    fn load(&self) -> Command<Message> {
        let herdr = self.herdr_bin.clone();
        Command::perform(async move { load_panes(&herdr).await }, Message::Loaded)
    }
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new()
    }
}

impl App for Dashboard {
    type Message = Message;
    type View = View;

    fn init(&mut self) -> Command<Self::Message> {
        self.loading = true;
        self.load()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::Loaded(Ok(panes)) => {
                let previous = self
                    .panes
                    .get(self.selected)
                    .map(|pane| pane.terminal_id.clone());
                self.selected = previous
                    .and_then(|terminal_id| {
                        panes
                            .iter()
                            .position(|pane| pane.terminal_id == terminal_id)
                    })
                    .or_else(|| panes.iter().position(|pane| pane.focused))
                    .unwrap_or(0);
                self.panes = panes;
                self.loading = false;
                self.error = None;
            }
            Message::Loaded(Err(error)) => {
                self.loading = false;
                self.error = Some(error);
            }
            Message::Refresh => {
                if !self.loading {
                    self.loading = true;
                    return self.load();
                }
                return Command::none();
            }
            Message::Select(index) => {
                if index < self.panes.len() {
                    self.selected = index;
                }
                return Command::none();
            }
        }
        Command::none()
    }

    fn view(&self) -> Self::View {
        View {
            panes: self.panes.clone(),
            selected: self.selected,
            loading: self.loading,
            error: self.error.clone(),
            herdr_bin: self.herdr_bin.clone(),
        }
    }

    fn tick_interval(&self) -> Option<Duration> {
        Some(Duration::from_secs(2))
    }

    fn tick(&mut self) -> Command<Self::Message> {
        if self.loading {
            return Command::none();
        }
        self.loading = true;
        self.load()
    }
}

async fn load_panes(herdr: &str) -> Result<Vec<Pane>, String> {
    let output = tokio::process::Command::new(herdr)
        .args(["pane", "list"])
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|error| format!("could not run `{herdr} pane list`: {error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Ok(value) = serde_json::from_str::<Value>(&stdout)
        && let Some(error) = json_error(&value)
    {
        return Err(error);
    }
    if !output.status.success() {
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            format!("`{herdr} pane list` exited with {}", output.status)
        } else {
            format!("`{herdr} pane list`: {detail}")
        });
    }

    parse_panes(&stdout)
}

fn parse_panes(raw: &str) -> Result<Vec<Pane>, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|error| format!("invalid pane list JSON: {error}"))?;
    let result = value.get("result").unwrap_or(&value);
    let Some(items) = result.get("panes").and_then(Value::as_array) else {
        return Err("pane list JSON has no `panes` array".into());
    };

    let mut panes = Vec::with_capacity(items.len());
    for item in items {
        let Some(terminal_id) = string(item.get("terminal_id")) else {
            continue;
        };
        let pane_id = string(item.get("pane_id")).unwrap_or_else(|| terminal_id.clone());
        let title = string(item.get("label"))
            .or_else(|| string(item.get("title")))
            .or_else(|| string(item.get("terminal_title")))
            .or_else(|| string(item.get("display_agent")))
            .unwrap_or_else(|| pane_id.clone());
        panes.push(Pane {
            terminal_id,
            pane_id,
            title,
            workspace_id: string(item.get("workspace_id")).unwrap_or_default(),
            agent: string(item.get("display_agent")).or_else(|| string(item.get("agent"))),
            status: string(item.get("agent_status")).unwrap_or_else(|| "unknown".into()),
            focused: item
                .get("focused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(panes)
}

fn string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn json_error(value: &Value) -> Option<String> {
    let error = value.get("error")?;
    let message = string(error.get("message"))
        .or_else(|| string(error.get("code")))
        .unwrap_or_else(|| error.to_string());
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_cli_envelope_and_keeps_pane_identity() {
        let panes = parse_panes(
            r#"{"id":"cli:pane:list","result":{"panes":[
                {"pane_id":"w1:p1","terminal_id":"t1","workspace_id":"w1","tab_id":"w1:t1",
                 "focused":true,"label":"build","agent":"codex","agent_status":"working"}
            ]}}"#,
        )
        .expect("valid pane list");

        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].terminal_id, "t1");
        assert_eq!(panes[0].pane_id, "w1:p1");
        assert_eq!(panes[0].title, "build");
        assert_eq!(panes[0].status, "working");
        assert!(panes[0].focused);
    }

    #[test]
    fn extracts_the_cli_error_message() {
        let value: Value = serde_json::from_str(
            r#"{"id":"cli:pane:list","error":{"code":"server_not_running","message":"no server"}}"#,
        )
        .expect("valid JSON");
        assert_eq!(json_error(&value).as_deref(), Some("no server"));
    }

    #[test]
    fn falls_back_to_the_pane_id_when_the_cli_returns_no_label() {
        let panes = parse_panes(
            r#"{"id":"cli:pane:list","result":{"panes":[
                {"pane_id":"wW:p1","terminal_id":"t1","workspace_id":"wW","focused":true}
            ]}}"#,
        )
        .expect("valid pane list");
        assert_eq!(panes[0].title, "wW:p1");
    }
}
