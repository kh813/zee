use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use wasmi::{Caller, Engine, Func, Linker, Memory, Module, Store, TypedFunc};
use crate::outline::OutlineNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    Wasm,
    Lua,
    Theme,
    Syntax,
    Locale,
}

impl Default for PluginType {
    fn default() -> Self {
        PluginType::Wasm
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub plugin_type: PluginType,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    #[serde(default)]
    pub experimental: bool,
}

impl PluginManifest {
    pub fn is_experimental(&self) -> bool {
        self.experimental
            || self.name.to_lowercase().contains("experimental")
            || self.description.as_ref().map(|d| d.to_lowercase().contains("experimental")).unwrap_or(false)
            || self.id == "git-easy"
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCapabilities {
    #[serde(default)]
    pub outline_provider: bool,
    #[serde(default)]
    pub commands: Vec<String>,
}

pub struct WasmPlugin {
    pub manifest: PluginManifest,
    #[allow(dead_code)]
    engine: Engine,
    store: Store<()>,
    memory: Memory,
    alloc_fn: Option<TypedFunc<i32, i32>>,
    dealloc_fn: Option<TypedFunc<(i32, i32), ()>>,
    parse_outline_fn: Option<TypedFunc<(i32, i32), i64>>,
    exec_command_fn: Option<TypedFunc<(i32, i32, i32, i32), i64>>,
    transform_text_fn: Option<TypedFunc<(i32, i32, i32, i32), i64>>,
}

impl WasmPlugin {
    pub fn load_from_bytes(manifest: PluginManifest, wasm_bytes: &[u8]) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm_bytes).context("Failed to parse WASM module")?;
        let mut store = Store::new(&engine, ());
        let mut linker = Linker::new(&engine);

        // Define host functions imported by WASM
        linker.define(
            "env",
            "host_log",
            Func::wrap(&mut store, |_caller: Caller<()>, level: i32, _ptr: i32, _len: i32| {
                if level <= 1 {
                    eprintln!("[WASM Plugin Log level={}]", level);
                }
            }),
        )?;

        let instance = linker
            .instantiate(&mut store, &module)
            .context("Failed to instantiate WASM plugin")?
            .start(&mut store)
            .context("Failed to start WASM plugin instance")?;

        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| anyhow::anyhow!("Plugin missing 'memory' export"))?;

        // Look up functions with zee_* prefix first, falling back to led_*
        let alloc_fn = instance
            .get_typed_func::<i32, i32>(&store, "zee_alloc")
            .ok()
            .or_else(|| instance.get_typed_func::<i32, i32>(&store, "led_alloc").ok());

        let dealloc_fn = instance
            .get_typed_func::<(i32, i32), ()>(&store, "zee_dealloc")
            .ok()
            .or_else(|| instance.get_typed_func::<(i32, i32), ()>(&store, "led_dealloc").ok());

        let parse_outline_fn = instance
            .get_typed_func::<(i32, i32), i64>(&store, "zee_parse_outline")
            .ok()
            .or_else(|| instance.get_typed_func::<(i32, i32), i64>(&store, "led_parse_outline").ok());

        let exec_command_fn = instance
            .get_typed_func::<(i32, i32, i32, i32), i64>(&store, "zee_execute_command")
            .ok()
            .or_else(|| instance.get_typed_func::<(i32, i32, i32, i32), i64>(&store, "led_execute_command").ok());

        let transform_text_fn = instance
            .get_typed_func::<(i32, i32, i32, i32), i64>(&store, "zee_transform_text")
            .ok()
            .or_else(|| instance.get_typed_func::<(i32, i32, i32, i32), i64>(&store, "led_transform_text").ok());

        // Optional init hook
        if let Ok(init_fn) = instance
            .get_typed_func::<(), i32>(&store, "zee_init")
            .or_else(|_| instance.get_typed_func::<(), i32>(&store, "led_init"))
        {
            let _ = init_fn.call(&mut store, ());
        }

        Ok(Self {
            manifest,
            engine,
            store,
            memory,
            alloc_fn,
            dealloc_fn,
            parse_outline_fn,
            exec_command_fn,
            transform_text_fn,
        })
    }

    pub fn parse_outline(&mut self, content: &str) -> Result<Vec<OutlineNode>> {
        let parse_fn = self
            .parse_outline_fn
            .ok_or_else(|| anyhow::anyhow!("Plugin does not export zee_parse_outline"))?;

        let input = self.write_bytes(content.as_bytes())?;
        let packed = parse_fn.call(&mut self.store, input);
        self.dealloc(input);

        match self.take_output(packed?)? {
            Some(bytes) => serde_json::from_slice(&bytes)
                .context("Failed to deserialize OutlineNode list from WASM plugin"),
            None => Ok(Vec::new()),
        }
    }

    pub fn execute_command(&mut self, command: &str, args: &str) -> Result<String> {
        let f = self
            .exec_command_fn
            .ok_or_else(|| anyhow::anyhow!("Plugin does not export zee_execute_command"))?;
        self.call_string_fn(f, command, args)
    }

    pub fn transform_text(&mut self, command: &str, text: &str) -> Result<String> {
        let f = self
            .transform_text_fn
            .or(self.exec_command_fn)
            .ok_or_else(|| anyhow::anyhow!("Plugin does not export zee_transform_text or zee_execute_command"))?;
        self.call_string_fn(f, command, text)
    }

    /// Calls a `(cmd_ptr, cmd_len, arg_ptr, arg_len) -> packed` export and decodes the UTF-8 result.
    fn call_string_fn(
        &mut self,
        f: TypedFunc<(i32, i32, i32, i32), i64>,
        command: &str,
        arg: &str,
    ) -> Result<String> {
        let cmd = self.write_bytes(command.as_bytes())?;
        let arg = match self.write_bytes(arg.as_bytes()) {
            Ok(a) => a,
            Err(e) => {
                self.dealloc(cmd);
                return Err(e);
            }
        };
        let packed = f.call(&mut self.store, (cmd.0, cmd.1, arg.0, arg.1));
        self.dealloc(cmd);
        self.dealloc(arg);

        Ok(self
            .take_output(packed?)?
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default())
    }

    /// Allocates guest memory and copies `bytes` into it. Returns `(ptr, len)`.
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(i32, i32)> {
        let alloc_fn = self
            .alloc_fn
            .ok_or_else(|| anyhow::anyhow!("Plugin does not export zee_alloc"))?;
        let len = i32::try_from(bytes.len()).context("Input too large for WASM plugin")?;
        let ptr = alloc_fn.call(&mut self.store, len)?;
        if let Err(e) = self.memory.write(&mut self.store, ptr as usize, bytes) {
            self.dealloc((ptr, len));
            return Err(e.into());
        }
        Ok((ptr, len))
    }

    /// Decodes a packed `(ptr << 32) | len` result, copies it out and frees the guest buffer.
    fn take_output(&mut self, packed: i64) -> Result<Option<Vec<u8>>> {
        let packed = packed as u64;
        let ptr = (packed >> 32) as i32;
        let len = (packed & 0xFFFF_FFFF) as i32;
        if len <= 0 || ptr == 0 {
            return Ok(None);
        }
        let mut out = vec![0u8; len as usize];
        let read = self.memory.read(&self.store, ptr as usize, &mut out);
        self.dealloc((ptr, len));
        read?;
        Ok(Some(out))
    }

    fn dealloc(&mut self, (ptr, len): (i32, i32)) {
        if let Some(dealloc) = self.dealloc_fn {
            if ptr != 0 && len > 0 {
                let _ = dealloc.call(&mut self.store, (ptr, len));
            }
        }
    }
}

