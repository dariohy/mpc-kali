use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const API_VERSION: &str = "mcp-kali/v1";
const MAX_REFERENCE_BYTES: u64 = 256 * 1024;
const REFERENCE_URI_PREFIX: &str = "mcp-kali://references/";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReferenceSummary {
    pub id: String,
    pub plugin: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub related_tools: Vec<String>,
    pub related_capabilities: Vec<String>,
    pub uri: String,
    pub mime_type: &'static str,
    pub layer: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReferenceDocument {
    #[serde(flatten)]
    pub summary: ReferenceSummary,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReferenceDiagnostic {
    pub layer: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct ReferenceRegistry {
    documents: BTreeMap<String, ReferenceDocument>,
    diagnostics: Vec<ReferenceDiagnostic>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReferenceFrontMatter {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: ReferenceMetadata,
    plugin: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    related_tools: Vec<String>,
    #[serde(default)]
    related_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReferenceMetadata {
    id: String,
    title: String,
    description: String,
}

#[derive(Debug)]
pub struct ReferenceImport<'a> {
    pub source: &'a Path,
    pub config_dir: &'a Path,
    pub id: &'a str,
    pub plugin: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub tags: Vec<String>,
    pub related_tools: Vec<String>,
    pub related_capabilities: Vec<String>,
}

impl ReferenceRegistry {
    pub fn load(
        system_data_dir: &Path,
        config_dir: &Path,
        plugins: &BTreeSet<String>,
        tool_owners: &BTreeMap<String, String>,
        capabilities: &BTreeSet<String>,
    ) -> Self {
        let mut registry = Self::default();
        registry.load_layer(
            "system",
            &[system_data_dir.join("plugins")],
            false,
            plugins,
            tool_owners,
            capabilities,
        );
        let overlay_roots = if config_dir == system_data_dir {
            vec![config_dir.join("references")]
        } else {
            vec![config_dir.join("plugins"), config_dir.join("references")]
        };
        registry.load_layer(
            "overlay",
            &overlay_roots,
            true,
            plugins,
            tool_owners,
            capabilities,
        );
        registry
    }

    pub fn summaries(&self) -> Vec<ReferenceSummary> {
        self.documents
            .values()
            .map(|document| document.summary.clone())
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<ReferenceDocument> {
        self.documents.get(id).cloned()
    }

    pub fn diagnostics(&self) -> &[ReferenceDiagnostic] {
        &self.diagnostics
    }

    fn load_layer(
        &mut self,
        layer: &str,
        roots: &[PathBuf],
        replace: bool,
        plugins: &BTreeSet<String>,
        tool_owners: &BTreeMap<String, String>,
        capabilities: &BTreeSet<String>,
    ) {
        let mut paths = roots
            .iter()
            .flat_map(|root| find_reference_files(root))
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        let mut seen = BTreeSet::new();
        for path in paths {
            let result = load_document(layer, &path, plugins, tool_owners, capabilities);
            match result {
                Ok(document) => {
                    let id = document.summary.id.clone();
                    if !seen.insert(id.clone()) {
                        self.diagnostics.push(ReferenceDiagnostic {
                            layer: layer.into(),
                            path: path.display().to_string(),
                            reference_id: Some(id.clone()),
                            message: format!("duplicate reference ID in {layer} layer: {id}"),
                        });
                    } else if !replace && self.documents.contains_key(&id) {
                        self.diagnostics.push(ReferenceDiagnostic {
                            layer: layer.into(),
                            path: path.display().to_string(),
                            reference_id: Some(id.clone()),
                            message: format!("duplicate reference ID: {id}"),
                        });
                    } else {
                        self.documents.insert(id, document);
                    }
                }
                Err(error) => self.diagnostics.push(ReferenceDiagnostic {
                    layer: layer.into(),
                    path: path.display().to_string(),
                    reference_id: reference_id_from_file(&path),
                    message: format!("{error:#}"),
                }),
            }
        }
    }
}

pub fn import_reference(request: ReferenceImport<'_>) -> Result<PathBuf> {
    validate_reference_id(request.id)?;
    validate_plugin_id(request.plugin)?;
    validate_text("title", request.title, 1, 160)?;
    validate_text("description", request.description, 1, 512)?;
    validate_string_list("tags", &request.tags, 32, 64)?;
    for tool in &request.related_tools {
        validate_tool_name(tool)?;
    }
    for capability in &request.related_capabilities {
        validate_capability_id(capability)?;
    }

    let metadata = fs::symlink_metadata(request.source)
        .with_context(|| format!("inspect {}", request.source.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("reference source must be a regular non-symlink file");
    }
    if metadata.len() > MAX_REFERENCE_BYTES {
        bail!("reference source exceeds {MAX_REFERENCE_BYTES} bytes");
    }
    let body = fs::read_to_string(request.source)
        .with_context(|| format!("read {} as UTF-8", request.source.display()))?;
    if body.trim().is_empty() {
        bail!("reference source is empty");
    }
    let front_matter = ReferenceFrontMatter {
        api_version: API_VERSION.into(),
        kind: "PluginReference".into(),
        metadata: ReferenceMetadata {
            id: request.id.into(),
            title: request.title.into(),
            description: request.description.into(),
        },
        plugin: request.plugin.into(),
        tags: sorted_unique(request.tags),
        related_tools: sorted_unique(request.related_tools),
        related_capabilities: sorted_unique(request.related_capabilities),
    };
    let yaml = serde_yaml::to_string(&front_matter)?;
    let rendered = format!("---\n{yaml}---\n\n{}", body.trim_start());
    if rendered.len() as u64 > MAX_REFERENCE_BYTES {
        bail!("rendered reference exceeds {MAX_REFERENCE_BYTES} bytes");
    }

    ensure_directory(request.config_dir, false)?;
    let references_dir = request.config_dir.join("references");
    ensure_directory(&references_dir, true)?;
    let destination_dir = references_dir.join(request.plugin);
    ensure_directory(&destination_dir, true)?;
    let destination = destination_dir.join(format!("{}.md", request.id));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options
        .open(&destination)
        .with_context(|| format!("create {}; refusing to overwrite", destination.display()))?;
    use std::io::Write;
    file.write_all(rendered.as_bytes())?;
    file.sync_all()?;
    Ok(destination)
}

fn load_document(
    layer: &str,
    path: &Path,
    plugins: &BTreeSet<String>,
    tool_owners: &BTreeMap<String, String>,
    capabilities: &BTreeSet<String>,
) -> Result<ReferenceDocument> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("reference must be a regular non-symlink file");
    }
    if metadata.len() > MAX_REFERENCE_BYTES {
        bail!("reference exceeds {MAX_REFERENCE_BYTES} bytes");
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("read {} as UTF-8", path.display()))?;
    let (mut front_matter, content) = parse_reference(&source)?;
    validate_front_matter(&front_matter)?;
    if !plugins.contains(&front_matter.plugin) {
        bail!(
            "reference plugin is not registered: {}",
            front_matter.plugin
        );
    }
    for tool in &front_matter.related_tools {
        match tool_owners.get(tool) {
            Some(owner) if owner == &front_matter.plugin => {}
            Some(owner) => bail!("related tool {tool} belongs to plugin {owner}"),
            None => bail!("related tool is not registered: {tool}"),
        }
    }
    for capability in &front_matter.related_capabilities {
        if !capabilities.contains(capability) {
            bail!("related capability is not registered: {capability}");
        }
    }
    front_matter.tags = sorted_unique(front_matter.tags);
    front_matter.related_tools = sorted_unique(front_matter.related_tools);
    front_matter.related_capabilities = sorted_unique(front_matter.related_capabilities);
    let id = front_matter.metadata.id;
    Ok(ReferenceDocument {
        summary: ReferenceSummary {
            uri: format!("{REFERENCE_URI_PREFIX}{id}"),
            id,
            plugin: front_matter.plugin,
            title: front_matter.metadata.title,
            description: front_matter.metadata.description,
            tags: front_matter.tags,
            related_tools: front_matter.related_tools,
            related_capabilities: front_matter.related_capabilities,
            mime_type: "text/markdown",
            layer: layer.into(),
            source: path.display().to_string(),
        },
        content,
    })
}

fn parse_reference(source: &str) -> Result<(ReferenceFrontMatter, String)> {
    let normalized = source.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix("---\n")
        .context("reference must start with YAML front matter delimited by ---")?;
    let (yaml, content) = rest
        .split_once("\n---\n")
        .context("reference front matter is missing its closing ---")?;
    let front_matter = serde_yaml::from_str(yaml).context("parse reference front matter")?;
    if content.trim().is_empty() {
        bail!("reference Markdown body is empty");
    }
    Ok((front_matter, content.trim_start_matches('\n').to_owned()))
}

fn validate_front_matter(document: &ReferenceFrontMatter) -> Result<()> {
    if document.api_version != API_VERSION || document.kind != "PluginReference" {
        bail!("reference must use apiVersion {API_VERSION} and kind PluginReference");
    }
    validate_reference_id(&document.metadata.id)?;
    validate_plugin_id(&document.plugin)?;
    validate_text("title", &document.metadata.title, 1, 160)?;
    validate_text("description", &document.metadata.description, 1, 512)?;
    validate_string_list("tags", &document.tags, 32, 64)?;
    validate_string_list("related_tools", &document.related_tools, 64, 128)?;
    validate_string_list(
        "related_capabilities",
        &document.related_capabilities,
        64,
        128,
    )?;
    for tool in &document.related_tools {
        validate_tool_name(tool)?;
    }
    for capability in &document.related_capabilities {
        validate_capability_id(capability)?;
    }
    Ok(())
}

fn validate_reference_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '-')
        })
    {
        bail!("reference IDs use lowercase ASCII letters, digits, dots, and hyphens");
    }
    Ok(())
}

