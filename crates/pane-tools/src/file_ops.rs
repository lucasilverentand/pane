use crate::ToolError;
use serde_json::Value;

pub async fn read_file(input: &Value) -> Result<String, ToolError> {
    let path = input["path"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidInput("missing 'path'".into()))?;
    tokio::fs::read_to_string(path)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
}

pub async fn write_file(input: &Value) -> Result<String, ToolError> {
    let path = input["path"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidInput("missing 'path'".into()))?;
    let content = input["content"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidInput("missing 'content'".into()))?;
    tokio::fs::write(path, content)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
    Ok(format!("wrote {} bytes to {path}", content.len()))
}

pub async fn list_directory(input: &Value) -> Result<String, ToolError> {
    let path = input["path"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidInput("missing 'path'".into()))?;
    let mut entries = tokio::fs::read_dir(path)
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
    let mut names = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
    {
        let file_type = entry
            .file_type()
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let suffix = if file_type.is_dir() { "/" } else { "" };
        names.push(format!("{}{suffix}", entry.file_name().to_string_lossy()));
    }
    names.sort();
    Ok(names.join("\n"))
}
