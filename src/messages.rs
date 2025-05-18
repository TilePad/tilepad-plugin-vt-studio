use crate::state::VtClientState;
use serde::{Deserialize, Serialize};

/// Messages from the inspector
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InspectorMessageIn {
    GetHotkeyOptions { model_id: String },
    GetModelOptions,
    GetVtState,
    Authorize,
}

/// Messages to the inspector
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InspectorMessageOut {
    HotkeyOptions { options: Vec<SelectOption> },
    ModelOptions { options: Vec<SelectOption> },
    VtState { state: VtClientState },
}

/// Option for a select dropdown menu
#[derive(Deserialize, Serialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}
