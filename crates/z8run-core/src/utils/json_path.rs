//! Shared JSON path utilities for dot-notation field access.
//!
//! Replaces duplicated `json_path_lookup` and `json_path_set` across
//! switch, filter, function, mapper, sanitize, and other nodes.

use serde_json::Value;

/// Look up a value in a JSON structure using dot-notation path.
///
/// Returns `Value::Null` if any segment is missing.
///
/// # Examples
/// ```ignore
/// let data = json!({"req": {"body": {"name": "Pool"}}});
/// assert_eq!(json_path_lookup(&data, "req.body.name"), json!("Pool"));
/// assert_eq!(json_path_lookup(&data, "req.body.missing"), Value::Null);
/// ```
pub fn json_path_lookup(data: &Value, path: &str) -> Value {
    let mut current = data;
    for segment in path.split('.') {
        match current {
            Value::Object(map) => {
                current = match map.get(segment) {
                    Some(v) => v,
                    None => return Value::Null,
                };
            }
            Value::Array(arr) => {
                if let Ok(idx) = segment.parse::<usize>() {
                    current = match arr.get(idx) {
                        Some(v) => v,
                        None => return Value::Null,
                    };
                } else {
                    return Value::Null;
                }
            }
            _ => return Value::Null,
        }
    }
    current.clone()
}

/// Like `json_path_lookup`, but returns `None` instead of `Value::Null`
/// when a path segment is missing.
pub fn json_path_get(data: &Value, path: &str) -> Option<Value> {
    let mut current = data;
    for segment in path.split('.') {
        match current {
            Value::Object(map) => {
                current = map.get(segment)?;
            }
            Value::Array(arr) => {
                if let Ok(idx) = segment.parse::<usize>() {
                    current = arr.get(idx)?;
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(current.clone())
}

/// Set a value in a JSON object using dot-notation path.
///
/// Creates intermediate objects as needed. If `value` is `Value::Null`,
/// the key is set to null (not removed).
///
/// # Examples
/// ```ignore
/// let mut data = json!({});
/// json_path_set(&mut data, "user.name", json!("Pool"));
/// assert_eq!(data, json!({"user": {"name": "Pool"}}));
/// ```
pub fn json_path_set(data: &mut Value, path: &str, value: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return;
    }

    let mut current = data;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            if let Value::Object(map) = current {
                map.insert(part.to_string(), value);
            }
            return;
        }

        // Ensure intermediate object exists
        if let Value::Object(map) = current {
            if !map.contains_key(*part) || !map[*part].is_object() {
                map.insert(part.to_string(), Value::Object(serde_json::Map::new()));
            }
            current = map.get_mut(*part).unwrap();
        } else {
            return;
        }
    }
}

/// Remove a key from a JSON object at the given dot-notation path.
pub fn json_path_remove(data: &mut Value, path: &str) {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return;
    }

    let mut current = data;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            if let Value::Object(map) = current {
                map.remove(*part);
            }
            return;
        }

        let next = if let Value::Object(map) = current {
            map.get_mut(*part)
        } else {
            None
        };

        match next {
            Some(v) => current = v,
            None => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_lookup_nested() {
        let data = json!({"req": {"body": {"name": "Pool"}}});
        assert_eq!(json_path_lookup(&data, "req.body.name"), json!("Pool"));
    }

    #[test]
    fn test_lookup_missing() {
        let data = json!({"req": {}});
        assert_eq!(json_path_lookup(&data, "req.body.name"), Value::Null);
    }

    #[test]
    fn test_lookup_array() {
        let data = json!({"items": [10, 20, 30]});
        assert_eq!(json_path_lookup(&data, "items.1"), json!(20));
    }

    #[test]
    fn test_get_returns_option() {
        let data = json!({"a": 1});
        assert_eq!(json_path_get(&data, "a"), Some(json!(1)));
        assert_eq!(json_path_get(&data, "b"), None);
    }

    #[test]
    fn test_set_creates_nested() {
        let mut data = json!({});
        json_path_set(&mut data, "user.name", json!("Pool"));
        assert_eq!(data, json!({"user": {"name": "Pool"}}));
    }

    #[test]
    fn test_remove() {
        let mut data = json!({"a": {"b": 1, "c": 2}});
        json_path_remove(&mut data, "a.b");
        assert_eq!(data, json!({"a": {"c": 2}}));
    }
}
