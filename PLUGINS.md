# Rust plugin API

The v1 plugin surface is **Rust-native and compile-time linked**. There is no
Java classloader, no Java jar loading, and no dynamic-library ABI yet. This is a
deliberate difference from the Java server, not a missing port detail.

## Supported model

A plugin is a Rust type implementing `hytale_builtin::BuiltinModule`:

```rust
use hytale_builtin::{BuiltinModule, ModuleContext, PluginManifest};

struct HelloPlugin {
    manifest: PluginManifest,
}

impl BuiltinModule for HelloPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn setup(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        Ok(())
    }

    fn start(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        Ok(())
    }

    fn shutdown(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        Ok(())
    }
}
```

`PluginManifest` keeps the Java field names (`Group`, `Name`, `Version`, `Main`,
`Dependencies`, `OptionalDependencies`, `LoadBefore`, `DisabledByDefault`,
`IncludesAssetPack`, `SubPlugins`) so manifests and diagnostics stay familiar.
The `Main` value is an identifier for the Rust plugin; it is not classloaded.

`ModuleContext` currently exposes the `BuiltinRuntime` switches. A plugin can
enable/disable the implemented simulation/NPC/weather switches during setup or
start. World mutation, command registration and network hooks are **not** part
of this API yet; adding them would require an explicit server-context trait and
a stable plugin ABI.

The complete runnable sample is:

```sh
cargo run -p hytale-builtin --example hello_plugin
```

`crates/hytale-builtin/tests/custom_plugin.rs` is the regression test for
manifest validation, deterministic start order, hooks and reverse shutdown.

## Embedding a custom plugin in a server binary

The shipped `hytale-server` binary only contains the selected builtins from
`crates/hytale-builtin`. To run a custom compiled-in plugin, build a small
launcher crate that:

1. constructs the plugin list with `BuiltinHost::with_modules(...)`;
2. calls `start` and keeps the host alive for the process lifetime;
3. passes `host.runtime()` into `ServeOptions.builtin`;
4. calls `serve_listener(...)` and `host.shutdown()` on exit.

The host lifecycle is synchronous and must not be started twice. A plugin that
fails `setup`/`start` fails the host start; dependencies that are disabled cause
a fail-closed start instead of silently running without them.

## Lifecycle and ordering

- Default-enabled modules start unless disabled with `--disable-module`.
- `--module Group:Name` opts into disabled-by-default modules.
- Dependencies start first; shutdown runs in reverse start order.
- Duplicate identifiers, bad manifests, disabled dependencies and cycles are
  rejected before hooks run.
- Disabled modules contribute no runtime switch and no work in the world tick.

## Java migration guide

| Java plugin concept | v1 Rust status |
|---|---|
| `JavaPlugin` / `JavaPluginInit` | `BuiltinModule` + `BuiltinHost` |
| Plugin jar discovery/classloading | Not supported; plugins are linked into the Rust binary |
| `manifest.json` fields | Mirrored by `PluginManifest` for builtins; custom manifests use the same struct |
| Event bus subscriptions | `hytale_core::EventBus` exists internally; it is not yet a stable plugin hook |
| Command registration | Not yet exposed to plugins |
| World/entity/network mutation API | Not yet exposed to plugins |
| Java asset-pack plugins | Use the asset-mod workflow in [`ASSET_MODS.md`](ASSET_MODS.md), not code plugins |

Do not port a Java plugin by dropping its jar into `mods/`; the Rust server does
not load it. Port the logic to Rust and link it through the lifecycle above, or
keep the feature server-side until a plugin context/ABI is designed.

## Build and feature switches

`hytale-builtin` modules are Cargo features. A build without a feature contains
no descriptor or implementation for that module. Check the compiled set with:

```sh
hytale-server --list-modules
```

Feature names and defaults are in `crates/hytale-builtin/Cargo.toml`.

## Follow-ups

- A stable `PluginContext` with world/command/event access.
- A dynamic-library or WASM ABI decision.
- A plugin packaging format separate from asset-only mods.