fn validate_plugin_id(value: &str) -> Result<()> {
    validate_reference_id(value).context("invalid plugin ID")
}

fn validate_capability_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '-' | '_')
        })
    {
        bail!("capability IDs use lowercase ASCII letters, digits, dots, hyphens, and underscores");
    }
    Ok(())
}

fn validate_tool_name(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        bail!("tool names use lowercase ASCII letters, digits, and underscores");
    }
    Ok(())
}

fn validate_text(name: &str, value: &str, minimum: usize, maximum: usize) -> Result<()> {
    let length = value.chars().count();
    if !(minimum..=maximum).contains(&length) || value.chars().any(char::is_control) {
        bail!("{name} must be {minimum}..={maximum} printable characters");
    }
    Ok(())
}

fn validate_string_list(
    name: &str,
    values: &[String],
    maximum: usize,
    max_item: usize,
) -> Result<()> {
    if values.len() > maximum {
        bail!("{name} contains more than {maximum} entries");
    }
    for value in values {
        validate_text(name, value, 1, max_item)?;
    }
    Ok(())
}

fn sorted_unique(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn ensure_directory(path: &Path, create: bool) -> Result<()> {
    if !path.exists() {
        if !create {
            bail!("configuration directory does not exist: {}", path.display());
        }
        fs::create_dir(path).with_context(|| format!("create {}", path.display()))?;
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "reference destination must use non-symlink directories: {}",
            path.display()
        );
    }
    Ok(())
}

