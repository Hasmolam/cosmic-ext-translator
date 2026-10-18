use std::sync::Arc;
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use ashpd::desktop::{CreateSessionOptions, Session};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Action triggered by an XDG Global Shortcut.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    SelectRoi,
    ToggleTranslation,
    ToggleHud,
    TranslateCurrentScreen,
    Custom(String),
}

impl ShortcutAction {
    pub fn id(&self) -> &str {
        match self {
            Self::SelectRoi => "select_roi",
            Self::ToggleTranslation => "toggle_translation",
            Self::ToggleHud => "toggle_hud",
            Self::TranslateCurrentScreen => "translate_current_screen",
            Self::Custom(id) => id.as_str(),
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "select_roi" => Self::SelectRoi,
            "toggle_translation" => Self::ToggleTranslation,
            "toggle_hud" => Self::ToggleHud,
            "translate_current_screen" => Self::TranslateCurrentScreen,
            other => Self::Custom(other.to_string()),
        }
    }
}

/// Manages XDG Desktop Portal Global Shortcuts session and event dispatching.
pub struct GlobalShortcutsManager {
    _session: Session<GlobalShortcuts>,
    _proxy: Arc<GlobalShortcuts>,
}

impl GlobalShortcutsManager {
    /// Builds the default set of global shortcuts for the application.
    pub fn default_shortcuts() -> Vec<NewShortcut> {
        vec![
            NewShortcut::new("select_roi", "Select Capture Region")
                .preferred_trigger(Some("Super+Alt+S")),
            NewShortcut::new("toggle_translation", "Toggle Real-time Translation")
                .preferred_trigger(Some("Super+Alt+T")),
            NewShortcut::new("toggle_hud", "Toggle Subtitle HUD Visibility")
                .preferred_trigger(Some("Super+Alt+H")),
            NewShortcut::new("translate_current_screen", "Translate Current Screen Now")
                .preferred_trigger(Some("Super+Alt+C")),
        ]
    }

    /// Registers global shortcuts with the XDG Desktop Portal.
    ///
    /// Returns the active manager (which keeps the session alive) and a channel receiver
    /// yielding activated shortcut events.
    pub async fn register(
        custom_shortcuts: Option<Vec<NewShortcut>>,
    ) -> ashpd::Result<(Self, mpsc::Receiver<ShortcutAction>)> {
        let proxy = Arc::new(GlobalShortcuts::new().await?);
        let session = proxy
            .create_session(CreateSessionOptions::default())
            .await?;

        let shortcuts = custom_shortcuts.unwrap_or_else(Self::default_shortcuts);

        info!("Binding {} global shortcuts via XDG portal", shortcuts.len());
        proxy
            .bind_shortcuts(&session, &shortcuts, None, BindShortcutsOptions::default())
            .await?
            .response()?;

        let mut stream = proxy.receive_activated().await?;
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            while let Some(activated) = stream.next().await {
                let action = ShortcutAction::from_id(activated.shortcut_id());
                debug!("Global shortcut activated: {:?}", action);
                if tx.send(action).await.is_err() {
                    break;
                }
            }
            debug!("Global shortcuts event loop terminated");
        });

        Ok((
            Self {
                _session: session,
                _proxy: proxy,
            },
            rx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shortcut_action_mapping() {
        assert_eq!(ShortcutAction::from_id("select_roi"), ShortcutAction::SelectRoi);
        assert_eq!(
            ShortcutAction::from_id("toggle_translation"),
            ShortcutAction::ToggleTranslation
        );
        assert_eq!(ShortcutAction::from_id("toggle_hud"), ShortcutAction::ToggleHud);
        assert_eq!(
            ShortcutAction::from_id("custom_action"),
            ShortcutAction::Custom("custom_action".to_string())
        );

        assert_eq!(ShortcutAction::SelectRoi.id(), "select_roi");
    }

    #[test]
    fn test_default_shortcuts_definitions() {
        let defaults = GlobalShortcutsManager::default_shortcuts();
        assert_eq!(defaults.len(), 4);
    }
}
