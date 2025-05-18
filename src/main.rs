use plugin::VtPlugin;
use state::{VtState, process_client_events};
use std::rc::Rc;
use tilepad_plugin_sdk::{setup_tracing, start_plugin};
use tokio::task::{LocalSet, spawn_local};

pub mod plugin;
pub mod state;

fn main() {
    let local_set = LocalSet::new();
    setup_tracing();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime");

    local_set.block_on(&rt, async move {
        let (client, events) = vtubestudio::Client::builder()
            .retry_on_disconnect(false)
            .build_tungstenite();
        let state = Rc::new(VtState::new(client));
        let plugin = VtPlugin::new(state.clone());
        spawn_local(process_client_events(state, events));
        start_plugin(plugin).await;
    });
}
