//! Small, strict helpers for reading the generated Java grammar.

use std::path::{Path, PathBuf};

use crate::{DumperError, Result};

/// A source file that has been read and split into lines.
#[derive(Debug, Clone)]
pub struct JavaSource {
    /// Path relative to the tool invocation for reporting.
    pub path: PathBuf,
    /// Full file text.
    pub text: String,
}

impl JavaSource {
    /// Read a source file.
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|source| DumperError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(Self {
            path: path.to_path_buf(),
            text,
        })
    }

    /// Return a one-based line number for a byte offset.
    pub fn line_of(&self, byte: usize) -> usize {
        self.text[..byte.min(self.text.len())]
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            + 1
    }

    /// Fail with this file's path and the line containing `needle`.
    pub fn err_at<T>(&self, needle: &str, message: impl Into<String>) -> Result<T> {
        let line = self
            .text
            .find(needle)
            .map(|offset| self.line_of(offset))
            .unwrap_or(1);
        Err(DumperError::Parse {
            path: self.path.clone(),
            line,
            message: message.into(),
        })
    }
}

/// Find `public static final <type> <name> = <literal>;` and return the literal.
pub fn const_literal<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name} = ");
    let start = text.find(&needle)? + needle.len();
    let rest = &text[start..];
    let end = rest.find(';')?;
    Some(rest[..end].trim())
}

/// Parse an integer constant.
pub fn int_const(text: &str, name: &str) -> Option<i64> {
    let literal = const_literal(text, name)?;
    literal.trim_end_matches('L').parse().ok()
}

/// Parse a boolean constant.
pub fn bool_const(text: &str, name: &str) -> Option<bool> {
    match const_literal(text, name)? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Extract a top-level `package foo.bar;` declaration.
pub fn package_name(text: &str) -> Option<&str> {
    let start = text.find("package ")? + "package ".len();
    let rest = &text[start..];
    let end = rest.find(';')?;
    Some(&rest[..end])
}

/// Extract the body of a method by matching braces, starting at `header`.
///
/// The generated sources contain no string literals with unbalanced braces in
/// the methods we inspect, so brace counting is sufficient and keeps the parser
/// dependency-free.
pub fn method_body<'a>(text: &'a str, header: &str) -> Option<&'a str> {
    let start = text.find(header)?;
    let open = text[start..].find('{')? + start;
    let mut depth = 0usize;
    for (offset, byte) in text[open..].bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + 1..open + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split `type name` from a generated field or getter.
pub fn split_type_and_name(decl: &str) -> Option<(&str, &str)> {
    let decl = decl.trim();
    let split = decl.rfind(char::is_whitespace)?;
    let (ty, name) = decl.split_at(split);
    let name = name.trim();
    let name = name.split('=').next()?.trim();
    Some((ty.trim(), name))
}

/// Normalise a filesystem path for JSON output.
pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
