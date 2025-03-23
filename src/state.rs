use std::{borrow::Cow, cell::RefCell, rc::Rc, time::Duration};

use serde::Serialize;
use tilepad_plugin_sdk::{inspector::Inspector, session::PluginSessionHandle, tracing};
use tokio::{sync::Mutex, task::spawn_local, time::sleep};
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

pub struct VtState {
    /// VTube studio client
    client: tokio::sync::Mutex<vtubestudio::Client>,
    /// State of the VT client
    state: RefCell<VtClientState>,

    /// Session handle for messaging the server
    session: RefCell<Option<PluginSessionHandle>>,
    /// Inspector context for messaging the inspector
    inspector: RefCell<Option<Inspector>>,
}

impl VtState {
    pub fn new(client: vtubestudio::Client) -> Self {
        Self {
            client: Mutex::new(client),
            session: Default::default(),
            inspector: Default::default(),
            state: Default::default(),
        }
    }

    pub fn get_inspector(&self) -> Option<Inspector> {
        self.inspector.borrow().clone()
    }

    pub fn set_inspector(&self, inspector: Option<Inspector>) {
        *self.inspector.borrow_mut() = inspector;
    }

    pub fn get_plugin_session(&self) -> Option<PluginSessionHandle> {
        self.session.borrow().clone()
    }

    pub fn set_plugin_session(&self, session: PluginSessionHandle) {
        let _ = self.session.borrow_mut().insert(session);
    }

    /// Get the current state of the VTube Studio client
    pub fn get_client_state(&self) -> VtClientState {
        *self.state.borrow()
    }

    /// Set the current state of the VTube Studio client
    pub fn set_client_state(&self, state: VtClientState) {
        {
            *self.state.borrow_mut() = state;
        }

        // Report the change in state to the inspector
        if let Some(inspector) = self.get_inspector() {
            _ = inspector.send(InspectorMessageOut::VtState { state });
        }
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
            Err(cause) => {
                tracing::error!(?cause, "failed to authenticate");
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

    pub async fn send_message<R: vtubestudio::data::Request>(
        &self,
        msg: &R,
    ) -> Result<R::Response, vtubestudio::Error> {
        let result = {
            let mut client = self.client.lock().await;
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

    pub fn set_auth_token(&self, token: Option<String>) {
        if let Some(session) = self.get_plugin_session() {
            // Update token stored in settings
            _ = session.set_properties(Properties {
                access_token: token.clone(),
            });
        }
    }
}

/// Task to ping the client in the background every 5 seconds
pub async fn ping_till_connected(state: Rc<VtState>) {
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

pub async fn process_client_events(state: Rc<VtState>, mut events: ClientEventStream) {
    while let Some(event) = events.next().await {
        tracing::debug!(?event, "received vt studio client event");

        match event {
            // Disconnected from VT studio
            ClientEvent::Disconnected => {
                state.set_client_state(VtClientState::Disconnected);

                // Send a state request to hopefully wake up the client
                spawn_local(ping_till_connected(state.clone()));
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
