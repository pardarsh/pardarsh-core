//! Namespaced, typed extensions.
//!
//! Applications add their own fields to records through extensions instead of
//! inventing a competing record model. Every extension namespace is described
//! by an [`ExtensionSchema`] and every value is typed ([`ExtValue`]); there is
//! no untyped JSON blob.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::id::{RecordId, is_namespace, is_simple_name};
use crate::model::{Record, RecordKind, Timestamp};
use crate::schema::SchemaVersion;
use crate::validation::ValidationIssue;

/// A typed extension value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    Timestamp(Timestamp),
    Date(NaiveDate),
    /// One of the values listed in the field's [`FieldType::Enum`].
    Enum(String),
    Reference(RecordId),
    List(Vec<ExtValue>),
    /// The value was redacted; see ADR 0006.
    Redacted,
}

impl ExtValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) | Self::Enum(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_reference(&self) -> Option<&RecordId> {
        match self {
            Self::Reference(r) => Some(r),
            _ => None,
        }
    }

    pub fn as_date(&self) -> Option<NaiveDate> {
        match self {
            Self::Date(d) => Some(*d),
            _ => None,
        }
    }

    /// A short human-readable rendering, used in diffs and timelines.
    pub fn display(&self) -> String {
        match self {
            Self::Text(s) | Self::Enum(s) => s.clone(),
            Self::Integer(i) => i.to_string(),
            Self::Boolean(b) => b.to_string(),
            Self::Timestamp(t) => t.to_rfc3339(),
            Self::Date(d) => d.to_string(),
            Self::Reference(r) => r.to_string(),
            Self::List(items) => {
                let parts: Vec<_> = items.iter().map(ExtValue::display).collect();
                format!("[{}]", parts.join(", "))
            }
            Self::Redacted => crate::event::REDACTED.to_string(),
        }
    }
}

/// The declared type of an extension field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldType {
    Text {
        max_len: usize,
    },
    Integer {
        min: Option<i64>,
        max: Option<i64>,
    },
    Boolean,
    Timestamp,
    Date,
    Enum {
        values: Vec<String>,
    },
    /// A reference to another record, optionally restricted to some kinds.
    Reference {
        kinds: Vec<RecordKind>,
    },
    List {
        item: Box<FieldType>,
        max_items: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldSpec {
    #[serde(rename = "type")]
    pub ty: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
}

impl FieldSpec {
    pub fn required(ty: FieldType, description: &str) -> Self {
        Self { ty, required: true, description: description.to_string() }
    }

    pub fn optional(ty: FieldType, description: &str) -> Self {
        Self { ty, required: false, description: description.to_string() }
    }
}

/// Describes one extension namespace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtensionSchema {
    pub namespace: String,
    pub version: SchemaVersion,
    /// Record kinds the extension may be attached to (empty = any kind).
    #[serde(default)]
    pub applies_to: Vec<RecordKind>,
    pub fields: BTreeMap<String, FieldSpec>,
    #[serde(default)]
    pub description: String,
}

impl ExtensionSchema {
    pub fn new(namespace: &str, version: SchemaVersion) -> Self {
        Self {
            namespace: namespace.to_string(),
            version,
            applies_to: Vec::new(),
            fields: BTreeMap::new(),
            description: String::new(),
        }
    }

    pub fn applies_to(mut self, kinds: impl IntoIterator<Item = RecordKind>) -> Self {
        self.applies_to = kinds.into_iter().collect();
        self
    }

    pub fn field(mut self, name: &str, spec: FieldSpec) -> Self {
        self.fields.insert(name.to_string(), spec);
        self
    }

    pub fn describe(mut self, d: &str) -> Self {
        self.description = d.to_string();
        self
    }
}

