use std::fmt::{Display, Formatter};

use async_trait::async_trait;

use crate::error::SimbaResult;
use crate::pipeline::PipelineStep;
use crate::pipeline::{StepTask, TaskState};
use crate::script::{ScriptContext, ScriptEngine};

pub struct ExecuteWhenClausePipelineStep<S: ScriptEngine> {
    script: S,
}

impl<S: ScriptEngine> ExecuteWhenClausePipelineStep<S> {
    pub fn new(script: S) -> Self {
        Self { script }
    }
}

impl<S: ScriptEngine> Display for ExecuteWhenClausePipelineStep<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "When Clause")
    }
}

#[async_trait]
impl<S: ScriptEngine> PipelineStep for ExecuteWhenClausePipelineStep<S> {
    async fn apply(
        &self,
        script_context: &mut ScriptContext,
        step_task: &mut StepTask,
    ) -> SimbaResult<TaskState> {
        let rendered_step = step_task
            .rendered_step
            .as_ref()
            .unwrap_or_else(|| panic!("Expected Rendered Step: {}", step_task.id));

        if let Some(when_clause) = rendered_step.when.as_ref() {
            let (updated_context, when_clause_result): (ScriptContext, bool) = self
                .script
                .execute(script_context.clone(), when_clause)
                .await?
                .result()?;
            script_context.merge(updated_context)?;

            if !when_clause_result {
                let skip_message = format!(
                    "Skipped: When clause evaluated false: {}",
                    when_clause.replace("\n", "\\n")
                );
                return Ok(TaskState::Skip(skip_message));
            }
        }

        Ok(TaskState::Complete("Rendered".to_string()))
    }
}