fn reference_id_from_file(path: &Path) -> Option<String> {
    let source = fs::read_to_string(path).ok()?;
    parse_reference(&source)
        .ok()
        .map(|(document, _)| document.metadata.id)
}

fn find_reference_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    visit_reference_tree(
        root,
        root.file_name().is_some_and(|name| name == "references"),
        &mut files,
    );
    files.sort();
    files
}

fn visit_reference_tree(path: &Path, inside_references: bool, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let child = entry.path();
        if file_type.is_dir() {
            let child_inside = inside_references || entry.file_name() == "references";
            visit_reference_tree(&child, child_inside, files);
        } else if file_type.is_file()
            && inside_references
            && child.extension().and_then(|value| value.to_str()) == Some("md")
        {
            files.push(child);
        }
    }
}

#[cfg(test)]
fn find_files_named(root: &Path, filename: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    fn visit(root: &Path, filename: &str, files: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                visit(&path, filename, files);
            } else if kind.is_file()
                && path.file_name().and_then(|value| value.to_str()) == Some(filename)
            {
                files.push(path);
            }
        }
    }
    visit(root, filename, &mut files);
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn context() -> (BTreeSet<String>, BTreeMap<String, String>, BTreeSet<String>) {
        (
            BTreeSet::from(["org.mcp-kali.nmap".into()]),
            BTreeMap::from([("nmap_host_discovery".into(), "org.mcp-kali.nmap".into())]),
            BTreeSet::from(["network.host_discovery".into()]),
        )
    }

    #[test]
    fn loads_packaged_reference_and_overlay_replacement() {
        let system = tempdir().unwrap();
        let overlay = tempdir().unwrap();
        let system_dir = system.path().join("plugins/nmap/references");
        let overlay_dir = overlay.path().join("references/nmap");
        fs::create_dir_all(&system_dir).unwrap();
        fs::create_dir_all(&overlay_dir).unwrap();
        let document = |title: &str| {
            format!(
                "---\napiVersion: mcp-kali/v1\nkind: PluginReference\nmetadata:\n  id: nmap.discovery\n  title: {title}\n  description: Safe discovery guidance.\nplugin: org.mcp-kali.nmap\nrelated_tools: [nmap_host_discovery]\nrelated_capabilities: [network.host_discovery]\n---\n\n# {title}\n"
            )
        };
        fs::write(system_dir.join("discovery.md"), document("Packaged")).unwrap();
        fs::write(overlay_dir.join("discovery.md"), document("Operator")).unwrap();
        let (plugins, tools, capabilities) = context();
        let registry = ReferenceRegistry::load(
            system.path(),
            overlay.path(),
            &plugins,
            &tools,
            &capabilities,
        );
        assert!(
            registry.diagnostics().is_empty(),
            "{:#?}",
            registry.diagnostics()
        );
        let reference = registry.get("nmap.discovery").unwrap();
        assert_eq!(reference.summary.title, "Operator");
        assert_eq!(reference.summary.layer, "overlay");
    }

    #[test]
    fn rejects_unknown_related_tool() {
        let system = tempdir().unwrap();
        let reference_dir = system.path().join("plugins/nmap/references");
        fs::create_dir_all(&reference_dir).unwrap();
        fs::write(
            reference_dir.join("bad.md"),
            "---\napiVersion: mcp-kali/v1\nkind: PluginReference\nmetadata:\n  id: nmap.bad\n  title: Bad\n  description: Bad relationship.\nplugin: org.mcp-kali.nmap\nrelated_tools: [missing_tool]\n---\n\nBody\n",
        )
        .unwrap();
        let (plugins, tools, capabilities) = context();
        let registry = ReferenceRegistry::load(
            system.path(),
            system.path(),
            &plugins,
            &tools,
            &capabilities,
        );
        assert_eq!(registry.diagnostics().len(), 1);
        assert!(registry.diagnostics()[0].message.contains("missing_tool"));
    }

    #[test]
    fn import_wraps_markdown_and_refuses_overwrite() {
        let directory = tempdir().unwrap();
        let source = directory.path().join("guide.md");
        fs::write(&source, "# Local procedure\n\nUse the approved scope.\n").unwrap();
        let request = || ReferenceImport {
            source: &source,
            config_dir: directory.path(),
            id: "nmap.local-procedure",
            plugin: "org.mcp-kali.nmap",
            title: "Local procedure",
            description: "Approved local Nmap procedure.",
            tags: vec!["nmap".into()],
            related_tools: vec!["nmap_host_discovery".into()],
            related_capabilities: vec!["network.host_discovery".into()],
        };
        let destination = import_reference(request()).unwrap();
        assert!(destination.is_file());
        let rendered = fs::read_to_string(&destination).unwrap();
        assert!(rendered.contains("kind: PluginReference"));
        assert!(rendered.contains("# Local procedure"));
        assert!(import_reference(request()).is_err());
    }

    #[test]
    fn packaged_references_match_packaged_plugins_and_capabilities() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut plugins = BTreeSet::new();
        let mut tools = BTreeMap::new();
        for manifest in find_files_named(&root.join("plugins"), "plugin.yaml") {
            let value: serde_json::Value =
                serde_yaml::from_slice(&fs::read(manifest).unwrap()).unwrap();
            let plugin = value.pointer("/metadata/id").unwrap().as_str().unwrap();
            plugins.insert(plugin.to_owned());
            for tool in value.pointer("/tools").unwrap().as_array().unwrap() {
                let name = tool.pointer("/metadata/name").unwrap().as_str().unwrap();
                tools.insert(name.to_owned(), plugin.to_owned());
            }
        }
        let catalog: serde_json::Value = serde_yaml::from_slice(
            &fs::read(root.join("plugins/capability-catalog.yaml")).unwrap(),
        )
        .unwrap();
        let capabilities = catalog
            .pointer("/capabilities")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.get("id").unwrap().as_str().unwrap().to_owned())
            .collect();
        let registry = ReferenceRegistry::load(root, root, &plugins, &tools, &capabilities);
        assert!(
            registry.diagnostics().is_empty(),
            "{:#?}",
            registry.diagnostics()
        );
        assert_eq!(
            registry.summaries().len(),
            find_reference_files(&root.join("plugins")).len()
        );
    }
}
