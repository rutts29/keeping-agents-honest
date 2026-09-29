use super::*;

pub(super) fn required_array<'a>(
    value: &'a Value,
    field: &str,
    location: &str,
) -> Result<&'a [Value], SourceError> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            SourceError::new(
                "CODEX_MUST_BE_AN_ARRAY",
                "source_fields",
                format!("{location}.{field} must be an array"),
            )
        })
}

pub(super) fn required_object<'a>(
    value: &'a Value,
    field: &str,
    location: &str,
) -> Result<&'a serde_json::Map<String, Value>, SourceError> {
    value.get(field).and_then(Value::as_object).ok_or_else(|| {
        SourceError::new(
            "CODEX_MUST_BE_AN_OBJECT",
            "source_fields",
            format!("{location}.{field} must be an object"),
        )
    })
}

pub(super) fn required_field<'a>(
    value: &'a Value,
    field: &str,
    location: &str,
) -> Result<&'a Value, SourceError> {
    value.get(field).ok_or_else(|| {
        SourceError::new(
            "CODEX_IS_MISSING",
            "source_fields",
            format!("{location}.{field} is missing"),
        )
    })
}

pub(super) fn required_string_array(
    value: &Value,
    field: &str,
    location: &str,
) -> Result<(), SourceError> {
    let values = required_array(value, field, location)?;
    if values.iter().all(Value::is_string) {
        Ok(())
    } else {
        Err(SourceError::new(
            "CODEX_MUST_CONTAIN_ONLY_STRINGS",
            "source_fields",
            format!("{location}.{field} must contain only strings"),
        ))
    }
}

pub(super) fn required_u64(value: &Value, field: &str, location: &str) -> Result<u64, SourceError> {
    value.get(field).and_then(Value::as_u64).ok_or_else(|| {
        SourceError::new(
            "CODEX_MUST_BE_AN_UNSIGNED_INTEGER",
            "source_fields",
            format!("{location}.{field} must be an unsigned integer"),
        )
    })
}

pub(super) fn required_string(
    value: &Value,
    field: &str,
    location: &str,
) -> Result<String, SourceError> {
    let result = value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= 16 * 1024 && !value.chars().any(char::is_control)
        })
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            SourceError::new(
                "CODEX_MUST_BE_A_NON_EMPTY_STRING",
                "source_fields",
                format!("{location}.{field} must be a non-empty string"),
            )
        })?;
    Ok(result)
}

pub(super) fn required_text(
    value: &Value,
    field: &str,
    location: &str,
) -> Result<String, SourceError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 1024 * 1024 && !value.contains('\0'))
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            SourceError::new(
                "CODEX_MUST_BE_A_NON_EMPTY_TEXT_STRING",
                "source_fields",
                format!("{location}.{field} must be a non-empty text string"),
            )
        })
}

pub(super) fn required_provider_text(
    value: &Value,
    field: &str,
    location: &str,
) -> Result<String, SourceError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| value.len() <= 1024 * 1024 && !value.contains('\0'))
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            SourceError::new(
                "CODEX_MUST_BE_A_TEXT_STRING",
                "source_fields",
                format!("{location}.{field} must be a text string"),
            )
        })
}

pub(super) fn optional_i64(
    value: &Value,
    field: &str,
    location: &str,
) -> Result<Option<i64>, SourceError> {
    match value.get(field) {
        Some(Value::Null) => Ok(None),
        Some(value) => value.as_i64().map(Some).ok_or_else(|| {
            SourceError::new(
                "CODEX_MUST_BE_AN_INTEGER_OR_NULL",
                "source_fields",
                format!("{location}.{field} must be an integer or null"),
            )
        }),
        None => Ok(None),
    }
}
