use crate::ToolError;
use serde_json::Value;
use std::path::Path;

pub async fn search_files(input: &Value) -> Result<String, ToolError> {
    let path = input["path"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidInput("missing 'path'".into()))?;
    let pattern = input["pattern"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidInput("missing 'pattern'".into()))?;

    let path = path.to_string();
    let pattern = pattern.to_string();

    tokio::task::spawn_blocking(move || search_recursive(&path, &pattern))
        .await
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
}

fn search_recursive(root: &str, pattern: &str) -> Result<String, ToolError> {
    use ignore::WalkBuilder;

    let mut matches = Vec::new();
    let walker = WalkBuilder::new(root).hidden(true).git_ignore(true).build();

    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        if let Ok(content) = std::fs::read_to_string(path) {
            for (line_num, line) in content.lines().enumerate() {
                if line.contains(&pattern) {
                    matches.push(format!(
                        "{}:{}: {}",
                        relative_path(Path::new(root), path),
                        line_num + 1,
                        line.trim()
                    ));
                }
            }
        }
        if matches.len() >= 100 {
            break;
        }
    }

    Ok(matches.join("\n"))
}

fn relative_path(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}
