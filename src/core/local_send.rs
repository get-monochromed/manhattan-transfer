//! LocalSend v2 integration. Networking and TLS are provided by the official LocalSend core.

use std::{
    collections::HashMap,
    io,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex as SyncMutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use bytes::Bytes;
use futures_util::StreamExt;
use localsend::{
    discovery::{
        DEFAULT_DISCOVERY_TIMEOUT, DeviceChannel, DeviceIdentity, DiscoveredDevice,
        DiscoveryConfig, DiscoveryHandle, HttpChannel,
    },
    http::{
        client::{ClientError, v2::LsHttpClientV2},
        dto_v2::{PrepareUploadRequestDtoV2, RegisterDtoV2},
        server::{
            ServerConfigV2, ServerHandle,
            common::save::FileUploadTarget,
            start_with_port,
            v2::{PrepareUploadDecisionV2, ServerEventV2, SessionEndReasonV2},
            web::WebConfig,
        },
    },
    model::{
        discovery::ProtocolType,
        transfer::{FileContent, FileDto},
    },
    multicast::{DEFAULT_MULTICAST_GROUP, DEFAULT_MULTICAST_GROUP_V6, DEFAULT_PORT},
    util::interface::InterfaceFilter,
};
use sha2::{Digest, Sha256};
use tokio::{
    io::AsyncWriteExt,
    sync::{Mutex, mpsc, oneshot},
};
use tokio_util::sync::CancellationToken;
use zbus::{Connection, fdo, interface};

use crate::{
    dbus::{self, TransferEvent},
    folder_transfer::{self, MAX_FILES, OfferedFiles},
    local_identity::Identity,
};

const CONSENT_TIMEOUT: Duration = Duration::from_secs(120);

pub struct Stop {
    server: oneshot::Sender<()>,
    discovery: oneshot::Sender<()>,
}

impl Stop {
    pub fn stop(self) {
        let _ = self.discovery.send(());
        let _ = self.server.send(());
    }
}

struct LocalSend {
    alias: String,
    destination: PathBuf,
    quick_error: String,
    network: Result<Arc<Network>, String>,
}

struct Network {
    bus: Connection,
    identity: Identity,
    server: ServerHandle,
    discovery: DiscoveryHandle,
    listener_error: Mutex<String>,
    incoming: Mutex<Option<Incoming>>,
    outgoing: Mutex<HashMap<String, Outgoing>>,
}

struct Incoming {
    event: TransferEvent,
    began: Instant,
    last_activity: Instant,
    text: Option<String>,
    decision: Option<oneshot::Sender<PrepareUploadDecisionV2>>,
    paths: HashMap<String, PathBuf>,
    roots: HashMap<String, PathBuf>,
    cancel: CancellationToken,
    active: HashMap<String, Arc<AtomicU64>>,
    saved_bytes: u64,
    failed: bool,
    ended: bool,
}

#[derive(Clone)]
struct Outgoing {
    cancel: CancellationToken,
    host: String,
    session: Arc<SyncMutex<Option<String>>>,
}

struct Saved {
    id: String,
    file_id: String,
    size: u64,
    path: Result<PathBuf>,
}

pub async fn start(
    bus: &Connection,
    alias: String,
    destination: PathBuf,
    quick_error: Option<anyhow::Error>,
) -> Result<Option<Stop>> {
    let (network, stop) = match start_network(bus.clone(), alias.clone(), destination.clone()).await
    {
        Ok((network, stop)) => (Ok(network), Some(stop)),
        Err(error) => {
            tracing::warn!("LocalSend unavailable: {error:#}");
            (Err(format!("{error:#}")), None)
        }
    };
    bus.object_server()
        .at(
            dbus::OBJECT_PATH,
            LocalSend {
                alias,
                destination,
                quick_error: quick_error
                    .map(|error| format!("{error:#}"))
                    .unwrap_or_default(),
                network,
            },
        )
        .await?;
    Ok(stop)
}

async fn start_network(
    bus: Connection,
    alias: String,
    destination: PathBuf,
) -> Result<(Arc<Network>, Stop)> {
    let dir = dirs::data_dir()
        .context("No user data directory")?
        .join("nearby-transfer");
    std::fs::create_dir_all(&dir)?;
    let mut identity = Identity::load_or_generate(&dir, alias, DEFAULT_PORT)?;
    let (server_tx, server_rx) = mpsc::channel(32);
    let launch = |port| {
        let (tx, rx) = oneshot::channel();
        let server = start_with_port(
            port,
            Some(identity.tls_config()),
            identity.client_info(),
            None,
            Some(ServerConfigV2 {
                pin: None,
                verify_checksums: true,
                event_tx: server_tx.clone(),
            }),
            WebConfig::default(),
            rx,
        );
        (server, tx)
    };
    let requested = std::env::var("NEARBY_LOCALSEND_PORT")
        .ok()
        .map(|port| port.parse::<u16>())
        .transpose()?;
    let (server, stop_server) = launch(requested.unwrap_or(DEFAULT_PORT));
    let (server, stop_server) = match server.await {
        Err(error)
            if requested.is_none()
                && error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|error| error.kind() == io::ErrorKind::AddrInUse) =>
        {
            let (server, stop) = launch(0);
            (server.await?, stop)
        }
        result => (result?, stop_server),
    };
    identity.port = server.port();
    tracing::info!(port = identity.port, "LocalSend HTTPS is listening");
    let (stop_discovery, discovery_rx) = oneshot::channel();
    let discovery = localsend::discovery::start(
        DiscoveryConfig {
            group: DEFAULT_MULTICAST_GROUP,
            group_v6: Some(DEFAULT_MULTICAST_GROUP_V6),
            port: DEFAULT_PORT,
            interface_filter: InterfaceFilter::default(),
            device: identity.multicast_device(),
            identity: DeviceIdentity {
                cert_pem: identity.cert_pem.clone(),
                private_key_pem: identity.key_pem.clone(),
            },
            timeout: DEFAULT_DISCOVERY_TIMEOUT,
            event_tx: None,
        },
        discovery_rx,
    )
    .await;
    let network = Arc::new(Network {
        bus,
        identity,
        server,
        discovery,
        listener_error: Mutex::new(String::new()),
        incoming: Mutex::new(None),
        outgoing: Mutex::new(HashMap::new()),
    });
    tokio::spawn(network.clone().run(server_rx, destination));
    tokio::spawn({
        let network = network.clone();
        async move {
            network.discovery.announce().await;
        }
    });
    Ok((
        network,
        Stop {
            server: stop_server,
            discovery: stop_discovery,
        },
    ))
}

