use std::path::Path;

use anyhow::{anyhow, Context};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ErrorData as McpError};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_router, RoleServer};
use standarflow_core::store::file_ref;

use super::helpers::{client_name, json_result, text_result, touch_participant};
use super::out::{FileDeleteWithSourceOut, FileRefOut};
use super::req::{
    FileAttachReq, FileClaimReq, FileDeleteWithSourceReq, FileListReq, FileReadReq, FileRemoveReq,
    MemoryImportReq,
};
use super::StandarflowMcp;
use crate::common::{canonical_path, resolve_group, resolve_session};

#[tool_router(router = file_router, vis = "pub(crate)")]
impl StandarflowMcp {
    #[tool(description = "Attach an external file to a session, with optional role and description. source is recorded as 'manual'.")]
    async fn file_attach(
        &self,
        Parameters(req): Parameters<FileAttachReq>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let by = client_name(&ctx);
        let conn = self.locked();
        json_result(|| -> anyhow::Result<FileRefOut> {
            let group_id = resolve_group(&conn, &req.group_path)?;
            let session_id = resolve_session(&conn, group_id, &req.session_slug)?;
            let p = canonical_path(Path::new(&req.path))?;
            let role = req.role.as_deref().unwrap_or(file_ref::ROLE_ATTACHMENT);
            let id = file_ref::attach(
                &conn,
                &file_ref::NewFileRef {
                    session_id,
                    path: &p,
                    role,
                    source: file_ref::SOURCE_MANUAL,
                    description: req.description.as_deref(),
                    created_by: &by,
                },
            )?;
            touch_participant(&conn, session_id);
            Ok(file_ref::get(&conn, id)?.into())
        })
    }

    #[tool(description = "List file_refs attached to a session.")]
    async fn file_list(
        &self,
        Parameters(req): Parameters<FileListReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<Vec<FileRefOut>> {
            let group_id = resolve_group(&conn, &req.group_path)?;
            let session_id = resolve_session(&conn, group_id, &req.session_slug)?;
            Ok(file_ref::list_for_session(&conn, session_id)?
                .into_iter()
                .map(Into::into)
                .collect())
        })
    }

    #[tool(description = "Read the current contents of a file_ref's target file from disk.")]
    async fn file_read(
        &self,
        Parameters(req): Parameters<FileReadReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        text_result(|| {
            let f = file_ref::get(&conn, req.file_ref_id)?;
            let content = std::fs::read_to_string(&f.path)
                .with_context(|| format!("cannot read {}", f.path))?;
            Ok(content)
        })
    }

    #[tool(description = "Detach a file_ref by id (does not delete the file on disk).")]
    async fn file_remove(
        &self,
        Parameters(req): Parameters<FileRemoveReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        text_result(|| {
            file_ref::detach(&conn, req.file_ref_id)?;
            Ok(format!("detached file_ref#{}", req.file_ref_id))
        })
    }

    #[tool(description = "Re-assign the created_by of an existing file_ref to the current MCP client.")]
    async fn file_claim(
        &self,
        Parameters(req): Parameters<FileClaimReq>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let by = client_name(&ctx);
        let conn = self.locked();
        json_result(|| -> anyhow::Result<FileRefOut> {
            file_ref::claim(&conn, req.file_ref_id, &by)?;
            Ok(file_ref::get(&conn, req.file_ref_id)?.into())
        })
    }

    #[tool(description = "Delete the on-disk file AND detach the file_ref. If the file is already missing, detach still happens and file_was_missing=true is returned.")]
    async fn file_delete_with_source(
        &self,
        Parameters(req): Parameters<FileDeleteWithSourceReq>,
    ) -> Result<CallToolResult, McpError> {
        let conn = self.locked();
        json_result(|| -> anyhow::Result<FileDeleteWithSourceOut> {
            let out = file_ref::delete_with_source(&conn, req.file_ref_id)?;
            Ok(FileDeleteWithSourceOut {
                file_ref_id: req.file_ref_id,
                path: out.path,
                file_deleted: out.file_deleted,
                file_was_missing: out.file_was_missing,
            })
        })
    }

    #[tool(description = "Scan a directory for files matching an extension (default 'md') and attach each as a file_ref with source 'memory_import'. Provider-agnostic.")]
    async fn memory_import(
        &self,
        Parameters(req): Parameters<MemoryImportReq>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let by = client_name(&ctx);
        let conn = self.locked();
        json_result(|| -> anyhow::Result<Vec<FileRefOut>> {
            let group_id = resolve_group(&conn, &req.group_path)?;
            let session_id = resolve_session(&conn, group_id, &req.session_slug)?;
            let dir = Path::new(&req.dir_path);
            let meta = std::fs::metadata(dir)
                .with_context(|| format!("cannot stat {}", dir.display()))?;
            if !meta.is_dir() {
                return Err(anyhow!("not a directory: {}", dir.display()));
            }
            let want = req.ext.as_deref().unwrap_or("md").trim_start_matches('.');
            let role = req.role.as_deref().unwrap_or(file_ref::ROLE_MEMORY);
            let mut out = Vec::new();
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                if p.extension().and_then(|e| e.to_str()) != Some(want) {
                    continue;
                }
                let canonical = canonical_path(&p)?;
                let id = file_ref::attach(
                    &conn,
                    &file_ref::NewFileRef {
                        session_id,
                        path: &canonical,
                        role,
                        source: file_ref::SOURCE_MEMORY_IMPORT,
                        description: None,
                        created_by: &by,
                    },
                )?;
                out.push(file_ref::get(&conn, id)?.into());
            }
            touch_participant(&conn, session_id);
            Ok(out)
        })
    }
}
