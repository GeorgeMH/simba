use linked_hash_map::LinkedHashMap;
use serde::{Deserialize, Serialize};

use crate::Result;
use tokio::io::AsyncReadExt;

pub async fn load_pipeline_def(pipeline_path: &str) -> Result<PipelineDef> {
    log::info!("Loading pipeline file {}", pipeline_path);
    let yaml_data = match pipeline_path {
        "-" => {
            let mut buffer = String::new();
            tokio::io::stdin().read_to_string(&mut buffer).await?;
            buffer
        }
        path => tokio::fs::read_to_string(path).await?,
    };

    let pipeline = serde_yaml::from_str(&yaml_data)?;
    Ok(pipeline)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDef {
    pub name: String,
    pub globals: LinkedHashMap<String, String>,

    pub stages: Vec<StageDef>,

    #[serde(default = "default_steps")]
    pub steps: Vec<PipelineStepDef>,

    #[serde(default = "default_true")]
    pub stop_on_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageDef {
    #[serde(default = "default_stage_name")]
    pub name: String,

    #[serde(default = "default_false")]
    pub concurrent: bool,

    #[serde(default = "default_steps")]
    pub steps: Vec<PipelineStepDef>,
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

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    Post,
    Get,
    Put,
    Delete,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_description() -> String {
    "Unknown".to_string()
}

fn default_stage_name() -> String {
    "default".to_string()
}

fn default_steps() -> Vec<PipelineStepDef> {
    Vec::new()
}