fn failed(error: impl std::fmt::Display) -> fdo::Error {
    fdo::Error::Failed(error.to_string())
}

#[interface(name = "io.github.NearbyTransfer.LocalSend1")]
impl LocalSend {
    async fn status(&self) -> String {
        let (port, error, discovery) = match &self.network {
            Ok(net) => (
                net.identity.port,
                net.listener_error.lock().await.clone(),
                net.discovery
                    .multicast_error()
                    .map(|e| e.to_string())
                    .unwrap_or_default(),
            ),
            Err(error) => (0, error.clone(), String::new()),
        };
        serde_json::json!({"name": self.alias, "folder": self.destination,
            "port": port, "error": error, "discoveryError": discovery, "quickError": self.quick_error}).to_string()
    }

    async fn devices(&self) -> String {
        let devices: Vec<_> = self
            .network
            .as_ref()
            .ok()
            .into_iter()
            .flat_map(|net| net.discovery.devices())
            .filter_map(|peer| {
                if !peer.logs.last().is_some_and(|entry| {
                    entry.timestamp.elapsed().unwrap_or_default() < CONSENT_TIMEOUT
                }) {
                    return None;
                }
                let channel = peer.get_best_channel()?.http()?;
                Some(
                    serde_json::json!({"id": peer.device.fingerprint, "name": peer.device.alias,
                    "ip": channel.host, "port": channel.port, "type": peer.device.device_type,
                    "encrypted": channel.protocol == ProtocolType::Https}),
                )
            })
            .collect();
        serde_json::to_string(&devices).expect("serializable device list")
    }

