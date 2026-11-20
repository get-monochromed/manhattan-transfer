mod allowed;
mod app;
mod consent;
mod dbus;
mod folder_transfer;
mod history;
mod link;
mod local_identity;
mod local_send;
mod persist;
mod ratelimit;
mod requirements;
mod safety;
mod settings;
mod trust;
mod wifi_credentials;

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

use crate::requirements::{EngineConfig, SystemEngine, SystemRequirementsProbe};
use crate::{app::Core, history::History, settings::Settings, trust::Trust};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,mdns_sd=error,bluez_async=error")),
        )
        .init();

    let settings_path = settings::default_path();
    let mut settings = settings_path
        .as_deref()
        .map(Settings::load)
        .unwrap_or_default();
    let device_name = std::env::var("NEARBY_DEVICE_NAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .or(settings.device_name.clone())
        .unwrap_or_else(settings::default_device_name);
    let destination = std::env::var_os("NEARBY_DOWNLOAD_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::download_dir().map(|dir| dir.join("Nearby Transfer")))
        .context("No downloads directory. Set NEARBY_DOWNLOAD_DIR to a writable path.")?;
    std::fs::create_dir_all(&destination).context("Could not create the download directory")?;
    let destination = std::fs::canonicalize(destination)?;
    settings.device_name = Some(device_name.clone());
    settings.download_dir = Some(destination.clone());
    settings.wifi_upgrade = false;
    settings.wifi_auto_join = false;
    settings.trusted_devices = false;
    settings.history_enabled = false;
    settings.allowed_devices.clear();
    if let Ok(port) = std::env::var("NEARBY_QUICK_SHARE_PORT") {
        settings.static_port = Some(
            port.parse::<u16>()
                .map(u32::from)
                .context("Invalid Quick Share port")?,
        );
    }

    let history = History::load(history::default_path());
    let (trust_tx, trust_rx) = tokio::sync::mpsc::unbounded_channel();
    let trust = Trust::load(
        trust::default_dir().as_deref(),
        device_name.clone(),
        trust_tx,
    )?;
    let core = Arc::new(Mutex::new(Core::new(
        settings,
        settings_path,
        history,
        trust,
        trust_rx,
    )));
    core.lock().await.set_handle(Arc::downgrade(&core));
    let requirements = SystemEngine::new(SystemRequirementsProbe::new(), EngineConfig::default());
    core.lock().await.set_requirements(requirements.clone());
    let consents = core.lock().await.consents();
    let (quit_tx, mut quit_rx) = tokio::sync::mpsc::unbounded_channel();
    let connection = dbus::serve(core.clone(), consents, requirements, quit_tx).await?;

    let quick_error = core.lock().await.start(connection.clone()).await.err();
    if let Some(error) = &quick_error {
        tracing::warn!("Quick Share unavailable: {error:#}");
    }
    let local = local_send::start(&connection, device_name, destination, quick_error).await?;
    tokio::spawn(app::watch_network(core.clone()));

    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result?,
        _ = terminate.recv() => {},
        _ = quit_rx.recv() => {},
    }
    if let Some(local) = local {
        local.stop();
    }
    let _ = tokio::time::timeout(app::SHUTDOWN_ENGINE_BOUND, async {
        core.lock().await.shutdown().await
    })
    .await;
    Ok(())
}
