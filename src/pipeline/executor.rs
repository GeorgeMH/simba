use std::collections::HashMap;

use std::time::{Duration, Instant};

use linked_hash_map::LinkedHashMap;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::config::{HttpMethod, PipelineDef, PipelineStepDef, RequestBody};
use crate::error::{SimbaError, SimbaResult};
use crate::output::{PipelineEventHandler, TaskUpdate};
use crate::pipeline::{ExecutionResult, HttpResponse, Pipeline, StepResult, StepTask, TaskState};
use crate::script::{ScriptContext, ScriptEngine};
use crate::template::TemplateEngine;
use crate::Result;
use async_trait::async_trait;
use std::fmt::{Display, Formatter};

#[derive(Clone)]
pub struct PipelineExecutor<E: PipelineEventHandler, S: ScriptEngine, T: TemplateEngine> {
    output_writer: E,
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
            output_writer,
            script,
            template,
            client: reqwest::Client::new(),
        })
    }

    pub fn build_pipeline(
        pipeline_def: &PipelineDef,
        // _stages_to_exec: Vec<String>,
    ) -> Result<Pipeline> {
        let mut pipeline = Pipeline::new(pipeline_def.name.clone());

        for step in &pipeline_def.steps {
            let stage = pipeline.get_stage(step.stage.as_str());
            stage.add_task(StepTask::new(step.clone(), stage.id()));
        }

        Ok(pipeline)
    }

    pub async fn execute_pipeline(&self, pipeline_def: &PipelineDef) -> Result<()> {
        let pipeline = Self::build_pipeline(pipeline_def)?;

        let mut script_context = ScriptContext::new();
        for (key, value) in &pipeline_def.globals {
            let tpl_value = self.template.template(value.as_str(), &script_context)?;
            script_context.set(key.as_str(), tpl_value)?;
        }

        self.output_writer.pipeline_init(&pipeline).await;
        for stage in pipeline.stages() {
            self.output_writer.stage_start(stage).await;

            let mut pending_steps = Vec::new();

            for task in stage.tasks() {
                let child_self: PipelineExecutor<E, S, T> = self.clone();
                let mut child_task = task.clone();
                let mut child_context = script_context.clone();

                let step_join_handle = tokio::spawn(async move {
                    log::info!("Child Step: {}", child_task.id);
                    child_self
                        .execute_step(&mut child_task, &mut child_context)
                        .await;
                    child_context
                });
                pending_steps.push(step_join_handle);
            }

            log::info!("Await stage");

            for step_join_handle in pending_steps {
                let child_context = step_join_handle.await?;
                script_context.merge(child_context)?;
            }

            log::info!("Call Stage End");
            self.output_writer.stage_end(stage).await;
        }

        self.output_writer.pipeline_finish(&pipeline).await;

        Ok(())
    }

    pub async fn execute_step(&self, step_task: &mut StepTask, script_context: &mut ScriptContext) {
        log::info!("Pipeline Step: {}", &step_task.step.desc);

        self.output_writer
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
            self.output_writer
                .task_update(&step_task, TaskUpdate::Processing(format!("{}", task)))
                .await;
            let task_step_result = task.apply(script_context, step_task).await;
            match task_step_result {
                Ok(task_state) => match task_state {
                    TaskState::Executing(msg) => {
                        self.output_writer
                            .task_update(&step_task, TaskUpdate::Processing(msg))
                            .await;
                        continue;
                    }
                    TaskState::Skipped(msg) => {
                        let step_result = StepResult {
                            step: step_task.step.clone(),
                            rendered_step: step_task.rendered_step.clone(),
                            result: ExecutionResult::Skipped(msg),
                            execution_time_ms: start_time.elapsed().as_millis(),
                        };

                        self.output_writer
                            .task_update(&step_task, TaskUpdate::Finished(step_result))
                            .await;
                        return;
                    }
                    TaskState::Error(msg) => {
                        let step_result = StepResult {
                            step: step_task.step.clone(),
                            rendered_step: step_task.rendered_step.clone(),
                            result: ExecutionResult::Error(msg),
                            execution_time_ms: start_time.elapsed().as_millis(),
                        };

                        self.output_writer
                            .task_update(&step_task, TaskUpdate::Finished(step_result))
                            .await;
                        return;
                    }
                },
                Err(task_error) => {
                    let step_result = StepResult {
                        step: step_task.step.clone(),
                        rendered_step: step_task.rendered_step.clone(),
                        result: ExecutionResult::Error(format!("{}", task_error)),
                        execution_time_ms: start_time.elapsed().as_millis(),
                    };

                    self.output_writer
                        .task_update(&step_task, TaskUpdate::Finished(step_result))
                        .await;
                    return;
                }
            }
        }

        let response: HttpResponse = script_context.get("http_response").unwrap().unwrap(); // TODO: Handle this case

        let step_result = StepResult {
            step: step_task.step.clone(),
            rendered_step: step_task.rendered_step.clone(),
            result: ExecutionResult::Response(response),
            execution_time_ms: start_time.elapsed().as_millis(),
        };

        self.output_writer
            .task_update(&step_task, TaskUpdate::Finished(step_result))
            .await;
    }
}