    async fn refresh(&self) -> fdo::Result<()> {
        let net = self.network.as_ref().map_err(failed)?;
        net.discovery.announce().await;
        Ok(())
    }

    async fn find_address(&self, host: &str, port: u32) -> fdo::Result<()> {
        let host = host
            .trim()
            .parse::<IpAddr>()
            .map_err(|_| failed("Enter the other device’s IP address"))?;
        let port = u16::try_from(port)
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| failed("Enter a port from 1 to 65535"))?;
        let net = self.network.as_ref().map_err(failed)?;
        let host = host.to_string();
        let found = match net
            .discovery
            .discover(&host, port, ProtocolType::Https)
            .await
        {
            Ok(peer) => peer,
            Err(_) => net
                .discovery
                .discover(&host, port, ProtocolType::Http)
                .await
                .ok()
                .flatten(),
        };
        if found.is_none() {
            return Err(failed("No LocalSend device answered at that address"));
        }
        Ok(())
    }

    async fn send_files(&self, fingerprint: &str, paths: Vec<String>) -> fdo::Result<String> {
        let net = self.network.as_ref().map_err(failed)?;
        let peer = net
            .discovery
            .device_by_fingerprint(fingerprint)
            .filter(|p| p.confirmed)
            .ok_or_else(|| failed("LocalSend device is no longer available"))?;
        let channel = peer
            .get_best_channel()
            .and_then(|c| c.http().cloned())
            .ok_or_else(|| failed("LocalSend device has no reachable address"))?;
        let files = folder_transfer::prepare(paths).await.map_err(failed)?;
        let id = format!("ls-{}", uuid::Uuid::new_v4());
        let send = Outgoing {
            host: channel.host.clone(),
            cancel: CancellationToken::new(),
            session: Arc::new(SyncMutex::new(None)),
        };
        net.outgoing.lock().await.insert(id.clone(), send.clone());
        tokio::spawn(net.clone().send(
            id.clone(),
            peer.device.alias,
            peer.device.fingerprint,
            channel,
            files,
            send,
        ));
        Ok(id)
    }

    async fn answer(&self, id: &str, accept: bool) -> fdo::Result<()> {
        let net = self.network.as_ref().map_err(failed)?;
        let mut current = net.incoming.lock().await;
        let transfer = current
            .as_mut()
            .filter(|t| t.event.id == id)
            .ok_or_else(|| failed("This request expired"))?;
        if transfer.decision.is_none() {
            return Err(failed("This request was already answered"));
        }
        net.answer(transfer, accept).await;
        if transfer.event.is_final() {
            *current = None;
        }
        Ok(())
    }

    async fn cancel(&self, id: &str) -> fdo::Result<()> {
        let net = self.network.as_ref().map_err(failed)?;
        if let Some(send) = net.outgoing.lock().await.get(id) {
            send.cancel.cancel();
            return Ok(());
        }
        let mut state = net.incoming.lock().await;
        let transfer = state
            .as_mut()
            .filter(|t| t.event.id == id)
            .ok_or_else(|| failed("This transfer has ended"))?;
        transfer.cancel.cancel();
        if transfer.decision.is_some() {
            net.answer(transfer, false).await;
        } else {
            let _ = net
                .server
                .cancel_v2_session(id.trim_start_matches("ls-"))
                .await;
            transfer.event.state = "Cancelled".into();
            dbus::emit_transfer(&net.bus, &transfer.event).await;
        }
        *state = None;
        Ok(())
    }
}

