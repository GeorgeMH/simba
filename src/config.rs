use linked_hash_map::LinkedHashMap;
use serde::{Deserialize, Serialize};

use crate::Result;
use lazy_static::lazy_static;
use std::sync::atomic::{AtomicU64, Ordering};

lazy_static! {
    static ref ANONYMOUS_STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);
}

pub fn load_pipeline_def(pipeline_path: &str) -> Result<PipelineDef> {
    log::info!("Loading pipeline file {}", pipeline_path);
    let yaml_data = std::fs::read_to_string(pipeline_path)?;

    let pipeline = serde_yaml::from_str(&yaml_data)?;

    Ok(pipeline)
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct PipelineDef {
    pub name: String,
    pub globals: LinkedHashMap<String, String>,

    pub steps: Vec<PipelineStepDef>,

    #[serde(default = "default_true")]
    pub stop_on_error: bool,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum RequestBody {
    BodyString(String),
    BodyStringTemplate(String),
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct PipelineStepDef {
    #[serde(default = "default_description")]
    pub desc: String,
    #[serde(default = "default_stage_name")]
    pub stage: String,
    pub when: Option<String>,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Option<LinkedHashMap<String, String>>,
    pub timeout_ms: Option<u64>,
    pub body: Option<RequestBody>,
    pub post_script: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_description() -> String {
    "Unknown".to_string()
}

fn default_stage_name() -> String {
    let anon_id = ANONYMOUS_STAGE_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("stage-{}", anon_id)
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    Post,
    Get,
    Put,
    Delete,
}
