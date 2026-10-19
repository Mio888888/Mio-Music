//! Persisted desktop-compatible audio effects. Only balance currently drives DSP.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurroundMode {
    Off,
    Small,
    Medium,
    Large,
}

/// `useAudioEffectsStore`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioEffects {
    pub surround_enabled: bool,
    pub surround_mode: SurroundMode,
    pub balance_enabled: bool,
    pub balance: f64,
}
impl Default for AudioEffects {
    fn default() -> Self {
        Self {
            surround_enabled: false,
            surround_mode: SurroundMode::Off,
            balance_enabled: true,
            balance: 0.,
        }
    }
}

impl AudioEffects {
    pub fn validate(&self) -> Result<(), String> {
        if !self.balance.is_finite() || !(-1.0..=1.0).contains(&self.balance) {
            return Err("invalid_input: channel balance".into());
        }
        Ok(())
    }
    pub fn apply(&self) {
        crate::effects::shared().set_balance(if self.balance_enabled {
            self.balance
        } else {
            0.0
        });
    }
}