impl Network {
    async fn run(self: Arc<Self>, mut events: mpsc::Receiver<ServerEventV2>, destination: PathBuf) {
        let (saved_tx, mut saved_rx) = mpsc::channel(32);
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let mut announce = tokio::time::interval(Duration::from_secs(35));
        announce.tick().await;
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Some(event) => self.event(event, &destination, &saved_tx).await,
                    None => break,
                },
                Some(saved) = saved_rx.recv() => self.saved(saved).await,
                _ = tick.tick() => self.update_incoming().await,
                _ = announce.tick() => { self.discovery.announce().await; }
            }
        }
    }

    async fn observe(&self, host: String, info: RegisterDtoV2) {
        if !info
            .fingerprint
            .eq_ignore_ascii_case(&self.identity.fingerprint)
        {
            self.discovery
                .observe_inbound(DiscoveredDevice {
                    alias: info.alias,
                    version: info.version,
                    device_model: info.device_model,
                    device_type: info.device_type,
                    fingerprint: info.fingerprint,
                    channel: DeviceChannel::Http(HttpChannel {
                        host,
                        port: info.port,
                        protocol: info.protocol,
                    }),
                    download: info.download,
                })
                .await;
        }
    }

    async fn event(&self, event: ServerEventV2, destination: &Path, saved: &mpsc::Sender<Saved>) {
        match event {
            ServerEventV2::Register { ip, info } => self.observe(ip.to_string(), info).await,
            ServerEventV2::PrepareUpload {
                session_id,
                info,
                files,
                decision_tx,
                ..
            } => {
                let mut current = self.incoming.lock().await;
                if current.is_some() || files.is_empty() || files.len() > MAX_FILES {
                    let _ = decision_tx.send(PrepareUploadDecisionV2::Decline);
                    return;
                }
                let paths: Result<HashMap<_, _>> = files
                    .iter()
                    .map(|(id, file)| {
                        Ok((id.clone(), folder_transfer::relative_name(&file.file_name)?))
                    })
                    .collect();
                let Ok(paths) = paths else {
                    let _ = decision_tx.send(PrepareUploadDecisionV2::Decline);
                    return;
                };
                let names = folder_transfer::preview_names(
                    paths.values().map(|p| p.display().to_string()).collect(),
                );
                let text = files
                    .values()
                    .next()
                    .filter(|f| {
                        files.len() == 1
                            && (f.file_type == "text" || f.file_type.starts_with("text/"))
                    })
                    .and_then(|f| f.preview.as_ref())
                    .map(|text| text.chars().take(8192).collect::<String>());
                let event = TransferEvent {
                    id: format!("ls-{session_id}"),
                    kind: "inbound",
                    state: "WaitingForUserConsent".into(),
                    device: info.alias,
                    files: names,
                    preview: text.clone().unwrap_or_default(),
                    consent: 1,
                    total_bytes: files
                        .values()
                        .fold(0u64, |sum, f| sum.saturating_add(f.size)),
                    ..Default::default()
                };
                dbus::emit_transfer(&self.bus, &event).await;
                *current = Some(Incoming {
                    event,
                    began: Instant::now(),
                    last_activity: Instant::now(),
                    text,
                    decision: Some(decision_tx),
                    paths,
                    roots: HashMap::new(),
                    cancel: Default::default(),
                    active: HashMap::new(),
                    saved_bytes: 0,
                    failed: false,
                    ended: false,
                });
            }
            ServerEventV2::FileUpload {
                session_id,
                file_id,
                file,
                target_tx,
            } => {
                let mut current = self.incoming.lock().await;
                let Some(transfer) = current
                    .as_mut()
                    .filter(|t| t.event.id == format!("ls-{session_id}") && t.decision.is_none())
                else {
                    return;
                };
                let Some(relative) = transfer.paths.get(&file_id) else {
                    return;
                };
                let parent = match folder_transfer::receive_parent(
                    destination,
                    relative,
                    &mut transfer.roots,
                )
                .await
                {
                    Ok(parent) => parent,
                    Err(error) => {
                        transfer.failed = true;
                        transfer.event.reason = error.to_string();
                        return;
                    }
                };
                let name = relative
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("untitled")
                    .to_owned();
                let progress = Arc::new(AtomicU64::new(0));
                let cancel = transfer.cancel.clone();
                let (binary_tx, binary_rx) = mpsc::channel(16);
                let (result_tx, result_rx) = oneshot::channel();
                if target_tx
                    .send(FileUploadTarget::Stream {
                        binary_tx,
                        result_rx,
                    })
                    .is_err()
                {
                    transfer.failed = true;
                    return;
                }
                transfer.active.insert(file_id.clone(), progress.clone());
                transfer.last_activity = Instant::now();
                let id = transfer.event.id.clone();
                let saved = saved.clone();
                tokio::spawn(async move {
                    let path = save_file(&parent, &name, &file, binary_rx, &cancel, progress).await;
                    let _ = result_tx
                        .send(path.as_ref().map(|_| ()).map_err(|error| error.to_string()));
                    let _ = saved
                        .send(Saved {
                            id,
                            file_id,
                            size: file.size,
                            path,
                        })
                        .await;
                });
            }
            ServerEventV2::SessionEnd { session_id, reason } => {
                let mut current = self.incoming.lock().await;
                let Some(transfer) = current
                    .as_mut()
                    .filter(|t| t.event.id == format!("ls-{session_id}"))
                else {
                    return;
                };
                if reason == SessionEndReasonV2::Cancelled {
                    transfer.cancel.cancel();
                    transfer.event.state = "Cancelled".into();
                    dbus::emit_transfer(&self.bus, &transfer.event).await;
                    *current = None;
                } else {
                    transfer.ended = true;
                    self.finish_if_done(&mut current).await;
                }
            }
            ServerEventV2::PrepareUploadAborted { session_id } => {
                let mut current = self.incoming.lock().await;
                if let Some(transfer) = current
                    .as_mut()
                    .filter(|t| t.event.id == format!("ls-{session_id}"))
                {
                    transfer.event.state = "Cancelled".into();
                    transfer.event.consent = 0;
                    dbus::emit_transfer(&self.bus, &transfer.event).await;
                    *current = None;
                }
            }
            ServerEventV2::CancelReceived { ip, session_id } => {
                for send in self.outgoing.lock().await.values() {
                    if send.host == ip.to_string()
                        && send.session.lock().unwrap().as_deref() == Some(&session_id)
                    {
                        send.cancel.cancel();
                    }
                }
            }
            ServerEventV2::ListenerFailed { error } => *self.listener_error.lock().await = error,
        }
    }

    async fn answer(&self, transfer: &mut Incoming, accept: bool) {
        use PrepareUploadDecisionV2 as Decision;
        let Some(decision) = transfer.decision.take() else {
            return;
        };
        let decision_value = if accept {
            Decision::Accept(if transfer.text.is_some() {
                Default::default()
            } else {
                transfer.paths.keys().cloned().collect()
            })
        } else {
            Decision::Decline
        };
        transfer.event.consent = 0;
        transfer.last_activity = Instant::now();
        transfer.event.state = match (
            decision.send(decision_value).is_ok(),
            accept,
            &transfer.text,
        ) {
            (true, true, Some(text)) => {
                transfer.event.text = text.clone();
                "Finished"
            }
            (true, true, None) => "ReceivingFiles",
            (true, false, _) => "Rejected",
            (false, _, _) => "Cancelled",
        }
        .into();
        dbus::emit_transfer(&self.bus, &transfer.event).await;
    }

    async fn saved(&self, saved: Saved) {
        let mut current = self.incoming.lock().await;
        let Some(transfer) = current.as_mut().filter(|t| t.event.id == saved.id) else {
            return;
        };
        transfer.active.remove(&saved.file_id);
        transfer.last_activity = Instant::now();
        match saved.path {
            Ok(path) => {
                transfer.saved_bytes = transfer.saved_bytes.saturating_add(saved.size);
                if transfer.event.saved_paths.len() < 20 {
                    transfer
                        .event
                        .saved_paths
                        .push(path.to_string_lossy().into_owned());
                }
            }
            Err(error) => {
                transfer.failed = true;
                transfer.event.reason = error.to_string();
            }
        }
        self.finish_if_done(&mut current).await;
    }

    async fn finish_if_done(&self, current: &mut Option<Incoming>) {
        if let Some(transfer) = current.as_mut().filter(|t| t.ended && t.active.is_empty()) {
            transfer.event.ack_bytes = transfer.saved_bytes;
            transfer.event.state = if transfer.failed {
                "Disconnected"
            } else {
                "Finished"
            }
            .into();
            dbus::emit_transfer(&self.bus, &transfer.event).await;
            *current = None;
        }
    }

    async fn update_incoming(&self) {
        let mut current = self.incoming.lock().await;
        let Some(transfer) = current.as_mut() else {
            return;
        };
        if transfer.decision.is_some() {
            if transfer.began.elapsed() >= CONSENT_TIMEOUT {
                self.answer(transfer, false).await;
                *current = None;
            }
            return;
        }
        let progress = transfer
            .active
            .values()
            .fold(transfer.saved_bytes, |sum, bytes| {
                sum.saturating_add(bytes.load(Ordering::Relaxed))
            });
        if progress != transfer.event.ack_bytes {
            transfer.last_activity = Instant::now();
            transfer.event.ack_bytes = progress;
            dbus::emit_transfer(&self.bus, &transfer.event).await;
        } else if transfer.last_activity.elapsed() >= CONSENT_TIMEOUT {
            transfer.cancel.cancel();
            let _ = self
                .server
                .cancel_v2_session(transfer.event.id.trim_start_matches("ls-"))
                .await;
            transfer.event.state = "Disconnected".into();
            transfer.event.reason = "The sender stopped responding".into();
            dbus::emit_transfer(&self.bus, &transfer.event).await;
            *current = None;
        }
    }

    async fn send(
        self: Arc<Self>,
        id: String,
        alias: String,
        fingerprint: String,
        channel: HttpChannel,
        files: OfferedFiles,
        send: Outgoing,
    ) {
        let names = folder_transfer::preview_names(
            files
                .values()
                .map(|(file, _)| file.file_name.clone())
                .collect(),
        );
        let mut event = TransferEvent {
            id: id.clone(),
            kind: "outbound",
            state: "WaitingForResponse".into(),
            device: alias,
            files: names,
            total_bytes: files
                .values()
                .fold(0u64, |sum, (f, _)| sum.saturating_add(f.size)),
            ..Default::default()
        };
        dbus::emit_transfer(&self.bus, &event).await;
        let progress = Arc::new(AtomicU64::new(0));
        let accepted = Arc::new(AtomicU64::new(0));
        let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut task = Box::pin(upload(
            &self.identity,
            fingerprint,
            channel,
            files,
            &send,
            progress.clone(),
            accepted.clone(),
            started.clone(),
        ));
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let result = loop {
            tokio::select! {
                result = &mut task => break result,
                _ = tick.tick() => {
                    let bytes = progress.load(Ordering::Relaxed);
                    if started.load(Ordering::Acquire) && (event.state != "SendingFiles" || bytes != event.ack_bytes) {
                        event.state = "SendingFiles".into();
                        event.total_bytes = accepted.load(Ordering::Relaxed);
                        event.ack_bytes = bytes;
                        dbus::emit_transfer(&self.bus, &event).await;
                    }
                }
            }
        };
        self.outgoing.lock().await.remove(&id);
        event.ack_bytes = progress.load(Ordering::Relaxed);
        match result {
            Ok(partial) => {
                event.state = "Finished".into();
                event.total_bytes = event.ack_bytes;
                if partial {
                    event.reason = "The recipient accepted some of the files".into();
                }
            }
            Err(error) => {
                event.state = if send.cancel.is_cancelled() {
                    "Cancelled"
                } else if is_status(&error, 403) {
                    "Rejected"
                } else {
                    "Disconnected"
                }
                .into();
                event.reason = if is_status(&error, 401) {
                    "The recipient requires a LocalSend PIN".into()
                } else if is_status(&error, 403) {
                    "The recipient declined the request".into()
                } else {
                    error.to_string()
                };
            }
        }
        dbus::emit_transfer(&self.bus, &event).await;
    }
}