/// The set of extension schemas a validator knows about.
#[derive(Clone, Debug, Default)]
pub struct ExtensionRegistry {
    schemas: HashMap<String, ExtensionSchema>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, schema: ExtensionSchema) -> Result<(), String> {
        if !is_namespace(&schema.namespace) {
            return Err(format!("invalid extension namespace {:?}", schema.namespace));
        }
        if let Some(bad) = schema.fields.keys().find(|k| !is_simple_name(k)) {
            return Err(format!("invalid field name {bad:?} in {}", schema.namespace));
        }
        if self.schemas.contains_key(&schema.namespace) {
            return Err(format!("extension namespace {} is already registered", schema.namespace));
        }
        self.schemas.insert(schema.namespace.clone(), schema);
        Ok(())
    }

    pub fn get(&self, namespace: &str) -> Option<&ExtensionSchema> {
        self.schemas.get(namespace)
    }

    pub fn schemas(&self) -> impl Iterator<Item = &ExtensionSchema> {
        self.schemas.values()
    }

    /// Validate the extension data on a record. Reference targets are only
    /// checked for syntax here; existence is checked by the repository.
    pub(crate) fn validate(&self, record: &Record, issues: &mut Vec<ValidationIssue>) {
        for (namespace, data) in &record.extensions {
            let path = |f: &str| format!("extensions.{namespace}.{f}");
            let Some(schema) = self.schemas.get(namespace) else {
                issues.push(ValidationIssue::new(format!("extensions.{namespace}"), "unknown extension namespace"));
                continue;
            };
            if !schema.version.can_read(&data.schema_version) {
                issues.push(ValidationIssue::new(
                    format!("extensions.{namespace}.schema_version"),
                    format!("version {} is not readable by schema {}", data.schema_version, schema.version),
                ));
            }
            if !schema.applies_to.is_empty() && !schema.applies_to.contains(&record.kind) {
                issues.push(ValidationIssue::new(
                    format!("extensions.{namespace}"),
                    format!("extension does not apply to kind {}", record.kind),
                ));
            }
            for (field, value) in &data.fields {
                match schema.fields.get(field) {
                    None => issues.push(ValidationIssue::new(path(field), "unknown field")),
                    Some(spec) => check_value(&spec.ty, value, &path(field), issues),
                }
            }
            for (field, spec) in &schema.fields {
                if spec.required && !data.fields.contains_key(field) {
                    issues.push(ValidationIssue::new(path(field), "required field is missing"));
                }
            }
        }
    }
}

fn check_value(ty: &FieldType, value: &ExtValue, path: &str, issues: &mut Vec<ValidationIssue>) {
    if matches!(value, ExtValue::Redacted) {
        return;
    }
    let mismatch = |issues: &mut Vec<ValidationIssue>| {
        issues.push(ValidationIssue::new(path, format!("value does not match declared type {ty:?}")))
    };
    match (ty, value) {
        (FieldType::Text { max_len }, ExtValue::Text(s)) => {
            if s.chars().count() > *max_len {
                issues.push(ValidationIssue::new(path, format!("text longer than {max_len} characters")));
            }
            if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
                issues.push(ValidationIssue::new(path, "text contains control characters"));
            }
        }
        (FieldType::Integer { min, max }, ExtValue::Integer(i)) => {
            if min.is_some_and(|m| *i < m) || max.is_some_and(|m| *i > m) {
                issues.push(ValidationIssue::new(path, "integer out of range"));
            }
        }
        (FieldType::Boolean, ExtValue::Boolean(_))
        | (FieldType::Timestamp, ExtValue::Timestamp(_))
        | (FieldType::Date, ExtValue::Date(_))
        | (FieldType::Reference { .. }, ExtValue::Reference(_)) => {}
        (FieldType::Enum { values }, ExtValue::Enum(v)) => {
            if !values.contains(v) {
                issues.push(ValidationIssue::new(path, format!("{v:?} is not one of {values:?}")));
            }
        }
        (FieldType::List { item, max_items }, ExtValue::List(items)) => {
            if items.len() > *max_items {
                issues.push(ValidationIssue::new(path, format!("more than {max_items} items")));
            }
            for (i, v) in items.iter().enumerate() {
                check_value(item, v, &format!("{path}[{i}]"), issues);
            }
        }
        _ => mismatch(issues),
    }
}

/// Collect every `(path, reference, allowed kinds)` in a record's extensions
/// so the repository can check targets exist and have the right kind.
pub(crate) fn extension_references<'a>(
    registry: &'a ExtensionRegistry,
    record: &'a Record,
) -> Vec<(String, &'a RecordId, &'a [RecordKind])> {
    let mut out = Vec::new();
    for (namespace, data) in &record.extensions {
        let Some(schema) = registry.get(namespace) else { continue };
        for (field, value) in &data.fields {
            let Some(spec) = schema.fields.get(field) else { continue };
            collect_refs(&spec.ty, value, format!("extensions.{namespace}.{field}"), &mut out);
        }
    }
    out
}

fn collect_refs<'a>(
    ty: &'a FieldType,
    value: &'a ExtValue,
    path: String,
    out: &mut Vec<(String, &'a RecordId, &'a [RecordKind])>,
) {
    match (ty, value) {
        (FieldType::Reference { kinds }, ExtValue::Reference(id)) => out.push((path, id, kinds.as_slice())),
        (FieldType::List { item, .. }, ExtValue::List(items)) => {
            for (i, v) in items.iter().enumerate() {
                collect_refs(item, v, format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}
