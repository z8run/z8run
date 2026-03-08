//! CSV node: parse CSV text into JSON arrays, or stringify JSON arrays into CSV.
//!
//! Actions:
//! - `parse`: CSV text → array of JSON objects (using headers as keys).
//! - `stringify`: Array of JSON objects → CSV text.
//!
//! Config example:
//! ```json
//! { "action": "parse", "delimiter": ",", "hasHeaders": true }
//! { "action": "stringify", "delimiter": ";", "columns": ["name", "email"] }
//! ```

use crate::engine::{NodeExecutor, NodeExecutorFactory};
use crate::error::Z8Result;
use crate::message::FlowMessage;
use serde_json::Value;
use tracing::debug;

pub struct CsvNode {
    name: String,
    action: String,       // "parse" | "stringify"
    delimiter: u8,        // byte delimiter (default b',')
    has_headers: bool,    // whether first row is headers
    columns: Vec<String>, // optional column filter (stringify: column order)
}

#[async_trait::async_trait]
impl NodeExecutor for CsvNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        debug!(node = %self.name, action = %self.action, "CSV processing");

        let result = match self.action.as_str() {
            "parse" => self.parse_csv(&msg)?,
            "stringify" => self.stringify_csv(&msg)?,
            other => {
                let err = serde_json::json!({
                    "error": format!("Unknown CSV action: {}", other),
                    "supported": ["parse", "stringify"]
                });
                let out = msg.derive(msg.source_node, "error", err);
                return Ok(vec![out]);
            }
        };

        debug!(node = %self.name, "CSV processing complete");
        let out = msg.derive(msg.source_node, "output", result);
        Ok(vec![out])
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(action) = config.get("action").and_then(|v| v.as_str()) {
            self.action = action.to_string();
        }
        if let Some(delim) = config.get("delimiter").and_then(|v| v.as_str()) {
            self.delimiter = match delim {
                ";" => b';',
                "\\t" | "tab" => b'\t',
                "|" => b'|',
                _ => b',',
            };
        }
        if let Some(h) = config.get("hasHeaders").and_then(|v| v.as_bool()) {
            self.has_headers = h;
        }
        if let Some(cols) = config.get("columns").and_then(|v| v.as_array()) {
            self.columns = cols
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        if !["parse", "stringify"].contains(&self.action.as_str()) {
            return Err(crate::error::Z8Error::Internal(format!(
                "Invalid CSV action: '{}'. Expected: parse, stringify",
                self.action
            )));
        }
        Ok(())
    }

    fn node_type(&self) -> &str {
        "csv"
    }
}

impl CsvNode {
    fn parse_csv(&self, msg: &FlowMessage) -> Z8Result<Value> {
        let text = match msg.payload.as_str() {
            Some(s) => s.to_string(),
            None => {
                // Try extracting from common fields
                msg.payload
                    .get("text")
                    .or_else(|| msg.payload.get("data"))
                    .or_else(|| msg.payload.get("body"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .ok_or_else(|| {
                        crate::error::Z8Error::Internal(
                            "CSV parse expects a string payload or {text}/{data}/{body} field"
                                .to_string(),
                        )
                    })?
            }
        };

        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(self.delimiter)
            .has_headers(self.has_headers)
            .from_reader(text.as_bytes());

        let mut rows: Vec<Value> = Vec::new();

        if self.has_headers {
            let headers: Vec<String> = rdr
                .headers()
                .map_err(|e| crate::error::Z8Error::Internal(format!("CSV header error: {}", e)))?
                .iter()
                .map(String::from)
                .collect();

            for record in rdr.records() {
                let record = record.map_err(|e| {
                    crate::error::Z8Error::Internal(format!("CSV record error: {}", e))
                })?;
                let mut obj = serde_json::Map::new();
                for (i, field) in record.iter().enumerate() {
                    if let Some(header) = headers.get(i) {
                        // Skip columns not in filter (if filter is set)
                        if !self.columns.is_empty() && !self.columns.contains(header) {
                            continue;
                        }
                        // Try to parse as number or boolean
                        let value = parse_field_value(field);
                        obj.insert(header.clone(), value);
                    }
                }
                rows.push(Value::Object(obj));
            }
        } else {
            for record in rdr.records() {
                let record = record.map_err(|e| {
                    crate::error::Z8Error::Internal(format!("CSV record error: {}", e))
                })?;
                let arr: Vec<Value> = record.iter().map(parse_field_value).collect();
                rows.push(Value::Array(arr));
            }
        }

        Ok(serde_json::json!({
            "rows": rows,
            "count": rows.len()
        }))
    }

    fn stringify_csv(&self, msg: &FlowMessage) -> Z8Result<Value> {
        let items = match msg.payload.as_array() {
            Some(arr) => arr.clone(),
            None => {
                // Try common payload fields
                msg.payload
                    .get("rows")
                    .or_else(|| msg.payload.get("data"))
                    .or_else(|| msg.payload.get("results"))
                    .and_then(|v| v.as_array())
                    .cloned()
                    .ok_or_else(|| {
                        crate::error::Z8Error::Internal(
                            "CSV stringify expects an array payload or {rows}/{data}/{results} field"
                                .to_string(),
                        )
                    })?
            }
        };

        if items.is_empty() {
            return Ok(Value::String(String::new()));
        }

        let mut wtr = csv::WriterBuilder::new()
            .delimiter(self.delimiter)
            .from_writer(Vec::new());

        // Determine columns: from config, or from first object's keys
        let columns = if !self.columns.is_empty() {
            self.columns.clone()
        } else if let Some(obj) = items.first().and_then(|v| v.as_object()) {
            obj.keys().cloned().collect()
        } else {
            // Array of arrays — no headers
            for item in &items {
                if let Some(arr) = item.as_array() {
                    let record: Vec<String> = arr.iter().map(value_to_csv_field).collect();
                    wtr.write_record(&record).map_err(|e| {
                        crate::error::Z8Error::Internal(format!("CSV write error: {}", e))
                    })?;
                }
            }
            let csv_bytes = wtr
                .into_inner()
                .map_err(|e| crate::error::Z8Error::Internal(format!("CSV flush error: {}", e)))?;
            let csv_text = String::from_utf8_lossy(&csv_bytes).to_string();
            return Ok(Value::String(csv_text));
        };

        // Write headers
        wtr.write_record(&columns)
            .map_err(|e| crate::error::Z8Error::Internal(format!("CSV write error: {}", e)))?;

        // Write rows
        for item in &items {
            if let Some(obj) = item.as_object() {
                let record: Vec<String> = columns
                    .iter()
                    .map(|col| obj.get(col).map(value_to_csv_field).unwrap_or_default())
                    .collect();
                wtr.write_record(&record).map_err(|e| {
                    crate::error::Z8Error::Internal(format!("CSV write error: {}", e))
                })?;
            }
        }

        let csv_bytes = wtr
            .into_inner()
            .map_err(|e| crate::error::Z8Error::Internal(format!("CSV flush error: {}", e)))?;
        let csv_text = String::from_utf8_lossy(&csv_bytes).to_string();
        Ok(Value::String(csv_text))
    }
}

/// Try to parse a CSV field as number/bool, fallback to string.
fn parse_field_value(field: &str) -> Value {
    let trimmed = field.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }
    if let Ok(n) = trimmed.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = trimmed.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return Value::Number(num);
        }
    }
    match trimmed.to_lowercase().as_str() {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(field.to_string()),
    }
}

