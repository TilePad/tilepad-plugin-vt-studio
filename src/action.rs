use serde::Deserialize;

pub enum Action {
    TriggerHotkey(TriggerHotkeyTileProperties),
    SwitchModel(SwitchModelTileProperties),
}

impl Action {
    pub fn from_action(
        action_id: &str,
        properties: serde_json::Value,
    ) -> Option<Result<Action, serde_json::Error>> {
        Some(match action_id {
            "trigger_hotkey" => serde_json::from_value(properties).map(Action::TriggerHotkey),
            "switch_model" => serde_json::from_value(properties).map(Action::SwitchModel),
            _ => return None,
        })
    }
}

/// Properties for a "TriggerHotkey" tile
#[derive(Deserialize)]
pub struct TriggerHotkeyTileProperties {
    /// Currently selected option
    pub hotkey_id: Option<String>,
}

/// Properties for a "TriggerHotkey" tile
#[derive(Deserialize)]
pub struct SwitchModelTileProperties {
    /// Currently selected option
    pub model_id: Option<String>,
}
