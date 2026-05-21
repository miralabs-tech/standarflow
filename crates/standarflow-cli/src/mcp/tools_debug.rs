use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ErrorData as McpError};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_router, RoleServer};

use super::helpers::{current_conversation, ok_json};
use super::out::DebugEnvOut;
use super::req::DebugEnvReq;
use super::StandarflowMcp;
use crate::common::DEBUG_ENV_PREFIXES;
use crate::proctree;

#[tool_router(router = debug_router, vis = "pub(crate)")]
impl StandarflowMcp {
    #[tool(
        description = "Diagnostic: dump the MCP server process state — pid, cwd, exe, args, env, agent process tree, and the resolved conversation id."
    )]
    async fn debug_env(
        &self,
        Parameters(req): Parameters<DebugEnvReq>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mcp_client_name = ctx
            .peer
            .peer_info()
            .map(|info| info.client_info.name.clone());
        let mcp_client_version = ctx
            .peer
            .peer_info()
            .map(|info| info.client_info.version.clone());

        let resolved_conversation_id = {
            let conn = self.locked();
            current_conversation(&conn).map(|c| c.id)
        };

        let envs: std::collections::BTreeMap<String, String> = std::env::vars()
            .filter(|(k, _)| {
                if req.all {
                    return true;
                }
                let up = k.to_uppercase();
                DEBUG_ENV_PREFIXES.iter().any(|p| up.starts_with(p))
            })
            .collect();

        let out = DebugEnvOut {
            pid: std::process::id(),
            cwd: std::env::current_dir()
                .ok()
                .map(|p| p.display().to_string()),
            exe: std::env::current_exe()
                .ok()
                .map(|p| p.display().to_string()),
            args: std::env::args().collect(),
            mcp_client_name,
            mcp_client_version,
            agent_root_pid: proctree::agent_root_pid(),
            conversation_pid: proctree::conversation_pid(),
            resolved_conversation_id,
            parent_chain: proctree::parent_chain(),
            env: envs,
        };
        ok_json(&out)
    }
}
