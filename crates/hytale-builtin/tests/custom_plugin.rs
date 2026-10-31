//! Regression test for the public Rust plugin lifecycle seam.
//!
//! A plugin is a compiled-in [`BuiltinModule`]. The host validates its manifest,
//! starts it in dependency order and shuts it down in reverse order.

use hytale_builtin::{
    BuiltinHost, BuiltinModule, ModuleContext, ModuleState, PluginManifest, StartOptions,
};

struct CountingPlugin {
    manifest: PluginManifest,
    events: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
}

impl CountingPlugin {
    fn new(name: &str, events: &std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>) -> Self {
        Self {
            manifest: PluginManifest::new("Example", name, "1.0.0", "custom_plugin::Plugin"),
            events: std::sync::Arc::clone(events),
        }
    }

    fn record(&self, event: &'static str) {
        self.events.lock().expect("events").push(event);
    }
}

impl BuiltinModule for CountingPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn setup(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        self.record("setup");
        Ok(())
    }

    fn start(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        self.record("start");
        Ok(())
    }

    fn shutdown(&mut self, _context: &mut ModuleContext<'_>) -> Result<(), String> {
        self.record("shutdown");
        Ok(())
    }

    fn is_started(&self) -> bool {
        self.events.lock().expect("events").contains(&"start")
    }
}

#[test]
fn a_custom_plugin_runs_through_the_public_lifecycle() {
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut host = BuiltinHost::with_modules(vec![
        Box::new(CountingPlugin::new("First", &events)),
        Box::new(CountingPlugin::new("Second", &events)),
    ])
    .expect("host");

    assert_eq!(host.module_ids(), vec!["Example:First", "Example:Second"]);
    let report = host.start(&StartOptions::default()).expect("start");
    assert_eq!(
        report.ids(),
        vec!["Example:First", "Example:Second"],
        "name order is deterministic"
    );
    assert_eq!(host.state("Example:First"), Some(ModuleState::Enabled));
    assert_eq!(
        *events.lock().expect("events"),
        vec!["setup", "start", "setup", "start"]
    );

    let stopped = host.shutdown();
    assert_eq!(
        stopped.stopped,
        vec!["Example:Second".to_string(), "Example:First".to_string()],
        "shutdown is reverse start order"
    );
    assert_eq!(
        *events.lock().expect("events"),
        vec!["setup", "start", "setup", "start", "shutdown", "shutdown"]
    );
    assert_eq!(host.state("Example:First"), Some(ModuleState::Disabled));
}

#[test]
fn a_plugin_with_a_bad_manifest_is_rejected() {
    struct BadPlugin(PluginManifest);

    impl BuiltinModule for BadPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.0
        }
    }

    let bad = BadPlugin(PluginManifest {
        group: String::new(),
        ..PluginManifest::new("Example", "Broken", "1.0.0", "custom_plugin::Broken")
    });
    assert!(BuiltinHost::with_modules(vec![Box::new(bad)]).is_err());
}
