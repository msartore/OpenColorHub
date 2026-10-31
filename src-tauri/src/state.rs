//! Shared application state: the core service plus persisted settings.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use colorctl_core::Service;
use colorctl_core::settings::Settings;

/// Everything guarded by the single application mutex.
///
/// One mutex is deliberate: hardware access must be serialised (register banks
/// and the HID frame are global resources), and the ticker plus IPC commands
/// are the only writers.
/// The hardware probe takes a few hundred milliseconds, so it runs after the
/// window is on screen. Until it finishes the slot is `Loading` and commands
/// answer with a "still starting" error the UI can retry.
pub enum ServiceSlot {
    Loading,
    Ready(Box<Service>),
    Failed(String),
}

pub struct Inner {
    pub service: ServiceSlot,
    pub settings: Settings,
    pub settings_path: PathBuf,
    /// Set when `settings.json` could not be parsed; surfaced in the UI.
    pub settings_warning: Option<String>,
    pub data_dir: PathBuf,
}

pub struct AppState {
    inner: Mutex<Inner>,
}

impl AppState {
    pub fn new(inner: Inner) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }

    /// Locks the state, mapping a poisoned mutex to a readable error instead of
    /// panicking the IPC handler.
    pub fn data_dir(&self) -> PathBuf {
        self.lock()
            .map(|inner| inner.data_dir.clone())
            .unwrap_or_default()
    }

    pub fn lock(&self) -> Result<MutexGuard<'_, Inner>, String> {
        self.inner
            .lock()
            .map_err(|_| "internal state lock poisoned — restart opencolorhub".to_string())
    }
}

impl Inner {
    /// Message returned while the probe is still running.
    pub const STARTING: &'static str = "still starting: probing the hardware";

    pub fn service(&self) -> Result<&Service, String> {
        match &self.service {
            ServiceSlot::Ready(service) => Ok(service.as_ref()),
            ServiceSlot::Loading => Err(Self::STARTING.to_string()),
            ServiceSlot::Failed(error) => Err(error.clone()),
        }
    }

    pub fn service_mut(&mut self) -> Result<&mut Service, String> {
        match &mut self.service {
            ServiceSlot::Ready(service) => Ok(service.as_mut()),
            ServiceSlot::Loading => Err(Self::STARTING.to_string()),
            ServiceSlot::Failed(error) => Err(error.clone()),
        }
    }

    /// Persists the current settings (atomic write in the core crate).
    pub fn persist_settings(&self) -> Result<(), String> {
        self.settings
            .save(&self.settings_path)
            .map_err(|e| format!("could not save settings: {e}"))
    }
}
