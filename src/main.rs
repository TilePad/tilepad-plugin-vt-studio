use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tilepad_plugin_sdk::{
    TilepadPlugin,
    protocol::{DeviceMessageContext, PluginMessageContext},
    socket::{PluginSession, PluginSessionRef},
};
use tokio::{sync::Mutex, time::sleep};
use vtubestudio::{
    ClientEvent,
    data::{HotkeyTriggerRequest, HotkeysInCurrentModelRequest, StatisticsRequest},
};

pub struct VtState {
    client: Mutex<Option<vtubestudio::Client>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Settings {
    /// Store access token
    access_token: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InspectorMessageIn {
    GetHotkeyOptions,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InspectorMessageOut {
    HotkeyOptions { options: Vec<HotkeyOption> },
}

#[derive(Deserialize, Serialize)]
pub struct HotkeyOption {
    label: String,
    value: String,
}

#[tokio::main]
async fn main() {
    TilepadPlugin::builder()
        .extension(Arc::new(VtState {
            client: Default::default(),
        }))
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

    let state = plugin
        .extension::<Arc<VtState>>()
        .expect("plugin state missing");

    let events = {
        // Check we aren't already connected
        let mut client_slot = state.client.lock().await;
        if client_slot.is_some() {
            return;
        }

        // Attempt to connect our new client
        let (client, events) = vtubestudio::Client::builder()
            .auth_token(settings.access_token)
            .authentication("Tilepad VT Studio", "Jacobtread", None)
            .build_tungstenite();

        // Store the current client
        *client_slot = Some(client);
        events
    };

    // Handle background events
    tokio::spawn({
        let state = state.clone();
        let session = session.clone();
        let mut events = events;

        async move {
            while let Some(event) = events.next().await {
                match event {
                    ClientEvent::Disconnected => loop {
                        println!("Got event: {:?}", event);
                        let mut client_lock = { state.client.lock().await };
                        let client = match client_lock.as_mut() {
                            Some(value) => value,
                            None => return,
                        };

                        // Send a statistics request to attempt to reconnect every 5 seconds of being disconnected
                        if client.send(&StatisticsRequest {}).await.is_err() {
                            drop(client_lock);
                            sleep(Duration::from_secs(5)).await;
                        } else {
                            break;
                        }
                    },
                    ClientEvent::NewAuthToken(new_token) => {
                        // Update token store in settings
                        // session.setSettings()
                    }
                    _ => {
                        // Other events, such as connections/disconnections, API events, etc
                        println!("Got event: {:?}", event);
                    }
                }
            }
        }
    });
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

    match msg {
        InspectorMessageIn::GetHotkeyOptions => on_get_hotkey_options(plugin, session, ctx).await,
    }
}

async fn on_get_hotkey_options(
    plugin: Arc<TilepadPlugin>,
    session: Arc<PluginSession>,
    ctx: PluginMessageContext,
) {
    // Get the current client
    let state = plugin
        .extension::<Arc<VtState>>()
        .expect("plugin state missing");

    let client = { state.client.lock().await.clone() };

    let mut client = match client {
        Some(value) => value,
        None => return,
    };

    // Request the from VT Studio
    let result = client
        .send(&HotkeysInCurrentModelRequest {
            ..Default::default()
        })
        .await
        .unwrap();

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
        let state = plugin
            .extension::<Arc<VtState>>()
            .expect("plugin state missing");

        let client = { state.client.lock().await.clone() };

        let mut client = match client {
            Some(value) => value,
            None => return,
        };

        // Request the from VT Studio
        _ = client
            .send(&HotkeyTriggerRequest {
                hotkey_id,
                ..Default::default()
            })
            .await
            .unwrap();
    }
}
