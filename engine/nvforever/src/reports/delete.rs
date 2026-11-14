//! "Delete my reports" (player-reports.md §6): the settings row Gameplay > Reports >
//! "Reports already sent". Choosing "Delete them now" asks the server to delete every
//! report this PC sent: the current install ID and the earlier ones kept since reports were
//! turned off (`[Reports] PastInstalls`), each with its delete token (§5.1 `DELETE
//! /v1/install/{id}`). The server removes them within 7 days. When it has taken every
//! request, the past IDs are forgotten and, with reports on, the install gets a new ID, so
//! later reports are not linked to the deleted ones.

use bevy::prelude::*;
use fnv_install::reports::Reports;
use fnv_install::EngineHome;
use std::sync::atomic::{AtomicU8, Ordering};

pub const SETTING_KEY: &str = "reports-delete";
const KEPT: &str = "Kept on the server";
const DELETING: &str = "Deleting…";
const DELETED: &str = "Deleted";
const FAILED: &str = "Could not delete, try again";
const NONE: &str = "None sent from this PC";
const DELETE: &str = "Delete them now";

static STATE: AtomicU8 = AtomicU8::new(0); // 0 idle, 1 deleting, 2 deleted, 3 failed

/// The installs to delete: the current one (if any) and the past ones.
fn installs(r: &Reports) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = r.install_id.clone().zip(r.delete_token.clone()).into_iter().collect();
    v.extend(r.past.iter().cloned());
    v
}

fn status() -> &'static str {
    match STATE.load(Ordering::Relaxed) {
        1 => DELETING,
        2 => DELETED,
        3 => FAILED,
        _ if EngineHome::user().is_none_or(|h| installs(&Reports::load(&h)).is_empty()) => NONE,
        _ => KEPT,
    }
}