pub struct LuaPlugin {
    pub manifest: PluginManifest,
    lua: mlua::Lua,
}

impl LuaPlugin {
    pub fn load_from_str(manifest: PluginManifest, script: &str) -> Result<Self> {
        let lua = mlua::Lua::new();

        // Host environment exposed to Lua
        let zee_table = lua.create_table()?;
        zee_table.set("version", env!("CARGO_PKG_VERSION"))?;
        let log_fn = lua.create_function(|_, msg: String| {
            eprintln!("[Lua Plugin Log]: {}", msg);
            Ok(())
        })?;
        zee_table.set("log", log_fn)?;
        lua.globals().set("zee", zee_table)?;

        // Execute plugin script
        {
            let val: mlua::Value = lua.load(script).eval().context("Failed to evaluate Lua plugin script")?;
            if let mlua::Value::Table(tbl) = val {
                lua.globals().set("__plugin_table", tbl)?;
            }
        }

        Ok(Self { manifest, lua })
    }

    pub fn execute_command(&mut self, command: &str, args: &str) -> Result<String> {
        let globals = self.lua.globals();

        if let Ok(tbl) = globals.get::<_, mlua::Table>("__plugin_table") {
            if let Ok(func) = tbl.get::<_, mlua::Function>("execute_command") {
                let res: mlua::Value = func.call((command, args))?;
                return Self::value_to_string(res);
            }
            if let Ok(func) = tbl.get::<_, mlua::Function>(command) {
                let res: mlua::Value = func.call(args)?;
                return Self::value_to_string(res);
            }
        }

        if let Ok(func) = globals.get::<_, mlua::Function>("execute_command") {
            let res: mlua::Value = func.call((command, args))?;
            return Self::value_to_string(res);
        }

        if let Ok(func) = globals.get::<_, mlua::Function>(command) {
            let res: mlua::Value = func.call(args)?;
            return Self::value_to_string(res);
        }

        anyhow::bail!("Command '{}' not defined in Lua plugin '{}'", command, self.manifest.id)
    }

    pub fn transform_text(&mut self, command: &str, text: &str) -> Result<String> {
        let custom_res = {
            let globals = self.lua.globals();

            let mut out = None;
            if let Ok(tbl) = globals.get::<_, mlua::Table>("__plugin_table") {
                if let Ok(func) = tbl.get::<_, mlua::Function>("transform_text") {
                    if let Ok(res) = func.call::<_, mlua::Value>((command, text)) {
                        out = Some(Self::value_to_string(res));
                    }
                }
            }

            if out.is_none() {
                if let Ok(func) = globals.get::<_, mlua::Function>("transform_text") {
                    if let Ok(res) = func.call::<_, mlua::Value>((command, text)) {
                        out = Some(Self::value_to_string(res));
                    }
                }
            }
            out
        };

        if let Some(res) = custom_res {
            return res;
        }

        self.execute_command(command, text)
    }

    pub fn parse_outline(&mut self, content: &str) -> Result<Vec<OutlineNode>> {
        let globals = self.lua.globals();
        let func = if let Ok(tbl) = globals.get::<_, mlua::Table>("__plugin_table") {
            tbl.get::<_, mlua::Function>("parse_outline")
                .or_else(|_| globals.get::<_, mlua::Function>("parse_outline"))
        } else {
            globals.get::<_, mlua::Function>("parse_outline")
        };

        if let Ok(func) = func {
            let val: mlua::Value = func.call(content)?;
            match val {
                mlua::Value::String(s) => {
                    let json_str = s.to_str()?;
                    let nodes: Vec<OutlineNode> = serde_json::from_str(json_str)?;
                    Ok(nodes)
                }
                _ => Ok(Vec::new()),
            }
        } else {
            Ok(Vec::new())
        }
    }

    fn value_to_string(val: mlua::Value) -> Result<String> {
        match val {
            mlua::Value::String(s) => Ok(s.to_str()?.to_string()),
            mlua::Value::Nil => Ok(String::new()),
            mlua::Value::Integer(i) => Ok(i.to_string()),
            mlua::Value::Number(n) => Ok(n.to_string()),
            mlua::Value::Boolean(b) => Ok(b.to_string()),
            _ => Ok(String::new()),
        }
    }
}

pub const DEFAULT_REGISTRY_URL: &str = "https://raw.githubusercontent.com/kh813/zee-plugins/main/index.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "type", default)]
    pub plugin_type: PluginType,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    #[serde(default)]
    pub experimental: bool,
}

impl RegistryPlugin {
    pub fn is_experimental(&self) -> bool {
        self.experimental
            || self.name.to_lowercase().contains("experimental")
            || self.description.as_ref().map(|d| d.to_lowercase().contains("experimental")).unwrap_or(false)
            || self.id == "git-easy"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistryIndex {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub updated_at: Option<String>,
    pub plugins: Vec<RegistryPlugin>,
}

pub struct PluginManager {
    pub plugins: Vec<WasmPlugin>,
    pub component_plugins: Vec<(PluginManifest, crate::component_plugin::ComponentPluginInstance)>,
    pub lua_plugins: Vec<LuaPlugin>,
    component_engine: Option<crate::component_plugin::ComponentEngine>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::empty()
    }
}

impl PluginManager {
    pub fn empty() -> Self {
        Self {
            plugins: Vec::new(),
            component_plugins: Vec::new(),
            lua_plugins: Vec::new(),
            component_engine: None,
        }
    }

    pub fn new() -> Self {
        let mut manager = Self::empty();
        manager.load_installed_plugins();
        manager
    }

    pub fn plugins_dir() -> Option<PathBuf> {
        crate::config::Config::config_dir().map(|d| d.join("plugins"))
    }

    fn get_or_init_engine(&mut self) -> Result<crate::component_plugin::ComponentEngine> {
        if let Some(engine) = &self.component_engine {
            Ok(engine.clone())
        } else {
            let engine = crate::component_plugin::ComponentEngine::new()?;
            self.component_engine = Some(engine.clone());
            Ok(engine)
        }
    }

