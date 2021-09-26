use crate::error::SimbaError;
use crate::pipeline::HttpResponse;
use crate::Result;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

pub mod lua;

#[async_trait]
pub trait ScriptEngine: Clone + Send + Sync {
    async fn execute(
        &self,
        context: ScriptContext,
        chunk_name: &str,
        code: &str,
    ) -> Result<ScriptResponse>;
}

#[derive(Debug)]
pub struct ScriptResponse {
    pub context: ScriptContext,
    pub value: Result<Value>,
}

impl ScriptResponse {
    pub fn new(context: ScriptContext, value: Result<Value>) -> Self {
        Self { context, value }
    }

    pub fn context(&self) -> &ScriptContext {
        &self.context
    }

    pub fn result<T: DeserializeOwned>(self) -> Result<(ScriptContext, T)> {
        let serde_value = self.value?;
        let deserialized_value: T = serde_json::from_value(serde_value)?;
        Ok((self.context, deserialized_value))
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptContext {
    data: HashMap<String, Value>,
}

const HTTP_RESPONSE_KEY: &str = "http_response";

impl ScriptContext {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn data(&self) -> &HashMap<String, Value> {
        &self.data
    }

    pub fn get_http_response(&self) -> Result<HttpResponse> {
        self.get_required(HTTP_RESPONSE_KEY)
    }

    pub fn set_http_response(
        &mut self,
        http_response: HttpResponse,
    ) -> Result<Option<HttpResponse>> {
        self.set(HTTP_RESPONSE_KEY, http_response)
    }

    pub fn get_required<T: DeserializeOwned>(&self, key: &str) -> Result<T> {
        let maybe_t = self.get(key)?;
        maybe_t.ok_or_else(|| SimbaError::Other(format!("Missing key {} in script context", key)))
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        if let Some(serde_value) = self.data.get(key) {
            let value: T = serde_json::from_value(serde_value.clone())?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    pub fn set<T: Serialize + DeserializeOwned>(
        &mut self,
        key: &str,
        value: T,
    ) -> Result<Option<T>> {
        let serde_value = serde_json::to_value(value)?;

        if let Some(old_value) = self.data.insert(key.to_string(), serde_value) {
            let old_val: T = serde_json::from_value(old_value)?;
            Ok(Some(old_val))
        } else {
            Ok(None)
        }
    }

    pub fn merge(&mut self, other_context: ScriptContext) -> Result<()> {
        self.data.extend(other_context.data);
        Ok(())
    }
}
