use std::{borrow::Cow, sync::Arc};

use tilepad_plugin_sdk::{protocol::PluginMessageContext, socket::PluginSessionRef};
use tokio::sync::Mutex;
use vtubestudio::{
    ClientEvent, ClientEventStream,
    data::{
        AuthenticationRequest, AuthenticationResponse, AuthenticationTokenRequest,
        AuthenticationTokenResponse,
    },
};

use crate::{InspectorMessageOut, Settings};

#[derive(Default, Debug, Clone, Copy)]
pub enum VtClientState {
    #[default]
    Disconnected,
    Connected,
    Authenticated,
}

#[derive(Clone)]
pub struct VtState {
    inner: Arc<Mutex<VtStateInner>>,
}

pub struct VtStateInner {
    client: vtubestudio::Client,
    session: Option<PluginSessionRef>,
    inspector_ctx: Option<PluginMessageContext>,
    state: VtClientState,
}

impl VtState {
    pub fn new(client: vtubestudio::Client) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VtStateInner {
                client,
                session: None,
                inspector_ctx: None,
                state: Default::default(),
            })),
        }
    }

    pub async fn get_inspector_ctx(&self) -> Option<PluginMessageContext> {
        self.inner.lock().await.inspector_ctx.clone()
    }

    pub async fn set_inspector_ctx(&self, inspector_ctx: PluginMessageContext) {
        self.inner.lock().await.inspector_ctx = Some(inspector_ctx);
    }

    pub async fn get_plugin_session(&self) -> Option<PluginSessionRef> {
        self.inner.lock().await.session.clone()
    }

    pub async fn set_plugin_session(&self, session: PluginSessionRef) {
        self.inner.lock().await.session = Some(session);
    }

    pub async fn authenticate(&self, access_token: String) {
        let response = self
            .send_message(&AuthenticationRequest {
                plugin_name: Cow::Borrowed("Tilepad VT Studio"),
                plugin_developer: Cow::Borrowed("Jacobtread"),
                authentication_token: access_token,
            })
            .await
            .unwrap();
        if response.authenticated {
            self.set_client_state(VtClientState::Authenticated).await;
        }
    }

    pub async fn request_authenticate(&self) {
        let response = match self
            .send_message(&AuthenticationTokenRequest {
                plugin_name: Cow::Borrowed("Tilepad VT Studio"),
                plugin_developer: Cow::Borrowed("Jacobtread"),
                plugin_icon: None,
            })
            .await
        {
            Ok(value) => value,
            Err(err) => {
                eprintln!("Error requesting token: {err:?}");
                return;
            }
        };

        self.set_auth_token(Some(response.authentication_token.clone()))
            .await;

        self.authenticate(response.authentication_token.clone())
            .await;
    }

    pub async fn set_auth_token(&self, token: Option<String>) {
        if let Some(session) = self.get_plugin_session().await {
            // Update token stored in settings
            _ = session.set_settings(Settings {
                access_token: token.clone(),
            });
        }
    }

    pub async fn get_client_state(&self) -> VtClientState {
        self.inner.lock().await.state
    }

    pub async fn set_client_state(&self, state: VtClientState) {
        self.inner.lock().await.state = state;

        let session = self.get_plugin_session().await;
        let ctx = self.get_inspector_ctx().await;

        if let (Some(session), Some(ctx)) = (session, ctx) {
            match state {
                VtClientState::Disconnected => {
                    _ = session.send_to_inspector(
                        ctx.clone(),
                        InspectorMessageOut::Connected { connected: false },
                    );
                    _ = session.send_to_inspector(
                        ctx,
                        InspectorMessageOut::Authorized { authorized: false },
                    );
                }
                VtClientState::Connected => {
                    _ = session.send_to_inspector(
                        ctx.clone(),
                        InspectorMessageOut::Connected { connected: true },
                    );
                    _ = session.send_to_inspector(
                        ctx,
                        InspectorMessageOut::Authorized { authorized: false },
                    );
                }
                VtClientState::Authenticated => {
                    _ = session.send_to_inspector(
                        ctx,
                        InspectorMessageOut::Authorized { authorized: true },
                    );
                }
            }
        }
    }

    pub async fn send_message<R: vtubestudio::data::Request>(
        &self,
        msg: &R,
    ) -> Result<R::Response, vtubestudio::Error> {
        let mut state = self.inner.lock().await;
        match state.client.send(msg).await {
            Ok(value) => Ok(value),
            Err(err) => {
                if err.is_unauthenticated_error() {
                    drop(state);
                    self.set_auth_token(None).await;
                    self.set_client_state(VtClientState::Connected).await;
                    println!("GOT AUTH ERROR");
                }
                Err(err)
            }
        }
    }
}

pub async fn process_client_events(state: VtState, mut events: ClientEventStream) {
    while let Some(event) = events.next().await {
        println!("Got event: {:?}", event);

        match event {
            // Disconnected from VT studio
            ClientEvent::Disconnected => {
                state.set_client_state(VtClientState::Disconnected).await;
            }

            // Socket connected to VT studio
            ClientEvent::Connected => {
                state.set_client_state(VtClientState::Connected).await;
            }

            _ => {}
        }
    }
}
