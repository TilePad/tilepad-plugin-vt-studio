use serde::{Deserialize, Serialize};
use tilepad_plugin_sdk::{
    inspector::Inspector, plugin::Plugin, protocol::TileInteractionContext,
    session::PluginSessionHandle, tracing,
};
use tokio::task::spawn_local;
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
    GetVtState,
    Authorize,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InspectorMessageOut {
    HotkeyOptions { options: Vec<HotkeyOption> },
    VtState { state: VtClientState },
}

#[derive(Deserialize, Serialize)]
pub struct HotkeyOption {
    label: String,
    value: String,
}

/// Properties for a "TriggerHotkey" tile
#[derive(Deserialize)]
pub struct TriggerHotkeyTileProperties {
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
            None => {
                self.state.set_client_state(VtClientState::NotAuthorized);
                return;
            }
        };

        let current_state = self.state.get_client_state();

        // Don't try and authenticate if already authenticated
        if matches!(current_state, VtClientState::Authorized) {
            return;
        }

        let state = self.state.clone();
        spawn_local(async move { state.authenticate(access_token).await });
    }

    fn on_inspector_open(&self, _session: &PluginSessionHandle, inspector: Inspector) {
        self.state.set_inspector(Some(inspector));
    }

    fn on_inspector_close(&self, _session: &PluginSessionHandle, _inspector: Inspector) {
        self.state.set_inspector(None);
    }

    fn on_inspector_message(
        &self,
        _session: &PluginSessionHandle,
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

        match msg {
            InspectorMessageIn::GetVtState => {
                let current_state = self.state.get_client_state();

                _ = inspector.send(InspectorMessageOut::VtState {
                    state: current_state,
                });
            }
            InspectorMessageIn::Authorize => {
                let state = self.state.clone();

                spawn_local(async move {
                    let token = state.request_authenticate().await;
                    if let Some(token) = token {
                        state.set_auth_token(Some(token));
                    }
                });
            }
            InspectorMessageIn::GetHotkeyOptions => {
                let state = self.state.clone();

                spawn_local(async move {
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
        match ctx.action_id.as_str() {
            "trigger_hotkey" => {
                let properties: TriggerHotkeyTileProperties =
                    match serde_json::from_value(properties) {
                        Ok(value) => value,
                        Err(cause) => {
                            tracing::error!(?cause, "failed to parse trigger_hotkey configuration");
                            return;
                        }
                    };

                let hotkey_id = match properties.hotkey_id {
                    Some(value) => value,

                    // No hotkey configured, ignore the tile click
                    None => return,
                };

                let state = self.state.clone();
                spawn_local(async move {
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
            "switch_model" => {
                // TODO: Not implemented
            }

            action_id => {
                // Unknown action requested
                tracing::debug!(?action_id, "unknown tile action requested")
            }
        }
    }
}
