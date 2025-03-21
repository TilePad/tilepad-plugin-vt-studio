use std::{borrow::Cow, sync::Arc, time::Duration};

use serde::Serialize;
use tilepad_plugin_sdk::{inspector::Inspector, session::PluginSessionHandle, tracing};
use tokio::{sync::Mutex, time::sleep};
use vtubestudio::{
    ClientEvent, ClientEventStream,
    data::{ApiStateRequest, AuthenticationRequest, AuthenticationTokenRequest},
};

use crate::plugin::{InspectorMessageOut, Properties};

#[derive(Default, Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VtClientState {
    #[default]
    Disconnected,
    Connected,
    NotAuthorized,
    Authorized,
}

#[derive(Clone)]
pub struct VtState {
    inner: Arc<VtStateInner>,
}

pub struct VtStateInner {
    client: tokio::sync::Mutex<vtubestudio::Client>,

    // Session handle for messaging the server
    session: parking_lot::Mutex<Option<PluginSessionHandle>>,

    // Inspector context for messaging the inspector
    inspector: parking_lot::Mutex<Option<Inspector>>,

    // State of the client
    state: parking_lot::Mutex<VtClientState>,
}

impl VtState {
    pub fn new(client: vtubestudio::Client) -> Self {
        Self {
            inner: Arc::new(VtStateInner {
                client: Mutex::new(client),
                session: Default::default(),
                inspector: Default::default(),
                state: Default::default(),
            }),
        }
    }

    pub fn get_inspector(&self) -> Option<Inspector> {
        self.inner.inspector.lock().clone()
    }

    pub fn set_inspector(&self, inspector: Option<Inspector>) {
        *self.inner.inspector.lock() = inspector;
    }

    pub fn get_plugin_session(&self) -> Option<PluginSessionHandle> {
        self.inner.session.lock().clone()
    }

    pub fn set_plugin_session(&self, session: PluginSessionHandle) {
        let _ = self.inner.session.lock().insert(session);
    }

    pub async fn authenticate(&self, access_token: String) {
        tracing::debug!("authenticating token");
        let response = match self
            .send_message(&AuthenticationRequest {
                plugin_name: Cow::Borrowed("Tilepad VT Studio"),
                plugin_developer: Cow::Borrowed("Jacobtread"),
                authentication_token: access_token,
            })
            .await
        {
            Ok(value) => value,
            Err(err) => {
                return;
            }
        };

        if response.authenticated {
            tracing::debug!("authenticated");
            self.set_client_state(VtClientState::Authorized);
        } else {
            tracing::debug!("failed to authenticate");
            self.set_client_state(VtClientState::NotAuthorized);
        }
    }

    pub async fn request_authenticate(&self) -> Option<String> {
        tracing::debug!("requesting authentication token");

        let response = match self
            .send_message(&AuthenticationTokenRequest {
                plugin_name: Cow::Borrowed("Tilepad VT Studio"),
                plugin_developer: Cow::Borrowed("Jacobtread"),
                plugin_icon: None,
            })
            .await
        {
            Ok(value) => value,
            Err(cause) => {
                tracing::error!(?cause, "error requesting token");
                return None;
            }
        };

        tracing::debug!("obtained authentication token");

        self.authenticate(response.authentication_token.clone())
            .await;

        Some(response.authentication_token.clone())
    }

    pub fn set_auth_token(&self, token: Option<String>) {
        if let Some(session) = self.get_plugin_session() {
            // Update token stored in settings
            _ = session.set_properties(Properties {
                access_token: token.clone(),
            });
        }
    }

    pub fn get_client_state(&self) -> VtClientState {
        *self.inner.state.lock()
    }

    pub fn set_client_state(&self, state: VtClientState) {
        {
            *self.inner.state.lock() = state;
        }

        let inspector = self.get_inspector();

        if let Some(inspector) = inspector {
            _ = inspector.send(InspectorMessageOut::VtState { state });
        }
    }

    pub async fn send_message<R: vtubestudio::data::Request>(
        &self,
        msg: &R,
    ) -> Result<R::Response, vtubestudio::Error> {
        let result = {
            let mut client = self.inner.client.lock().await;
            client.send(msg).await
        };

        match result {
            Ok(value) => Ok(value),
            Err(err) => {
                if err.is_unauthenticated_error() {
                    self.set_auth_token(None);
                    self.set_client_state(VtClientState::NotAuthorized);
                }

                Err(err)
            }
        }
    }
}

pub async fn process_client_events(state: VtState, mut events: ClientEventStream) {
    while let Some(event) = events.next().await {
        tracing::debug!(?event, "received vt studio client event");

        match event {
            // Disconnected from VT studio
            ClientEvent::Disconnected => {
                state.set_client_state(VtClientState::Disconnected);

                // Send a state request to hopefully wake up the client
                tokio::spawn({
                    let state = state.clone();
                    async move {
                        // Poll the server every 5 seconds until we are no longer disconnected
                        loop {
                            _ = state.send_message(&ApiStateRequest {}).await;
                            sleep(Duration::from_secs(5)).await;

                            let state = state.get_client_state();
                            if !matches!(state, VtClientState::Disconnected) {
                                break;
                            }
                        }
                    }
                });
            }

            // Socket connected to VT studio
            ClientEvent::Connected => {
                state.set_client_state(VtClientState::Connected);

                // Request the session properties to trigger authentication
                if let Some(session) = state.get_plugin_session() {
                    _ = session.get_properties();
                }
            }

            _ => {}
        }
    }
}
