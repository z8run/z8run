//! CRM node: integrates with HubSpot or Salesforce APIs for lead/contact/deal management.
//!
//! Supports creating, updating, searching, and listing contacts and deals across
//! HubSpot and Salesforce platforms.
//!
//! Config example (HubSpot):
//! ```json
//! {
//!   "name": "CRM",
//!   "provider": "hubspot",
//!   "apiKey": "pat-na1-...",
//!   "action": "create_contact",
//!   "timeoutMs": 5000
//! }
//! ```
//!
//! Config example (Salesforce):
//! ```json
//! {
//!   "name": "CRM",
//!   "provider": "salesforce",
//!   "apiKey": "...",
//!   "baseUrl": "https://mycompany.my.salesforce.com",
//!   "action": "create_contact",
//!   "timeoutMs": 5000
//! }
//! ```
//!
//! Supported actions:
//!   - create_contact, update_contact, get_contact, search_contacts
//!   - create_deal, list_deals
//!
//! Output ports: "result", "error"

use super::switch::json_path_lookup;
use crate::engine::NodeExecutor;
use crate::error::Z8Result;
use crate::message::FlowMessage;
use crate::node_factory;
use crate::utils::node_helpers::require_non_empty;
use serde_json::{json, Value};
use tracing::{info, warn};

pub struct CrmNode {
    name: String,
    provider: String, // "hubspot" or "salesforce"
    api_key: String,
    base_url: String,
    action: String, // create_contact, update_contact, get_contact, search_contacts, create_deal, list_deals
    timeout_ms: u64,
}

impl CrmNode {
    /// Extract a value from payload using multiple possible field names
    fn extract_field(payload: &Value, keys: &[&str]) -> Option<String> {
        for key in keys {
            if let Some(val) = json_path_lookup(payload, key).as_str() {
                return Some(val.to_string());
            }
        }
        None
    }

    /// Extract a numeric value from payload
    fn extract_number(payload: &Value, keys: &[&str]) -> Option<f64> {
        for key in keys {
            let val = json_path_lookup(payload, key);
            if let Some(n) = val.as_f64() {
                return Some(n);
            }
        }
        None
    }

    /// Build properties object from payload for HubSpot contact/deal creation
    fn build_hubspot_contact_properties(payload: &Value) -> Value {
        let mut props = serde_json::Map::new();

        if let Some(email) = Self::extract_field(payload, &["email"]) {
            props.insert("email".to_string(), Value::String(email));
        }
        if let Some(firstname) =
            Self::extract_field(payload, &["firstName", "firstname", "first_name"])
        {
            props.insert("firstname".to_string(), Value::String(firstname));
        }
        if let Some(lastname) = Self::extract_field(payload, &["lastName", "lastname", "last_name"])
        {
            props.insert("lastname".to_string(), Value::String(lastname));
        }
        if let Some(phone) = Self::extract_field(payload, &["phone", "phoneNumber"]) {
            props.insert("phone".to_string(), Value::String(phone));
        }
        if let Some(company) = Self::extract_field(payload, &["company", "accountName", "account"])
        {
            props.insert("company".to_string(), Value::String(company));
        }

        Value::Object(props)
    }

    /// Build properties object from payload for HubSpot deal creation
    fn build_hubspot_deal_properties(payload: &Value) -> Value {
        let mut props = serde_json::Map::new();

        if let Some(dealname) = Self::extract_field(payload, &["dealName", "deal_name", "name"]) {
            props.insert("dealname".to_string(), Value::String(dealname));
        }
        if let Some(amount) = Self::extract_number(payload, &["amount", "value"]) {
            props.insert(
                "amount".to_string(),
                Value::Number(
                    serde_json::Number::from_f64(amount).unwrap_or(serde_json::Number::from(0)),
                ),
            );
        }
        if let Some(pipeline) = Self::extract_field(payload, &["pipeline"]) {
            props.insert("pipeline".to_string(), Value::String(pipeline));
        } else {
            props.insert("pipeline".to_string(), Value::String("default".to_string()));
        }
        if let Some(dealstage) = Self::extract_field(payload, &["dealStage", "stage"]) {
            props.insert("dealstage".to_string(), Value::String(dealstage));
        } else {
            props.insert(
                "dealstage".to_string(),
                Value::String("appointmentscheduled".to_string()),
            );
        }

        Value::Object(props)
    }

