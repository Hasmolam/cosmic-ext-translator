//! Global shortcuts through the portal.

use cosmic::iced::Task;
use tracing::{debug, info};
use translator_core::shortcuts::{GlobalShortcutsManager, ShortcutAction};
use translator_protocol::HudMessage;

use super::{AppModel, Message};
use crate::services::RegionKind;

impl AppModel {
    pub(super) fn shortcut_activated(
        &mut self,
        action: ShortcutAction,
    ) -> Task<cosmic::Action<Message>> {
        match action {
            ShortcutAction::SelectRoi => self.run_region_selector(RegionKind::Capture),
            ShortcutAction::ToggleTranslation => {
                if self.capture_active {
                    self.stop_capture()
                } else {
                    self.start_capture()
                }
            }
            ShortcutAction::ToggleHud => {
                self.hud_visible = !self.hud_visible;
                self.hud.send_task(HudMessage::SetVisible {
                    visible: self.hud_visible,
                })
            }
            ShortcutAction::TranslateCurrentScreen => self.one_shot_translate(),
            ShortcutAction::Custom(id) => {
                info!("Unhandled custom global shortcut: {id}");
                Task::none()
            }
        }
    }
}

pub(super) fn global_shortcuts_subscription() -> cosmic::iced::Subscription<Message> {
    struct GlobalShortcutsSub;

    cosmic::iced::Subscription::run_with(std::any::TypeId::of::<GlobalShortcutsSub>(), |_| {
        cosmic::iced::stream::channel(
            16,
            |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                use cosmic::iced::futures::SinkExt;
                match GlobalShortcutsManager::register(None).await {
                    Ok((_manager, mut shortcut_rx)) => {
                        while let Some(action) = shortcut_rx.recv().await {
                            if output
                                .send(Message::ShortcutActivated(action))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        debug!("Global shortcuts portal unavailable or declined: {err}");
                        cosmic::iced::futures::future::pending::<()>().await;
                    }
                }
            },
        )
    })
}