fn is_status(error: &anyhow::Error, code: u16) -> bool {
    matches!(error.downcast_ref::<ClientError>(), Some(ClientError::StatusCode(err)) if err.status == code)
}

async fn upload(
    identity: &Identity,
    fingerprint: String,
    channel: HttpChannel,
    files: OfferedFiles,
    send: &Outgoing,
    progress: Arc<AtomicU64>,
    accepted: Arc<AtomicU64>,
    started: Arc<std::sync::atomic::AtomicBool>,
) -> Result<bool> {
    let HttpChannel {
        host,
        port,
        protocol,
    } = channel;
    let expected = (protocol == ProtocolType::Https).then(|| fingerprint.to_ascii_uppercase());
    let client = LsHttpClientV2::try_new(&identity.key_pem, &identity.cert_pem, expected, None)?;
    let payload = PrepareUploadRequestDtoV2 {
        info: identity.register_dto(),
        files: files
            .iter()
            .map(|(id, (dto, _))| (id.clone(), dto.clone()))
            .collect(),
    };
    let prepared = tokio::time::timeout(
        CONSENT_TIMEOUT,
        client.prepare_upload(
            protocol.clone(),
            &host,
            port,
            None,
            payload,
            None,
            send.cancel.clone(),
        ),
    )
    .await
    .context("Timed out waiting for the recipient")??;
    let response = prepared
        .response
        .context("The recipient accepted no files")?;
    ensure!(
        !response.session_id.is_empty()
            && !response.files.is_empty()
            && response
                .files
                .iter()
                .all(|(id, token)| files.contains_key(id) && !token.is_empty()),
        "Invalid LocalSend session response"
    );
    *send.session.lock().unwrap() = Some(response.session_id.clone());
    let total = response
        .files
        .keys()
        .fold(0u64, |sum, id| sum.saturating_add(files[id].0.size));
    accepted.store(total, Ordering::Relaxed);
    started.store(true, Ordering::Release);
    let partial = response.files.len() < files.len();
    let mut uploads: Vec<_> = response.files.into_iter().collect();
    uploads.sort_unstable_by(|(a, _), (b, _)| files[a].0.file_name.cmp(&files[b].0.file_name));
    for (id, token) in uploads {
        let (_, path) = &files[&id];
        let body = localsend::reqwest::Body::wrap_stream(
            FileContent::Path(path.clone()).into_stream().map({
                let progress = progress.clone();
                move |result| {
                    let chunk = result?;
                    progress.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                    Ok::<Bytes, anyhow::Error>(chunk)
                }
            }),
        );
        if let Err(error) = client
            .upload(
                protocol.clone(),
                &host,
                port,
                None,
                &response.session_id,
                &id,
                &token,
                body,
                send.cancel.clone(),
            )
            .await
        {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                client.cancel(protocol, &host, port, &response.session_id),
            )
            .await;
            return Err(error.into());
        }
    }
    Ok(partial)
}