    /// Build request body for Salesforce contact/deal creation
    fn build_salesforce_contact_body(payload: &Value) -> Value {
        let mut body = serde_json::Map::new();

        if let Some(firstname) =
            Self::extract_field(payload, &["firstName", "firstname", "first_name"])
        {
            body.insert("FirstName".to_string(), Value::String(firstname));
        }
        if let Some(lastname) = Self::extract_field(payload, &["lastName", "lastname", "last_name"])
        {
            body.insert("LastName".to_string(), Value::String(lastname));
        }
        if let Some(email) = Self::extract_field(payload, &["email"]) {
            body.insert("Email".to_string(), Value::String(email));
        }
        if let Some(phone) = Self::extract_field(payload, &["phone", "phoneNumber"]) {
            body.insert("Phone".to_string(), Value::String(phone));
        }
        if let Some(account_id) = Self::extract_field(payload, &["accountId", "AccountId"]) {
            body.insert("AccountId".to_string(), Value::String(account_id));
        }

        Value::Object(body)
    }

    /// Build request body for Salesforce opportunity (deal) creation
    fn build_salesforce_deal_body(payload: &Value) -> Value {
        let mut body = serde_json::Map::new();

        if let Some(name) = Self::extract_field(payload, &["dealName", "deal_name", "name"]) {
            body.insert("Name".to_string(), Value::String(name));
        }
        if let Some(amount) = Self::extract_number(payload, &["amount", "value"]) {
            body.insert(
                "Amount".to_string(),
                Value::Number(
                    serde_json::Number::from_f64(amount).unwrap_or(serde_json::Number::from(0)),
                ),
            );
        }
        if let Some(stage) = Self::extract_field(payload, &["stage", "stageName"]) {
            body.insert("StageName".to_string(), Value::String(stage));
        }
        if let Some(close_date) = Self::extract_field(payload, &["closeDate", "close_date"]) {
            body.insert("CloseDate".to_string(), Value::String(close_date));
        }

        Value::Object(body)
    }

