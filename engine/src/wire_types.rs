use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestSource {
    Cli,
    Api,
    Dashboard,
    Agent,
    Workflow,
    TestFixture,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispatchRequest {
    pub schema_version: String,
    pub raw_request: String,
    pub request_source: RequestSource,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DispatchBundleValue {
    pub record: Value,
    pub analysis: Value,
    pub decision: Value,
    pub execution_result: Value,
    pub evaluation_result: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ApiStatus {
    pub schema_version: String,
    pub status: String,
    pub tenant_id: Option<String>,
}
