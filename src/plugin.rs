use std::rc::Rc;

use serde::{Deserialize, Serialize};
use tilepad_plugin_sdk::{Inspector, Plugin, PluginSessionHandle, TileInteractionContext, tracing};
use tokio::task::spawn_local;
use vtubestudio::data::{
    AvailableModelsRequest, HotkeyTriggerRequest, HotkeysInCurrentModelRequest, ModelLoadRequest,
};

use crate::{
    action::Action,
    messages::{InspectorMessageIn, InspectorMessageOut, SelectOption},
    state::{VtClientState, VtState},
};

/// Properties for the plugin itself
#[derive(Debug, Deserialize, Serialize)]
pub struct Properties {
    /// Store access token
    pub access_token: Option<String>,
}

pub struct VtPlugin {
    state: Rc<VtState>,
}

impl VtPlugin {
    pub fn new(state: Rc<VtState>) -> Self {
        Self { state }
    }
}

impl Plugin for VtPlugin {
    fn on_properties(&mut self, session: &PluginSessionHandle, properties: serde_json::Value) {
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

    fn on_inspector_open(&mut self, _session: &PluginSessionHandle, inspector: Inspector) {
        self.state.set_inspector(Some(inspector));
    }

    fn on_inspector_close(&mut self, _session: &PluginSessionHandle, _inspector: Inspector) {
        self.state.set_inspector(None);
    }

    fn on_inspector_message(
        &mut self,
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
            InspectorMessageIn::GetHotkeyOptions { model_id } => {
                let state = self.state.clone();

                spawn_local(async move {
                    // Request the hotkeys from VT Studio
                    let result = match state
                        .send_message(&HotkeysInCurrentModelRequest {
                            model_id: Some(model_id),
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
                            .map(|value| SelectOption {
                                label: value.name,
                                value: value.hotkey_id,
                            })
                            .collect(),
                    });
                });
            }
            InspectorMessageIn::GetModelOptions => {
                let state = self.state.clone();

                spawn_local(async move {
                    // Request the models from VT Studio
                    let result = match state.send_message(&AvailableModelsRequest {}).await {
                        Ok(response) => response,
                        Err(cause) => {
                            tracing::error!(?cause, "failed to get hotkeys in current model");
                            return;
                        }
                    };

                    // Send the available options to the inspector
                    _ = inspector.send(InspectorMessageOut::ModelOptions {
                        options: result
                            .available_models
                            .into_iter()
                            .map(|value| SelectOption {
                                label: value.model_name,
                                value: value.model_id,
                            })
                            .collect(),
                    });
                });
            }
        }
    }

    fn on_tile_clicked(
        &mut self,
        _session: &PluginSessionHandle,
        ctx: TileInteractionContext,
        properties: serde_json::Value,
    ) {
        let action_id = ctx.action_id.as_str();
        let action = match Action::from_action(action_id, properties) {
            Some(Ok(value)) => value,
            Some(Err(cause)) => {
                tracing::error!(?cause, ?action_id, "failed to deserialize action");
                return;
            }
            None => {
                tracing::debug!(?action_id, "unknown tile action requested");
                return;
            }
        };

        match action {
            Action::TriggerHotkey(properties) => {
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
                        tracing::error!(?cause, "failed to trigger hotkey");
                    }
                });
            }
            Action::SwitchModel(properties) => {
                let model_id = match properties.model_id {
                    Some(value) => value,

                    // No hotkey configured, ignore the tile click
                    None => return,
                };

                let state = self.state.clone();
                spawn_local(async move {
                    // Request the hotkeys from VT Studio
                    if let Err(cause) = state.send_message(&ModelLoadRequest { model_id }).await {
                        tracing::error!(?cause, "failed to load model");
                    }
                });
            }
        }
    }
}
