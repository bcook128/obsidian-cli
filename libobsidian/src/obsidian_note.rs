use std::{fs, path::PathBuf};

pub type Properties = serde_yaml::Value;

#[derive(Debug, PartialEq, Eq)]
pub struct ObsidianNote {
    pub file_path: PathBuf,
    /// The raw contents of the underlying markdown file, including any
    /// frontmatter. This field is optional to allow callers to construct an
    /// `ObsidianNote` without needing to have read the original file from
    /// disk (for example when creating a new note in-memory).
    pub file_raw_contents: Option<String>,
    pub file_body: String,
    pub properties: Option<Properties>,
}

impl ObsidianNote {
    pub fn read_from_path(file_path: &PathBuf) -> anyhow::Result<Self> {
        let file_contents = fs::read_to_string(file_path)?;
        Self::parse(file_path, file_contents)
    }

    pub fn parse(file_path: &PathBuf, file_contents: String) -> anyhow::Result<Self> {
        let (frontmatter_str, file_body) = extract_frontmatter(&file_contents);

        let frontmatter = frontmatter_str
            .map(|s| serde_yaml::from_str::<Properties>(&s))
            .transpose()?
            .and_then(|fm| {
                if fm == serde_yaml::Value::Null {
                    None
                } else {
                    Some(fm)
                }
            });

        let note = Self {
            file_path: file_path.clone(),
            file_body: file_body.unwrap_or_default(),
            file_raw_contents: Some(file_contents),
            properties: frontmatter,
        };

        Ok(note)
    }
}

fn extract_frontmatter(content: &str) -> (Option<String>, Option<String>) {
    let delimiter = "---";
    let mut parts = content.splitn(3, delimiter);

    match (parts.next(), parts.next(), parts.next()) {
        (Some(""), Some(frontmatter), Some(body)) => (
            Some(frontmatter.trim().to_string()),
            Some(body.trim().to_string()),
        ),
        (Some(""), Some(frontmatter), None) => (Some(frontmatter.trim().to_string()), None),
        _ => (None, Some(content.trim().to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn parse_returns_body() {
        let note_content = indoc! {r"
            ---
            some-property: foo
            ---
            The note body
        "};
        let note =
            ObsidianNote::parse(&PathBuf::from("a-note.md"), note_content.to_string()).unwrap();

        assert_eq!(note.file_body.trim(), "The note body");
    }

    #[test]
    fn parse_returns_properties() {
        let note_content = indoc! {r"
            ---
            some-property: foo
            ---
        "};
        let note =
            ObsidianNote::parse(&PathBuf::from("a-note.md"), note_content.to_string()).unwrap();

        assert_eq!(
            note.properties,
            Some(serde_yaml::Value::Mapping(serde_yaml::Mapping::from_iter(
                vec![(
                    serde_yaml::Value::String("some-property".to_string()),
                    serde_yaml::Value::String("foo".to_string())
                )]
                .into_iter()
            )))
        );
    }

    #[test]
    fn parse_populates_file_raw_contents() {
        let note_content = indoc! {r"
            ---
            some-property: foo
            ---
            The note body
        "};

        let note =
            ObsidianNote::parse(&PathBuf::from("a-note.md"), note_content.to_string()).unwrap();

        assert_eq!(note.file_raw_contents, Some(note_content.to_string()));
    }

    #[test]
    fn parse_handles_missing_frontmatter() {
        let note =
            ObsidianNote::parse(&PathBuf::from("a-note.md"), "The note contents".to_string())
                .unwrap();
        assert_eq!(note.properties, None);
    }

    #[test]
    fn parse_handles_empty_frontmatter() {
        let note_content = indoc! {r"
            ---
            ---
            The note content
        "};

        let note =
            ObsidianNote::parse(&PathBuf::from("a-note.md"), note_content.to_string()).unwrap();
        assert_eq!(note.properties, None);
    }

    #[test]
    fn parse_handles_tables() {
        // Markdown tables also contain `---`
        let note_content = indoc! {r"
            | Col1      | Col2      |
            |-----------|-----------|
            | Row1 Col1 | Row1 Col2 |
            | Row2 Col1 | Row2 Col2 |
        "};

        let note =
            ObsidianNote::parse(&PathBuf::from("a-note.md"), note_content.to_string()).unwrap();
        assert_eq!(note.properties, None);
    }
}