async fn save_file(
    destination: &Path,
    name: &str,
    info: &FileDto,
    mut chunks: mpsc::Receiver<Bytes>,
    cancel: &CancellationToken,
    progress: Arc<AtomicU64>,
) -> Result<PathBuf> {
    let staged = tempfile::Builder::new()
        .prefix(".nearby-")
        .suffix(".part")
        .tempfile_in(destination)?;
    let mut writer = tokio::fs::File::from_std(staged.reopen()?);
    let mut hasher = info.sha256.as_ref().map(|_| Sha256::new());
    let mut written = 0u64;
    loop {
        let Some(chunk) = (tokio::select! { biased; _ = cancel.cancelled() => bail!("Transfer cancelled"), chunk = chunks.recv() => chunk })
        else {
            break;
        };
        written = written.saturating_add(chunk.len() as u64);
        ensure!(written <= info.size, "Received more bytes than offered");
        writer.write_all(&chunk).await?;
        if let Some(hasher) = &mut hasher {
            hasher.update(&chunk);
        }
        progress.store(written, Ordering::Relaxed);
    }
    ensure!(
        written == info.size,
        "The sender did not finish uploading the file"
    );
    if let (Some(hasher), Some(expected)) = (hasher, &info.sha256) {
        ensure!(
            hex::encode(hasher.finalize()).eq_ignore_ascii_case(expected),
            "File checksum did not match"
        );
    }
    writer.flush().await?;
    drop(writer);
    ensure!(!cancel.is_cancelled(), "Transfer cancelled");
    let mut staged = staged;
    for suffix in 0..10_000 {
        let candidate = destination.join(folder_transfer::unique_name(name, suffix));
        match staged.persist_noclobber(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => staged = error.file,
            Err(error) => return Err(error.error.into()),
        }
    }
    bail!("Too many files with this name")
}
