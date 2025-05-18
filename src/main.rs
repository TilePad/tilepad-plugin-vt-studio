use plugin::VtPlugin;
use state::{VtState, process_client_events};
use std::rc::Rc;
use tilepad_plugin_sdk::{setup_tracing, start_plugin};
use tokio::task::LocalSet;

mod action;
mod messages;
mod plugin;
mod state;

fn main() {
    setup_tracing();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime");
    let local_set = LocalSet::new();

    let _rt_guard = rt.enter();
    let _guard = local_set.enter();

    let (client, events) = vtubestudio::Client::builder()
        .retry_on_disconnect(false)
        .build_tungstenite();
    let state = Rc::new(VtState::new(client));
    let plugin = VtPlugin::new(state.clone());

    local_set.spawn_local(process_client_events(state, events));
    local_set.block_on(&rt, start_plugin(plugin));
}
