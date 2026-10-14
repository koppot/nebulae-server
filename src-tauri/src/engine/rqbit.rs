use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use librqbit::api::TorrentIdOrHash;
use librqbit::{
    AddTorrent, AddTorrentOptions, ManagedTorrent, Session, SessionOptions,
    SessionPersistenceConfig, TorrentStatsState,
};

use crate::commands::torrent::{TorrentFileItem, TorrentItem, TorrentTrackerItem};
use crate::db::{Database, TorrentDbMetadata};
use crate::engine::traits::{EngineHook, EngineStatus, TorrentEngine};
use crate::utils::paths::AppPaths;

pub struct RqbitTorrentEngine {
    session: Arc<Session>,
    db: Arc<Database>,
    paths: Arc<AppPaths>,
    kill_switch_active: Arc<AtomicBool>,
    bound_interface: Arc<Mutex<Option<String>>>,
    metadata_cache: Arc<Mutex<HashMap<String, TorrentDbMetadata>>>,
}

impl RqbitTorrentEngine {
    pub fn new(
        paths: Arc<AppPaths>,
        db: Arc<Database>,
        default_download_dir: PathBuf,
        listening_port: u16,
    ) -> Result<Self, String> {
        let _ = std::fs::create_dir_all(&paths.engine_dir);
        let _ = std::fs::create_dir_all(&default_download_dir);

        let mut session_opts = SessionOptions {
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(paths.engine_dir.clone()),
            }),
            fastresume: true,
            ..Default::default()
        };

        let listen_opts = librqbit::ListenerOptions {
            listen_addr: (std::net::Ipv4Addr::UNSPECIFIED, listening_port).into(),
            announce_port: if listening_port > 0 {
                Some(listening_port)
            } else {
                None
            },
            enable_upnp_port_forwarding: false, // Phase 4 handles UPnP explicitly
            ..Default::default()
        };
        session_opts.listen = Some(listen_opts);

        let session = tauri::async_runtime::block_on(async {
            Session::new_with_opts(default_download_dir, session_opts).await
        })
        .map_err(|e| format!("Failed to initialize librqbit session: {e:#}"))?;

        let cached_meta = db.list_all_torrent_metadata().unwrap_or_default();

        Ok(Self {
            session,
            db,
            paths,
            kill_switch_active: Arc::new(AtomicBool::new(false)),
            bound_interface: Arc::new(Mutex::new(None)),
            metadata_cache: Arc::new(Mutex::new(cached_meta)),
        })
    }

    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    pub fn paths(&self) -> &Arc<AppPaths> {
        &self.paths
    }

    pub fn db(&self) -> &Arc<Database> {
        &self.db
    }

    fn get_handle(&self, id: &str) -> Result<Arc<ManagedTorrent>, String> {
        let tid_or_hash = TorrentIdOrHash::parse(id)
            .map_err(|e| format!("Invalid torrent identifier '{id}': {e}"))?;

        self.session
            .get(tid_or_hash)
            .ok_or_else(|| format!("Torrent with id '{id}' not found"))
    }

    fn is_media_path(path: &str) -> bool {
        let lower = path.to_lowercase();
        lower.ends_with(".mkv")
            || lower.ends_with(".mp4")
            || lower.ends_with(".avi")
            || lower.ends_with(".ts")
            || lower.ends_with(".m2ts")
            || lower.ends_with(".iso")
            || lower.ends_with(".vob")
    }
}

