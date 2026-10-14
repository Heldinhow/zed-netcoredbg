# zed-netcoredbg

Debug C#/.NET (CoreCLR) in [Zed](https://zed.dev) using Samsung's
[netcoredbg](https://github.com/Samsung/netcoredbg) as the debug adapter.

> Status: early. Verified on macOS arm64 with .NET 10. Other platforms are
> wired up but untested.

## Install

From the Zed extension list (search `netcoredbg`): one click, no extra setup.
> Pending review: PR [zed-industries/extensions#7987](https://github.com/zed-industries/extensions/pull/7987).

### Dev install (contributors only)

Building a dev extension compiles Rust to WebAssembly, so Zed must find `rustc`:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
"$HOME/.cargo/bin/rustup" target add wasm32-wasip2
```

If Zed was launched from the Dock it may not see `~/.cargo/bin`: start it from a terminal (`zed .`) or link the tools somewhere already on your PATH (`ln -sf "$HOME/.cargo/bin/"{cargo,rustc,rustup} ~/.local/bin/`).

## Requirements

- .NET SDK/runtime on the PATH (Zed must see it too, same PATH caveat as above).
- No other tools: the adapter binary is downloaded from netcoredbg's GitHub
  releases on first use and cached per version.

## Configure

Add `.zed/debug.json` to your project. `program` must point at the built
assembly and match your TFM and configuration.

```json
[
  {
    "label": "Build & debug console app",
    "adapter": "netcoredbg",
    "request": "launch",
    "build": { "command": "dotnet", "args": ["build"] },
    "program": "$ZED_WORKTREE_ROOT/bin/Debug/net10.0/MyApp.dll",
    "cwd": "$ZED_WORKTREE_ROOT"
  },
  {
    "label": "Attach (edit processId first)",
    "adapter": "netcoredbg",
    "request": "attach",
    "processId": 0
  }
]
```

Launch fields: `program` (required, `*.dll`/`*.exe`), `args`, `cwd`, `env`,
`stopAtEntry`. Attach requires `processId`.

### ASP.NET Core

Set the content root through `cwd` (the project folder, where `appsettings.json`
lives) and configure the app through `env`:

```json
{
  "label": "Debug web API",
  "adapter": "netcoredbg",
  "request": "launch",
  "build": { "command": "dotnet", "args": ["build", "src/Api"] },
  "program": "$ZED_WORKTREE_ROOT/src/Api/bin/Debug/net10.0/Api.dll",
  "cwd": "$ZED_WORKTREE_ROOT/src/Api",
  "env": {
    "ASPNETCORE_ENVIRONMENT": "Development",
    "ASPNETCORE_URLS": "http://localhost:5001"
  }
}
```

`launchSettings.json` is not read.

### Attach

The process picker is not available for .NET. Find the PID and put it in the
config before attaching; it changes on every run:

```sh
pgrep -f MyApp.dll
```

Use **Detach** (not Stop) to leave the process running.

## What works

- Launch and attach, breakpoints (line and conditional), stepping, variables,
  call stack, captured console output.
- Build task before launch; a failed build blocks the launch.

## Limitations

- No interactive stdin: `Console.ReadLine()` in a launched app gets no input.
  To interact, run the app in a terminal and attach.
- No logpoints or hit-count breakpoints (netcoredbg ignores them).
- Exception breakpoints: best effort. netcoredbg stops on unhandled exceptions
  regardless of filters, and Zed cannot edit exception conditions.
- Expression evaluation is basic; complex expressions and `DebuggerDisplay`
  may fail. No hot reload.
- `Stop` on an attach session sends `terminate` and may kill the target.

## Platforms

| Platform | Asset | Verified |
|---|---|---|
| macOS arm64 | `netcoredbg-osx-arm64.zip` (3.2.0) | yes |
| macOS x64 | `netcoredbg-osx-amd64.tar.gz` (3.1.3) | no |
| Linux x64 / arm64 | `netcoredbg-linux-*.tar.gz` (3.2.0) | no |
| Windows x64 | `netcoredbg-win64.zip` (3.2.0) | no |

Windows arm64 and 32-bit are not supported (no upstream build).

## License

MIT. netcoredbg is MIT, © Samsung Electronics.
