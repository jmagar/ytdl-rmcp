# ytdl-rmcp

Node launcher for the `ytdl-rmcp` Rust MCP server and CLI binary.

```bash
npx -y ytdl-rmcp
```

Run the guided setup:

```bash
npx -y ytdl-rmcp setup
```

Install globally when you want the command on `PATH`:

```bash
npm i -g ytdl-rmcp
ytdl-rmcp --version
ytdl-rmcp setup
```

The package downloads the matching GitHub Release binary during `postinstall`.
The npm package version and the `ytdl-rmcp` release tag are expected to match.
Release automation publishes this package from the repository `v*` tag workflow;
the GitHub repository must have an `NPM_TOKEN` secret with publish access.

## MCP stdio

Run without subcommands, `ytdl-rmcp` serves MCP over stdio. MCP clients can launch
it with:

```json
{
  "command": "npx",
  "args": ["-y", "ytdl-rmcp"],
  "env": {
    "YTDLP_REMOTE": "tootie",
    "YTDLP_REMOTE_PATH": "/media/music"
  }
}
```

## Overrides

```bash
YTDL_RMCP_BINARY_VERSION=v0.7.0 npm i -g ytdl-rmcp
YTDL_RMCP_RELEASE_BASE_URL=https://github.com/jmagar/ytdl-rmcp/releases/download npm i -g ytdl-rmcp
YTDL_RMCP_SKIP_DOWNLOAD=1 npm i -g ytdl-rmcp
```

Supported binary targets are Linux x64 and Windows x64, matching the current
GitHub Release assets.
