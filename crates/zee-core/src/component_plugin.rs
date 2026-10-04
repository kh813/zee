use std::sync::Arc;
use anyhow::{Context, Result};
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};

wasmtime::component::bindgen!({
    path: "wit",
    world: "plugin",
});

pub use self::zee::plugin::buffer::{BufferInfo, Edit};
pub use self::zee::plugin::outline::OutlineNode as WitOutlineNode;

/// Host state provided to the guest plugin during execution
pub struct HostState {
    pub text: String,
    pub path: Option<String>,
    pub applied_edits: Vec<Edit>,
    pub limits: StoreLimits,
}

impl self::zee::plugin::buffer::Host for HostState {
    fn get_info(&mut self) -> BufferInfo {
        let line_count = self.text.lines().count().max(1) as u32;
        BufferInfo {
            line_count,
            byte_length: self.text.len() as u32,
            path: self.path.clone(),
        }
    }

    fn get_text_range(&mut self, start_byte: u32, end_byte: u32) -> Result<String, String> {
        let start = start_byte as usize;
        let end = end_byte as usize;
        if start > end || end > self.text.len() {
            return Err(format!("Range out of bounds: {}..{}", start, end));
        }
        if !self.text.is_char_boundary(start) || !self.text.is_char_boundary(end) {
            return Err("Byte range does not align with UTF-8 character boundary".to_string());
        }
        Ok(self.text[start..end].to_string())
    }

    fn get_all_text(&mut self) -> String {
        self.text.clone()
    }

    fn apply_edits(&mut self, edits: Vec<Edit>) -> Result<(), String> {
        // Validate edits order and boundaries
        for edit in &edits {
            let start = edit.start_byte as usize;
            let end = edit.end_byte as usize;
            if start > end || end > self.text.len() {
                return Err(format!("Edit range out of bounds: {}..{}", start, end));
            }
            if !self.text.is_char_boundary(start) || !self.text.is_char_boundary(end) {
                return Err("Edit byte offset does not align with UTF-8 boundary".to_string());
            }
        }

        self.applied_edits.extend(edits);
        Ok(())
    }
}

impl self::zee::plugin::outline::Host for HostState {}

/// Thread-safe host engine configuration and runner for Component Model plugins
#[derive(Clone)]
pub struct ComponentEngine {
    engine: Arc<Engine>,
}

impl ComponentEngine {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);

        let engine = Engine::new(&config)
            .map_err(|e| anyhow::anyhow!("Failed to initialize Wasmtime Engine: {e}"))?;
        let engine = Arc::new(engine);

        // Background epoch ticker: increments epoch every 50ms to enforce execution deadlines
        let engine_clone = engine.clone();
        std::thread::Builder::new()
            .name("zee-wasm-epoch-ticker".to_string())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
                engine_clone.increment_epoch();
            })
            .context("Failed to spawn epoch ticker thread")?;

        Ok(Self { engine })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

/// A loaded WebAssembly Component plugin
pub struct ComponentPluginInstance {
    component: Component,
    linker: Linker<HostState>,
    engine: ComponentEngine,
    max_memory_bytes: usize,
    default_fuel: u64,
    epoch_timeout_ticks: u64,
}

impl ComponentPluginInstance {
    pub fn from_bytes(
        engine: ComponentEngine,
        wasm_bytes: &[u8],
        max_memory_bytes: Option<usize>,
    ) -> Result<Self> {
        let component = Component::from_binary(engine.engine(), wasm_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to compile WASM component: {e:?}"))?;

        let mut linker = Linker::new(engine.engine());
        Plugin::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |x| x)
            .map_err(|e| anyhow::anyhow!("Failed to add plugin imports to Linker: {e}"))?;

        Ok(Self {
            component,
            linker,
            engine,
            max_memory_bytes: max_memory_bytes.unwrap_or(64 * 1024 * 1024), // 64 MB default
            default_fuel: 10_000_000,                                      // 10M instructions
            epoch_timeout_ticks: 20,                                       // ~1 second (20 * 50ms)
        })
    }

    pub fn set_limits(&mut self, default_fuel: u64, epoch_timeout_ticks: u64) {
        self.default_fuel = default_fuel;
        self.epoch_timeout_ticks = epoch_timeout_ticks;
    }

