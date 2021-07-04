use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::config::PipelineStepDef;
use lazy_static::lazy_static;

mod executor;
pub use executor::PipelineExecutor;
use linked_hash_map::LinkedHashMap;

pub type NodeId = u64;

lazy_static! {
    static ref NODE_COUNTER: AtomicU64 = AtomicU64::new(0);
}

#[derive(Debug, Clone)]
pub struct Pipeline {
    name: String,
    stages: LinkedHashMap<String, Stage>,
}

impl Pipeline {
    pub fn new(name: String) -> Self {
        Self {
            name,
            stages: LinkedHashMap::new(),
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn get_stage(&mut self, stage_name: &str) -> &mut Stage {
        if self.stages.contains_key(stage_name) {
            self.stages.get_mut(stage_name).unwrap()
        } else {
            self.stages
                .insert(stage_name.to_string(), Stage::new(stage_name));
            self.stages
                .get_mut(stage_name)
                .expect("Failed finding newly created stage")
        }
    }

    pub fn stages(&self) -> Vec<&Stage> {
        let mut stages = Vec::new();
        for stage in self.stages.values() {
            stages.push(stage);
        }
        stages
    }
}

#[derive(Debug, Clone)]
pub struct Stage {
    id: NodeId,
    name: String,
    steps: Vec<StepTask>,
}

impl Stage {
    pub fn new(name: &str) -> Self {
        Self {
            id: NODE_COUNTER.fetch_add(1, Ordering::Relaxed),
            name: name.to_string(),
            steps: Vec::new(),
        }
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn add_task(&mut self, task: StepTask) {
        self.steps.push(task);
    }

    pub fn tasks(&self) -> &[StepTask] {
        self.steps.as_slice()
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum TaskState {
    Executing(String),
    Skipped(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct StepTask {
    pub id: NodeId,
    pub parent_id: NodeId,
    pub step: PipelineStepDef,
    pub rendered_step: Option<PipelineStepDef>,
    // TODO: Make this optional
    pub start_time: Instant,
}

impl StepTask {
    pub fn new(step: PipelineStepDef, parent_id: NodeId) -> Self {
        Self {
            id: NODE_COUNTER.fetch_add(1, Ordering::Relaxed),
            parent_id,
            step,
            rendered_step: None,
            start_time: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResults {
    pub name: String,
    pub step_responses: Vec<StepResult>,
    pub execution_time_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step: PipelineStepDef,
    pub rendered_step: Option<PipelineStepDef>,
    pub result: ExecutionResult,
    pub execution_time_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub post_script_result: Option<bool>,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body_string: Option<String>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionResult {
    Response(HttpResponse),
    Skipped(String),
    Error(String),
}