/// Where deletions go: the endpoint reports use (`NVF_REPORTS_ENDPOINT`, `[Reports]
/// Endpoint`, else the project's server). Deleting is allowed from any build.
fn endpoint(r: &Reports) -> String {
    std::env::var("NVF_REPORTS_ENDPOINT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| r.endpoint.clone())
        .unwrap_or_else(|| super::upload::DEFAULT_ENDPOINT.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn start() {
    if STATE.swap(1, Ordering::Relaxed) == 1 {
        return;
    }
    let _ = std::thread::Builder::new().name("nvf-reports-delete".into()).spawn(|| {
        let Some(home) = EngineHome::user() else {
            STATE.store(3, Ordering::Relaxed);
            return;
        };
        let r = Reports::load(&home);
        let url = endpoint(&r);
        let all = installs(&r);
        // Each install is forgotten as soon as the server has taken its request, so a retry
        // after a partial failure asks only for the rest. 404 or 410: the server has nothing
        // (left) for that ID, which is what was asked.
        let mut failed = 0;
        for (id, token) in &all {
            let status = super::upload::http::delete(&format!("{url}/v1/install/{id}"), token);
            if done(status) {
                let mut r = Reports::load(&home);
                r.past.retain(|(i, _)| i != id);
                if let Err(e) = r.save(&home) {
                    warn!("reports: install {id} deleted on the server, but could not be forgotten here: {e}");
                }
            } else {
                failed += 1;
            }
        }
        if failed > 0 {
            warn!("reports: deleting the reports sent from this PC failed for {failed} of {} install ID(s) (the server could not be reached or refused); try again later", all.len());
            STATE.store(3, Ordering::Relaxed);
            return;
        }
        let mut r = Reports::load(&home);
        r.past.clear();
        if r.send == Some(true) {
            r.install_id = Some(super::new_uuid());
            r.delete_token = Some(super::new_token());
        } else {
            r.install_id = None;
            r.delete_token = None;
        }
        // Summaries not sent yet carry a deleted ID: they go too. This run's own summary is
        // rewritten under the new ID (`renew`), so it is kept.
        let ids: Vec<&str> = all.iter().map(|(i, _)| i.as_str()).collect();
        let dropped = super::folders().map_or(0, |(pending, _)| drop_pending(&pending, &ids, std::process::id()));
        if dropped > 0 {
            info!("reports: {dropped} report(s) not sent yet were deleted with them");
        }
        match r.save(&home) {
            Ok(()) => {
                if let (Some(id), Some(token)) = (r.install_id.clone(), r.delete_token.clone()) {
                    super::renew(id, token);
                }
                info!("reports: the server was asked to delete the reports of {} install ID(s) from this PC; later reports, this session's too, get a new ID", all.len());
                STATE.store(2, Ordering::Relaxed);
            }
            Err(e) => {
                warn!("reports: deleted on the server, but the new ID could not be stored: {e}");
                STATE.store(2, Ordering::Relaxed);
            }
        }
    });
}

/// The server took a deletion: 2xx, or 404/410 (nothing (left) under that ID).
fn done(status: Option<u16>) -> bool {
    matches!(status, Some(200..=299 | 404 | 410))
}

/// Removes the pending summaries written under one of `ids`,
/// except this process's own. Returns how many summaries went.
fn drop_pending(pending: &std::path::Path, ids: &[&str], own_pid: u32) -> usize {
    let Ok(dir) = std::fs::read_dir(pending) else { return 0 };
    let own = format!("-{own_pid}.json");
    let mut n = 0;
    for e in dir.flatten() {
        let path = e.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.starts_with("session-") || !name.ends_with(".json") || name.ends_with(&own) {
            continue;
        }
        let id = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| v.get("install_id").and_then(|i| i.as_str()).map(str::to_string));
        if id.is_some_and(|id| ids.contains(&id.as_str())) && std::fs::remove_file(&path).is_ok() {
            n += 1;
        }
    }
    n
}

pub fn add_row(app: &mut App) {
    nvf_play::settings::add_choice_list(
        app,
        nvf_play::settings::Setting {
            key: SETTING_KEY,
            group: "Reports",
            label: "Reports already sent",
            note: "Delete them now asks the server to delete every report this PC sent (within 7 days) and drops unsent ones; later reports get a new ID.",
            switches: "",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        nvf_play::settings::ChoiceList {
            names: |_| {
                let s = status();
                if matches!(s, NONE | DELETING) {
                    vec![s.to_string()]
                } else {
                    vec![s.to_string(), DELETE.to_string()]
                }
            },
            chosen: |_| status().to_string(),
            pick: |_, name| {
                if name != DELETE {
                    return false;
                }
                start();
                true
            },
            // Nothing to keep: a start never deletes by itself.
            save: |_| "kept".to_string(),
            load: |_, _| true,
            ask_keep: false,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_install_this_pc_had_is_deleted() {
        let r = Reports {
            install_id: Some("now".into()),
            delete_token: Some("t-now".into()),
            past: vec![("old".into(), "t-old".into())],
            ..Default::default()
        };
        assert_eq!(installs(&r), vec![("now".to_string(), "t-now".to_string()), ("old".to_string(), "t-old".to_string())]);
        assert!(installs(&Reports::default()).is_empty());
    }

    #[test]
    fn an_install_the_server_no_longer_has_counts_as_deleted() {
        assert!(done(Some(202)) && done(Some(200)) && done(Some(404)) && done(Some(410)));
        assert!(!done(None) && !done(Some(403)) && !done(Some(500)));
    }

    #[test]
    fn pending_reports_of_deleted_ids_are_dropped_but_not_this_runs() {
        let dir = std::env::temp_dir().join(format!("nvf-delete-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let w = |name: &str, id: &str| std::fs::write(dir.join(name), format!("{{\"install_id\":\"{id}\"}}")).unwrap();
        w("session-20261111-000000-11.json", "old");
        w("session-20261111-000100-12.json", "other");
        w("session-20261111-000200-99.json", "old");
        assert_eq!(drop_pending(&dir, &["old"], 99), 1);
        assert!(!dir.join("session-20261111-000000-11.json").exists());
        assert!(dir.join("session-20261111-000100-12.json").exists());
        assert!(dir.join("session-20261111-000200-99.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