fn convert_header_map(header_map: &HeaderMap) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    for (key, value) in header_map.iter() {
        let k = key.as_str().to_owned();
        let v = String::from_utf8_lossy(value.as_bytes()).into_owned();
        headers.insert(k, v);
    }
    headers
}

#[async_trait]
pub trait PipelineStep: Display + Send + Sync {
    async fn apply(
        &self,
        script_context: &mut ScriptContext,
        step_task: &mut StepTask,
    ) -> Result<TaskState>;
}

struct RenderPipelineStep<T: TemplateEngine> {
    template: T,
}

impl<T: TemplateEngine> RenderPipelineStep<T> {
    fn new(template: T) -> Self {
        Self { template }
    }
}

impl<T: TemplateEngine> Display for RenderPipelineStep<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rendering Step")
    }
}

#[async_trait]
impl<T: TemplateEngine> PipelineStep for RenderPipelineStep<T> {
    async fn apply(
        &self,
        script_context: &mut ScriptContext,
        step_task: &mut StepTask,
    ) -> Result<TaskState> {
        let url = self
            .template
            .template(&step_task.step.url, &script_context)?;
        let timeout_ms = step_task.step.timeout_ms;

        let headers = match step_task.step.headers.as_ref() {
            Some(headers) => {
                let mut generated_headers = LinkedHashMap::new();
                for (header, value) in headers {
                    let header = self.template.template(header, &script_context)?;
                    let value = self.template.template(value, &script_context)?;
                    generated_headers.insert(header, value);
                }
                Some(generated_headers)
            }
            None => None,
        };
        let body = match &step_task.step.body {
            Some(request_body) => {
                let rendered_body = match request_body {
                    RequestBody::BodyString(body) => body.clone(),
                    RequestBody::BodyStringTemplate(template) => {
                        self.template.template(template, &script_context)?
                    }
                };
                Some(RequestBody::BodyString(rendered_body))
            }
            None => None,
        };

        let rendered_step = PipelineStepDef {
            desc: step_task.step.desc.clone(),
            method: step_task.step.method.clone(),
            stage: step_task.step.stage.clone(),
            url,
            headers,
            timeout_ms,
            when: step_task.step.when.clone(),
            body,
            tags: step_task.step.tags.clone(),
            post_script: step_task.step.post_script.clone(),
        };

        step_task.rendered_step = Some(rendered_step);
        Ok(TaskState::Executing("Rendered".to_string()))
    }
}

struct ExecuteWhenClausePipelineStep<S: ScriptEngine> {
    script: S,
}

impl<S: ScriptEngine> ExecuteWhenClausePipelineStep<S> {
    fn new(script: S) -> Self {
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
                return Ok(TaskState::Skipped(skip_message));
            }
        }

        Ok(TaskState::Executing("Rendered".to_string()))
    }
}

