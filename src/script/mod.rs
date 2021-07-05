use crate::Result;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

pub mod lua;

#[async_trait]
pub trait ScriptEngine: Clone + Send + Sync {
    async fn execute(&self, context: ScriptContext, code: &str) -> Result<ScriptResponse>;
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptContext {
    data: HashMap<String, Value>,
}

impl ScriptContext {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn data(&self) -> &HashMap<String, Value> {
        &self.data
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