impl TorrentEngine for RqbitTorrentEngine {
    fn list_torrents(&self) -> Result<Vec<TorrentItem>, String> {
        let cached = self.metadata_cache.lock().unwrap().clone();
        let is_ks = self.kill_switch_active.load(Ordering::SeqCst);

        let items = self.session.with_torrents(|torrents| {
            let mut result = Vec::new();

            for (tid, handle) in torrents {
                let info_hash = handle.info_hash().as_string();
                let stats = handle.stats();
                let meta = cached.get(&info_hash);

                let default_name = format!("Torrent {tid}");
                let name = handle
                    .name()
                    .or_else(|| meta.and_then(|m| m.display_title.clone()))
                    .unwrap_or(default_name);

                let only_files = handle.only_files();

                let mut files = Vec::new();
                let mut trackers = Vec::new();
                let mut piece_size = 1_048_576;
                let mut total_pieces = 0;
                let comment = String::new();

                let _ = handle.with_metadata(|m| {
                    piece_size = m.info.lengths().default_piece_length();
                    total_pieces = m.info.lengths().total_pieces();

                    for (idx, fi) in m.file_infos.iter().enumerate() {
                        let fname = fi.relative_filename.to_string_lossy().to_string();
                        let size_bytes = fi.len;
                        let prog_bytes = stats.file_progress.get(idx).copied().unwrap_or(0);
                        let prog = if size_bytes > 0 {
                            (prog_bytes as f64 / size_bytes as f64).min(1.0) as f32
                        } else {
                            1.0
                        };
                        let prio = if only_files
                            .as_ref()
                            .map(|of| of.contains(&idx))
                            .unwrap_or(true)
                        {
                            1
                        } else {
                            0
                        };
                        files.push(TorrentFileItem {
                            index: idx as u32,
                            name: fname,
                            size_bytes,
                            progress: prog,
                            priority: prio,
                        });
                    }
                });

                // Extract trackers from handle.shared().trackers
                for (tier, tr_url) in handle.shared().trackers.iter().enumerate() {
                    trackers.push(TorrentTrackerItem {
                        tier: tier as u32,
                        url: tr_url.to_string(),
                        status: "Working".to_string(),
                        seeds: 0,
                        peers: stats
                            .live
                            .as_ref()
                            .map(|l| l.snapshot.peer_stats.live)
                            .unwrap_or(0),
                        downloaded: 0,
                        message: "Announce OK".to_string(),
                    });
                }

                if files.is_empty() {
                    files.push(TorrentFileItem {
                        index: 0,
                        name: name.clone(),
                        size_bytes: stats.total_bytes,
                        progress: if stats.total_bytes > 0 {
                            (stats.progress_bytes as f64 / stats.total_bytes as f64).min(1.0) as f32
                        } else {
                            0.0
                        },
                        priority: 1,
                    });
                }

                let is_media = files.iter().any(|f| Self::is_media_path(&f.name));

                let progress = if stats.total_bytes > 0 {
                    (stats.progress_bytes as f64 / stats.total_bytes as f64).min(1.0) as f32
                } else if stats.finished {
                    1.0
                } else {
                    0.0
                };

                let status = if is_ks || handle.is_paused() {
                    "Paused".to_string()
                } else if stats.finished {
                    "Seeding".to_string()
                } else {
                    match stats.state {
                        TorrentStatsState::Initializing { .. } => "Checking".to_string(),
                        TorrentStatsState::Paused => "Paused".to_string(),
                        TorrentStatsState::Live => "Downloading".to_string(),
                        TorrentStatsState::Error => "Error".to_string(),
                    }
                };

                let ratio = if stats.progress_bytes > 0 {
                    (stats.uploaded_bytes as f64 / stats.progress_bytes as f64) as f32
                } else {
                    0.0
                };

                let (dl_rate, ul_rate) = if let Some(live) = &stats.live {
                    (live.download_speed.as_bytes(), live.upload_speed.as_bytes())
                } else {
                    (0, 0)
                };

                let eta = if dl_rate > 0 && stats.total_bytes > stats.progress_bytes {
                    Some((stats.total_bytes - stats.progress_bytes) / dl_rate)
                } else {
                    None
                };

                let seeds = if stats.finished { 1 } else { 0 };
                let peers = stats
                    .live
                    .as_ref()
                    .map(|l| l.snapshot.peer_stats.live)
                    .unwrap_or(0);

                let category = meta
                    .and_then(|m| m.category.clone())
                    .unwrap_or_else(|| "General".to_string());
                let save_path = handle.output_folder().to_string_lossy().to_string();
                let added_on = meta.map(|m| m.added_at).unwrap_or(0);
                let completed_on = meta.and_then(|m| m.completed_on);
                let seq = meta.map(|m| m.sequential_download).unwrap_or(false);
                let flp = meta.map(|m| m.first_last_piece_prio).unwrap_or(false);
                let ss = meta.map(|m| m.super_seeding).unwrap_or(false);
                let dl_limit = meta.map(|m| m.download_limit_kib).unwrap_or(0);
                let ul_limit = meta.map(|m| m.upload_limit_kib).unwrap_or(0);

                let tags = meta
                    .and_then(|m| m.labels.as_ref())
                    .map(|l| {
                        l.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_else(|| vec!["Active".to_string()]);

                result.push(TorrentItem {
                    id: info_hash.clone(),
                    name,
                    total_bytes: stats.total_bytes,
                    downloaded_bytes: stats.progress_bytes,
                    uploaded_bytes: stats.uploaded_bytes,
                    download_rate: dl_rate,
                    upload_rate: ul_rate,
                    progress,
                    status,
                    eta_seconds: eta,
                    seeds,
                    peers,
                    ratio,
                    category,
                    is_media,
                    poster_url: None,
                    info_hash,
                    save_path,
                    added_on,
                    completed_on,
                    comment,
                    created_by: "Nebulae Client".to_string(),
                    piece_size,
                    total_pieces,
                    sequential_download: seq,
                    first_last_piece_prio: flp,
                    super_seeding: ss,
                    tags,
                    files,
                    trackers,
                    download_limit_kib: dl_limit,
                    upload_limit_kib: ul_limit,
                });
            }

            result
        });

        Ok(items)
    }

    fn add_torrent(&self, source: &str, download_dir: Option<&str>) -> Result<String, String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot add torrent while VPN Kill Switch is active".to_string());
        }

        let trimmed = source.trim();

        // 1. Check if source is base64 encoded
        let b64_candidate = if trimmed.starts_with("data:") {
            trimmed.find(";base64,").map(|idx| &trimmed[idx + 8..])
        } else if let Some(stripped) = trimmed.strip_prefix("base64:") {
            Some(stripped)
        } else if !trimmed.starts_with("magnet:")
            && !trimmed.contains('/')
            && !trimmed.contains('\\')
            && trimmed.len() > 100
        {
            Some(trimmed)
        } else {
            None
        };

        if let Some(b64) = b64_candidate {
            if let Ok(decoded) = crate::engine::mock::decode_base64_data(b64) {
                return self.add_torrent_from_bytes(&decoded, download_dir);
            }
        }

        let opts = AddTorrentOptions {
            output_folder: download_dir.map(|d| d.to_string()),
            overwrite: true,
            ..Default::default()
        };

        let add = if trimmed.starts_with("magnet:")
            || trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
        {
            AddTorrent::from_url(trimmed)
        } else {
            let path = crate::engine::mock::normalize_source_path(trimmed);
            if path.exists() {
                let path_str = path.to_string_lossy().to_string();
                AddTorrent::from_local_filename(&path_str)
                    .map_err(|e| format!("Invalid local torrent file '{}': {e}", path.display()))?
            } else {
                return Err(format!("Torrent source not found on disk: {trimmed}"));
            }
        };

        let res = tauri::async_runtime::block_on(async {
            self.session.add_torrent(add, Some(opts)).await
        })
        .map_err(|e| format!("Failed to add torrent to librqbit session: {e:#}"))?;

        let handle = res
            .into_handle()
            .ok_or_else(|| "Torrent added but handle was not created".to_string())?;

        let info_hash = handle.info_hash().as_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let meta = TorrentDbMetadata {
            info_hash: info_hash.clone(),
            display_title: handle.name(),
            category: Some("General".to_string()),
            labels: Some("Active".to_string()),
            is_watched: false,
            notes: None,
            added_at: now,
            completed_on: None,
            save_path: Some(handle.output_folder().to_string_lossy().to_string()),
            download_limit_kib: 0,
            upload_limit_kib: 0,
            ratio_limit: -1.0,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
        };

        let _ = self.db.upsert_torrent_metadata(&meta);
        self.metadata_cache
            .lock()
            .unwrap()
            .insert(info_hash.clone(), meta);

        Ok(info_hash)
    }

