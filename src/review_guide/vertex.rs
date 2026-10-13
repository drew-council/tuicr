//! Gemini on Vertex AI via the official `PredictionService` client.
//!
//! The project and credentials come from configuration or the environment,
//! never from the source tree: `project` falls back to `GOOGLE_CLOUD_PROJECT`
//! and then to the credentials file's quota project, and credentials fall
//! back to Application Default Credentials discovery.

use std::path::{Path, PathBuf};
use std::time::Duration;

use google_cloud_aiplatform_v1::client::PredictionService;
use google_cloud_aiplatform_v1::model::generation_config::ThinkingConfig;
use google_cloud_aiplatform_v1::model::generation_config::thinking_config::ThinkingLevel;
use google_cloud_aiplatform_v1::model::{Content, GenerationConfig, Part};
use google_cloud_auth::credentials::{Credentials, service_account, user_account};

use super::{Prompt, ReviewGuide, parse_response, response_schema};
use crate::config::ReviewGuideConfig;

/// Run one guide request to completion on a private current-thread runtime.
/// Called from the guide's own background thread, never the UI thread.
pub(super) fn generate(config: &ReviewGuideConfig, prompt: &Prompt) -> Result<ReviewGuide, String> {
    // Without a process default, rustls panics when the client connects (both
    // `ring` and `aws-lc-rs` are compiled in), and tuicr's panic hook would
    // then tear down the terminal from this background thread. `Err` only
    // means a provider is already installed.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to start async runtime: {e}"))?;
    let timeout = Duration::from_secs(config.timeout_secs);
    runtime.block_on(async {
        tokio::time::timeout(timeout, request(config, prompt))
            .await
            .map_err(|_| format!("Vertex AI did not answer within {}s", config.timeout_secs))?
    })
}

async fn request(config: &ReviewGuideConfig, prompt: &Prompt) -> Result<ReviewGuide, String> {
    let credentials_file = credentials_file(config);
    let project = resolve_project(config, credentials_file.as_deref())?;

    let mut builder = PredictionService::builder().with_endpoint(endpoint(&config.location));
    if let Some(path) = config.credentials_file.as_deref() {
        builder = builder.with_credentials(load_credentials(path)?);
    }
    let client = builder
        .build()
        .await
        .map_err(|e| format!("failed to create Vertex AI client: {e}"))?;

    let model = format!(
        "projects/{project}/locations/{}/publishers/google/models/{}",
        config.location, config.model
    );
    let generation = GenerationConfig::new()
        .set_response_mime_type("application/json")
        .set_response_json_schema(response_schema())
        .set_thinking_config(ThinkingConfig::new().set_thinking_level(thinking_level(config)));
    let response = client
        .generate_content()
        .set_model(model)
        .set_system_instruction(Content::new().set_parts([Part::new().set_text(&prompt.system)]))
        .set_contents([Content::new()
            .set_role("user")
            .set_parts([Part::new().set_text(&prompt.user)])])
        .set_generation_config(generation)
        .send()
        .await
        .map_err(|e| format!("Vertex AI request failed: {e}"))?;

    let text: String = response
        .candidates
        .iter()
        .filter_map(|candidate| candidate.content.as_ref())
        .flat_map(|content| content.parts.iter())
        // Thought summaries are not part of the answer.
        .filter(|part| !part.thought)
        .filter_map(|part| part.text())
        .map(String::as_str)
        .collect();
    if text.trim().is_empty() {
        return Err("Vertex AI returned an empty response".to_string());
    }
    parse_response(&text)
}

/// `global` has no regional prefix; every other location does.
fn endpoint(location: &str) -> String {
    if location == "global" {
        "https://aiplatform.googleapis.com".to_string()
    } else {
        format!("https://{location}-aiplatform.googleapis.com")
    }
}

fn thinking_level(config: &ReviewGuideConfig) -> ThinkingLevel {
    match config.thinking.as_str() {
        "minimal" => ThinkingLevel::Minimal,
        "medium" => ThinkingLevel::Medium,
        "high" => ThinkingLevel::High,
        _ => ThinkingLevel::Low,
    }
}

/// The credentials file that will authenticate the request, if one can be
/// located: the configured file, else the ADC discovery locations.
fn credentials_file(config: &ReviewGuideConfig) -> Option<PathBuf> {
    config
        .credentials_file
        .clone()
        .or_else(|| {
            std::env::var_os("GOOGLE_APPLICATION_CREDENTIALS")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
        .or_else(adc_well_known_file)
        .filter(|path| path.is_file())
}

/// gcloud's `application-default login` output location.
fn adc_well_known_file() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(|dir| PathBuf::from(dir).join("gcloud/application_default_credentials.json"))
    } else {
        directories::BaseDirs::new().map(|dirs| {
            dirs.home_dir()
                .join(".config/gcloud/application_default_credentials.json")
        })
    }
}

fn resolve_project(
    config: &ReviewGuideConfig,
    credentials_file: Option<&Path>,
) -> Result<String, String> {
    if let Some(project) = &config.project {
        return Ok(project.clone());
    }
    for var in ["GOOGLE_CLOUD_PROJECT", "GCLOUD_PROJECT"] {
        if let Some(project) = std::env::var(var).ok().filter(|p| !p.trim().is_empty()) {
            return Ok(project.trim().to_string());
        }
    }
    credentials_file
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|contents| project_from_credentials_json(&contents))
        .ok_or_else(|| {
            "no Google Cloud project: set review_guide.project, GOOGLE_CLOUD_PROJECT, \
             or a quota project on your credentials \
             (`gcloud auth application-default set-quota-project <project>`)"
                .to_string()
        })
}

/// `authorized_user` files carry `quota_project_id`; service account keys
/// carry `project_id`.
fn project_from_credentials_json(contents: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(contents).ok()?;
    ["quota_project_id", "project_id"]
        .iter()
        .find_map(|key| json.get(key)?.as_str())
        .map(str::trim)
        .filter(|project| !project.is_empty())
        .map(str::to_string)
}

fn load_credentials(path: &Path) -> Result<Credentials, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()))?;
    let kind = json
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let built = match kind.as_str() {
        "authorized_user" => user_account::Builder::new(json).build(),
        "service_account" => service_account::Builder::new(json).build(),
        other => {
            return Err(format!(
                "unsupported credential type {other:?} in {}; \
                 use an authorized_user or service_account file, or unset \
                 review_guide.credentials_file to use Application Default Credentials",
                path.display()
            ));
        }
    };
    built.map_err(|e| format!("invalid credentials in {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_use_regional_endpoint_except_for_global() {
        assert_eq!(endpoint("global"), "https://aiplatform.googleapis.com");
        assert_eq!(
            endpoint("us-central1"),
            "https://us-central1-aiplatform.googleapis.com"
        );
    }

    #[test]
    fn should_read_quota_or_service_account_project_from_credentials() {
        assert_eq!(
            project_from_credentials_json(r#"{"type":"authorized_user","quota_project_id":"q"}"#),
            Some("q".to_string())
        );
        assert_eq!(
            project_from_credentials_json(r#"{"type":"service_account","project_id":"p"}"#),
            Some("p".to_string())
        );
        assert_eq!(
            project_from_credentials_json(r#"{"type":"authorized_user"}"#),
            None
        );
    }

    #[test]
    fn should_prefer_configured_project() {
        let config = ReviewGuideConfig {
            project: Some("configured".to_string()),
            ..ReviewGuideConfig::default()
        };
        assert_eq!(resolve_project(&config, None).unwrap(), "configured");
    }
}