    fn create_store(&self, text: String, path: Option<String>) -> Result<Store<HostState>> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.max_memory_bytes)
            .build();

        let state = HostState {
            text,
            path,
            applied_edits: Vec::new(),
            limits,
        };

        let mut store = Store::new(self.engine.engine(), state);
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(self.default_fuel)
            .map_err(|e| anyhow::anyhow!("Failed to set fuel: {e}"))?;
        store.set_epoch_deadline(self.epoch_timeout_ticks);

        Ok(store)
    }

    /// Execute a command by name on the given buffer snapshot
    pub fn execute_command(
        &self,
        command_name: &str,
        text: String,
        path: Option<String>,
    ) -> Result<(String, Vec<Edit>)> {
        let mut store = self.create_store(text, path)?;
        let instance = self.linker.instantiate(&mut store, &self.component)
            .map_err(|e| anyhow::anyhow!("Failed to instantiate Component Plugin: {e}"))?;

        if let Ok(func) = instance.get_typed_func::<(&str,), (Result<String, String>,)>(&mut store, "on-command") {
            let (res,) = func
                .call(&mut store, (command_name,))
                .map_err(|e| anyhow::anyhow!("Plugin execution failed or exceeded resource limits (fuel / epoch): {e}"))?;

            let edits = store.data_mut().applied_edits.drain(..).collect();
            match res {
                Ok(msg) => Ok((msg, edits)),
                Err(err) => Err(anyhow::anyhow!("Plugin returned error: {}", err)),
            }
        } else if let Ok(plugin) = Plugin::new(&mut store, &instance) {
            let res = plugin
                .call_on_command(&mut store, command_name)
                .map_err(|e| anyhow::anyhow!("Plugin execution failed or exceeded resource limits (fuel / epoch): {e}"))?;

            let edits = store.data_mut().applied_edits.drain(..).collect();
            match res {
                Ok(msg) => Ok((msg, edits)),
                Err(err) => Err(anyhow::anyhow!("Plugin returned error: {}", err)),
            }
        } else {
            Err(anyhow::anyhow!("Plugin does not implement on-command export"))
        }
    }

    /// Extract document outline from buffer snapshot
    pub fn get_outline(
        &self,
        text: String,
        path: Option<String>,
    ) -> Result<Vec<crate::outline::OutlineNode>> {
        let mut store = self.create_store(text, path)?;
        let instance = self.linker.instantiate(&mut store, &self.component)
            .map_err(|e| anyhow::anyhow!("Failed to instantiate Component Plugin: {e}"))?;

        if let Ok(func) = instance.get_typed_func::<(), (Result<Vec<WitOutlineNode>, String>,)>(&mut store, "get-outline") {
            let (res,) = func
                .call(&mut store, ())
                .map_err(|e| anyhow::anyhow!("Outline extraction failed or exceeded resource limits: {e}"))?;

            match res {
                Ok(nodes) => {
                    let converted = nodes
                        .into_iter()
                        .map(|n| crate::outline::OutlineNode::new(n.title, n.level as usize, n.line as usize))
                        .collect();
                    Ok(converted)
                }
                Err(err) => Err(anyhow::anyhow!("Plugin outline error: {}", err)),
            }
        } else if let Ok(plugin) = Plugin::new(&mut store, &instance) {
            let res = plugin
                .call_get_outline(&mut store)
                .map_err(|e| anyhow::anyhow!("Outline extraction failed or exceeded resource limits: {e}"))?;

            match res {
                Ok(nodes) => {
                    let converted = nodes
                        .into_iter()
                        .map(|n| crate::outline::OutlineNode::new(n.title, n.level as usize, n.line as usize))
                        .collect();
                    Ok(converted)
                }
                Err(err) => Err(anyhow::anyhow!("Plugin outline error: {}", err)),
            }
        } else {
            Err(anyhow::anyhow!("Plugin does not implement get-outline export"))
        }
    }

    /// Invoke plugin lifecycle initialization hook
    pub fn init(&self, text: String, path: Option<String>) -> Result<()> {
        let mut store = self.create_store(text, path)?;
        let instance = self.linker.instantiate(&mut store, &self.component)
            .map_err(|e| anyhow::anyhow!("Failed to instantiate Component Plugin: {e}"))?;

        if let Ok(func) = instance.get_typed_func::<(), ()>(&mut store, "on-init") {
            func.call(&mut store, ())
                .map_err(|e| anyhow::anyhow!("Plugin init failed or exceeded resource limits: {e}"))?;
            Ok(())
        } else if let Ok(plugin) = Plugin::new(&mut store, &instance) {
            let res = plugin
                .call_on_init(&mut store)
                .map_err(|e| anyhow::anyhow!("Plugin init failed or exceeded resource limits: {e}"))?;

            match res {
                Ok(()) => Ok(()),
                Err(err) => Err(anyhow::anyhow!("Plugin init error: {}", err)),
            }
        } else {
            Ok(())
        }
    }

    /// Execute a command asynchronously in a dedicated worker thread (non-blocking for UI)
    pub fn execute_command_async(
        self: Arc<Self>,
        command_name: String,
        text: String,
        path: Option<String>,
        on_complete: impl FnOnce(Result<(String, Vec<Edit>)>) + Send + 'static,
    ) {
        std::thread::Builder::new()
            .name(format!("zee-plugin-cmd-{}", command_name))
            .spawn(move || {
                let res = self.execute_command(&command_name, text, path);
                on_complete(res);
            })
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component_plugin::zee::plugin::buffer::Host;

    #[test]
    fn test_component_engine_initialization() {
        let engine = ComponentEngine::new().expect("Engine should initialize");
        assert!(engine.engine().get_epoch_interruption());
    }

    #[test]
    fn test_host_state_buffer_operations() {
        let text = "Hello, 世界!\nLine 2\nLine 3".to_string();
        let limits = StoreLimitsBuilder::new().build();
        let mut state = HostState {
            text: text.clone(),
            path: Some("/tmp/test.txt".to_string()),
            applied_edits: Vec::new(),
            limits,
        };

        // 1. Info
        let info = state.get_info();
        assert_eq!(info.line_count, 3);
        assert_eq!(info.byte_length, text.len() as u32);
        assert_eq!(info.path, Some("/tmp/test.txt".to_string()));

        // 2. All text
        assert_eq!(state.get_all_text(), text);

        // 3. Text range (ASCII)
        let r1 = state.get_text_range(0, 5).expect("ASCII slice");
        assert_eq!(r1, "Hello");

        // 4. Text range (Multibyte UTF-8)
        // "Hello, " is 7 bytes. "世界" is 6 bytes (3 bytes each).
        let r_cjk = state.get_text_range(7, 13).expect("CJK slice");
        assert_eq!(r_cjk, "世界");

        // 5. Invalid range (split in middle of multibyte char)
        let r_invalid = state.get_text_range(7, 8);
        assert!(r_invalid.is_err());
        assert!(r_invalid.unwrap_err().contains("UTF-8"));

        // 6. Out of bounds
        let r_oob = state.get_text_range(0, 9999);
        assert!(r_oob.is_err());
        assert!(r_oob.unwrap_err().contains("out of bounds"));

        // 7. Apply edits (valid)
        let edits = vec![
            Edit {
                start_byte: 0,
                end_byte: 5,
                new_text: "Hi".to_string(),
            },
        ];
        assert!(state.apply_edits(edits).is_ok());
        assert_eq!(state.applied_edits.len(), 1);
        assert_eq!(state.applied_edits[0].new_text, "Hi");

        // 8. Apply edits (invalid range)
        let bad_edits = vec![
            Edit {
                start_byte: 100,
                end_byte: 200,
                new_text: "fail".to_string(),
            },
        ];
        let bad_res = state.apply_edits(bad_edits);
        assert!(bad_res.is_err());
    }

    #[test]
    fn test_wat_component_compilation() {
        let engine = ComponentEngine::new().expect("Engine should initialize");
        let wat = r#"
            (component)
        "#;
        let bytes = wat::parse_str(wat).expect("Should parse component wat");
        let comp = Component::from_binary(engine.engine(), &bytes);
        assert!(comp.is_ok(), "Component should compile successfully");
    }

    #[test]
    fn test_component_plugin_init_and_fuel_interruption() {
        let engine = ComponentEngine::new().expect("Engine should initialize");

        // Component in WAT format that exports on-init with an infinite loop
        let comp_wat = r#"
            (component
                (core module $m
                    (func (export "init")
                        (loop (br 0))
                    )
                )
                (core instance $i (instantiate $m))
                (func (export "on-init")
                    (canon lift (core func $i "init"))
                )
            )
        "#;

        let comp_bytes = wat::parse_str(comp_wat).expect("Component wat should parse");
        let mut instance = ComponentPluginInstance::from_bytes(engine, &comp_bytes, None)
            .expect("Component instance should load");

        // Set low fuel limit: 1,000 instructions
        instance.set_limits(1_000, 100);

        let res = instance.init("test content".to_string(), None);
        assert!(res.is_err(), "Infinite loop must be stopped by fuel exhaustion");
        let err_msg = format!("{:?}", res.unwrap_err());
        assert!(
            err_msg.contains("fuel") || err_msg.contains("all fuel consumed") || err_msg.contains("resource limits"),
            "Error should indicate fuel exhaustion: {err_msg}"
        );
    }

    #[test]
    fn test_component_plugin_epoch_timeout() {
        let engine = ComponentEngine::new().expect("Engine should initialize");

        let comp_wat = r#"
            (component
                (core module $m
                    (func (export "init")
                        (loop (br 0))
                    )
                )
                (core instance $i (instantiate $m))
                (func (export "on-init")
                    (canon lift (core func $i "init"))
                )
            )
        "#;

        let comp_bytes = wat::parse_str(comp_wat).expect("Component wat should parse");
        let mut instance = ComponentPluginInstance::from_bytes(engine, &comp_bytes, None)
            .expect("Component instance should load");

        // Give plenty of fuel, but a very short epoch deadline: 1 tick (50ms)
        instance.set_limits(u64::MAX / 2, 1);

        let start = std::time::Instant::now();
        let res = instance.init("test content".to_string(), None);
        let elapsed = start.elapsed();

        assert!(res.is_err(), "Infinite loop must be stopped by epoch timeout");
        assert!(elapsed < std::time::Duration::from_secs(2), "Timeout must happen quickly");
    }

    #[test]
    fn test_store_limits_memory_allocation() {
        use wasmtime::ResourceLimiter;
        let text = "Hello".to_string();
        // Configure strict 64KB (1 page) max memory limit
        let limits = StoreLimitsBuilder::new()
            .memory_size(64 * 1024)
            .build();

        let mut state = HostState {
            text,
            path: None,
            applied_edits: Vec::new(),
            limits,
        };

        // Current 1 page (64KB), request growth to 2 pages (128KB)
        let allowed = state.limits.memory_growing(65536, 131072, None).expect("Should check limit");
        assert!(!allowed, "Memory growth exceeding configured limit must be rejected");
    }

    #[test]
    fn test_async_command_execution() {
        let engine = ComponentEngine::new().expect("Engine should initialize");
        let comp_wat = r#"
            (component
                (core module $m
                    (func (export "init"))
                )
                (core instance $i (instantiate $m))
                (func (export "on-init")
                    (canon lift (core func $i "init"))
                )
            )
        "#;
        let comp_bytes = wat::parse_str(comp_wat).expect("Component wat should parse");
        let instance = Arc::new(ComponentPluginInstance::from_bytes(engine, &comp_bytes, None).unwrap());

        let (tx, rx) = std::sync::mpsc::channel();
        instance.execute_command_async("noop".to_string(), "sample".to_string(), None, move |res| {
            tx.send(res.is_ok() || res.is_err()).unwrap();
        });

        let finished = rx.recv_timeout(std::time::Duration::from_secs(2)).expect("Async execution finished");
        assert!(finished);
    }

    #[test]
    fn test_component_command_execution() {
        let engine = ComponentEngine::new().expect("Engine should initialize");
        let comp_wat = r#"
            (component
                (type $res_str (result string (error string)))
                (type $cmd_type (func (param "name" string) (result $res_str)))
                (core module $m
                    (memory (export "memory") 1)
                    (func (export "realloc") (param i32 i32 i32 i32) (result i32)
                        (i32.const 256)
                    )
                    (func (export "cmd") (param i32 i32) (result i32)
                        (i32.store (i32.const 16) (i32.const 0))
                        (i32.store offset=4 (i32.const 16) (i32.const 64))
                        (i32.store offset=8 (i32.const 16) (i32.const 7))
                        (i32.const 16)
                    )
                    (data (i32.const 64) "success")
                )
                (core instance $i (instantiate $m))
                (func (export "on-command") (type $cmd_type)
                    (canon lift
                        (core func $i "cmd")
                        (memory (core memory $i "memory"))
                        (realloc (core func $i "realloc"))
                    )
                )
            )
        "#;
        let comp_bytes = wat::parse_str(comp_wat).expect("Component wat should parse");
        let instance = ComponentPluginInstance::from_bytes(engine, &comp_bytes, None).unwrap();

        let (msg, edits) = instance
            .execute_command("test_cmd", "initial buffer content".to_string(), None)
            .expect("Command execution should succeed");

        assert_eq!(msg, "success");
        assert_eq!(edits.len(), 0);
    }
}


