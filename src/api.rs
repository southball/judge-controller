use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiSuccess<T> {
    pub success: bool,
    pub data: T,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PartialSubmission {
    pub id: i32,
    pub problem_slug: String,
    pub language: String,
    pub source_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProblemMetadata {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub last_update: String,
    pub testcases: Vec<serde_yaml::Value>,
}
