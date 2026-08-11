use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use keeper_project::ProjectId;
use keeper_storage_sqlite::RuntimeSlotId;
use serde::Serialize;
use uuid::Uuid;

use crate::{GROK_API_KEY_ENV, RuntimeSlotError};

#[derive(Serialize)]
struct GrokConfigDocument {
    model: BTreeMap<String, GrokModelConfig>,
}

#[derive(Serialize)]
struct GrokModelConfig {
    model: String,
    base_url: String,
    env_key: &'static str,
    name: String,
}

pub(crate) fn write_private_grok_config(
    root: &Path,
    project_id: ProjectId,
    slot_id: RuntimeSlotId,
    display_name: &str,
    base_url: &str,
    model_id: &str,
) -> Result<PathBuf, RuntimeSlotError> {
    let private_home = root
        .join(project_id.0.to_string())
        .join(slot_id.0.to_string());
    let grok_dir = private_home.join(".grok");
    let auth_path = grok_dir.join("auth.json");
    if auth_path.exists() || private_home.join("auth.json").exists() {
        return Err(RuntimeSlotError::PrivateHomeContaminated);
    }
    std::fs::create_dir_all(&grok_dir).map_err(|_| RuntimeSlotError::ConfigUnavailable)?;

    let mut models = BTreeMap::new();
    models.insert(
        model_id.to_owned(),
        GrokModelConfig {
            model: model_id.to_owned(),
            base_url: base_url.to_owned(),
            env_key: GROK_API_KEY_ENV,
            name: display_name.to_owned(),
        },
    );
    let document = GrokConfigDocument { model: models };
    let config = toml::to_string(&document).map_err(|_| RuntimeSlotError::ConfigUnavailable)?;
    let target = grok_dir.join("config.toml");
    let temporary = grok_dir.join(format!(".config.{}.tmp", Uuid::new_v4()));
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| RuntimeSlotError::ConfigUnavailable)?;
        file.write_all(config.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| RuntimeSlotError::ConfigUnavailable)?;
        if target.exists() {
            std::fs::remove_file(&target).map_err(|_| RuntimeSlotError::ConfigUnavailable)?;
        }
        std::fs::rename(&temporary, &target).map_err(|_| RuntimeSlotError::ConfigUnavailable)
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    write_result.map(|()| private_home)
}
