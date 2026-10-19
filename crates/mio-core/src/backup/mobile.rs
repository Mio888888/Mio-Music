//! Mobile-only bounded settings and outcome-aware upload; desktop APIs stay intact.
use super::*;
use crate::backup_restore::PreparedRestore;
use std::io::Read;

pub struct MobileSaved {
    before: Option<Vec<u8>>,
    saved: Saved,
}
fn read_saved(store: &Storage) -> Result<Option<Vec<u8>>, String> {
    let file = match std::fs::File::open(store.root().join(SAVED_FILE)) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("storage_unavailable: S3 settings".into()),
    };
    let mut bytes = Vec::new();
    file.take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "storage_unavailable: S3 settings")?;
    if bytes.len() > 64 * 1024 {
        return Err("capacity_exceeded: S3 settings".into());
    }
    Ok(Some(bytes))
}
fn validate_saved(saved: &Saved) -> Result<(), String> {
    for value in [
        &saved.endpoint,
        &saved.region,
        &saved.access_key_id,
        &saved.bucket,
    ] {
        if value.len() > 4096 || value.chars().any(char::is_control) {
            return Err("invalid_input: S3 settings".into());
        }
    }
    if !(1..=100).contains(&saved.max_backups)
        || saved
            .last_backup_time
            .as_ref()
            .is_some_and(|s| s.len() > 128 || chrono::DateTime::parse_from_rfc3339(s).is_err())
    {
        return Err("invalid_input: S3 retention or time".into());
    }
    Ok(())
}
impl MobileSaved {
    pub fn capture(store: &Storage) -> Result<Self, String> {
        let before = read_saved(store)?;
        let mut saved = match &before {
            Some(bytes) => {
                serde_json::from_value(crate::backup_restore::parse_json(bytes, &|| Ok(()))?)
                    .map_err(|_| "invalid_data: S3 settings")?
            }
            None => Saved::default(),
        };
        saved.max_backups = saved.max_backups.clamp(1, 100);
        if saved.region.trim().is_empty() {
            saved.region = "auto".into();
        }
        validate_saved(&saved)?;
        Ok(Self { before, saved })
    }
    pub fn value(&self) -> &Saved {
        &self.saved
    }
    pub fn check(&self, store: &Storage) -> Result<(), String> {
        if read_saved(store)? != self.before {
            return Err("storage_changed: reopen S3 settings".into());
        }
        Ok(())
    }
    /// Owner-only. The DTO has no secret or password fields. Preserve actual
    /// last-upload time when merely editing connection settings.
    pub fn save(
        &mut self,
        store: &Storage,
        mut next: Saved,
        uploaded: Option<&str>,
    ) -> Result<(), String> {
        next.last_backup_time = uploaded
            .map(str::to_owned)
            .or_else(|| self.saved.last_backup_time.clone());
        validate_saved(&next)?;
        self.check(store)?;
        let mut bytes =
            serde_json::to_vec_pretty(&next).map_err(|_| "invalid_data: S3 settings")?;
        bytes.push(b'\n');
        store
            .save(SAVED_FILE, &next)
            .map_err(|_| "storage_unavailable: S3 settings could not be saved")?;
        self.before = Some(bytes);
        self.saved = next;
        Ok(())
    }
}

pub struct UploadOutcome {
    pub timestamp: String,
    pub deleted: u32,
    pub warnings: Vec<String>,
}

pub fn upload_prepared(
    config: S3Config,
    password: &str,
    max_backups: u32,
    prepared: &PreparedRestore,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<UploadOutcome, String> {
    check()?;
    if password.is_empty() || password.len() > 4096 || !(1..=100).contains(&max_backups) {
        return Err("invalid_input: backup password or retention".into());
    }
    let config = mobile_config(config)?;
    let target = target(&config)?;
    let date = chrono::DateTime::parse_from_rfc3339(prepared.timestamp())
        .map_err(|_| "invalid_data: backup time")?;
    let key = format!(
        "{BACKUP_PREFIX}{}-{}.enc",
        date.with_timezone(&chrono::Utc)
            .format("%Y-%m-%dT%H-%M-%S-%fZ"),
        uuid::Uuid::new_v4()
    );
    let plain = prepared.document_bytes(check)?;
    let body = encode_envelope(&plain, password).map_err(|_| "unavailable: backup encryption")?;
    drop(plain);
    check()?;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "unavailable: S3 client")?;
    // A transport error may arrive after the server accepted PUT. Never report
    // definite failure or automatically resend this immutable upload.
    let response = target
        .request(&client, reqwest::Method::PUT, Some(&key), "", Some(body))
        .map_err(|_| "upload_unknown: server outcome could not be confirmed")?;
    if response.status().is_server_error() {
        return Err("upload_unknown: server outcome could not be confirmed".into());
    }
    if !response.status().is_success() {
        return Err("upload_rejected: S3 response".into());
    }
    drop(response);
    let mut outcome = UploadOutcome {
        timestamp: prepared.timestamp().to_owned(),
        deleted: 0,
        warnings: Vec::new(),
    };
    // Once PUT succeeded, cancellation or retention failure must preserve it.
    if check().is_err() {
        outcome.warnings.push("retention_cancelled".into());
        return Ok(outcome);
    }
    let objects = match list_backups_checked(&target, &client, check) {
        Ok(v) => v,
        Err(_) => {
            outcome.warnings.push("retention_not_completed".into());
            return Ok(outcome);
        }
    };
    let mut incomplete = false;
    for (old, _) in objects.iter().skip(max_backups as usize) {
        if old == &key {
            continue;
        }
        if check().is_err() {
            outcome.warnings.push("retention_cancelled".into());
            break;
        }
        match target.request(&client, reqwest::Method::DELETE, Some(old), "", None) {
            Ok(response) if response.status().is_success() => outcome.deleted += 1,
            _ => incomplete = true,
        }
    }
    if incomplete {
        outcome.warnings.push("retention_partial_or_unknown".into());
    }
    Ok(outcome)
}