    pub fn all_manifests(&self) -> Vec<PluginManifest> {
        let mut list: Vec<PluginManifest> = self.plugins.iter().map(|p| p.manifest.clone()).collect();
        for (m, _) in &self.component_plugins {
            list.push(m.clone());
        }
        for p in &self.lua_plugins {
            list.push(p.manifest.clone());
        }

        // Custom Themes
        if let Some(themes_dir) = crate::config::Config::themes_dir() {
            if themes_dir.exists() {
                if let Ok(entries) = fs::read_dir(themes_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("toml") {
                            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("theme");
                            let (name, author) = if let Ok(content) = fs::read_to_string(&path) {
                                #[derive(Deserialize)]
                                struct SimpleMeta {
                                    #[serde(default)]
                                    name: String,
                                    author: Option<String>,
                                }
                                #[derive(Deserialize)]
                                struct SimpleThemeDoc {
                                    meta: Option<SimpleMeta>,
                                }
                                if let Ok(t) = toml::from_str::<SimpleThemeDoc>(&content) {
                                    if let Some(m) = t.meta {
                                        (
                                            if m.name.is_empty() { stem.to_string() } else { m.name },
                                            m.author,
                                        )
                                    } else {
                                        (stem.to_string(), None)
                                    }
                                } else {
                                    (stem.to_string(), None)
                                }
                            } else {
                                (stem.to_string(), None)
                            };
                            list.push(PluginManifest {
                                id: stem.to_string(),
                                name,
                                version: "1.0.0".to_string(),
                                description: Some("Color scheme theme".to_string()),
                                plugin_type: PluginType::Theme,
                                entry: path.file_name().map(|f| f.to_string_lossy().to_string()),
                                author,
                                homepage: None,
                                languages: Vec::new(),
                                capabilities: PluginCapabilities::default(),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }

        // Custom Syntax definitions
        if let Some(syntax_dir) = crate::config::Config::syntax_dir() {
            if syntax_dir.exists() {
                if let Ok(entries) = fs::read_dir(syntax_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("toml") {
                            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("syntax");
                            let (name, exts) = if let Ok(content) = fs::read_to_string(&path) {
                                #[derive(Deserialize)]
                                struct SimpleSyntaxMeta {
                                    #[serde(default)]
                                    name: String,
                                    #[serde(default)]
                                    extensions: Vec<String>,
                                }
                                #[derive(Deserialize)]
                                struct SimpleSyntaxDoc {
                                    meta: Option<SimpleSyntaxMeta>,
                                }
                                if let Ok(s) = toml::from_str::<SimpleSyntaxDoc>(&content) {
                                    if let Some(m) = s.meta {
                                        (
                                            if m.name.is_empty() { stem.to_string() } else { m.name },
                                            m.extensions,
                                        )
                                    } else {
                                        (stem.to_string(), Vec::new())
                                    }
                                } else {
                                    (stem.to_string(), Vec::new())
                                }
                            } else {
                                (stem.to_string(), Vec::new())
                            };
                            let desc = if exts.is_empty() {
                                "Syntax definition".to_string()
                            } else {
                                format!("Syntax definition for {}", exts.join(", "))
                            };
                            list.push(PluginManifest {
                                id: stem.to_string(),
                                name,
                                version: "1.0.0".to_string(),
                                description: Some(desc),
                                plugin_type: PluginType::Syntax,
                                entry: path.file_name().map(|f| f.to_string_lossy().to_string()),
                                author: None,
                                homepage: None,
                                languages: exts,
                                capabilities: PluginCapabilities::default(),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }

        // Custom Locales
        if let Some(locales_dir) = crate::config::Config::locales_dir() {
            if locales_dir.exists() {
                if let Ok(entries) = fs::read_dir(locales_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("toml") {
                            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("locale");
                            let name = if let Ok(content) = fs::read_to_string(&path) {
                                if let Ok(map) = crate::i18n::I18n::parse_locale_toml(&content) {
                                    map.get("meta.name")
                                        .or_else(|| map.get("language.name"))
                                        .cloned()
                                        .unwrap_or_else(|| format!("Language ({})", stem))
                                } else {
                                    format!("Language ({})", stem)
                                }
                            } else {
                                format!("Language ({})", stem)
                            };
                            list.push(PluginManifest {
                                id: stem.to_string(),
                                name,
                                version: "1.0.0".to_string(),
                                description: Some("UI and menu localization pack".to_string()),
                                plugin_type: PluginType::Locale,
                                entry: path.file_name().map(|f| f.to_string_lossy().to_string()),
                                author: None,
                                homepage: None,
                                languages: vec![stem.to_string()],
                                capabilities: PluginCapabilities::default(),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }

        list
    }

    /// Check if Git Easy plugin is installed and loaded
    pub fn has_git_easy(&self) -> bool {
        self.all_manifests().iter().any(|m| {
            m.id == "git-easy"
                || m.name == "Git Easy"
                || m.name.starts_with("Git Easy")
                || m.capabilities.commands.iter().any(|c| c.starts_with("git_easy_"))
        })
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty() && self.component_plugins.is_empty() && self.lua_plugins.is_empty()
    }

    pub fn load_installed_plugins(&mut self) {
        if let Some(plugins_dir) = Self::plugins_dir() {
            if plugins_dir.exists() {
                if let Ok(entries) = fs::read_dir(plugins_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let _ = self.load_plugin_dir(&path);
                        } else if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                            let _ = self.load_standalone_wasm(&path);
                        } else if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                            let _ = self.load_standalone_lua(&path);
                        }
                    }
                }
            }
        }
    }

    pub fn load_plugin_dir(&mut self, dir: &Path) -> Result<()> {
        let manifest_path = dir.join("plugin.toml");
        let wasm_path = dir.join("plugin.wasm");
        let lua_path = dir.join("init.lua");

        if !wasm_path.exists() && !lua_path.exists() {
            return Ok(());
        }

        let manifest: PluginManifest = if manifest_path.exists() {
            let manifest_str = fs::read_to_string(&manifest_path)?;
            toml::from_str(&manifest_str)?
        } else {
            let id = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unnamed_plugin".to_string());
            let ptype = if lua_path.exists() { PluginType::Lua } else { PluginType::Wasm };
            PluginManifest {
                id: id.clone(),
                name: id,
                version: "0.1.0".to_string(),
                description: None,
                plugin_type: ptype,
                entry: None,
                author: None,
                homepage: None,
                languages: Vec::new(),
                capabilities: PluginCapabilities {
                    outline_provider: true,
                    commands: Vec::new(),
                },
                ..Default::default()
            }
        };

        if manifest.plugin_type == PluginType::Lua || lua_path.exists() {
            let entry_file = manifest.entry.as_deref().unwrap_or("init.lua");
            let target_lua = dir.join(entry_file);
            if target_lua.exists() {
                let script = fs::read_to_string(&target_lua)?;
                let plugin = LuaPlugin::load_from_str(manifest, &script)?;
                self.lua_plugins.push(plugin);
                return Ok(());
            }
        }

        if wasm_path.exists() {
            let wasm_bytes = fs::read(&wasm_path)?;
            if let Ok(engine) = self.get_or_init_engine() {
                if let Ok(comp) = crate::component_plugin::ComponentPluginInstance::from_bytes(engine, &wasm_bytes, None) {
                    self.component_plugins.push((manifest, comp));
                    return Ok(());
                }
            }

            let plugin = WasmPlugin::load_from_bytes(manifest, &wasm_bytes)?;
            self.plugins.push(plugin);
            return Ok(());
        }

        Ok(())
    }

    pub fn load_standalone_wasm(&mut self, path: &Path) -> Result<()> {
        let id = path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string());

        let manifest = PluginManifest {
            id: id.clone(),
            name: id,
            version: "0.1.0".to_string(),
            description: None,
            plugin_type: PluginType::Wasm,
            entry: None,
            author: None,
            homepage: None,
            languages: Vec::new(),
            capabilities: PluginCapabilities {
                outline_provider: true,
                commands: Vec::new(),
            },
            ..Default::default()
        };

        let wasm_bytes = fs::read(path)?;
        if let Ok(engine) = self.get_or_init_engine() {
            if let Ok(comp) = crate::component_plugin::ComponentPluginInstance::from_bytes(engine, &wasm_bytes, None) {
                self.component_plugins.push((manifest, comp));
                return Ok(());
            }
        }

        let plugin = WasmPlugin::load_from_bytes(manifest, &wasm_bytes)?;
        self.plugins.push(plugin);
        Ok(())
    }

    pub fn load_standalone_lua(&mut self, path: &Path) -> Result<()> {
        let id = path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string());

        let script = fs::read_to_string(path)?;
        let manifest = PluginManifest {
            id: id.clone(),
            name: id,
            version: "0.1.0".to_string(),
            description: None,
            plugin_type: PluginType::Lua,
            entry: None,
            author: None,
            homepage: None,
            languages: Vec::new(),
            capabilities: PluginCapabilities {
                outline_provider: false,
                commands: Vec::new(),
            },
            ..Default::default()
        };

        let plugin = LuaPlugin::load_from_str(manifest, &script)?;
        self.lua_plugins.push(plugin);
        Ok(())
    }

    pub fn parse_outline(&mut self, lang_or_ext: &str, content: &str) -> Option<Vec<OutlineNode>> {
        let lang = lang_or_ext.to_lowercase();
        for (manifest, comp) in &self.component_plugins {
            if manifest.capabilities.outline_provider
                && (manifest.languages.is_empty()
                    || manifest
                        .languages
                        .iter()
                        .any(|l| l.to_lowercase() == lang))
            {
                if let Ok(nodes) = comp.get_outline(content.to_string(), None) {
                    if !nodes.is_empty() {
                        return Some(nodes);
                    }
                }
            }
        }

        for plugin in &mut self.plugins {
            if plugin.manifest.capabilities.outline_provider
                && (plugin.manifest.languages.is_empty()
                    || plugin
                        .manifest
                        .languages
                        .iter()
                        .any(|l| l.to_lowercase() == lang))
            {
                if let Ok(nodes) = plugin.parse_outline(content) {
                    if !nodes.is_empty() {
                        return Some(nodes);
                    }
                }
            }
        }

        for plugin in &mut self.lua_plugins {
            if plugin.manifest.capabilities.outline_provider
                && (plugin.manifest.languages.is_empty()
                    || plugin
                        .manifest
                        .languages
                        .iter()
                        .any(|l| l.to_lowercase() == lang))
            {
                if let Ok(nodes) = plugin.parse_outline(content) {
                    if !nodes.is_empty() {
                        return Some(nodes);
                    }
                }
            }
        }

        None
    }

    pub fn execute_command(&mut self, command: &str, args: &str) -> Option<String> {
        for plugin in &mut self.lua_plugins {
            if plugin.manifest.capabilities.commands.is_empty()
                || plugin.manifest.capabilities.commands.iter().any(|c| c == command)
            {
                if let Ok(res) = plugin.execute_command(command, args) {
                    return Some(res);
                }
            }
        }

        for (manifest, comp) in &self.component_plugins {
            if manifest.capabilities.commands.iter().any(|c| c == command) {
                if let Ok((res, _edits)) = comp.execute_command(command, args.to_string(), None) {
                    return Some(res);
                }
            }
        }

        for plugin in &mut self.plugins {
            if plugin.manifest.capabilities.commands.iter().any(|c| c == command) {
                if let Ok(res) = plugin.execute_command(command, args) {
                    return Some(res);
                }
            }
        }
        None
    }

    pub fn transform_text(&mut self, command: &str, text: &str) -> Option<String> {
        for plugin in &mut self.lua_plugins {
            if plugin.manifest.capabilities.commands.is_empty()
                || plugin.manifest.capabilities.commands.iter().any(|c| c == command)
            {
                if let Ok(res) = plugin.transform_text(command, text) {
                    return Some(res);
                }
            }
        }

        for (manifest, comp) in &self.component_plugins {
            if manifest.capabilities.commands.iter().any(|c| c == command) {
                if let Ok((res, _edits)) = comp.execute_command(command, text.to_string(), None) {
                    return Some(res);
                }
            }
        }

        for plugin in &mut self.plugins {
            if plugin.manifest.capabilities.commands.iter().any(|c| c == command) {
                if let Ok(res) = plugin.transform_text(command, text) {
                    return Some(res);
                }
            }
        }

        // Built-in fallback transforms when no plugin handled the command
        match command {
            "sort_lines" => {
                let mut lines: Vec<&str> = text.lines().collect();
                lines.sort();
                Some(lines.join("\n"))
            }
            "reverse_lines" => {
                let mut lines: Vec<&str> = text.lines().collect();
                lines.reverse();
                Some(lines.join("\n"))
            }
            "format_json" => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
                    Some(serde_json::to_string_pretty(&val).unwrap_or_else(|_| text.to_string()))
                } else {
                    Some(text.to_string())
                }
            }
            _ => None,
        }
    }

    pub fn install_plugin_from_path(src: &Path) -> Result<String> {
        let plugins_dir = Self::plugins_dir().context("Could not determine plugins directory")?;
        fs::create_dir_all(&plugins_dir)?;

        let filename = src.file_name().context("Invalid source filename")?;
        let filename_str = filename.to_string_lossy().to_string();

        if src.is_file() {
            if src.extension().and_then(|e| e.to_str()) == Some("toml") {
                if let Ok(content) = fs::read_to_string(src) {
                    let is_syntax = toml::from_str::<crate::syntax::SyntaxDefinition>(&content).is_ok()
                        || content.contains("[[rule]]")
                        || (content.contains("[meta]") && content.contains("extensions"));
                    let is_theme = toml::from_str::<crate::theme::Theme>(&content).is_ok()
                        || content.contains("[editor]")
                        || content.contains("[ui]")
                        || (content.contains("[meta]") && (content.contains("background") || content.contains("foreground") || content.contains("bg = ")));
                    let is_locale = content.contains("menu.") || content.contains("[menu") || content.contains("[dialog");

                    if is_theme {
                        let themes_dir = crate::config::Config::themes_dir().context("Could not determine themes directory")?;
                        fs::create_dir_all(&themes_dir)?;
                        let dest = themes_dir.join(filename);
                        fs::copy(src, &dest)?;
                        return Ok(filename_str);
                    } else if is_syntax {
                        let syntax_dir = crate::config::Config::syntax_dir().context("Could not determine syntax directory")?;
                        fs::create_dir_all(&syntax_dir)?;
                        let dest = syntax_dir.join(filename);
                        fs::copy(src, &dest)?;
                        return Ok(filename_str);
                    } else if is_locale {
                        let locales_dir = crate::config::Config::locales_dir().context("Could not determine locales directory")?;
                        fs::create_dir_all(&locales_dir)?;
                        let dest = locales_dir.join(filename);
                        fs::copy(src, &dest)?;
                        return Ok(filename_str);
                    }
                }
            }
            let dest = plugins_dir.join(filename);
            fs::copy(src, &dest)?;
            Ok(filename_str)
        } else if src.is_dir() {
            let dest_dir = plugins_dir.join(filename);
            fs::create_dir_all(&dest_dir)?;
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                let file_path = entry.path();
                if file_path.is_file() {
                    if let Some(name) = file_path.file_name() {
                        fs::copy(&file_path, dest_dir.join(name))?;
                    }
                }
            }
            Ok(filename_str)
        } else {
            Err(anyhow::anyhow!("Source path is neither a file nor a directory"))
        }
    }

    /// Normalize a repository URL (e.g. GitHub URL or raw JSON URL) to an index.json URL
    pub fn normalize_registry_index_url(repo_or_url: &str) -> String {
        let trimmed = repo_or_url.trim().trim_end_matches('/');
        if trimmed.ends_with(".json") {
            return trimmed.to_string();
        }
        // Handle https://github.com/owner/repo
        if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
            let clean = rest.trim_end_matches(".git").trim_matches('/');
            let parts: Vec<&str> = clean.split('/').collect();
            if parts.len() >= 2 {
                return format!("https://raw.githubusercontent.com/{}/{}/main/index.json", parts[0], parts[1]);
            }
        }
        // Handle http://github.com/owner/repo
        if let Some(rest) = trimmed.strip_prefix("http://github.com/") {
            let clean = rest.trim_end_matches(".git").trim_matches('/');
            let parts: Vec<&str> = clean.split('/').collect();
            if parts.len() >= 2 {
                return format!("https://raw.githubusercontent.com/{}/{}/main/index.json", parts[0], parts[1]);
            }
        }
        // Handle git@github.com:owner/repo
        if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
            let clean = rest.trim_end_matches(".git").trim_matches('/');
            let parts: Vec<&str> = clean.split('/').collect();
            if parts.len() >= 2 {
                return format!("https://raw.githubusercontent.com/{}/{}/main/index.json", parts[0], parts[1]);
            }
        }
        // Handle owner/repo shorthand (e.g. "kh813/zee-plugins")
        if !trimmed.contains("://") && !trimmed.contains(' ') {
            let parts: Vec<&str> = trimmed.split('/').collect();
            if parts.len() == 2 {
                return format!("https://raw.githubusercontent.com/{}/{}/main/index.json", parts[0], parts[1]);
            }
        }
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return format!("{}/index.json", trimmed);
        }
        trimmed.to_string()
    }

    /// Derive short repository display name (e.g. "kh813/zee-plugins" or domain/path)
    pub fn repo_display_name(url: &str) -> String {
        let trimmed = url.trim().trim_end_matches('/');
        if let Some(rest) = trimmed.strip_prefix("https://raw.githubusercontent.com/") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() >= 2 {
                return format!("{}/{}", parts[0], parts[1]);
            }
        }
        if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
            let clean = rest.trim_end_matches(".git").trim_matches('/');
            return clean.to_string();
        }
        if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
            let clean = rest.trim_end_matches(".git").trim_matches('/');
            return clean.to_string();
        }
        if let Some(rest) = trimmed.strip_prefix("https://") {
            return rest.to_string();
        }
        if let Some(rest) = trimmed.strip_prefix("http://") {
            return rest.to_string();
        }
        trimmed.to_string()
    }

