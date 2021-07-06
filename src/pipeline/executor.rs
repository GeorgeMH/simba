use crate::config::PipelineDef;
use std::time::Instant;

use crate::event::{PipelineEventHandler, TaskUpdate};
use crate::pipeline::http_call::HttpCallPipelineStep;
use crate::pipeline::post_script::PostScriptPipelineStep;
use crate::pipeline::render_step::RenderPipelineStep;
use crate::pipeline::when_clause::ExecuteWhenClausePipelineStep;
use crate::pipeline::{ExecutionResult, Pipeline, PipelineStep, StepResult, StepTask, TaskState};
use crate::script::{ScriptContext, ScriptEngine};
use crate::template::TemplateEngine;
use crate::Result;

#[derive(Clone)]
pub struct PipelineExecutor<E: PipelineEventHandler, S: ScriptEngine, T: TemplateEngine> {
    event_handler: E,
    script: S,
    template: T,
    client: reqwest::Client,
}

impl<E, S, T> PipelineExecutor<E, S, T>
where
    E: PipelineEventHandler + 'static,
    S: ScriptEngine + 'static,
    T: TemplateEngine + 'static,
{
    pub async fn new(
        output_writer: E,
        script: S,
        template: T,
    ) -> Result<PipelineExecutor<E, S, T>> {
        Ok(PipelineExecutor {
            event_handler: output_writer,
            script,
            template,
            client: reqwest::Client::new(),
        })
    }

    pub fn build_pipeline(
        pipeline_def: &PipelineDef,
        stages_to_exec: &[String],
    ) -> Result<Pipeline> {
        let mut pipeline = Pipeline::new(pipeline_def.name.clone());

        let stage_name_predicate =
            |s: &String| stages_to_exec.is_empty() || stages_to_exec.contains(s);

        for stage_def in &pipeline_def.stages {
            let mut stage = pipeline.stage(&stage_def.name);
            stage.concurrent = stage_def.concurrent;
            for step_def in &stage_def.steps {
                let mut task_step_def = step_def.clone();
                if task_step_def.stage != stage.name {
                    // TODO: Should this be a fatal error
                    log::error!(
                        "Step Def {} defined with mismatching stage name {} in stage {}",
                        step_def.desc,
                        step_def.stage,
                        stage.name
                    );
                    task_step_def.stage = stage.name.clone();
                }
                stage.add_task(StepTask::new(task_step_def, stage.id))
            }
        }

        for step in &pipeline_def.steps {
            if stage_name_predicate(&step.stage) {
                let stage = pipeline.stage(step.stage.as_str());
                stage.add_task(StepTask::new(step.clone(), stage.id));
            }
        }

        Ok(pipeline)
    }

    pub async fn execute_pipeline(
        &self,
        pipeline_def: &PipelineDef,
        stages_to_exec: &[String],
    ) -> Result<()> {
        let mut pipeline = Self::build_pipeline(pipeline_def, stages_to_exec)?;
        let mut script_context = ScriptContext::new();
        for (key, value) in &pipeline_def.globals {
            let tpl_value = self.template.render(value.as_str(), &script_context)?;
            script_context.set(key.as_str(), tpl_value)?;
        }

        self.event_handler.pipeline_init(&pipeline).await;
        for stage in pipeline.stages_mut() {
            self.event_handler.stage_start(stage).await;

            if stage.concurrent {
                let mut pending_steps = Vec::new();
                for step_task in &mut stage.tasks {
                    let child_self: PipelineExecutor<E, S, T> = self.clone();
                    let mut child_task = step_task.clone();
                    let mut child_context = script_context.clone();

                    let step_join_handle = tokio::spawn(async move {
                        log::info!("Concurrent Child Step: {}", child_task.id);
                        child_self
                            .execute_step(&mut child_task, &mut child_context)
                            .await;
                        child_context
                    });
                    pending_steps.push(step_join_handle);
                }

                for task_result in futures::future::join_all(pending_steps).await {
                    let child_context = task_result?;
                    script_context.merge(child_context)?;
                }
            } else {
                for step_task in &mut stage.tasks {
                    log::info!("Executing Child Step: {}", step_task.id);
                    self.execute_step(step_task, &mut script_context).await;
                }
            }

            log::info!("Call Stage End");
            self.event_handler.stage_end(stage).await;
        }

        self.event_handler.pipeline_finish(&pipeline).await;

        Ok(())
    }

    pub async fn execute_step(&self, step_task: &mut StepTask, script_context: &mut ScriptContext) {
        log::info!("Pipeline Step: {}", &step_task.step.desc);

        self.event_handler
            .task_update(&step_task, TaskUpdate::Processing("Starting".to_string()))
            .await;

        let tasks: Vec<Box<dyn PipelineStep>> = vec![
            Box::new(RenderPipelineStep::new(self.template.clone())),
            Box::new(ExecuteWhenClausePipelineStep::new(self.script.clone())),
            Box::new(HttpCallPipelineStep::new(self.client.clone())),
            Box::new(PostScriptPipelineStep::new(self.script.clone())),
        ];

        let start_time = Instant::now();
        for task in tasks {
            self.event_handler
                .task_update(&step_task, TaskUpdate::Processing(format!("{}", task)))
                .await;

            match task.apply(script_context, step_task).await {
                // The step was processed successfully and processing should continue to the next step
                Ok(TaskState::Complete(msg)) => {
                    self.event_handler
                        .task_update(&step_task, TaskUpdate::Processing(msg))
                        .await;
                    continue;
                }

                // All other TaskState's are terminal
                task_step_result => {
                    let execution_result = match task_step_result {
                        // This is impossible
                        Ok(TaskState::Complete(_)) => {
                            panic!("TaskState::Complete should have already been handled")
                        }
                        Ok(TaskState::Skip(msg)) => ExecutionResult::Skipped(msg),
                        Ok(TaskState::Error(msg)) => ExecutionResult::Error(msg),
                        Err(error) => ExecutionResult::Error(format!("{}", error)),
                    };

                    let step_result = StepResult::new(
                        step_task.step.clone(),
                        step_task.rendered_step.clone(),
                        execution_result,
                        start_time.elapsed().as_millis(),
                    );
                    self.event_handler
                        .task_update(&step_task, TaskUpdate::Finished(step_result))
                        .await;
                    return;
                }
            }
        }

        // TODO: can we remove the implicit contract that if we've gotten this far, `http_response` was set by a task
        let execution_result = match script_context.get("http_response") {
            Ok(Some(response)) => ExecutionResult::Response(response),
            Ok(None) => ExecutionResult::Error("Failed finding result".to_string()),
            Err(error) => ExecutionResult::Error(format!("{}", error)),
        };

        let step_result = StepResult::new(
            step_task.step.clone(),
            step_task.rendered_step.clone(),
            execution_result,
            start_time.elapsed().as_millis(),
        );

        self.event_handler
            .task_update(&step_task, TaskUpdate::Finished(step_result))
            .await;
    }
}
