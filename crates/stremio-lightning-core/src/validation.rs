use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("Invalid filename: path separators or traversal not allowed")]
    InvalidFilename,
}

impl From<ValidationError> for String {
    fn from(error: ValidationError) -> Self {
        error.to_string()
    }
}

pub fn validate_filename(filename: &str) -> Result<(), ValidationError> {
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
        || filename.contains('\0')
    {
        return Err(ValidationError::InvalidFilename);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_and_separators() {
        for filename in [
            "",
            "../mod.js",
            "folder/mod.js",
            "folder\\mod.js",
            "bad\0name",
        ] {
            assert!(validate_filename(filename).is_err());
        }
    }

    #[test]
    fn accepts_plain_filenames() {
        assert!(validate_filename("mod.js").is_ok());
    }
}