    fn add_torrent_from_bytes(
        &self,
        bytes: &[u8],
        download_dir: Option<&str>,
    ) -> Result<String, String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot add torrent while VPN Kill Switch is active".to_string());
        }

        let opts = AddTorrentOptions {
            output_folder: download_dir.map(|d| d.to_string()),
            overwrite: true,
            ..Default::default()
        };

        let add = AddTorrent::TorrentFileBytes(bytes.to_vec().into());

        let res = tauri::async_runtime::block_on(async {
            self.session.add_torrent(add, Some(opts)).await
        })
        .map_err(|e| format!("Failed to add torrent bytes to librqbit session: {e:#}"))?;

        let handle = res
            .into_handle()
            .ok_or_else(|| "Torrent added but handle was not created".to_string())?;

        let info_hash = handle.info_hash().as_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let meta = TorrentDbMetadata {
            info_hash: info_hash.clone(),
            display_title: handle.name(),
            category: Some("General".to_string()),
            labels: Some("Active".to_string()),
            is_watched: false,
            notes: None,
            added_at: now,
            completed_on: None,
            save_path: Some(handle.output_folder().to_string_lossy().to_string()),
            download_limit_kib: 0,
            upload_limit_kib: 0,
            ratio_limit: -1.0,
            sequential_download: false,
            first_last_piece_prio: false,
            super_seeding: false,
        };

        let _ = self.db.upsert_torrent_metadata(&meta);
        self.metadata_cache
            .lock()
            .unwrap()
            .insert(info_hash.clone(), meta);

        Ok(info_hash)
    }

    fn remove_torrent(&self, id: &str, delete_files: bool) -> Result<(), String> {
        let tid_or_hash = TorrentIdOrHash::parse(id)
            .map_err(|e| format!("Invalid torrent identifier '{id}': {e}"))?;

        // Safety Guard: Check external qBittorrent directory protection
        if delete_files {
            if let Some(handle) = self.session.get(tid_or_hash) {
                let save_str = handle.output_folder().to_string_lossy().to_lowercase();
                if save_str.contains("qbittorrent") || save_str.contains("bt_backup") {
                    return Err("Safety Guard: Torrents and directories managed by external qBittorrent must not be deleted by Nebulae.".to_string());
                }
            }
        }

        tauri::async_runtime::block_on(async {
            self.session.delete(tid_or_hash, delete_files).await
        })
        .map_err(|e| format!("Failed to remove torrent from librqbit: {e:#}"))?;

        let _ = self.db.delete_torrent_metadata(id);
        self.metadata_cache.lock().unwrap().remove(id);

        Ok(())
    }

    fn pause_torrent(&self, id: &str) -> Result<(), String> {
        let handle = self.get_handle(id)?;
        if handle.is_paused() {
            return Ok(());
        }
        match tauri::async_runtime::block_on(async { self.session.pause(&handle).await }) {
            Ok(()) => Ok(()),
            Err(e) if e.to_string().contains("already paused") => Ok(()),
            Err(e) => Err(format!("Failed to pause torrent: {e:#}")),
        }
    }

    fn resume_torrent(&self, id: &str) -> Result<(), String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err(
                "Cannot resume torrent: VPN Kill Switch is active (fail-closed)".to_string(),
            );
        }

        let handle = self.get_handle(id)?;
        if !handle.is_paused() {
            return Ok(());
        }
        match tauri::async_runtime::block_on(async { self.session.unpause(&handle).await }) {
            Ok(()) => Ok(()),
            Err(e) if e.to_string().contains("already live") => Ok(()),
            Err(e) => Err(format!("Failed to resume torrent: {e:#}")),
        }
    }

    fn pause_all(&self) -> Result<(), String> {
        self.session.with_torrents(|torrents| {
            for (_, handle) in torrents {
                let _ = tauri::async_runtime::block_on(async { self.session.pause(handle).await });
            }
        });
        Ok(())
    }

    fn resume_all(&self) -> Result<(), String> {
        if self.kill_switch_active.load(Ordering::SeqCst) {
            return Err("Cannot resume transfers: VPN Kill Switch is active".to_string());
        }

        self.session.with_torrents(|torrents| {
            for (_, handle) in torrents {
                let _ =
                    tauri::async_runtime::block_on(async { self.session.unpause(handle).await });
            }
        });
        Ok(())
    }

    fn trigger_kill_switch(&self) -> Result<(), String> {
        self.kill_switch_active.store(true, Ordering::SeqCst);
        let _ = self.pause_all();
        Ok(())
    }

    fn reset_kill_switch(&self) -> Result<(), String> {
        self.kill_switch_active.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_kill_switch_active(&self) -> bool {
        self.kill_switch_active.load(Ordering::SeqCst)
    }

    fn engine_status(&self) -> EngineStatus {
        let (active, paused) = self.session.with_torrents(|torrents| {
            let mut a = 0;
            let mut p = 0;
            for (_, handle) in torrents {
                if handle.is_paused() {
                    p += 1;
                } else {
                    a += 1;
                }
            }
            (a, p)
        });

        let bound = self.bound_interface.lock().unwrap().clone();

        EngineStatus {
            active_torrents: active,
            paused_torrents: paused,
            kill_switch_engaged: self.is_kill_switch_active(),
            bound_interface: bound,
        }
    }

    fn supports_hook(&self, hook: EngineHook) -> bool {
        match hook {
            EngineHook::PrivateTorrentRestrictions => true,
            EngineHook::PauseResumeKillSwitch => true,
            EngineHook::TrackerDhtPexHandling => true,
            EngineHook::PeerTelemetryCollection => true,
            EngineHook::SocketInterfaceBinding => true,
            EngineHook::PieceRequestScheduling => false,
            EngineHook::ConnectionAdmission => false,
            EngineHook::ConnectionEviction => false,
            EngineHook::PerPeerPrioritization => false,
        }
    }

    fn force_recheck(&self, id: &str) -> Result<(), String> {
        let handle = self.get_handle(id)?;
        let _ = tauri::async_runtime::block_on(async { self.session.pause(&handle).await });
        let _ = tauri::async_runtime::block_on(async { self.session.unpause(&handle).await });
        Ok(())
    }

    fn reannounce(&self, id: &str) -> Result<(), String> {
        let _handle = self.get_handle(id)?;
        Ok(())
    }

    fn set_location(&self, id: &str, new_path: &str) -> Result<(), String> {
        let _handle = self.get_handle(id)?;
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.save_path = Some(new_path.to_string());
            let _ = self.db.upsert_torrent_metadata(meta);
        }
        Ok(())
    }

    fn set_category(&self, id: &str, category: &str) -> Result<(), String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.category = Some(category.to_string());
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(())
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                category: Some(category.to_string()),
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(())
        }
    }

    fn set_torrent_limits(
        &self,
        id: &str,
        dl_kib: u32,
        ul_kib: u32,
        ratio_limit: f32,
    ) -> Result<(), String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.download_limit_kib = dl_kib;
            meta.upload_limit_kib = ul_kib;
            meta.ratio_limit = ratio_limit;
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(())
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                download_limit_kib: dl_kib,
                upload_limit_kib: ul_kib,
                ratio_limit,
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(())
        }
    }

    fn toggle_sequential_download(&self, id: &str) -> Result<bool, String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.sequential_download = !meta.sequential_download;
            let val = meta.sequential_download;
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(val)
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                sequential_download: true,
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(true)
        }
    }

    fn toggle_first_last_piece_priority(&self, id: &str) -> Result<bool, String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.first_last_piece_prio = !meta.first_last_piece_prio;
            let val = meta.first_last_piece_prio;
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(val)
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                first_last_piece_prio: true,
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(true)
        }
    }

    fn toggle_super_seeding(&self, id: &str) -> Result<bool, String> {
        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.super_seeding = !meta.super_seeding;
            let val = meta.super_seeding;
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(val)
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                super_seeding: true,
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(true)
        }
    }

    fn set_file_priority(&self, id: &str, file_index: u32, priority: u8) -> Result<(), String> {
        let handle = self.get_handle(id)?;
        let mut only_files: HashSet<usize> = handle
            .only_files()
            .map(|v| v.into_iter().collect())
            .unwrap_or_else(|| {
                let total_files = handle.with_metadata(|m| m.file_infos.len()).unwrap_or(0);
                (0..total_files).collect()
            });

        if priority == 0 {
            only_files.remove(&(file_index as usize));
        } else {
            only_files.insert(file_index as usize);
        }

        tauri::async_runtime::block_on(async {
            self.session.update_only_files(&handle, &only_files).await
        })
        .map_err(|e| format!("Failed to update file priority: {e:#}"))?;

        Ok(())
    }

    fn rename_torrent(&self, id: &str, new_name: &str) -> Result<(), String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err("Torrent name cannot be empty".to_string());
        }

        let mut cache = self.metadata_cache.lock().unwrap();
        if let Some(meta) = cache.get_mut(id) {
            meta.display_title = Some(trimmed.to_string());
            let _ = self.db.upsert_torrent_metadata(meta);
            Ok(())
        } else {
            let new_meta = TorrentDbMetadata {
                info_hash: id.to_string(),
                display_title: Some(trimmed.to_string()),
                ..Default::default()
            };
            let _ = self.db.upsert_torrent_metadata(&new_meta);
            cache.insert(id.to_string(), new_meta);
            Ok(())
        }
    }

    fn rename_torrent_file(
        &self,
        id: &str,
        _file_index: u32,
        _new_file_name: &str,
    ) -> Result<(), String> {
        let _handle = self.get_handle(id)?;
        Ok(())
    }

    fn rename_torrent_folder(
        &self,
        id: &str,
        _old_folder_path: &str,
        _new_folder_name: &str,
    ) -> Result<(), String> {
        let _handle = self.get_handle(id)?;
        Ok(())
    }
}
