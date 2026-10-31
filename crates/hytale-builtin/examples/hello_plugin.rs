//! Minimal Rust-native plugin using the `hytale-builtin` lifecycle.
//!
//! ```sh
//! cargo run -p hytale-builtin --example hello_plugin
//! ```
//!
//! The plugin is linked into the binary. This API does not load Java jars or
//! dynamic libraries; see `PLUGINS.md` for the supported embedding model.

use hytale_builtin::{
    BuiltinHost, BuiltinModule, ModuleContext, ModuleState, PluginManifest, StartOptions,
};

struct HelloPlugin {
    manifest: PluginManifest,
    started: bool,
}

impl HelloPlugin {
    fn new() -> Self {
        Self {
            manifest: PluginManifest::new("Example", "Hello", "1.0.0", "hello_plugin::HelloPlugin"),
            started: false,
        }
    }
}

impl BuiltinModule for HelloPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn setup(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        println!("hello: setup");
        Ok(())
    }

    fn start(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        println!("hello: start");
        self.started = true;
        Ok(())
    }

    fn shutdown(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        println!("hello: shutdown");
        self.started = false;
        Ok(())
    }

    fn is_started(&self) -> bool {
        self.started
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut host = BuiltinHost::with_modules(vec![Box::new(HelloPlugin::new())])?;
    let report = host.start(&StartOptions::default())?;
    assert!(report.ids().contains(&"Example:Hello"));
    assert_eq!(host.state("Example:Hello"), Some(ModuleState::Enabled));
    let stopped = host.shutdown();
    assert_eq!(stopped.stopped, vec!["Example:Hello".to_string()]);
    println!("hello plugin lifecycle complete");
    Ok(())
}
