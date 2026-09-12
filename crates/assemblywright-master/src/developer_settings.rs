//! Durable, Windows-owned model selections for supervised developer planning and review.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, io::Read as _, path::Path};

pub const DEFAULT_MODEL: &str = "gpt-5.6-sol";
pub const DEFAULT_REASONING_EFFORT: &str = "high";
const MAX_CACHE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_MODELS: usize = 64;

fn default_model() -> String {
    DEFAULT_MODEL.into()
}

fn default_reasoning_effort() -> String {
    DEFAULT_REASONING_EFFORT.into()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiSelection {
    pub model: String,
    pub reasoning_effort: String,
}

impl Default for AiSelection {
    fn default() -> Self {
        Self {
            model: default_model(),
            reasoning_effort: default_reasoning_effort(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeveloperAiSettings {
    pub revision: u64,
    pub orchestrator: AiSelection,
    pub reviewer: AiSelection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiModel {
    pub id: String,
    pub name: String,
    pub reasoning_efforts: Vec<String>,
    pub default_reasoning_effort: String,
}

#[derive(Clone, Debug)]
pub struct AiModelCatalog {
    pub models: Vec<AiModel>,
    pub source: &'static str,
}

#[derive(Deserialize)]
struct Cache {
    models: Vec<CacheModel>,
}

#[derive(Deserialize)]
struct CacheModel {
    slug: String,
    display_name: String,
    default_reasoning_level: String,
    supported_reasoning_levels: Vec<CacheReasoning>,
    visibility: String,
}

#[derive(Deserialize)]
struct CacheReasoning {
    effort: String,
}

pub fn load_catalog(codex_home: &Path) -> Result<AiModelCatalog> {
    let path = codex_home.join("models_cache.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return bundled_catalog(),
        Err(error) => return Err(error).context("ChatGPT model cache metadata is unavailable"),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_CACHE_BYTES
    {
        bail!("ChatGPT model cache must be a bounded direct file");
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(&path)
        .context("ChatGPT model cache cannot be opened")?;
    let opened = file.metadata()?;
    if !opened.is_file() || opened.len() != metadata.len() || opened.len() > MAX_CACHE_BYTES {
        bail!("ChatGPT model cache changed while opening");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        if opened.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            bail!("ChatGPT model cache is a reparse point");
        }
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    file.take(MAX_CACHE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != opened.len() || bytes.len() as u64 > MAX_CACHE_BYTES {
        bail!("ChatGPT model cache changed while reading");
    }
    let cache: Cache =
        serde_json::from_slice(&bytes).context("ChatGPT model cache JSON is invalid")?;
    let visible = cache
        .models
        .into_iter()
        .filter(|model| model.visibility == "list")
        .map(|model| AiModel {
            id: model.slug,
            name: model.display_name,
            reasoning_efforts: model
                .supported_reasoning_levels
                .into_iter()
                .map(|level| level.effort)
                .collect(),
            default_reasoning_effort: model.default_reasoning_level,
        })
        .collect::<Vec<_>>();
    validate_models(&visible).context("ChatGPT model cache catalog failed validation")?;
    Ok(AiModelCatalog {
        models: visible,
        source: "codex_home_models_cache",
    })
}

fn bundled_catalog() -> Result<AiModelCatalog> {
    let models: Vec<AiModel> = serde_json::from_str(include_str!("developer_ai_models.json"))
        .context("Bundled developer AI model catalog is invalid")?;
    validate_models(&models).context("Bundled developer AI model catalog failed validation")?;
    Ok(AiModelCatalog {
        models,
        source: "bundled",
    })
}

pub fn validate_selection(catalog: &AiModelCatalog, selection: &AiSelection) -> Result<()> {
    validate_model_id(&selection.model)?;
    validate_reasoning_effort(&selection.reasoning_effort)?;
    let model = catalog
        .models
        .iter()
        .find(|model| model.id == selection.model)
        .context("Selected ChatGPT model is unavailable")?;
    if !model
        .reasoning_efforts
        .contains(&selection.reasoning_effort)
    {
        bail!("Selected reasoning effort is unavailable for this ChatGPT model");
    }
    Ok(())
}

pub fn validate_model_id(value: &str) -> Result<()> {
    if !value.starts_with("gpt-")
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
    {
        bail!("ChatGPT model ID is invalid");
    }
    Ok(())
}

pub fn validate_reasoning_effort(value: &str) -> Result<()> {
    if !matches!(
        value,
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
    ) {
        bail!("ChatGPT reasoning effort is invalid");
    }
    Ok(())
}

fn validate_models(models: &[AiModel]) -> Result<()> {
    if models.is_empty() || models.len() > MAX_MODELS {
        bail!("ChatGPT model catalog size is invalid");
    }
    let mut ids = BTreeSet::new();
    for model in models {
        validate_model_id(&model.id)?;
        if model.name.trim().is_empty()
            || model.name.len() > 120
            || model.name.chars().any(char::is_control)
        {
            bail!("ChatGPT model display name is invalid");
        }
        if !ids.insert(model.id.as_str())
            || model.reasoning_efforts.is_empty()
            || model.reasoning_efforts.len() > 8
        {
            bail!("ChatGPT model catalog entry is invalid");
        }
        let mut efforts = BTreeSet::new();
        for effort in &model.reasoning_efforts {
            validate_reasoning_effort(effort)?;
            if !efforts.insert(effort.as_str()) {
                bail!("ChatGPT model reasoning efforts are invalid");
            }
        }
        if !model
            .reasoning_efforts
            .contains(&model.default_reasoning_effort)
        {
            bail!("ChatGPT model default reasoning effort is invalid");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_includes_visible_chatgpt_models_and_spark() {
        let models: Vec<AiModel> =
            serde_json::from_str(include_str!("developer_ai_models.json")).unwrap();
        validate_models(&models).unwrap();
        assert_eq!(models.len(), 7);
        assert!(models.iter().any(|model| model.id == "gpt-5.3-codex-spark"));
        assert!(!models.iter().any(|model| model.id == "gpt-reserve"));
    }

    #[test]
    fn selection_rejects_unknown_model_and_unsupported_effort() {
        let catalog = load_catalog(Path::new("/path/that/does/not/exist")).unwrap();
        assert!(validate_selection(
            &catalog,
            &AiSelection {
                model: "unknown".into(),
                reasoning_effort: "high".into()
            }
        )
        .is_err());
        assert!(validate_selection(
            &catalog,
            &AiSelection {
                model: "gpt-5.5".into(),
                reasoning_effort: "ultra".into()
            }
        )
        .is_err());
    }
}
