use std::rc::Rc;

use plugin::VtPlugin;
use state::{VtState, process_client_events};
use tilepad_plugin_sdk::{
    start_plugin, tracing,
    tracing_subscriber::{self, EnvFilter},
};
use tokio::task::{LocalSet, spawn_local};

pub mod plugin;
pub mod state;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let local_set = LocalSet::new();

    let filter = EnvFilter::from_default_env();
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_env_filter(filter)
        .with_line_number(true)
        .with_thread_ids(false)
        .with_target(false)
        .with_ansi(false)
        .without_time()
        .finish();

    // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber).expect("failed to setup tracing");

    local_set
        .run_until(async move {
            // Initialize a new client
            let (client, events) = vtubestudio::Client::builder()
                .retry_on_disconnect(false)
                .build_tungstenite();

            let state = Rc::new(VtState::new(client));
            let plugin = VtPlugin::new(state.clone());

            spawn_local(process_client_events(state, events));

            start_plugin(plugin).await;
        })
        .await;
}
