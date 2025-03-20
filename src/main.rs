use std::sync::Arc;

use serde::{Deserialize, Serialize};
use state::{VtClientState, VtState, process_client_events};
use tilepad_plugin_sdk::{
    TilepadPlugin,
    protocol::{DeviceMessageContext, PluginMessageContext},
    socket::{PluginSession, PluginSessionRef},
};
use vtubestudio::data::{HotkeyTriggerRequest, HotkeysInCurrentModelRequest, StatisticsRequest};

pub mod state;

#[derive(Debug, Deserialize, Serialize)]
pub struct Settings {
    /// Store access token
    access_token: Option<String>,
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

#[tokio::main]
async fn main() {
    // Initialize a new client
    let (client, events) = vtubestudio::Client::builder().build_tungstenite();
    let state = VtState::new(client);

    tokio::spawn(process_client_events(state.clone(), events));

    TilepadPlugin::builder()
        .extension(state)
        // Got settings from the plugin server
        .on_settings(on_settings)
        .on_inspector_message(on_inspector_message)
        // Tile was clicked on the remote device
        .on_tile_clicked(on_tile_clicked)
        .build()
        .run()
        .await;
}

async fn on_settings(
    plugin: Arc<TilepadPlugin>,
    session: PluginSessionRef,
    settings: serde_json::Value,
) {
    let settings: Settings = serde_json::from_value(settings).expect("settings had invalid shape");
    let state = plugin.extension::<VtState>().expect("plugin state missing");

    // Set the active session
    state.set_plugin_session(session).await;

    // Attempt to authenticate the current session
    if let Some(access_token) = settings.access_token {
        let current_state = state.get_client_state().await;
        if !matches!(current_state, VtClientState::Authenticated) {
            _ = state.authenticate(access_token).await;
        }
    }

    _ = state.send_message(&StatisticsRequest {}).await;
}

async fn on_inspector_message(
    plugin: Arc<TilepadPlugin>,
    session: Arc<PluginSession>,
    ctx: PluginMessageContext,
    msg: serde_json::Value,
) {
    let msg: InspectorMessageIn = match serde_json::from_value(msg) {
        Ok(value) => value,
        Err(_) => return,
    };

    println!("Got inspector message: {msg:?}");

    let state = plugin.extension::<VtState>().expect("plugin state missing");
    state.set_inspector_ctx(ctx.clone()).await;

    match msg {
        InspectorMessageIn::GetHotkeyOptions => on_get_hotkey_options(plugin, session, ctx).await,
        InspectorMessageIn::GetConnected => {
            let current_state = state.get_client_state().await;

            // Send the available options to the inspector
            session
                .send_to_inspector(
                    ctx,
                    InspectorMessageOut::Connected {
                        connected: !matches!(current_state, VtClientState::Disconnected),
                    },
                )
                .unwrap();
        }
        InspectorMessageIn::GetAuthorized => {
            let current_state = state.get_client_state().await;

            // Send the available options to the inspector
            session
                .send_to_inspector(
                    ctx,
                    InspectorMessageOut::Authorized {
                        authorized: matches!(current_state, VtClientState::Authenticated),
                    },
                )
                .unwrap();
        }
        InspectorMessageIn::Authorize => {
            let _ = state.request_authenticate().await;
        }
    }
}

async fn on_get_hotkey_options(
    plugin: Arc<TilepadPlugin>,
    session: Arc<PluginSession>,
    ctx: PluginMessageContext,
) {
    // Get the current client
    let state = plugin.extension::<VtState>().expect("plugin state missing");

    // Request the hotkeys from VT Studio
    let result = match state
        .send_message(&HotkeysInCurrentModelRequest {
            ..Default::default()
        })
        .await
    {
        Ok(response) => response,
        Err(err) => {
            // HANDLE ERROR
            return;
        }
    };

    // Send the available options to the inspector
    session
        .send_to_inspector(
            ctx,
            InspectorMessageOut::HotkeyOptions {
                options: result
                    .available_hotkeys
                    .into_iter()
                    .map(|value| HotkeyOption {
                        label: value.name,
                        value: value.hotkey_id,
                    })
                    .collect(),
            },
        )
        .unwrap();
}

#[derive(Deserialize)]
pub struct TileProperties {
    pub hotkey_id: Option<String>,
}

async fn on_tile_clicked(
    plugin: Arc<TilepadPlugin>,
    session: Arc<PluginSession>,
    ctx: DeviceMessageContext,
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

        // TODO: Run the action
        println!("TRIGGER HOTKEY");
        // Get the current client
        let state = plugin.extension::<VtState>().expect("plugin state missing");

        // Request the hotkeys from VT Studio
        match state
            .send_message(&HotkeyTriggerRequest {
                hotkey_id,
                ..Default::default()
            })
            .await
        {
            Ok(response) => response,

            Err(err) => {
                // HANDLE ERROR
                return;
            }
        };
    }
}
