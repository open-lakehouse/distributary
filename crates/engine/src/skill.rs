//! Agent skill manifests: what a shared skill declares in its `SKILL.md`.
//!
//! A skill's metadata lives only in the YAML frontmatter that opens its
//! `SKILL.md` (see the [Agent Skills specification]). Neither the share store
//! nor the source catalog models it, so the server reads it from storage, which
//! keeps it identical to what an agent loads.
//!
//! The frontmatter is a small prefix of the file, so [`read`] fetches only the
//! head of it, in one speculative range read of [`FRONTMATTER_PREFETCH`] bytes.
//! On an object store the per-request latency dominates the cost of a few more
//! KiB, so the prefetch is generous — the same trade-off as Arrow's 64 KiB
//! Parquet footer read. A frontmatter that does not fit takes one follow-up
//! read, up to [`FRONTMATTER_MAX`].
//!
//! [Agent Skills specification]: https://agentskills.io/specification

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
use object_store::path::Path;
use object_store::{DynObjectStore, GetOptions, GetRange, ObjectStoreExt};
use serde::Deserialize;
use url::Url;

use crate::{Error, Result};

/// The file a skill directory declares itself in.
pub const SKILL_FILE: &str = "SKILL.md";

/// How much of `SKILL.md` the first read fetches.
///
/// The specification caps `description` at 1024 characters and recommends
/// keeping the whole file under 500 lines, so this covers the frontmatter many
/// times over, and usually the whole file.
pub const FRONTMATTER_PREFETCH: u64 = 64 * 1024;

/// The most of `SKILL.md` read to find the end of its frontmatter.
pub const FRONTMATTER_MAX: u64 = 1024 * 1024;

/// The metadata a skill declares in its `SKILL.md` frontmatter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillManifest {
    /// The skill's declared name. It may differ from the name it is shared under.
    pub name: Option<String>,
    /// What the skill does and when to use it.
    pub description: String,
    /// The skill's license.
    pub license: Option<String>,
    /// The environment the skill requires.
    pub compatibility: Option<String>,
    /// The tools the skill is permitted to use.
    pub allowed_tools: Vec<String>,
    /// Additional metadata.
    pub metadata: BTreeMap<String, String>,
}

impl SkillManifest {
    /// Parse a `SKILL.md` (or a prefix of it holding the whole frontmatter).
    pub fn parse(content: &[u8]) -> Result<Self> {
        match frontmatter(content) {
            Frontmatter::Complete(yaml) => Self::from_yaml(yaml),
            Frontmatter::Incomplete => Err(invalid("its frontmatter is not terminated")),
            Frontmatter::Missing => Err(invalid("it does not open with a frontmatter block")),
        }
    }

    fn from_yaml(yaml: &str) -> Result<Self> {
        let raw: RawManifest =
            serde_saphyr::from_str(yaml).map_err(|e| invalid(format!("its frontmatter: {e}")))?;
        let description = raw
            .description
            .filter(|d| !d.trim().is_empty())
            .ok_or_else(|| invalid("its frontmatter has no description"))?;
        let allowed_tools = match raw.allowed_tools {
            None => Vec::new(),
            Some(AllowedTools::Delimited(tools)) => {
                tools.split_whitespace().map(ToString::to_string).collect()
            }
            Some(AllowedTools::List(tools)) => tools,
        };
        Ok(Self {
            name: raw.name,
            description,
            license: raw.license,
            compatibility: raw.compatibility,
            allowed_tools,
            metadata: raw.metadata,
        })
    }
}

/// The frontmatter as written. Unknown keys are ignored.
#[derive(Deserialize)]
struct RawManifest {
    name: Option<String>,
    description: Option<String>,
    license: Option<String>,
    compatibility: Option<String>,
    #[serde(rename = "allowed-tools")]
    allowed_tools: Option<AllowedTools>,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
}