    /// Derive plugin download base URL from index URL and plugin ID
    pub fn derive_plugin_base_url(index_url: &str, plugin_id: &str) -> String {
        if let Some(base) = index_url.strip_suffix("/index.json") {
            format!("{}/plugins/{}", base, plugin_id)
        } else if let Some(idx) = index_url.rfind('/') {
            format!("{}/plugins/{}", &index_url[..idx], plugin_id)
        } else {
            format!("https://raw.githubusercontent.com/kh813/zee-plugins/main/plugins/{}", plugin_id)
        }
    }

    /// Fetch the plugin catalog from a single registry URL
    pub fn fetch_registry(registry_url: Option<&str>) -> Result<PluginRegistryIndex> {
        let raw_url = registry_url.unwrap_or(DEFAULT_REGISTRY_URL);
        let normalized = Self::normalize_registry_index_url(raw_url);
        let resp = ureq::get(&normalized)
            .set("User-Agent", "zee-editor")
            .timeout(std::time::Duration::from_secs(10))
            .call()
            .context(format!("Failed to fetch plugin registry from {}", normalized))?;

        let index: PluginRegistryIndex = resp.into_json()
            .context("Failed to parse registry index JSON")?;
        Ok(index)
    }

    /// Fetch registry indices from multiple URLs and combine the results.
    pub fn fetch_registries(registry_sources: &[String]) -> Result<Vec<RegistryPlugin>> {
        let mut all_plugins = Vec::new();
        let mut errors = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        let sources: Vec<String> = if registry_sources.is_empty() {
            vec![DEFAULT_REGISTRY_URL.to_string()]
        } else {
            registry_sources.to_vec()
        };

        for source in &sources {
            let index_url = Self::normalize_registry_index_url(source);
            let display_repo = Self::repo_display_name(source);
            match Self::fetch_registry(Some(&index_url)) {
                Ok(index) => {
                    for mut plugin in index.plugins {
                        plugin.repository = Some(display_repo.clone());
                        if plugin.download_url.is_none() {
                            plugin.download_url = Some(Self::derive_plugin_base_url(&index_url, &plugin.id));
                        }
                        if seen_ids.insert(plugin.id.clone()) {
                            all_plugins.push(plugin);
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!("{}: {}", display_repo, e));
                }
            }
        }

        if all_plugins.is_empty() && !errors.is_empty() {
            return Err(anyhow::anyhow!("Failed to fetch plugins:\n{}", errors.join("\n")));
        }

        Ok(all_plugins)
    }

    /// Compare two version strings. Returns true if `candidate` is strictly newer than `installed`.
    /// Supports SemVer (e.g. 0.1.0 vs 0.2.0) and CalVer / Date-based (e.g. 20261025.1 vs 20261025.2).
    pub fn is_newer_version(installed: &str, candidate: &str) -> bool {
        fn clean(s: &str) -> &str {
            s.trim().trim_start_matches(|c| c == 'v' || c == 'V')
        }
        let parse_parts = |s: &str| -> Vec<u64> {
            clean(s)
                .split(|c: char| c == '.' || c == '-' || c == '_')
                .filter(|p| !p.is_empty())
                .filter_map(|p| p.parse::<u64>().ok())
                .collect()
        };

        let inst_parts = parse_parts(installed);
        let cand_parts = parse_parts(candidate);

        if !inst_parts.is_empty() && !cand_parts.is_empty() {
            let max_len = inst_parts.len().max(cand_parts.len());
            for i in 0..max_len {
                let inst = inst_parts.get(i).copied().unwrap_or(0);
                let cand = cand_parts.get(i).copied().unwrap_or(0);
                if cand > inst {
                    return true;
                } else if cand < inst {
                    return false;
                }
            }
            false
        } else {
            // Fallback to lexicographical comparison
            clean(candidate) > clean(installed)
        }
    }

    /// Install a plugin by ID from online registries
    pub fn install_from_registry(
        plugin_id: &str,
        registry_url: Option<&str>,
        configured_sources: Option<&[String]>,
    ) -> Result<()> {
        let target_plugin = if let Some(url) = registry_url {
            let index_url = Self::normalize_registry_index_url(url);
            let registry = Self::fetch_registry(Some(&index_url))?;
            let mut found = registry.plugins.into_iter().find(|p| p.id == plugin_id)
                .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found in registry", plugin_id))?;
            if found.download_url.is_none() {
                found.download_url = Some(Self::derive_plugin_base_url(&index_url, &found.id));
            }
            found
        } else {
            let default_sources = crate::config::default_plugin_registries();
            let sources = configured_sources.unwrap_or(&default_sources);
            let plugins = Self::fetch_registries(sources)?;
            plugins.into_iter().find(|p| p.id == plugin_id)
                .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found in configured registries", plugin_id))?
        };

        match target_plugin.plugin_type {
            PluginType::Theme => {
                let themes_dir = crate::config::Config::themes_dir()
                    .context("Could not determine themes directory")?;
                fs::create_dir_all(&themes_dir)?;

                let filename = target_plugin.entry.clone().unwrap_or_else(|| format!("{}.toml", target_plugin.id));
                let dest = themes_dir.join(&filename);

                let file_url = if let Some(ref dl) = target_plugin.download_url {
                    if dl.ends_with(".toml") {
                        dl.clone()
                    } else {
                        format!("{}/{}", dl.trim_end_matches('/'), filename)
                    }
                } else {
                    format!("https://raw.githubusercontent.com/kh813/zee-plugins/main/themes/{}", filename)
                };

                let resp = ureq::get(&file_url)
                    .set("User-Agent", "zee-editor")
                    .timeout(std::time::Duration::from_secs(15))
                    .call()
                    .context(format!("Failed to download theme file from '{}'", file_url))?;
                let mut reader = resp.into_reader();
                let mut file = fs::File::create(&dest)?;
                std::io::copy(&mut reader, &mut file)?;
                Ok(())
            }
            PluginType::Syntax => {
                let syntax_dir = crate::config::Config::syntax_dir()
                    .context("Could not determine syntax directory")?;
                fs::create_dir_all(&syntax_dir)?;

                let filename = target_plugin.entry.clone().unwrap_or_else(|| format!("{}.toml", target_plugin.id));
                let dest = syntax_dir.join(&filename);

                let file_url = if let Some(ref dl) = target_plugin.download_url {
                    if dl.ends_with(".toml") {
                        dl.clone()
                    } else {
                        format!("{}/{}", dl.trim_end_matches('/'), filename)
                    }
                } else {
                    format!("https://raw.githubusercontent.com/kh813/zee-plugins/main/syntax/{}", filename)
                };

                let resp = ureq::get(&file_url)
                    .set("User-Agent", "zee-editor")
                    .timeout(std::time::Duration::from_secs(15))
                    .call()
                    .context(format!("Failed to download syntax file from '{}'", file_url))?;
                let mut reader = resp.into_reader();
                let mut file = fs::File::create(&dest)?;
                std::io::copy(&mut reader, &mut file)?;
                Ok(())
            }
            PluginType::Locale => {
                let locales_dir = crate::config::Config::locales_dir()
                    .context("Could not determine locales directory")?;
                fs::create_dir_all(&locales_dir)?;

                let filename = target_plugin.entry.clone().unwrap_or_else(|| format!("{}.toml", target_plugin.id));
                let dest = locales_dir.join(&filename);

                let file_url = if let Some(ref dl) = target_plugin.download_url {
                    if dl.ends_with(".toml") {
                        dl.clone()
                    } else {
                        format!("{}/{}", dl.trim_end_matches('/'), filename)
                    }
                } else {
                    format!("https://raw.githubusercontent.com/kh813/zee-plugins/main/locales/{}", filename)
                };

                let resp = ureq::get(&file_url)
                    .set("User-Agent", "zee-editor")
                    .timeout(std::time::Duration::from_secs(15))
                    .call()
                    .context(format!("Failed to download locale file from '{}'", file_url))?;
                let mut reader = resp.into_reader();
                let mut file = fs::File::create(&dest)?;
                std::io::copy(&mut reader, &mut file)?;
                Ok(())
            }
            PluginType::Wasm | PluginType::Lua => {
                let plugins_dir = Self::plugins_dir().context("Could not determine plugins directory")?;
                let target_dir = plugins_dir.join(&target_plugin.id);
                fs::create_dir_all(&target_dir)?;

                let base_download_url = target_plugin.download_url
                    .unwrap_or_else(|| format!("https://raw.githubusercontent.com/kh813/zee-plugins/main/plugins/{}", target_plugin.id));

                // 1. Download plugin.toml
                let toml_url = format!("{}/plugin.toml", base_download_url.trim_end_matches('/'));
                if let Ok(resp) = ureq::get(&toml_url).set("User-Agent", "zee-editor").timeout(std::time::Duration::from_secs(10)).call() {
                    let mut reader = resp.into_reader();
                    let mut file = fs::File::create(target_dir.join("plugin.toml"))?;
                    std::io::copy(&mut reader, &mut file)?;
                }

                // 2. Download entry file based on type
                let entry_filename = target_plugin.entry.clone().unwrap_or_else(|| match target_plugin.plugin_type {
                    PluginType::Lua => "init.lua".to_string(),
                    _ => "plugin.wasm".to_string(),
                });

                let entry_url = format!("{}/{}", base_download_url.trim_end_matches('/'), entry_filename);
                let resp = ureq::get(&entry_url)
                    .set("User-Agent", "zee-editor")
                    .timeout(std::time::Duration::from_secs(15))
                    .call()
                    .context(format!("Failed to download plugin entry file '{}'", entry_filename))?;
                let mut reader = resp.into_reader();
                let mut file = fs::File::create(target_dir.join(&entry_filename))?;
                std::io::copy(&mut reader, &mut file)?;

                Ok(())
            }
        }
    }

    pub fn uninstall_plugin_by_id(id: &str) -> Result<bool> {
        let mut removed = false;

        // 1. Check plugins_dir
        if let Some(plugins_dir) = Self::plugins_dir() {
            if plugins_dir.exists() {
                let wasm_file = plugins_dir.join(format!("{}.wasm", id));
                if wasm_file.exists() {
                    let _ = fs::remove_file(&wasm_file);
                    removed = true;
                }

                let lua_file = plugins_dir.join(format!("{}.lua", id));
                if lua_file.exists() {
                    let _ = fs::remove_file(&lua_file);
                    removed = true;
                }

                let plugin_dir = plugins_dir.join(id);
                if plugin_dir.exists() {
                    let _ = fs::remove_dir_all(&plugin_dir);
                    removed = true;
                }

                if let Ok(entries) = fs::read_dir(&plugins_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let toml_path = path.join("plugin.toml");
                            if toml_path.exists() {
                                if let Ok(content) = fs::read_to_string(&toml_path) {
                                    if let Ok(manifest) = toml::from_str::<PluginManifest>(&content) {
                                        if manifest.id == id {
                                            let _ = fs::remove_dir_all(&path);
                                            removed = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Check themes_dir
        if let Some(themes_dir) = crate::config::Config::themes_dir() {
            if themes_dir.exists() {
                let direct_file = themes_dir.join(format!("{}.toml", id));
                if direct_file.exists() {
                    let _ = fs::remove_file(&direct_file);
                    removed = true;
                }
                if let Ok(entries) = fs::read_dir(&themes_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("toml") {
                            if let Ok(content) = fs::read_to_string(&path) {
                                if let Ok(theme) = toml::from_str::<crate::theme::Theme>(&content) {
                                    if theme.meta.name.eq_ignore_ascii_case(id) || theme.meta.name.replace(' ', "-").eq_ignore_ascii_case(id) {
                                        let _ = fs::remove_file(&path);
                                        removed = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Check syntax_dir
        if let Some(syntax_dir) = crate::config::Config::syntax_dir() {
            if syntax_dir.exists() {
                let direct_file = syntax_dir.join(format!("{}.toml", id));
                if direct_file.exists() {
                    let _ = fs::remove_file(&direct_file);
                    removed = true;
                }
                if let Ok(entries) = fs::read_dir(&syntax_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("toml") {
                            if let Ok(content) = fs::read_to_string(&path) {
                                if let Ok(syntax) = toml::from_str::<crate::syntax::SyntaxDefinition>(&content) {
                                    if syntax.meta.name.eq_ignore_ascii_case(id) {
                                        let _ = fs::remove_file(&path);
                                        removed = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4. Check locales_dir
        if let Some(locales_dir) = crate::config::Config::locales_dir() {
            if locales_dir.exists() {
                let direct_file = locales_dir.join(format!("{}.toml", id));
                if direct_file.exists() {
                    let _ = fs::remove_file(&direct_file);
                    removed = true;
                }
            }
        }

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manager_creation() {
        let manager = PluginManager::new();
        assert!(manager.plugins.is_empty() || !manager.plugins.is_empty());
    }

    #[test]
    fn test_plugin_manifest_deserialization() {
        let toml_str = r#"
id = "markdown-outline"
name = "Markdown Outline"
version = "0.1.0"
description = "Extracts headings from markdown"
languages = ["markdown", "md"]

[capabilities]
outline_provider = true
commands = ["outline.refresh"]
"#;
        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.id, "markdown-outline");
        assert_eq!(manifest.languages, vec!["markdown", "md"]);
        assert!(manifest.capabilities.outline_provider);
        assert_eq!(manifest.capabilities.commands, vec!["outline.refresh"]);
    }

    #[test]
    fn test_load_and_run_wasm_plugin() {
        let plugin_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("plugins/zee-plugin-text");
        
        let wasm_file = plugin_dir.join("plugin.wasm");
        if !wasm_file.exists() {
            return;
        }

        let mut manager = PluginManager::default();
        manager.load_plugin_dir(&plugin_dir).unwrap();
        assert_eq!(manager.plugins.len(), 1);

        // Test outline extraction via WASM
        let md_content = "# Chapter 1\nContent 1\n## Sub 1.1\nContent 1.1\n# Chapter 2\n";
        let outline = manager.parse_outline("markdown", md_content).unwrap();
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0].title, "Chapter 1");
        assert_eq!(outline[0].children.len(), 1);
        assert_eq!(outline[0].children[0].title, "Sub 1.1");
        assert_eq!(outline[1].title, "Chapter 2");

        // Test text transformations
        // 1. JSON formatting
        let unformatted_json = r#"{"name":"zee","fast":true,"version":"0.1.1"}"#;
        let formatted = manager.transform_text("format_json", unformatted_json).unwrap();
        assert!(formatted.contains("\n  \"name\": \"zee\""));

        // 2. Line sorting
        let unsorted = "zebra\napple\ncat\nbanana";
        let sorted = manager.transform_text("sort_lines", unsorted).unwrap();
        assert_eq!(sorted, "apple\nbanana\ncat\nzebra");

        // 3. Case conversion
        let camel = manager.transform_text("to_camel_case", "hello_world_text").unwrap();
        assert_eq!(camel, "helloWorldText");

        let snake = manager.transform_text("to_snake_case", "HelloWorldText").unwrap();
        assert_eq!(snake, "hello_world_text");
    }

    #[test]
    fn test_load_and_run_lua_plugin() {
        let manifest = PluginManifest {
            id: "test-lua".to_string(),
            name: "Test Lua Plugin".to_string(),
            version: "0.1.0".to_string(),
            description: Some("Lua test plugin".to_string()),
            plugin_type: PluginType::Lua,
            entry: Some("init.lua".to_string()),
            author: Some("kh813".to_string()),
            homepage: None,
            languages: vec!["*".to_string()],
            capabilities: PluginCapabilities {
                outline_provider: false,
                commands: vec!["custom_reverse".to_string(), "custom_shout".to_string()],
            },
            ..Default::default()
        };

        let lua_script = r#"
            local M = {}
            function M.execute_command(cmd, text)
                if cmd == "custom_reverse" then
                    return string.reverse(text)
                elseif cmd == "custom_shout" then
                    return string.upper(text) .. "!!!"
                end
                return text
            end
            return M
        "#;

        let mut plugin = LuaPlugin::load_from_str(manifest, lua_script).unwrap();
        let res1 = plugin.execute_command("custom_reverse", "hello world").unwrap();
        assert_eq!(res1, "dlrow olleh");

        let res2 = plugin.transform_text("custom_shout", "zee editor").unwrap();
        assert_eq!(res2, "ZEE EDITOR!!!");
    }

    #[test]
    fn test_lua_plugin_directory_workflow() {
        let _lock = crate::config::TEST_MUTEX.lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!("zee_test_lua_dir_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let toml_content = r#"
id = "sample-lorem"
name = "Sample Lorem"
version = "0.1.0"
type = "lua"
entry = "init.lua"

[capabilities]
commands = ["lorem"]
"#;
        fs::write(temp_dir.join("plugin.toml"), toml_content).unwrap();

        let lua_content = r#"
            local M = {}
            function M.execute_command(cmd, text)
                if cmd == "lorem" then
                    return "Lorem ipsum dolor sit amet."
                end
                return text
            end
            return M
        "#;
        fs::write(temp_dir.join("init.lua"), lua_content).unwrap();

        let mut manager = PluginManager::default();
        manager.load_plugin_dir(&temp_dir).unwrap();
        assert_eq!(manager.lua_plugins.len(), 1);
        assert!(manager.all_manifests().iter().any(|m| m.id == "sample-lorem"));

        let res = manager.execute_command("lorem", "").unwrap();
        assert_eq!(res, "Lorem ipsum dolor sit amet.");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_registry_index_deserialization() {
        let index_json = r#"{
            "version": 1,
            "plugins": [
                {
                    "id": "case-converter",
                    "name": "Case Converter",
                    "version": "0.1.0",
                    "type": "lua",
                    "author": "kh813",
                    "description": "Convert words and text selections.",
                    "languages": ["*"],
                    "capabilities": {
                        "outline_provider": false,
                        "commands": ["to_camel_case", "to_snake_case"]
                    }
                }
            ]
        }"#;

        let index: PluginRegistryIndex = serde_json::from_str(index_json).unwrap();
        assert_eq!(index.plugins.len(), 1);
        let p = &index.plugins[0];
        assert_eq!(p.id, "case-converter");
        assert_eq!(p.plugin_type, PluginType::Lua);
        assert_eq!(p.capabilities.commands.len(), 2);
    }

    #[test]
    fn test_multi_registry_url_helpers() {
        // Normalize GitHub URLs
        assert_eq!(
            PluginManager::normalize_registry_index_url("https://github.com/kh813/zee-plugins"),
            "https://raw.githubusercontent.com/kh813/zee-plugins/main/index.json"
        );
        assert_eq!(
            PluginManager::normalize_registry_index_url("https://github.com/user/custom-repo.git/"),
            "https://raw.githubusercontent.com/user/custom-repo/main/index.json"
        );
        assert_eq!(
            PluginManager::normalize_registry_index_url("git@github.com:team/plugins.git"),
            "https://raw.githubusercontent.com/team/plugins/main/index.json"
        );
        assert_eq!(
            PluginManager::normalize_registry_index_url("community/zee-plugins"),
            "https://raw.githubusercontent.com/community/zee-plugins/main/index.json"
        );
        assert_eq!(
            PluginManager::normalize_registry_index_url("https://raw.githubusercontent.com/kh813/zee-plugins/main/index.json"),
            "https://raw.githubusercontent.com/kh813/zee-plugins/main/index.json"
        );
        assert_eq!(
            PluginManager::normalize_registry_index_url("https://example.com/custom/registry.json"),
            "https://example.com/custom/registry.json"
        );

        // Repo display name
        assert_eq!(
            PluginManager::repo_display_name("https://github.com/kh813/zee-plugins"),
            "kh813/zee-plugins"
        );
        assert_eq!(
            PluginManager::repo_display_name("https://raw.githubusercontent.com/kh813/zee-plugins/main/index.json"),
            "kh813/zee-plugins"
        );

        // Derive plugin download base URL
        assert_eq!(
            PluginManager::derive_plugin_base_url("https://raw.githubusercontent.com/user/repo/main/index.json", "my-plugin"),
            "https://raw.githubusercontent.com/user/repo/main/plugins/my-plugin"
        );
    }

    #[test]
    fn test_theme_syntax_locale_plugin_types_deserialization() {
        let index_json = r#"{
            "version": 1,
            "plugins": [
                {
                    "id": "my-theme",
                    "name": "My Theme",
                    "version": "1.0.0",
                    "type": "theme"
                },
                {
                    "id": "my-syntax",
                    "name": "My Syntax",
                    "version": "1.0.0",
                    "type": "syntax"
                },
                {
                    "id": "my-locale",
                    "name": "My Locale",
                    "version": "1.0.0",
                    "type": "locale"
                }
            ]
        }"#;

        let index: PluginRegistryIndex = serde_json::from_str(index_json).unwrap();
        assert_eq!(index.plugins.len(), 3);
        assert_eq!(index.plugins[0].plugin_type, PluginType::Theme);
        assert_eq!(index.plugins[1].plugin_type, PluginType::Syntax);
        assert_eq!(index.plugins[2].plugin_type, PluginType::Locale);
    }

    #[test]
    fn test_install_and_uninstall_theme_syntax_locale() {
        let _lock = crate::config::TEST_MUTEX.lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!("zee_test_plugin_types_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        crate::config::Config::set_custom_config_dir(Some(temp_dir.clone()));

        // 1. Create a dummy theme toml
        let theme_file = temp_dir.join("test_custom_theme.toml");
        fs::write(&theme_file, r##"
[meta]
name = "Test Custom Theme"
author = "Tester"

[editor]
background = "#112233"
foreground = "#ffffff"
"##).unwrap();

        // 2. Install theme from path
        let res = PluginManager::install_plugin_from_path(&theme_file);
        assert!(res.is_ok());

        let themes_dir = crate::config::Config::themes_dir().unwrap();
        assert!(themes_dir.join("test_custom_theme.toml").exists());

        // 3. Create dummy syntax toml
        let syntax_file = temp_dir.join("test_custom_syntax.toml");
        fs::write(&syntax_file, r#"
[meta]
name = "MyLang"
extensions = ["mylang"]

[[rule]]
token = "keyword"
pattern = "def|fn"
"#).unwrap();

        let s_res = PluginManager::install_plugin_from_path(&syntax_file);
        assert!(s_res.is_ok());

        let syntax_dir = crate::config::Config::syntax_dir().unwrap();
        assert!(syntax_dir.join("test_custom_syntax.toml").exists());

        // 4. Create dummy locale toml
        let locale_file = temp_dir.join("eo.toml");
        fs::write(&locale_file, r#"
[meta]
name = "Esperanto"

[menu.file]
new = "Nova"
"#).unwrap();

        let l_res = PluginManager::install_plugin_from_path(&locale_file);
        assert!(l_res.is_ok());

        let locales_dir = crate::config::Config::locales_dir().unwrap();
        assert!(locales_dir.join("eo.toml").exists());

        // 5. Check all_manifests
        let pm = PluginManager::empty();
        let manifests = pm.all_manifests();
        assert!(manifests.iter().any(|m| m.id == "test_custom_theme" && m.plugin_type == PluginType::Theme));
        assert!(manifests.iter().any(|m| m.id == "test_custom_syntax" && m.plugin_type == PluginType::Syntax));
        assert!(manifests.iter().any(|m| m.id == "eo" && m.plugin_type == PluginType::Locale));

        // 6. Test SyntaxDefinition::load_all includes custom syntax
        let all_syntaxes = crate::syntax::SyntaxDefinition::load_all();
        assert!(all_syntaxes.iter().any(|s| s.meta.name == "MyLang"));

        // 7. Test I18n::all_available_languages includes Esperanto
        let all_langs = crate::i18n::I18n::all_available_languages();
        assert!(all_langs.iter().any(|l| l.id == "eo" && l.name == "Esperanto"));

        // 8. Uninstall
        assert!(PluginManager::uninstall_plugin_by_id("test_custom_theme").unwrap());
        assert!(!themes_dir.join("test_custom_theme.toml").exists());

        assert!(PluginManager::uninstall_plugin_by_id("test_custom_syntax").unwrap());
        assert!(!syntax_dir.join("test_custom_syntax.toml").exists());

        assert!(PluginManager::uninstall_plugin_by_id("eo").unwrap());
        assert!(!locales_dir.join("eo.toml").exists());

        crate::config::Config::set_custom_config_dir(None);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_is_newer_version() {
        // SemVer comparisons
        assert!(PluginManager::is_newer_version("0.1.0", "0.2.0"));
        assert!(PluginManager::is_newer_version("0.1.9", "0.2.0"));
        assert!(PluginManager::is_newer_version("0.2.0", "0.2.1"));
        assert!(!PluginManager::is_newer_version("0.2.0", "0.2.0"));
        assert!(!PluginManager::is_newer_version("0.2.0", "0.1.9"));

        // CalVer / Date-based comparisons
        assert!(PluginManager::is_newer_version("20261024.1", "20261025.1"));
        assert!(PluginManager::is_newer_version("20261025.1", "20261025.2"));
        assert!(PluginManager::is_newer_version("20261025.99", "20261026.1"));
        assert!(!PluginManager::is_newer_version("20261025.2", "20261025.1"));

        // SemVer to CalVer transition
        assert!(PluginManager::is_newer_version("0.2.0", "20261025.1"));
        assert!(!PluginManager::is_newer_version("20261025.1", "0.2.0"));

        // Prefixed with 'v'
        assert!(PluginManager::is_newer_version("v0.1.0", "v0.2.0"));
        assert!(PluginManager::is_newer_version("v20261025.1", "20261025.2"));
    }

    #[test]
    fn test_git_easy_plugin_real_load() {
        let mut pm = PluginManager::new();
        let plugin_dir = PathBuf::from("/Users/hiroshi/dev/zee-plugins/plugins/git-easy");
        if plugin_dir.exists() {
            pm.load_plugin_dir(&plugin_dir).expect("Failed to load git-easy plugin");
            let manifests = pm.all_manifests();
            let ge = manifests.iter().find(|m| m.id == "git-easy").expect("git-easy manifest not found");
            assert!(ge.name.starts_with("Git Easy"));
            assert!(!ge.version.is_empty());
            assert!(ge.capabilities.commands.len() >= 9);
            assert!(pm.has_git_easy());
            assert!(ge.capabilities.commands.contains(&"git_easy_snapshot".to_string()));
        } else {
            // Self-contained test for CI / environments without local dev checkout
            let temp_dir = std::env::temp_dir().join(format!("zee_test_git_easy_{}", std::process::id()));
            let _ = fs::remove_dir_all(&temp_dir);
            fs::create_dir_all(&temp_dir).unwrap();

            let plugin_toml = r#"
id = "git-easy"
name = "Git Easy"
version = "20261025.1"
description = "Git operations for markdown notes"
type = "lua"

[capabilities]
commands = [
    "git_easy_snapshot",
    "git_easy_new_draft",
    "git_easy_merge_draft",
    "git_easy_switch_draft",
    "git_easy_restore_latest",
    "git_easy_add_file",
    "git_easy_ignore_file",
    "git_easy_history",
    "git_easy_status"
]
"#;
            fs::write(temp_dir.join("plugin.toml"), plugin_toml).unwrap();
            fs::write(temp_dir.join("init.lua"), "local M = {}\nreturn M").unwrap();
            pm.load_plugin_dir(&temp_dir).expect("Failed to load mock git-easy plugin");

            let manifests = pm.all_manifests();
            let ge = manifests.iter().find(|m| m.id == "git-easy").expect("git-easy manifest not found");
            assert!(ge.name.starts_with("Git Easy"));
            assert!(!ge.version.is_empty());
            assert!(ge.capabilities.commands.len() >= 9);
            assert!(pm.has_git_easy());
            assert!(ge.capabilities.commands.contains(&"git_easy_snapshot".to_string()));

            let _ = fs::remove_dir_all(&temp_dir);
        }
    }
}

