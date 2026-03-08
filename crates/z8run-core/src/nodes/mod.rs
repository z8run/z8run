//! Built-in node executor implementations.
//!
//! These are the native nodes shipped with z8run.
//! Each implements `NodeExecutor` and has a corresponding factory.

pub mod aggregator;
pub mod ai_agent;
pub mod batch;
pub mod classifier;
pub mod conversation_memory;
pub mod crm;
pub mod cron_trigger;
pub mod csv_node;
pub mod database;
pub mod debug;
pub mod delay;
pub mod embeddings;
pub mod filter;
pub mod function;
pub mod http_in;
pub mod http_out;
pub mod http_request;
pub mod human_handoff;
pub mod if_else;
pub mod image_gen;
pub mod json_transform;
pub mod llm;
pub mod loop_node;
pub mod mapper;
pub mod mqtt;
pub mod prompt_template;
pub mod sanitize;
pub mod structured_output;
pub mod stt;
pub mod summarizer;
pub mod switch;
pub mod text_splitter;
pub mod timer;
pub mod tts;
pub mod twilio;
pub mod vector_store;
pub mod webhook;
pub mod webhook_trigger;
pub mod whatsapp;

use crate::engine::{FlowEngine, NodeExecutorFactory};
use std::sync::Arc;

/// Registers all built-in node types with the engine.
pub async fn register_builtin_nodes(engine: &FlowEngine) {
    engine
        .register_node_type(Arc::new(debug::DebugNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(function::FunctionNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(http_in::HttpInNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(http_out::HttpOutNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(delay::DelayNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(switch::SwitchNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(filter::FilterNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(
            Arc::new(http_request::HttpRequestNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(
            Arc::new(json_transform::JsonTransformNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(Arc::new(timer::TimerNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(webhook::WebhookNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(database::DatabaseNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(llm::LlmNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(mqtt::MqttNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(
            Arc::new(embeddings::EmbeddingsNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(
            Arc::new(classifier::ClassifierNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(
            Arc::new(prompt_template::PromptTemplateNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(
            Arc::new(text_splitter::TextSplitterNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(Arc::new(structured_output::StructuredOutputNodeFactory)
            as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(
            Arc::new(summarizer::SummarizerNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(
            Arc::new(vector_store::VectorStoreNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(Arc::new(ai_agent::AiAgentNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(image_gen::ImageGenNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(stt::SttNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(tts::TtsNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;

    // Communication nodes
    engine
        .register_node_type(Arc::new(twilio::TwilioNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(whatsapp::WhatsAppNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(conversation_memory::ConversationMemoryNodeFactory)
            as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(crm::CrmNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(
            Arc::new(human_handoff::HumanHandoffNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;

    // Trigger nodes
    engine
        .register_node_type(
            Arc::new(cron_trigger::CronTriggerNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(
            Arc::new(webhook_trigger::WebhookTriggerNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;

    // Control flow nodes
    engine
        .register_node_type(Arc::new(if_else::IfElseNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(Arc::new(loop_node::LoopNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;

    // Data Engineering nodes
    engine
        .register_node_type(Arc::new(csv_node::CsvNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;
    engine
        .register_node_type(
            Arc::new(aggregator::AggregatorNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
    engine
        .register_node_type(Arc::new(batch::BatchNodeFactory) as Arc<dyn NodeExecutorFactory>)
        .await;

    // Security nodes
    engine
        .register_node_type(
            Arc::new(sanitize::SanitizeNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;

    // Data shaping nodes
    engine
        .register_node_type(
            Arc::new(mapper::MapperNodeFactory) as Arc<dyn NodeExecutorFactory>
        )
        .await;
}