/// `allowed-tools` is space-delimited in the specification; a YAML list is
/// accepted too.
#[derive(Deserialize)]
#[serde(untagged)]
enum AllowedTools {
    Delimited(String),
    List(Vec<String>),
}

fn invalid(reason: impl std::fmt::Display) -> Error {
    Error::generic(format!("invalid {SKILL_FILE}: {reason}"))
}

/// The frontmatter block found at the start of (a prefix of) `SKILL.md`.
#[derive(Debug, PartialEq, Eq)]
enum Frontmatter<'a> {
    /// The YAML between the opening and closing `---` lines.
    Complete(&'a str),
    /// The file opens a frontmatter block that this prefix does not close.
    Incomplete,
    /// The file does not open with a frontmatter block.
    Missing,
}

fn frontmatter(content: &[u8]) -> Frontmatter<'_> {
    // A range read may end inside a multi-byte character: keep the valid prefix.
    let text = match std::str::from_utf8(content) {
        Ok(text) => text,
        Err(e) if e.error_len().is_none() => {
            std::str::from_utf8(&content[..e.valid_up_to()]).unwrap_or_default()
        }
        Err(_) => return Frontmatter::Missing,
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    match lines.next() {
        Some(line) if is_delimiter(line) && line.ends_with('\n') => {}
        Some(line) if "---".starts_with(line.trim_end()) && !line.ends_with('\n') => {
            return Frontmatter::Incomplete;
        }
        _ => return Frontmatter::Missing,
    }
    let start = text.len() - lines.clone().map(str::len).sum::<usize>();
    let mut end = start;
    for line in lines {
        if is_delimiter(line) {
            return Frontmatter::Complete(&text[start..end]);
        }
        end += line.len();
    }
    Frontmatter::Incomplete
}

fn is_delimiter(line: &str) -> bool {
    line.trim_end() == "---"
}

/// Read the manifest of the skill whose directory is `root`, through `store`.
///
/// `store` addresses objects by the URL path of `root`, as
/// [`TableStorage`](crate::access::TableStorage) does.
pub async fn read(store: &DynObjectStore, root: &Url) -> Result<SkillManifest> {
    let path = skill_file(root)?;
    let (mut content, size) = match get(store, &path, 0..FRONTMATTER_PREFETCH).await {
        Ok(read) => read,
        // An empty object has no range to read.
        Err(e) if !matches!(e, object_store::Error::NotFound { .. }) => {
            match store.head(&path).await {
                Ok(meta) if meta.size == 0 => (Bytes::new(), 0),
                _ => return Err(storage_error(e)),
            }
        }
        Err(e) => return Err(storage_error(e)),
    };
    let read = content.len() as u64;
    if frontmatter(&content) == Frontmatter::Incomplete && read < size.min(FRONTMATTER_MAX) {
        let (rest, _) = get(store, &path, read..FRONTMATTER_MAX)
            .await
            .map_err(storage_error)?;
        let mut whole = BytesMut::with_capacity(content.len() + rest.len());
        whole.extend_from_slice(&content);
        whole.extend_from_slice(&rest);
        content = whole.freeze();
    }
    SkillManifest::parse(&content)
}

/// The path of `SKILL.md` under the skill directory `root`.
fn skill_file(root: &Url) -> Result<Path> {
    let mut directory = root.clone();
    if !directory.path().ends_with('/') {
        directory.set_path(&format!("{}/", directory.path()));
    }
    let url = directory.join(SKILL_FILE)?;
    Path::from_url_path(url.path())
        .map_err(|e| Error::generic(format!("invalid skill location '{root}': {e}")))
}

/// One range of `path` (clamped to the object's end), and the object's size.
async fn get(
    store: &DynObjectStore,
    path: &Path,
    range: Range<u64>,
) -> object_store::Result<(Bytes, u64)> {
    let options = GetOptions {
        range: Some(GetRange::Bounded(range)),
        ..Default::default()
    };
    let result = store.get_opts(path, options).await?;
    let size = result.meta.size;
    Ok((result.bytes().await?, size))
}

