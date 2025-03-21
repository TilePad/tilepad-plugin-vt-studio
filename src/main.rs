use plugin::VtPlugin;
use state::{VtState, process_client_events};
use tilepad_plugin_sdk::{
    start_plugin, tracing,
    tracing_subscriber::{self, EnvFilter},
};

pub mod plugin;
pub mod state;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let filter = EnvFilter::from_default_env();
    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_file(true)
        .with_env_filter(filter)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(false)
        .finish();

    // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber).expect("failed to setup tracing");

    // Initialize a new client
    let (client, events) = vtubestudio::Client::builder().build_tungstenite();
    let state = VtState::new(client);
    tokio::spawn(process_client_events(state.clone(), events));

    let plugin = VtPlugin { state };
    start_plugin(plugin).await;
}