struct HttpCallPipelineStep {
    client: reqwest::Client,
}

impl HttpCallPipelineStep {
    fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Display for HttpCallPipelineStep {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP Call")
    }
}

#[async_trait]
impl PipelineStep for HttpCallPipelineStep {
    #[allow(clippy::cast_possible_truncation)]
    async fn apply(
        &self,
        script_context: &mut ScriptContext,
        step_task: &mut StepTask,
    ) -> SimbaResult<TaskState> {
        let rendered_step = step_task
            .rendered_step
            .as_ref()
            .unwrap_or_else(|| panic!("Expected Rendered Step: {}", step_task.id));

        let start_time = Instant::now();
        let url = rendered_step.url.clone();
        let mut request = match rendered_step.method {
            HttpMethod::Post => self.client.post(url.as_str()),
            HttpMethod::Get => self.client.get(url.as_str()),
            HttpMethod::Put => self.client.put(url.as_str()),
            HttpMethod::Delete => self.client.delete(url.as_str()),
            // TODO: Others
        };

        if let Some(timeout_ms) = rendered_step.timeout_ms {
            request = request.timeout(Duration::from_millis(timeout_ms));
        }

        if let Some(headers) = rendered_step.headers.as_ref() {
            for (header, value) in headers {
                request = request.header(
                    HeaderName::from_bytes(header.as_bytes())?,
                    HeaderValue::from_bytes(value.as_bytes())?,
                );
            }
        }

        if let Some(body) = rendered_step.body.as_ref() {
            match body {
                RequestBody::BodyString(body_string) => {
                    request = request.body(body_string.clone());
                }
                unhandled_body => {
                    return Err(SimbaError::Other(format!(
                        "Unexpected RequestBody type {:?}",
                        unhandled_body
                    )));
                }
            }
        }

        log::info!("Calling URL: {}", url);
        let result = request.send().await?;

        let status = result.status().into();
        let headers = convert_header_map(result.headers());

        let body_raw = result.bytes().await?.to_vec();
        // TODO: Support binary / non-utf8 responses
        let body_string = String::from_utf8(body_raw)?;

        let duration = start_time.elapsed();

        script_context.set("call_duration_ms", duration.as_millis() as u64)?;

        script_context.set(
            "http_response",
            HttpResponse {
                post_script_result: None,
                status,
                headers,
                body_string: Some(body_string),
                execution_time_ms: duration.as_millis() as u64,
            },
        )?;

        log::info!("Http Call Finished");

        Ok(TaskState::Executing("HTTP Call finished".to_string()))
    }
}

struct PostScriptPipelineStep<S: ScriptEngine> {
    script: S,
}

impl<S: ScriptEngine> PostScriptPipelineStep<S> {
    fn new(script: S) -> Self {
        Self { script }
    }
}

impl<S: ScriptEngine> Display for PostScriptPipelineStep<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Post Script")
    }
}

#[async_trait]
impl<S: ScriptEngine> PipelineStep for PostScriptPipelineStep<S> {
    async fn apply(
        &self,
        script_context: &mut ScriptContext,
        step_task: &mut StepTask,
    ) -> SimbaResult<TaskState> {
        let rendered_step = step_task
            .rendered_step
            .as_ref()
            .unwrap_or_else(|| panic!("Expected Rendered Step: {}", step_task.id));

        if let Some(post_script) = rendered_step.post_script.as_ref() {
            log::info!("Calling PostScript\n{:#?}", script_context);
            let (updated_context, post_script_result): (ScriptContext, bool) = self
                .script
                .execute(script_context.clone(), post_script)
                .await?
                .result()?;

            script_context.merge(updated_context)?;
            log::info!("Finished PostScript\n{:#?}", script_context);
            script_context.set("post_script_result", post_script_result)?;

            if !post_script_result {
                return Ok(TaskState::Error("Post Script returned false".to_string()));
            }
        }

        Ok(TaskState::Executing("Post Script Finished".to_string()))
    }
}
