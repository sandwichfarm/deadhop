//! ContextVM surface: strictly typed MCP tools served over Nostr. Five are
//! for invoicing; the rest are operator-only management tools.
//!
//! Inputs are deserialised into `deny_unknown_fields` structs and outputs are
//! serialised from structs with published JSON schemas, so nothing free-form
//! crosses the boundary in either direction.

use std::sync::Arc;

use contextvm_sdk::transport::server::{ClientPubkey, InboundEvent};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ErrorCode, Extensions, Implementation, ServerCapabilities, ServerInfo};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler};
use serde_json::json;

use crate::error::Error;
use crate::manage::{
    AddOperatorParams, AuditLog, AuditParams, Backup, BackupList, CheckProxyParams, ControlParams, ControlResult,
    Health, Metrics, RemoveOperatorParams, Series, SeriesParams, SettingsView, UpdateSettingsParams, WipeParams,
    WipeResult,
};
use crate::privacy::{PrivacyAudit, ProxyCheck};
use crate::service::{
    Caller, CreateInvoiceParams, GetInvoiceParams, Info, Invoice, InvoiceList, ListInvoicesParams, Service, Stats,
};

/// Identity of the requester as injected by the Nostr transport.
pub fn caller_from(ext: &Extensions) -> Caller {
    Caller {
        pubkey: ext.get::<ClientPubkey>().map(|p| p.0.clone()),
        event: ext.get::<InboundEvent>().map(|e| e.0.clone()),
    }
}

/// Convert a service error into a JSON-RPC error with a stable `data.code`.
pub fn rpc_error(e: Error) -> ErrorData {
    let code = match e.code() {
        "invalid_params" => -32602,
        "unauthorized" => -32001,
        "not_found" => -32002,
        "pow_rejected" => -32003,
        "rate_limited" => -32004,
        "capacity" => -32005,
        "paused" => -32006,
        _ => {
            tracing::error!(error = %e, "internal error while serving a request");
            -32603
        }
    };
    ErrorData::new(ErrorCode(code), e.public_message(), Some(json!({ "code": e.code() })))
}

/// The MCP server exposed over ContextVM.
#[derive(Clone)]
pub struct PayServer {
    svc: Arc<Service>,
}

impl PayServer {
    pub fn new(svc: Arc<Service>) -> Self {
        PayServer { svc }
    }
}