fn storage_error(e: object_store::Error) -> Error {
    match e {
        object_store::Error::NotFound { path, .. } => {
            Error::generic(format!("the skill has no {SKILL_FILE} (at '{path}')"))
        }
        e => Error::generic(format!("reading {SKILL_FILE}: {e}")),
    }
}

/// Parsed manifests by skill location, each kept for a fixed time to live.
#[derive(Debug)]
pub(crate) struct ManifestCache {
    ttl: Duration,
    entries: Mutex<HashMap<String, (Instant, Arc<SkillManifest>)>>,
}

impl ManifestCache {
    pub(crate) fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::default(),
        }
    }

    pub(crate) fn get(&self, location: &str) -> Option<Arc<SkillManifest>> {
        let entries = self.entries.lock().expect("manifest cache poisoned");
        entries
            .get(location)
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, manifest)| manifest.clone())
    }

    pub(crate) fn insert(&self, location: String, manifest: Arc<SkillManifest>) {
        if self.ttl.is_zero() {
            return;
        }
        let mut entries = self.entries.lock().expect("manifest cache poisoned");
        entries.retain(|_, (at, _)| at.elapsed() < self.ttl);
        entries.insert(location, (Instant::now(), manifest));
    }
}

#[cfg(test)]
mod tests {
    use object_store::PutPayload;
    use object_store::memory::InMemory;

    use super::*;

    const FULL: &str = "---
name: pdf-processing
description: >
  Extract text and tables from PDF files.
  Use when working with PDFs.
license: Apache-2.0
compatibility: Requires poppler-utils
allowed-tools: Bash(pdftotext:*) Read
metadata:
  author: example-org
  version: 1.0
---

# PDF processing
";

    #[test]
    fn test_parse_reads_every_field() {
        let manifest = SkillManifest::parse(FULL.as_bytes()).unwrap();
        assert_eq!(
            manifest,
            SkillManifest {
                name: Some("pdf-processing".into()),
                description:
                    "Extract text and tables from PDF files. Use when working with PDFs.\n".into(),
                license: Some("Apache-2.0".into()),
                compatibility: Some("Requires poppler-utils".into()),
                allowed_tools: vec!["Bash(pdftotext:*)".into(), "Read".into()],
                metadata: BTreeMap::from([
                    ("author".into(), "example-org".into()),
                    ("version".into(), "1.0".into()),
                ]),
            }
        );
    }

    #[test]
    fn test_parse_minimal_frontmatter() {
        let manifest =
            SkillManifest::parse(b"---\nname: s\ndescription: Does things.\n---\n").unwrap();
        assert_eq!(manifest.description, "Does things.");
        assert!(manifest.allowed_tools.is_empty());
        assert!(manifest.metadata.is_empty());
    }

    #[test]
    fn test_parse_accepts_an_allowed_tools_list() {
        let manifest =
            SkillManifest::parse(b"---\ndescription: d\nallowed-tools:\n  - Read\n  - Grep\n---\n")
                .unwrap();
        assert_eq!(manifest.allowed_tools, vec!["Read", "Grep"]);
    }

    #[test]
    fn test_parse_ignores_unknown_keys_and_crlf() {
        let manifest =
            SkillManifest::parse(b"---\r\ndescription: d\r\nversion: 3\r\n---\r\nbody\r\n")
                .unwrap();
        assert_eq!(manifest.description, "d");
    }

    #[test]
    fn test_parse_rejects_missing_description_or_frontmatter() {
        assert!(SkillManifest::parse(b"---\nname: s\n---\n").is_err());
        assert!(SkillManifest::parse(b"# Just a heading\n").is_err());
        assert!(SkillManifest::parse(b"").is_err());
        assert!(SkillManifest::parse(b"---\ndescription: d\n").is_err());
    }

