use std::path::PathBuf;

use ape_core::{
    Error, create_macro, execute_macro_sampling_prompt,
    process_execute_macro_sampling_response,
};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ContextInclusion, CreateMessageRequestParams, ModelPreferences,
        SamplingMessage, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateMacroParams {
    #[schemars(description = "Absolute or relative path to the target file")]
    pub file_path: PathBuf,
    #[schemars(description = "Root path of the git repository (optional)")]
    pub repo_path: Option<PathBuf>,
    #[schemars(description = "Whether to inspect only staged changes (default: false)")]
    pub staged: Option<bool>,
    #[schemars(description = "Human-readable name for this macro (optional)")]
    pub name: Option<String>,
}

#[allow(unused)]
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteMacroParams {
    #[schemars(description = "UUID returned by create-macro tool invoked previously")]
    pub id: Uuid,
    #[schemars(description = "Path to the file the macro should operate on")]
    pub file_path: PathBuf,
    #[schemars(description = "Optional extra instruction forwarded to the LLM")]
    pub user_message: Option<String>,
}

#[derive(Clone)]
pub struct ApeServer {
    tool_router: ToolRouter<ApeServer>,
}

#[tool_router]
impl ApeServer {
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            tool_router: Self::tool_router(),
        })
    }

    #[tool(description = "Create new APE macro based on changes to a tracked file in git")]
    async fn create_macro(
        &self,
        Parameters(params): Parameters<CreateMacroParams>,
    ) -> Result<CallToolResult, McpError> {
        let staged = params.staged.unwrap_or_default();
        let id = create_macro(
            &params.file_path,
            params.repo_path.as_deref(),
            params.name.as_deref(),
            staged,
        )
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        info!(macro_id = %id, file_path = %params.file_path.display(), "APE macro created");
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::json!({ "id": id.to_string() }).to_string(),
        )]))
    }

    #[tool(description = "Execute an APE macro")]
    async fn execute_macro(
        &self,
        Parameters(params): Parameters<ExecuteMacroParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let prompt = execute_macro_sampling_prompt(
            &params.id,
            &params.file_path,
            params.user_message.as_deref(),
        )
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let response = context
            .peer
            .create_message(
                CreateMessageRequestParams::new(vec![SamplingMessage::user_text(prompt.user)], 150)
                    .with_model_preferences(ModelPreferences::default())
                    .with_system_prompt("You are a helpful assistant.")
                    .with_include_context(ContextInclusion::None)
                    .with_temperature(0.7),
            )
            .await
            .map_err(|e| McpError::internal_error(format!("Sampling request failed: {e}"), None))?;
        debug!("Response: {:?}", response);
        let output = response
            .message
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| &t.text)
            .ok_or(McpError::internal_error(
                "Failed to obtain LLM response in expected format".to_string(),
                None,
            ))?;
        let change =
            process_execute_macro_sampling_response(&params.file_path, output).map_err(|e| {
                McpError::internal_error(format!("Failed to process sampling response: {e}"), None)
            })?;
        // @TODO: Remove unwrap
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string(&change).unwrap(),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for ApeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(concat!(
                "MCP server for the APE tool that allows recording a change as a macro ",
                "which can then be executed or replayed with the help of LLMs.\n",
                "IMPORTANT: This server requires a client that supports the 'sampling/createMessage' method. ",
                "Without sampling support, the tools will return errors.\n",
                "Use 'create-macro' to register an APE macro and 'execute-macro' to execute/replay it.\n",
                "Typically, the user may explicitly ask to call the tool by name but ",
                "these tools can also be used if the user wants to 'show' a change to the llm ",
                "and have it replicate similar change in other parts of code.\n"
            ))
    }
}
