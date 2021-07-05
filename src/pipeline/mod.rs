use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use lazy_static::lazy_static;
use linked_hash_map::LinkedHashMap;
use serde::{Deserialize, Serialize};

pub use executor::PipelineExecutor;

use crate::config::PipelineStepDef;
use crate::error;
use crate::script::ScriptContext;

mod executor;
mod render_step;
mod when_clause;
mod http_call;
mod post_script;

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
        self.stages.values().collect()
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
    Complete(String),
    Skip(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct StepTask {
    pub id: NodeId,
    pub parent_id: NodeId,
    pub step: PipelineStepDef,
    pub rendered_step: Option<PipelineStepDef>,
}

impl StepTask {
    pub fn new(step: PipelineStepDef, parent_id: NodeId) -> Self {
        Self {
            id: NODE_COUNTER.fetch_add(1, Ordering::Relaxed),
            parent_id,
            step,
            rendered_step: None,
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

impl StepResult {
    pub fn new(
        step: PipelineStepDef,
        rendered_step: Option<PipelineStepDef>,
        result: ExecutionResult,
        execution_time_ms: u128,
    ) -> Self {
        Self {
            step,
            rendered_step,
            result,
            execution_time_ms,
        }
    }
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

#[async_trait]
pub trait PipelineStep: Display + Send + Sync {
    async fn apply(
        &self,
        script_context: &mut ScriptContext,
        step_task: &mut StepTask,
    ) -> error::SimbaResult<TaskState>;
}
