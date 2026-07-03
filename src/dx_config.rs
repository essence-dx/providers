use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct RawDxConfig {
    workspace: WorkspaceConfig,
    paths: PathsConfig,
}

impl RawDxConfig {
    fn load(cwd: &Path) -> Self {
        Self::discover(cwd).unwrap_or_default()
    }

    fn discover(root: &Path) -> Option<Self> {
        for ancestor in root.ancestors() {
            let candidate = ancestor.join("dx");
            if candidate.is_file() && !looks_like_project_config(&candidate) {
                return Self::from_path(&candidate).ok();
            }
        }
        None
    }

    fn from_path(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let source = std::fs::read_to_string(path)?;
        let mut config: Self = toml::from_str(&source)?;
        let base = path.parent().unwrap_or(Path::new("."));
        config.absolutize(base);
        Ok(config)
    }

    fn absolutize(&mut self, base: &Path) {
        let root = abs_path(base, &self.workspace.root);
        self.workspace.root = root;
    }
}

fn abs_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() { path.to_path_buf() } else { root.join(path) }
}

fn looks_like_project_config(path: &Path) -> bool {
    let Ok(source) = std::fs::read_to_string(path) else { return false };
    for line in source.lines() {
        let s = line.trim().trim_start_matches('\u{feff}');
        if s.is_empty() || s.starts_with('#') { continue; }
        if s.starts_with("project(") || s.starts_with("contract(") || 
           s.starts_with("runtime(") || s.starts_with("www(") { return true; }
        if s.contains('[') && s.contains('(') { return true; }
        break;
    }
    false
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct WorkspaceConfig { name: String, root: PathBuf }
impl Default for WorkspaceConfig {
    fn default() -> Self { Self { name: "DX".to_string(), root: PathBuf::from(".") } }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct PathsConfig { }
impl Default for PathsConfig { fn default() -> Self { Self { } } }

pub struct ProvidersDxConfig {
    pub workspace_root: PathBuf,
    pub sr_dir: PathBuf,
    pub receipts_dir: PathBuf,
}

impl ProvidersDxConfig {
    pub fn load() -> Self {
        let raw = RawDxConfig::load(&std::env::current_dir().unwrap_or_default());
        let ws = raw.workspace.root;
        let sr = ws.join(".dx").join("serializer");
        let receipts = ws.join(".dx").join("receipts").join("providers");
        Self { workspace_root: ws, sr_dir: sr, receipts_dir: receipts }
    }

    pub fn sr_path(&self, name: &str) -> PathBuf {
        self.sr_dir.join(format!("{}.sr", name))
    }

    pub fn global_sr_dir(&self) -> PathBuf {
        dirs::cache_dir()
            .map(|b| b.join("dx").join("providers"))
            .unwrap_or_else(|| PathBuf::from("~/.cache/dx/providers"))
    }

    pub fn write_global_sr(&self, name: &str, entries: &[(&str, &str)]) -> std::io::Result<()> {
        let dir = self.global_sr_dir();
        let path = dir.join(format!("{}.sr", name));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut buf: Vec<u8> = Vec::new();
        for (key, value) in entries {
            write!(buf, "{key}=")?;
            Self::write_llm_value(&mut buf, value)?;
            buf.push(b'\n');
        }
        let tmp = path.with_extension("sr.tmp");
        std::fs::write(&tmp, &buf)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn write_sr(&self, name: &str, entries: &[(&str, &str)]) -> std::io::Result<()> {
        let path = self.sr_path(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut buf: Vec<u8> = Vec::new();
        for (key, value) in entries {
            write!(buf, "{key}=")?;
            Self::write_llm_value(&mut buf, value)?;
            buf.push(b'\n');
        }
        let tmp = path.with_extension("sr.tmp");
        std::fs::write(&tmp, &buf)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn read_status(&self, name: &str) -> Option<HashMap<String, String>> {
        let sr_path = self.sr_path(name);
        let (doc, _from_machine) = serializer::try_read_machine_or_sr(&sr_path)?;
        let mut map = HashMap::new();
        for (key, value) in &doc.context {
            map.insert(key.clone(), value.to_string());
        }
        Some(map)
    }

    pub fn machine_path(&self, name: &str) -> PathBuf {
        self.sr_dir.join(format!("{}.machine", name))
    }

    fn write_llm_value(buf: &mut Vec<u8>, value: &str) -> std::io::Result<()> {
        if value.is_empty() {
            buf.extend_from_slice(b"\"\"");
            return Ok(());
        }
        let needs_quoting = value.contains(|c: char| {
            c.is_ascii_whitespace() || c == '"' || c == '[' || c == ']' || c == '=' || c == '#'
        });
        if needs_quoting {
            buf.push(b'"');
            for c in value.chars() {
                if c == '"' || c == '\\' { buf.push(b'\\'); }
                let mut tmp = [0u8; 4];
                buf.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
            }
            buf.push(b'"');
        } else {
            buf.extend_from_slice(value.as_bytes());
        }
        Ok(())
    }
}
