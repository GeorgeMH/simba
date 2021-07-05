///
use crate::output::{PipelineEventHandler, TaskUpdate};
use crate::pipeline::{Pipeline, Stage, StepTask};
use async_trait::async_trait;

#[derive(Clone)]
pub struct JsonWriter {}

impl JsonWriter {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl PipelineEventHandler for JsonWriter {
    async fn pipeline_init(&self, pipeline: &Pipeline) {
        log::info!("Pipeline Init {}", pipeline.name());
    }

    async fn stage_start(&self, stage: &Stage) {
        log::info!("Stage Start {}", stage.id());
    }

    async fn task_update(&self, task: &StepTask, task_update: TaskUpdate) {
        log::info!("JSON task_update {}: {:?}", task.id, task_update);

        match task_update {
            TaskUpdate::Processing(_) => {}
            TaskUpdate::Finished(step_result) => {
                let json =
                    serde_json::to_string(&step_result).expect("Failed serializing StepResult");
                println!("{}", json);
            }
        }
    }

    async fn stage_end(&self, stage: &Stage) {
        log::info!("stage_end {}", stage.id());
    }

    async fn pipeline_finish(&self, _pipeline: &Pipeline) {
        log::info!("Pipeline Finish");
    }
}