    #[test]
    fn test_frontmatter_boundaries() {
        assert_eq!(
            frontmatter(b"---\na: 1\n---\n"),
            Frontmatter::Complete("a: 1\n")
        );
        assert_eq!(
            frontmatter(b"---\na: 1\n---"),
            Frontmatter::Complete("a: 1\n")
        );
        assert_eq!(
            frontmatter(b"\xef\xbb\xbf---\n---\n"),
            Frontmatter::Complete("")
        );
        assert_eq!(frontmatter(b"---\na: 1\n"), Frontmatter::Incomplete);
        assert_eq!(frontmatter(b"--"), Frontmatter::Incomplete);
        assert_eq!(frontmatter(b"# title\n---\n"), Frontmatter::Missing);
        // A prefix cut inside a multi-byte character.
        assert_eq!(frontmatter(b"---\na: \xc3"), Frontmatter::Incomplete);
    }

    async fn store_with(content: impl Into<PutPayload>) -> (InMemory, Url) {
        let store = InMemory::new();
        store
            .put(&Path::from("skills/pdf/SKILL.md"), content.into())
            .await
            .unwrap();
        (store, Url::parse("memory:///skills/pdf").unwrap())
    }

    #[tokio::test]
    async fn test_read_small_file() {
        let (store, root) = store_with(FULL.as_bytes().to_vec()).await;
        let manifest = read(&store, &root).await.unwrap();
        assert_eq!(manifest.license.as_deref(), Some("Apache-2.0"));
    }

    #[tokio::test]
    async fn test_read_large_body_reads_only_the_head() {
        let body = "x".repeat(2 * FRONTMATTER_PREFETCH as usize);
        let (store, root) = store_with(format!("{FULL}{body}").into_bytes()).await;
        let path = skill_file(&root).unwrap();
        let (head, size) = get(&store, &path, 0..FRONTMATTER_PREFETCH).await.unwrap();
        assert_eq!(head.len() as u64, FRONTMATTER_PREFETCH);
        assert!(size > FRONTMATTER_PREFETCH);
        assert!(matches!(frontmatter(&head), Frontmatter::Complete(_)));
        assert_eq!(read(&store, &root).await.unwrap().description.len(), 68);
    }

    #[tokio::test]
    async fn test_read_frontmatter_past_the_prefetch() {
        let long = "y".repeat(FRONTMATTER_PREFETCH as usize);
        let content = format!("---\ndescription: d\nmetadata:\n  long: {long}\n---\nbody\n");
        let (store, root) = store_with(content.into_bytes()).await;
        let manifest = read(&store, &root).await.unwrap();
        assert_eq!(manifest.metadata["long"].len(), long.len());
    }

    #[tokio::test]
    async fn test_read_rejects_frontmatter_past_the_cap() {
        let long = "y".repeat(FRONTMATTER_MAX as usize);
        let content = format!("---\ndescription: d\nnote: {long}\n---\n");
        let (store, root) = store_with(content.into_bytes()).await;
        assert!(read(&store, &root).await.is_err());
    }

    #[tokio::test]
    async fn test_read_empty_or_missing_file() {
        let (store, root) = store_with(Vec::new()).await;
        let err = read(&store, &root).await.unwrap_err().to_string();
        assert!(err.contains("frontmatter"), "{err}");

        let missing = Url::parse("memory:///skills/none").unwrap();
        let err = read(&store, &missing).await.unwrap_err().to_string();
        assert!(err.contains("has no SKILL.md"), "{err}");
    }

    #[test]
    fn test_cache_expires_entries() {
        let manifest = Arc::new(SkillManifest::default());
        let cache = ManifestCache::new(Duration::from_secs(60));
        cache.insert("s3://b/k".into(), manifest.clone());
        assert_eq!(cache.get("s3://b/k"), Some(manifest.clone()));
        assert_eq!(cache.get("s3://b/other"), None);

        let disabled = ManifestCache::new(Duration::ZERO);
        disabled.insert("s3://b/k".into(), manifest);
        assert_eq!(disabled.get("s3://b/k"), None);
    }
}
