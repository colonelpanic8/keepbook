use std::collections::HashMap;

/// Parsed multiline credential entry.
///
/// The format intentionally matches password-store entries so the same secret
/// payload can be stored in `pass` or encrypted into an age file.
#[derive(Debug, Default)]
pub(crate) struct FieldEntry {
    /// The first line, exposed as the conventional "password" field.
    pub password: Option<String>,
    /// Additional fields in `name: value` format.
    pub fields: HashMap<String, String>,
}

impl FieldEntry {
    pub(crate) fn parse(content: &str) -> Self {
        let mut lines = content.lines();
        let password = lines.next().map(|s| s.to_string());
        let mut fields = HashMap::new();

        if let Some(ref pw) = password {
            fields.insert("password".to_string(), pw.clone());
        }

        for line in lines {
            if let Some((key, value)) = line.split_once(": ") {
                fields.insert(key.to_string(), value.replace("\\n", "\n"));
            }
        }

        Self { password, fields }
    }
}

impl std::fmt::Display for FieldEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref password) = self.password {
            writeln!(f, "{password}")?;
        }

        for (key, value) in &self.fields {
            if key != "password" {
                writeln!(f, "{key}: {}", value.replace('\n', "\\n"))?;
            }
        }

        Ok(())
    }
}