/// Convert a JSON value to a CSV-safe string.
fn value_to_csv_field(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

pub struct CsvNodeFactory;

#[async_trait::async_trait]
impl NodeExecutorFactory for CsvNodeFactory {
    async fn create(&self, config: Value) -> Z8Result<Box<dyn NodeExecutor>> {
        let mut node = CsvNode {
            name: "CSV".to_string(),
            action: "parse".to_string(),
            delimiter: b',',
            has_headers: true,
            columns: Vec::new(),
        };
        node.configure(config).await?;
        Ok(Box::new(node))
    }

    fn node_type(&self) -> &str {
        "csv"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_csv_with_headers() {
        let node = CsvNode {
            name: "test".into(),
            action: "parse".into(),
            delimiter: b',',
            has_headers: true,
            columns: Vec::new(),
        };
        let csv_text = "name,age,active\nAlice,30,true\nBob,25,false";
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            Value::String(csv_text.into()),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "output");

        let rows = results[0].payload["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[0]["age"], 30);
        assert_eq!(rows[0]["active"], true);
        assert_eq!(rows[1]["name"], "Bob");
    }

    #[tokio::test]
    async fn test_parse_csv_column_filter() {
        let node = CsvNode {
            name: "test".into(),
            action: "parse".into(),
            delimiter: b',',
            has_headers: true,
            columns: vec!["name".into()],
        };
        let csv_text = "name,age\nAlice,30";
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            Value::String(csv_text.into()),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        let rows = results[0].payload["rows"].as_array().unwrap();
        assert_eq!(rows[0]["name"], "Alice");
        assert!(rows[0].get("age").is_none());
    }

    #[tokio::test]
    async fn test_stringify_objects_to_csv() {
        let node = CsvNode {
            name: "test".into(),
            action: "stringify".into(),
            delimiter: b',',
            has_headers: true,
            columns: Vec::new(),
        };
        let data = serde_json::json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]);
        let msg = FlowMessage::new(uuid::Uuid::now_v7(), "input", data, uuid::Uuid::now_v7());
        let results = node.process(msg).await.unwrap();
        assert_eq!(results[0].source_port, "output");
        let csv_text = results[0].payload.as_str().unwrap();
        assert!(csv_text.contains("Alice"));
        assert!(csv_text.contains("Bob"));
    }

    #[tokio::test]
    async fn test_parse_semicolon_delimiter() {
        let node = CsvNode {
            name: "test".into(),
            action: "parse".into(),
            delimiter: b';',
            has_headers: true,
            columns: Vec::new(),
        };
        let csv_text = "x;y\n1;2\n3;4";
        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "input",
            Value::String(csv_text.into()),
            uuid::Uuid::now_v7(),
        );
        let results = node.process(msg).await.unwrap();
        let rows = results[0].payload["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["x"], 1);
    }
}
