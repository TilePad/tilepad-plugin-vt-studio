use serde::{Deserialize, Serialize};
use tilepad_plugin_sdk::{
    inspector::Inspector, plugin::Plugin, protocol::TileInteractionContext,
    session::PluginSessionHandle, tracing,
};
use vtubestudio::data::{HotkeyTriggerRequest, HotkeysInCurrentModelRequest};

use crate::state::{VtClientState, VtState};

#[derive(Debug, Deserialize, Serialize)]
pub struct Properties {
    /// Store access token
    pub access_token: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InspectorMessageIn {
    GetHotkeyOptions,
    GetConnected,
    GetAuthorized,
    Authorize,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InspectorMessageOut {
    HotkeyOptions { options: Vec<HotkeyOption> },
    Connected { connected: bool },
    Authorized { authorized: bool },
}

#[derive(Deserialize, Serialize)]
pub struct HotkeyOption {
    label: String,
    value: String,
}

#[derive(Deserialize)]
pub struct TileProperties {
    pub hotkey_id: Option<String>,
}

pub struct VtPlugin {
    pub state: VtState,
}

impl Plugin for VtPlugin {
    fn on_properties(&self, session: &PluginSessionHandle, properties: serde_json::Value) {
        self.state.set_plugin_session(session.clone());

        let properties: Properties =
            serde_json::from_value(properties).expect("settings had invalid shape");

        // Don't try authenticating if theres no access token
        let access_token = match properties.access_token {
            Some(value) => value,
            None => return,
        };

        let current_state = self.state.get_client_state();

        // Don't try and authenticate if already authenticated
        if matches!(current_state, VtClientState::Authenticated) {
            return;
        }

        let state = self.state.clone();
        tokio::spawn(async move { state.authenticate(access_token).await });
    }

    fn on_inspector_message(
        &self,
        session: &PluginSessionHandle,
        inspector: Inspector,
        message: serde_json::Value,
    ) {
        let msg: InspectorMessageIn = match serde_json::from_value(message) {
            Ok(value) => value,
            Err(cause) => {
                tracing::error!(?cause, "error deserializing inspector message");
                return;
            }
        };

        let state = &self.state;
        state.set_inspector(inspector.clone());

        match msg {
            InspectorMessageIn::GetConnected => {
                let current_state = state.get_client_state();

                _ = inspector.send(InspectorMessageOut::Connected {
                    connected: !matches!(current_state, VtClientState::Disconnected),
                });
            }
            InspectorMessageIn::GetAuthorized => {
                let current_state = state.get_client_state();

                _ = inspector.send(InspectorMessageOut::Authorized {
                    authorized: matches!(current_state, VtClientState::Authenticated),
                });
            }
            InspectorMessageIn::Authorize => {
                let state = state.clone();

                tokio::spawn(async move {
                    let token = state.request_authenticate().await;
                    if let Some(token) = token {
                        state.set_auth_token(Some(token));
                    }
                });
            }
            InspectorMessageIn::GetHotkeyOptions => {
                let state = state.clone();

                tokio::spawn(async move {
                    // Request the hotkeys from VT Studio
                    let result = match state
                        .send_message(&HotkeysInCurrentModelRequest {
                            ..Default::default()
                        })
                        .await
                    {
                        Ok(response) => response,
                        Err(cause) => {
                            tracing::error!(?cause, "failed to get hotkeys in current model");
                            return;
                        }
                    };

                    // Send the available options to the inspector
                    _ = inspector.send(InspectorMessageOut::HotkeyOptions {
                        options: result
                            .available_hotkeys
                            .into_iter()
                            .map(|value| HotkeyOption {
                                label: value.name,
                                value: value.hotkey_id,
                            })
                            .collect(),
                    });
                });
            }
        }
    }

    fn on_tile_clicked(
        &self,
        _session: &PluginSessionHandle,
        ctx: TileInteractionContext,
        properties: serde_json::Value,
    ) {
        if ctx.action_id.as_str() == "trigger_hotkey" {
            let properties: TileProperties = match serde_json::from_value(properties) {
                Ok(value) => value,
                Err(_) => return,
            };

            let hotkey_id = match properties.hotkey_id {
                Some(value) => value,
                None => return,
            };

            let state = self.state.clone();
            tokio::spawn(async move {
                // Request the hotkeys from VT Studio
                if let Err(cause) = state
                    .send_message(&HotkeyTriggerRequest {
                        hotkey_id,
                        ..Default::default()
                    })
                    .await
                {
                    tracing::error!(?cause, "failed to get hotkeys in current model");
                }
            });
        }
    }
}
