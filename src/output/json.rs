///


use crate::output::PipelineEventHandler;
use crate::pipeline::{Pipeline, Stage, StepTask, TaskState};

#[derive(Clone)]
pub struct JsonWriter {}

impl JsonWriter {
    pub fn new() -> Self {
        Self {}
    }
}

impl PipelineEventHandler for JsonWriter {
    fn pipeline_init(&self, pipeline: &Pipeline) {
        log::info!("Pipeline Init {}", pipeline.name());
    }

    fn stage_start(&self, stage: &Stage) {
        log::info!("Stage Start {}", stage.id());
    }

    fn task_update(&self, task: &StepTask, task_state: TaskState) {
        log::info!("JSON task_update");

        match task_state {
            TaskState::Pending => {
                log::info!("Task {} - Pending", task.id);
            }
            TaskState::Executing(message) => {
                log::info!("Task {} - Executing - {}", task.id, message);
            }
            TaskState::Result(step_result) => {
                let json =
                    serde_json::to_string(&step_result).expect("Failed serializing StepResult");
                println!("{}", json);
            }
        }
    }

    fn stage_end(&self, stage: &Stage) {
        log::info!("stage_end {}", stage.id());
    }

    fn pipeline_finish(&self, _pipeline: &Pipeline) {
        log::info!("Pipeline Finish");
    }
}
