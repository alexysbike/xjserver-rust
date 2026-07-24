use std::path::{Path, PathBuf};

use crate::error::XJError;

/// Write generated protobuf text to `proto_path`, creating parent dirs as needed.
pub fn write_generated_proto(proto_path: &Path, content: &str) -> Result<PathBuf, XJError> {
    if let Some(parent) = proto_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|err| {
                XJError::internal(format!(
                    "Failed to create proto directory {}: {err}",
                    parent.display()
                ))
            })?;
        }
    }

    std::fs::write(proto_path, content).map_err(|err| {
        XJError::internal(format!(
            "Failed to write generated proto to {}: {err}",
            proto_path.display()
        ))
    })?;

    Ok(proto_path.to_path_buf())
}
