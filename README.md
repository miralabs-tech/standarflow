# standarflow

<p align="center">
  <a href="https://github.com/miralabs-tech/standarflow/releases"><img src="https://img.shields.io/badge/status-alpha-orange?style=flat-square" alt="Status: alpha"></a>
  <a href="https://github.com/miralabs-tech/standarflow/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/miralabs-tech/standarflow/ci.yml?branch=main&label=ci&style=flat-square" alt="CI"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/built%20with-Rust-orange?style=flat-square" alt="Built with Rust"></a>
  <a href="#quick-start"><img src="https://img.shields.io/badge/surfaces-CLI%20·%20MCP%20stdio-blue?style=flat-square" alt="Surfaces: CLI · MCP stdio"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License: MIT"></a>
  <a href="https://github.com/miralabs-tech/standarflow/stargazers"><img src="https://img.shields.io/github/stars/miralabs-tech/standarflow?label=stars&style=flat-square" alt="Stars"></a>
  <a href="https://github.com/miralabs-tech/standarflow/releases"><img src="https://img.shields.io/github/downloads/miralabs-tech/standarflow/total?label=release%20downloads&style=flat-square" alt="Release downloads"></a>
</p>

**Session, artefact, link and conversation store for AI-assisted work.**

The Rust core of standarflow: a local SQLite database plus the `standarflow`
binary, which exposes a clap CLI and an MCP stdio server. The same data is
reachable from Claude Code, the terminal, and the VS Code extension.

## Crates

| Crate | Role |
| --- | --- |
| `crates/standarflow-core` | Library: SQLite schema + migrations, all CRUD, the provider adapters, the event ingest/tail pipeline, the hooks installer, the export pipeline. |
| `crates/standarflow-cli` | Binary `standarflow`: a clap CLI plus the `mcp` stdio server. |
| `crates/standarflow-overlay` | Placeholder for a cross-OS desktop overlay. Not a priority. |

## Quick start

```sh
# Build the binary
cargo build --release -p standarflow-cli

# Initialise the per-workspace database
standarflow init

# Wire Claude Code so its hook events flow into standarflow
standarflow hooks install --provider claude-code
```

## Data location

One database per workspace, at `<workspace>/.standarflow/standarflow.db` (SQLite,
WAL mode). The per-workspace event log sits next to it at
`<workspace>/.standarflow/events.jsonl`.

## Related repositories

This crate workspace is consumed as a git submodule by the umbrella repo
[miralabs-tech/standarflow-project](https://github.com/miralabs-tech/standarflow-project),
which also hosts the [VS Code extension](https://github.com/miralabs-tech/standarflow-vscode)
and the full documentation under `docs/`.

## License

MIT — see [LICENSE](LICENSE).
