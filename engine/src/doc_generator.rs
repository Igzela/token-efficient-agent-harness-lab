use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DOC_GENERATOR_SCHEMA_VERSION: &str = "doc_generator.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleDoc {
    pub name: String,
    pub description: String,
    pub structs: Vec<StructDoc>,
    pub functions: Vec<FunctionDoc>,
    pub constants: Vec<ConstantDoc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructDoc {
    pub name: String,
    pub fields: Vec<FieldDoc>,
    pub derives: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDoc {
    pub name: String,
    pub type_name: String,
    pub is_public: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDoc {
    pub name: String,
    pub params: Vec<ParamDoc>,
    pub return_type: Option<String>,
    pub is_public: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamDoc {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstantDoc {
    pub name: String,
    pub type_name: String,
    pub value_preview: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchemaRegistryEntry {
    pub schema_version: String,
    pub module: String,
    pub struct_name: String,
}

pub struct DocGenerator {
    modules: HashMap<String, ModuleDoc>,
    schema_registry: Vec<SchemaRegistryEntry>,
}

impl Default for DocGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DocGenerator {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            schema_registry: Vec::new(),
        }
    }

    pub fn register_module(&mut self, doc: ModuleDoc) {
        self.modules.insert(doc.name.clone(), doc);
    }

    pub fn register_schema(&mut self, entry: SchemaRegistryEntry) {
        self.schema_registry.push(entry);
    }

    pub fn get_module(&self, name: &str) -> Option<&ModuleDoc> {
        self.modules.get(name)
    }

    pub fn list_modules(&self) -> Vec<&str> {
        self.modules.keys().map(|s| s.as_str()).collect()
    }

    pub fn schema_registry(&self) -> &[SchemaRegistryEntry] {
        &self.schema_registry
    }

    pub fn generate_module_docs(&self, module_name: &str) -> Option<String> {
        let module = self.modules.get(module_name)?;
        let mut out = String::new();

        out.push_str(&format!("# Module: {}\n\n", module.name));
        if !module.description.is_empty() {
            out.push_str(&format!("{}\n\n", module.description));
        }

        if !module.constants.is_empty() {
            out.push_str("## Constants\n\n");
            for c in &module.constants {
                out.push_str(&format!(
                    "- `{}` ({}): {}\n",
                    c.name, c.type_name, c.value_preview
                ));
            }
            out.push('\n');
        }

        if !module.structs.is_empty() {
            out.push_str("## Structs\n\n");
            for s in &module.structs {
                out.push_str(&format!("### `{}`\n\n", s.name));
                if !s.derives.is_empty() {
                    out.push_str(&format!("Derives: {}\n\n", s.derives.join(", ")));
                }
                if !s.fields.is_empty() {
                    out.push_str("| Field | Type | Public |\n|---|---|---|\n");
                    for f in &s.fields {
                        out.push_str(&format!(
                            "| `{}` | `{}` | {} |\n",
                            f.name,
                            f.type_name,
                            if f.is_public { "yes" } else { "no" }
                        ));
                    }
                    out.push('\n');
                }
            }
        }

        if !module.functions.is_empty() {
            out.push_str("## Functions\n\n");
            for f in &module.functions {
                let params: Vec<String> = f
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name, p.type_name))
                    .collect();
                let ret = f.return_type.as_deref().unwrap_or("()");
                out.push_str(&format!(
                    "- `{}({}) -> {}`\n",
                    f.name,
                    params.join(", "),
                    ret
                ));
            }
            out.push('\n');
        }

        Some(out)
    }

    pub fn generate_schema_registry(&self) -> String {
        let mut out = String::from("# Schema Registry\n\n");
        out.push_str("| Schema Version | Module | Struct |\n|---|---|---|\n");
        for entry in &self.schema_registry {
            out.push_str(&format!(
                "| `{}` | `{}` | `{}` |\n",
                entry.schema_version, entry.module, entry.struct_name
            ));
        }
        out
    }

    pub fn generate_api_reference(&self) -> String {
        let mut out = String::from("# API Reference\n\n");
        let mut module_names: Vec<&str> = self.modules.keys().map(|s| s.as_str()).collect();
        module_names.sort();

        for name in &module_names {
            if let Some(doc) = self.generate_module_docs(name) {
                out.push_str(&doc);
                out.push_str("---\n\n");
            }
        }
        out
    }
}

pub fn parse_module_from_source(source: &str, module_name: &str) -> ModuleDoc {
    let mut structs = Vec::new();
    let mut functions = Vec::new();
    let mut constants = Vec::new();

    let mut current_struct: Option<String> = None;
    let mut current_fields: Vec<FieldDoc> = Vec::new();
    let mut current_derives: Vec<String> = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Parse derives
        if trimmed.starts_with("#[derive(") {
            let derives_str = trimmed
                .trim_start_matches("#[derive(")
                .trim_end_matches(")]");
            current_derives = derives_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            continue;
        }

        // Parse pub struct
        if trimmed.starts_with("pub struct ") {
            let rest = trimmed.trim_start_matches("pub struct ");
            let name = rest
                .split(|c: char| c == '{' || c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            if !name.is_empty() {
                // Save previous struct if any
                if let Some(prev_name) = current_struct.take() {
                    structs.push(StructDoc {
                        name: prev_name,
                        fields: current_fields.clone(),
                        derives: current_derives.clone(),
                    });
                    current_fields.clear();
                    current_derives.clear();
                }
                current_struct = Some(name);
            }
            continue;
        }

        // Parse pub fields inside struct
        if current_struct.is_some() && trimmed.starts_with("pub ") {
            let rest = trimmed.trim_start_matches("pub ");
            if let Some(colon_pos) = rest.find(':') {
                let field_name = rest[..colon_pos].trim().to_string();
                let type_name = rest[colon_pos + 1..]
                    .trim()
                    .trim_end_matches(',')
                    .to_string();
                current_fields.push(FieldDoc {
                    name: field_name,
                    type_name,
                    is_public: true,
                });
            }
            continue;
        }

        // Close struct on '}'
        if current_struct.is_some() && trimmed == "}" {
            if let Some(name) = current_struct.take() {
                structs.push(StructDoc {
                    name,
                    fields: current_fields.clone(),
                    derives: current_derives.clone(),
                });
                current_fields.clear();
                current_derives.clear();
            }
            continue;
        }

        // Parse pub fn
        if trimmed.starts_with("pub fn ") || trimmed.starts_with("pub const ") {
            if trimmed.starts_with("pub const ") {
                let rest = trimmed.trim_start_matches("pub const ");
                if let Some(colon_pos) = rest.find(':') {
                    let name = rest[..colon_pos].trim().to_string();
                    let after_colon = rest[colon_pos + 1..].trim();
                    let (type_name, value) = if let Some(eq_pos) = after_colon.find('=') {
                        (
                            after_colon[..eq_pos].trim().to_string(),
                            after_colon[eq_pos + 1..].trim().to_string(),
                        )
                    } else {
                        (after_colon.to_string(), String::new())
                    };
                    constants.push(ConstantDoc {
                        name,
                        type_name,
                        value_preview: value.chars().take(50).collect(),
                    });
                }
            } else {
                let rest = trimmed.trim_start_matches("pub fn ");
                let name = rest.split('(').next().unwrap_or("").to_string();
                functions.push(FunctionDoc {
                    name,
                    params: Vec::new(),
                    return_type: None,
                    is_public: true,
                });
            }
        }
    }

    // Flush last struct
    if let Some(name) = current_struct.take() {
        structs.push(StructDoc {
            name,
            fields: current_fields,
            derives: current_derives,
        });
    }

    ModuleDoc {
        name: module_name.to_string(),
        description: String::new(),
        structs,
        functions,
        constants,
    }
}