    async fn execute_hubspot(&self, msg: &FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let client = reqwest::Client::new();
        let base = "https://api.hubapi.com";

        match self.action.as_str() {
            "create_contact" => {
                let properties = Self::build_hubspot_contact_properties(&msg.payload);
                let body = json!({ "properties": properties });

                info!(node = %self.name, action = "create_contact", "Executing HubSpot action");

                let resp = client
                    .post(format!("{}/crm/v3/objects/contacts", base))
                    .bearer_auth(&self.api_key)
                    .json(&body)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "HubSpot API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "HubSpot create_contact failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "create_contact"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "update_contact" => {
                let contact_id =
                    Self::extract_field(&msg.payload, &["contactId", "contact_id", "id"])
                        .ok_or_else(|| {
                            crate::error::Z8Error::Internal("Missing contactId".to_string())
                        })?;

                let properties = Self::build_hubspot_contact_properties(&msg.payload);
                let body = json!({ "properties": properties });

                info!(node = %self.name, action = "update_contact", contact_id = %contact_id, "Executing HubSpot action");

                let resp = client
                    .patch(format!("{}/crm/v3/objects/contacts/{}", base, contact_id))
                    .bearer_auth(&self.api_key)
                    .json(&body)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "HubSpot API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "HubSpot update_contact failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "update_contact"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "get_contact" => {
                let contact_id =
                    Self::extract_field(&msg.payload, &["contactId", "contact_id", "id"])
                        .ok_or_else(|| {
                            crate::error::Z8Error::Internal("Missing contactId".to_string())
                        })?;

                info!(node = %self.name, action = "get_contact", contact_id = %contact_id, "Executing HubSpot action");

                let resp = client
                    .get(format!(
                        "{}/crm/v3/objects/contacts/{}?properties=email,firstname,lastname,phone,company",
                        base, contact_id
                    ))
                    .bearer_auth(&self.api_key)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "HubSpot API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "HubSpot get_contact failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "get_contact"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "search_contacts" => {
                let email = Self::extract_field(&msg.payload, &["email"]).ok_or_else(|| {
                    crate::error::Z8Error::Internal("Missing email for search".to_string())
                })?;

                let body = json!({
                    "filterGroups": [
                        {
                            "filters": [
                                {
                                    "propertyName": "email",
                                    "operator": "EQ",
                                    "value": email
                                }
                            ]
                        }
                    ]
                });

                info!(node = %self.name, action = "search_contacts", "Executing HubSpot action");

                let resp = client
                    .post(format!("{}/crm/v3/objects/contacts/search", base))
                    .bearer_auth(&self.api_key)
                    .json(&body)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "HubSpot API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "HubSpot search_contacts failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "search_contacts"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "create_deal" => {
                let properties = Self::build_hubspot_deal_properties(&msg.payload);
                let body = json!({ "properties": properties });

                info!(node = %self.name, action = "create_deal", "Executing HubSpot action");

                let resp = client
                    .post(format!("{}/crm/v3/objects/deals", base))
                    .bearer_auth(&self.api_key)
                    .json(&body)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "HubSpot API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "HubSpot create_deal failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "create_deal"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "list_deals" => {
                info!(node = %self.name, action = "list_deals", "Executing HubSpot action");

                let resp = client
                    .get(format!("{}/crm/v3/objects/deals?limit=100", base))
                    .bearer_auth(&self.api_key)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "HubSpot API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "HubSpot list_deals failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "list_deals"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            _ => {
                let err = json!({
                    "error": format!("Unsupported HubSpot action: {}", self.action)
                });
                let out = msg.derive(msg.source_node, "error", err);
                Ok(vec![out])
            }
        }
    }

    async fn execute_salesforce(&self, msg: &FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        let client = reqwest::Client::new();
        let base = &self.base_url;

        match self.action.as_str() {
            "create_contact" => {
                let body = Self::build_salesforce_contact_body(&msg.payload);

                info!(node = %self.name, action = "create_contact", "Executing Salesforce action");

                let resp = client
                    .post(format!("{}/services/data/v59.0/sobjects/Contact", base))
                    .bearer_auth(&self.api_key)
                    .json(&body)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "Salesforce API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "Salesforce create_contact failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "create_contact"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "update_contact" => {
                let contact_id =
                    Self::extract_field(&msg.payload, &["contactId", "contact_id", "id"])
                        .ok_or_else(|| {
                            crate::error::Z8Error::Internal("Missing contactId".to_string())
                        })?;

                let body = Self::build_salesforce_contact_body(&msg.payload);

                info!(node = %self.name, action = "update_contact", contact_id = %contact_id, "Executing Salesforce action");

                let resp = client
                    .patch(format!(
                        "{}/services/data/v59.0/sobjects/Contact/{}",
                        base, contact_id
                    ))
                    .bearer_auth(&self.api_key)
                    .json(&body)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "Salesforce API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "Salesforce update_contact failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "update_contact"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "get_contact" => {
                let contact_id =
                    Self::extract_field(&msg.payload, &["contactId", "contact_id", "id"])
                        .ok_or_else(|| {
                            crate::error::Z8Error::Internal("Missing contactId".to_string())
                        })?;

                info!(node = %self.name, action = "get_contact", contact_id = %contact_id, "Executing Salesforce action");

                let resp = client
                    .get(format!(
                        "{}/services/data/v59.0/sobjects/Contact/{}",
                        base, contact_id
                    ))
                    .bearer_auth(&self.api_key)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "Salesforce API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "Salesforce get_contact failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "get_contact"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "search_contacts" => {
                let email = Self::extract_field(&msg.payload, &["email"]).ok_or_else(|| {
                    crate::error::Z8Error::Internal("Missing email for search".to_string())
                })?;

                info!(node = %self.name, action = "search_contacts", "Executing Salesforce action");

                let query = format!(
                    "SELECT Id,FirstName,LastName,Email FROM Contact WHERE Email='{}'",
                    email.replace("'", "\\'")
                );

                let resp = client
                    .get(format!(
                        "{}/services/data/v59.0/query/?q={}",
                        base,
                        urlencoding::encode(&query)
                    ))
                    .bearer_auth(&self.api_key)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "Salesforce API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "Salesforce search_contacts failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "search_contacts"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "create_deal" => {
                let body = Self::build_salesforce_deal_body(&msg.payload);

                info!(node = %self.name, action = "create_deal", "Executing Salesforce action");

                let resp = client
                    .post(format!("{}/services/data/v59.0/sobjects/Opportunity", base))
                    .bearer_auth(&self.api_key)
                    .json(&body)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "Salesforce API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "Salesforce create_deal failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "create_deal"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            "list_deals" => {
                info!(node = %self.name, action = "list_deals", "Executing Salesforce action");

                let query = "SELECT Id,Name,Amount,StageName FROM Opportunity LIMIT 100";

                let resp = client
                    .get(format!(
                        "{}/services/data/v59.0/query/?q={}",
                        base,
                        urlencoding::encode(query)
                    ))
                    .bearer_auth(&self.api_key)
                    .timeout(std::time::Duration::from_millis(self.timeout_ms))
                    .send()
                    .await;

                match resp {
                    Ok(response) => {
                        let status = response.status().as_u16();
                        let body_text = response.text().await.unwrap_or_default();
                        let body_json: Value =
                            serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));

                        if status < 400 {
                            let out = msg.derive(msg.source_node, "result", body_json);
                            Ok(vec![out])
                        } else {
                            let err = json!({
                                "error": "Salesforce API error",
                                "status": status,
                                "details": body_json
                            });
                            let out = msg.derive(msg.source_node, "error", err);
                            Ok(vec![out])
                        }
                    }
                    Err(e) => {
                        warn!(node = %self.name, error = %e, "Salesforce list_deals failed");
                        let err = json!({
                            "error": e.to_string(),
                            "action": "list_deals"
                        });
                        let out = msg.derive(msg.source_node, "error", err);
                        Ok(vec![out])
                    }
                }
            }

            _ => {
                let err = json!({
                    "error": format!("Unsupported Salesforce action: {}", self.action)
                });
                let out = msg.derive(msg.source_node, "error", err);
                Ok(vec![out])
            }
        }
    }
}

#[async_trait::async_trait]
impl NodeExecutor for CrmNode {
    async fn process(&self, msg: FlowMessage) -> Z8Result<Vec<FlowMessage>> {
        match self.provider.as_str() {
            "hubspot" => self.execute_hubspot(&msg).await,
            "salesforce" => self.execute_salesforce(&msg).await,
            _ => {
                let err = json!({
                    "error": format!("Unsupported CRM provider: {}", self.provider)
                });
                let out = msg.derive(msg.source_node, "error", err);
                Ok(vec![out])
            }
        }
    }

    async fn configure(&mut self, config: Value) -> Z8Result<()> {
        if let Some(name) = config.get("name").and_then(|v| v.as_str()) {
            self.name = name.to_string();
        }
        if let Some(provider) = config.get("provider").and_then(|v| v.as_str()) {
            self.provider = provider.to_string();
        }
        if let Some(api_key) = config.get("apiKey").and_then(|v| v.as_str()) {
            self.api_key = api_key.to_string();
        }
        if let Some(base_url) = config.get("baseUrl").and_then(|v| v.as_str()) {
            self.base_url = base_url.to_string();
        }
        if let Some(action) = config.get("action").and_then(|v| v.as_str()) {
            self.action = action.to_string();
        }
        if let Some(timeout) = config.get("timeoutMs").and_then(|v| v.as_u64()) {
            self.timeout_ms = timeout;
        }
        Ok(())
    }

    async fn validate(&self) -> Z8Result<()> {
        require_non_empty(&self.api_key, "CRM node requires 'apiKey'")?;
        require_non_empty(&self.action, "CRM node requires 'action'")?;
        match self.provider.as_str() {
            "hubspot" | "salesforce" => Ok(()),
            _ => Err(crate::error::Z8Error::Internal(format!(
                "Unsupported CRM provider: '{}'. Supported: hubspot, salesforce",
                self.provider
            ))),
        }
    }

    fn node_type(&self) -> &str {
        "crm"
    }
}

// ---------- Factory ----------

node_factory!(CrmNodeFactory, CrmNode, "crm", {
    name: "CRM".to_string(),
    provider: "hubspot".to_string(),
    api_key: String::new(),
    base_url: String::new(),
    action: String::new(),
    timeout_ms: 5000
});

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_crm_node_validate_missing_api_key() {
        let node = CrmNode {
            name: "test".to_string(),
            provider: "hubspot".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            action: "create_contact".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_crm_node_validate_missing_action() {
        let node = CrmNode {
            name: "test".to_string(),
            provider: "hubspot".to_string(),
            api_key: "test-key".to_string(),
            base_url: String::new(),
            action: String::new(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_crm_node_validate_invalid_provider() {
        let node = CrmNode {
            name: "test".to_string(),
            provider: "invalid".to_string(),
            api_key: "test-key".to_string(),
            base_url: String::new(),
            action: "create_contact".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_crm_node_validate_success() {
        let node = CrmNode {
            name: "test".to_string(),
            provider: "hubspot".to_string(),
            api_key: "test-key".to_string(),
            base_url: String::new(),
            action: "create_contact".to_string(),
            timeout_ms: 5000,
        };
        let result = node.validate().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_crm_node_configure() {
        let mut node = CrmNode {
            name: "test".to_string(),
            provider: "hubspot".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            action: String::new(),
            timeout_ms: 5000,
        };

        let config = json!({
            "name": "My CRM",
            "provider": "salesforce",
            "apiKey": "secret-key",
            "baseUrl": "https://example.salesforce.com",
            "action": "create_deal",
            "timeoutMs": 10000
        });

        node.configure(config).await.unwrap();

        assert_eq!(node.name, "My CRM");
        assert_eq!(node.provider, "salesforce");
        assert_eq!(node.api_key, "secret-key");
        assert_eq!(node.base_url, "https://example.salesforce.com");
        assert_eq!(node.action, "create_deal");
        assert_eq!(node.timeout_ms, 10000);
    }

    #[test]
    fn test_extract_field_single_name() {
        let payload = json!({
            "email": "test@example.com"
        });
        let result = CrmNode::extract_field(&payload, &["email"]);
        assert_eq!(result, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_extract_field_multiple_names() {
        let payload = json!({
            "firstName": "John"
        });
        let result = CrmNode::extract_field(&payload, &["firstname", "firstName", "first_name"]);
        assert_eq!(result, Some("John".to_string()));
    }

    #[test]
    fn test_extract_field_missing() {
        let payload = json!({});
        let result = CrmNode::extract_field(&payload, &["email", "firstName"]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_number() {
        let payload = json!({
            "amount": 1500.50
        });
        let result = CrmNode::extract_number(&payload, &["amount", "value"]);
        assert_eq!(result, Some(1500.50));
    }

    #[test]
    fn test_build_hubspot_contact_properties() {
        let payload = json!({
            "email": "john@example.com",
            "firstName": "John",
            "lastName": "Doe",
            "phone": "555-1234",
            "company": "ACME Inc"
        });

        let props = CrmNode::build_hubspot_contact_properties(&payload);

        assert_eq!(
            props.get("email").and_then(|v| v.as_str()),
            Some("john@example.com")
        );
        assert_eq!(
            props.get("firstname").and_then(|v| v.as_str()),
            Some("John")
        );
        assert_eq!(props.get("lastname").and_then(|v| v.as_str()), Some("Doe"));
        assert_eq!(
            props.get("phone").and_then(|v| v.as_str()),
            Some("555-1234")
        );
        assert_eq!(
            props.get("company").and_then(|v| v.as_str()),
            Some("ACME Inc")
        );
    }

    #[test]
    fn test_build_hubspot_deal_properties() {
        let payload = json!({
            "dealName": "Enterprise Plan",
            "amount": 50000.0,
            "pipeline": "sales",
            "dealStage": "negotiation"
        });

        let props = CrmNode::build_hubspot_deal_properties(&payload);

        assert_eq!(
            props.get("dealname").and_then(|v| v.as_str()),
            Some("Enterprise Plan")
        );
        assert_eq!(props.get("amount").and_then(|v| v.as_f64()), Some(50000.0));
        assert_eq!(
            props.get("pipeline").and_then(|v| v.as_str()),
            Some("sales")
        );
        assert_eq!(
            props.get("dealstage").and_then(|v| v.as_str()),
            Some("negotiation")
        );
    }

    #[test]
    fn test_build_hubspot_deal_properties_defaults() {
        let payload = json!({
            "dealName": "Small Deal"
        });

        let props = CrmNode::build_hubspot_deal_properties(&payload);

        assert_eq!(
            props.get("dealname").and_then(|v| v.as_str()),
            Some("Small Deal")
        );
        assert_eq!(
            props.get("pipeline").and_then(|v| v.as_str()),
            Some("default")
        );
        assert_eq!(
            props.get("dealstage").and_then(|v| v.as_str()),
            Some("appointmentscheduled")
        );
    }

    #[test]
    fn test_build_salesforce_contact_body() {
        let payload = json!({
            "firstName": "Jane",
            "lastName": "Smith",
            "email": "jane@example.com",
            "phone": "555-5678",
            "accountId": "acc-123"
        });

        let body = CrmNode::build_salesforce_contact_body(&payload);

        assert_eq!(body.get("FirstName").and_then(|v| v.as_str()), Some("Jane"));
        assert_eq!(body.get("LastName").and_then(|v| v.as_str()), Some("Smith"));
        assert_eq!(
            body.get("Email").and_then(|v| v.as_str()),
            Some("jane@example.com")
        );
        assert_eq!(body.get("Phone").and_then(|v| v.as_str()), Some("555-5678"));
        assert_eq!(
            body.get("AccountId").and_then(|v| v.as_str()),
            Some("acc-123")
        );
    }

    #[test]
    fn test_build_salesforce_deal_body() {
        let payload = json!({
            "dealName": "Big Contract",
            "amount": 100000.0,
            "stage": "closed_won",
            "closeDate": "2026-03-31"
        });

        let body = CrmNode::build_salesforce_deal_body(&payload);

        assert_eq!(
            body.get("Name").and_then(|v| v.as_str()),
            Some("Big Contract")
        );
        assert_eq!(body.get("Amount").and_then(|v| v.as_f64()), Some(100000.0));
        assert_eq!(
            body.get("StageName").and_then(|v| v.as_str()),
            Some("closed_won")
        );
        assert_eq!(
            body.get("CloseDate").and_then(|v| v.as_str()),
            Some("2026-03-31")
        );
    }

    #[tokio::test]
    async fn test_crm_node_unsupported_provider() {
        let node = CrmNode {
            name: "test".to_string(),
            provider: "unsupported".to_string(),
            api_key: "test-key".to_string(),
            base_url: String::new(),
            action: "create_contact".to_string(),
            timeout_ms: 5000,
        };

        let msg = FlowMessage::new(
            uuid::Uuid::now_v7(),
            "trigger",
            json!({}),
            uuid::Uuid::now_v7(),
        );

        let results = node.process(msg).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_port, "error");
        assert!(results[0].payload["error"]
            .as_str()
            .unwrap()
            .contains("Unsupported CRM provider"));
    }

    #[tokio::test]
    async fn test_crm_node_type() {
        let node = CrmNode {
            name: "test".to_string(),
            provider: "hubspot".to_string(),
            api_key: "test-key".to_string(),
            base_url: String::new(),
            action: "create_contact".to_string(),
            timeout_ms: 5000,
        };

        assert_eq!(node.node_type(), "crm");
    }
}