#[tool_router]
impl PayServer {
    #[tool(
        name = "get_info",
        description = "Service parameters: network, limits, required proof of work, and whether the caller is an operator."
    )]
    async fn tool_get_info(&self, ctx: RequestContext<RoleServer>) -> Result<Json<Info>, ErrorData> {
        self.svc
            .info(&caller_from(&ctx.extensions))
            .map(Json)
            .map_err(rpc_error)
    }

    #[tool(
        name = "create_invoice",
        description = "Create an on-chain invoice with a fresh address. Non-operators must mine NIP-13 proof of work into the request event."
    )]
    async fn tool_create_invoice(
        &self,
        Parameters(p): Parameters<CreateInvoiceParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<Invoice>, ErrorData> {
        self.svc
            .create_invoice(&caller_from(&ctx.extensions), p)
            .map(Json)
            .map_err(rpc_error)
    }

    #[tool(
        name = "get_invoice",
        description = "Look up an invoice and its payment status by id."
    )]
    async fn tool_get_invoice(
        &self,
        Parameters(p): Parameters<GetInvoiceParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<Invoice>, ErrorData> {
        self.svc
            .get_invoice(&caller_from(&ctx.extensions), p)
            .map(Json)
            .map_err(rpc_error)
    }

    #[tool(
        name = "list_invoices",
        description = "Operators only: page through invoices, newest first."
    )]
    async fn tool_list_invoices(
        &self,
        Parameters(p): Parameters<ListInvoicesParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<InvoiceList>, ErrorData> {
        self.svc
            .list_invoices(&caller_from(&ctx.extensions), p)
            .map(Json)
            .map_err(rpc_error)
    }

    #[tool(
        name = "get_stats",
        description = "Operators only: totals per status and chain backend health."
    )]
    async fn tool_get_stats(&self, ctx: RequestContext<RoleServer>) -> Result<Json<Stats>, ErrorData> {
        self.svc
            .stats(&caller_from(&ctx.extensions))
            .map(Json)
            .map_err(rpc_error)
    }

    #[tool(
        name = "get_health",
        description = "Operators only: relay connections, chain backend, database integrity and process uptime."
    )]
    async fn tool_get_health(&self, ctx: RequestContext<RoleServer>) -> Result<Json<Health>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.health(&c).await.map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "get_metrics",
        description = "Operators only: addresses created, invoices, payments and payments within 24h, 7d, 30d and 365d."
    )]
    async fn tool_get_metrics(&self, ctx: RequestContext<RoleServer>) -> Result<Json<Metrics>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.metrics(&c).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "get_payment_series",
        description = "Operators only: confirmed payments per hour, day or week for a line chart."
    )]
    async fn tool_get_payment_series(
        &self,
        Parameters(p): Parameters<SeriesParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<Series>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.payment_series(&c, p).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "get_settings",
        description = "Operators only: live settings, environment defaults, overrides and operator keys."
    )]
    async fn tool_get_settings(&self, ctx: RequestContext<RoleServer>) -> Result<Json<SettingsView>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.get_settings(&c).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "update_settings",
        description = "Operators only: change settings at runtime. A new relay list restarts the relay connections."
    )]
    async fn tool_update_settings(
        &self,
        Parameters(p): Parameters<UpdateSettingsParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<SettingsView>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.update_settings(&c, p).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "reset_settings",
        description = "Operators only: forget settings changed from the dashboard and use the environment values."
    )]
    async fn tool_reset_settings(&self, ctx: RequestContext<RoleServer>) -> Result<Json<SettingsView>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.reset_settings(&c).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "add_operator",
        description = "Operators only: let another Nostr key (hex, npub or nprofile) operate the service."
    )]
    async fn tool_add_operator(
        &self,
        Parameters(p): Parameters<AddOperatorParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<SettingsView>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.add_operator(&c, p).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "remove_operator",
        description = "Operators only: revoke an operator key added from the dashboard."
    )]
    async fn tool_remove_operator(
        &self,
        Parameters(p): Parameters<RemoveOperatorParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<SettingsView>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.remove_operator(&c, p).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "control_service",
        description = "Operators only: stop or start invoicing, or restart the relay connections."
    )]
    async fn tool_control_service(
        &self,
        Parameters(p): Parameters<ControlParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<ControlResult>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.control(&c, p).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "backup_database",
        description = "Operators only: write a consistent backup of the database on the service host."
    )]
    async fn tool_backup_database(&self, ctx: RequestContext<RoleServer>) -> Result<Json<Backup>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.backup(&c).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "list_backups",
        description = "Operators only: the database backups kept on the service host, newest first."
    )]
    async fn tool_list_backups(&self, ctx: RequestContext<RoleServer>) -> Result<Json<BackupList>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.list_backups(&c).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "wipe_database",
        description = "Operators only: delete every invoice after taking a backup. Requires confirm = WIPE."
    )]
    async fn tool_wipe_database(
        &self,
        Parameters(p): Parameters<WipeParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<WipeResult>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.wipe(&c, p).map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "check_proxy",
        description = "Operators only: open a tunnel through a proxy (default: the current one) to each live relay and report, per relay, whether it worked and why not."
    )]
    async fn tool_check_proxy(
        &self,
        Parameters(p): Parameters<CheckProxyParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<ProxyCheck>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.check_proxy(&c, p).await.map(Json).map_err(rpc_error)
    }

    #[tool(
        name = "get_privacy_audit",
        description = "Operators only: what can be learned about where this service runs, scored from the checks it can make."
    )]
    async fn tool_get_privacy_audit(&self, ctx: RequestContext<RoleServer>) -> Result<Json<PrivacyAudit>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.privacy_audit(&c).await.map(Json).map_err(rpc_error)
    }

    #[tool(name = "get_audit_log", description = "Operators only: recent operator actions.")]
    async fn tool_get_audit_log(
        &self,
        Parameters(p): Parameters<AuditParams>,
        ctx: RequestContext<RoleServer>,
    ) -> Result<Json<AuditLog>, ErrorData> {
        let c = caller_from(&ctx.extensions);
        self.svc.audit_log(&c, p).map(Json).map_err(rpc_error)
    }
}

#[tool_handler]
impl ServerHandler for PayServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("deadhop", env!("CARGO_PKG_VERSION")))
            .with_instructions("On-chain bitcoin invoices over ContextVM.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::{EventBuilder, Keys, Kind};

    #[test]
    fn caller_is_read_from_extensions() {
        let empty = caller_from(&Extensions::new());
        assert!(empty.pubkey.is_none() && empty.event.is_none());

        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(25910), "{}")
            .sign_with_keys(&keys)
            .unwrap();
        let mut ext = Extensions::new();
        ext.insert(ClientPubkey("abc".into()));
        ext.insert(InboundEvent(event.clone()));
        let c = caller_from(&ext);
        assert_eq!(c.pubkey.as_deref(), Some("abc"));
        assert_eq!(c.event.unwrap().id, event.id);
    }

    #[test]
    fn errors_map_to_stable_codes_without_leaking_internals() {
        let cases = [
            (Error::InvalidParams("bad".into()), -32602, "invalid_params"),
            (Error::Unauthorized, -32001, "unauthorized"),
            (Error::NotFound, -32002, "not_found"),
            (Error::Pow("weak".into()), -32003, "pow_rejected"),
            (Error::RateLimited, -32004, "rate_limited"),
            (Error::Capacity, -32005, "capacity"),
            (Error::Paused, -32006, "paused"),
            (Error::Chain("10.1.2.3:50001 refused".into()), -32603, "internal"),
        ];
        for (err, code, name) in cases {
            let e = rpc_error(err);
            assert_eq!(e.code.0, code);
            assert_eq!(e.data.unwrap()["code"], name);
            assert!(!e.message.contains("10.1.2.3"));
        }
    }
}
