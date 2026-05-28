use engine::doc_generator::*;

fn sample_module() -> ModuleDoc {
    ModuleDoc {
        name: "test_module".to_string(),
        description: "A test module.".to_string(),
        structs: vec![StructDoc {
            name: "MyStruct".to_string(),
            fields: vec![
                FieldDoc {
                    name: "id".to_string(),
                    type_name: "String".to_string(),
                    is_public: true,
                },
                FieldDoc {
                    name: "count".to_string(),
                    type_name: "u64".to_string(),
                    is_public: true,
                },
            ],
            derives: vec!["Debug".to_string(), "Clone".to_string()],
        }],
        functions: vec![FunctionDoc {
            name: "do_thing".to_string(),
            params: vec![ParamDoc {
                name: "input".to_string(),
                type_name: "str".to_string(),
            }],
            return_type: Some("Result<()>".to_string()),
            is_public: true,
        }],
        constants: vec![ConstantDoc {
            name: "VERSION".to_string(),
            type_name: "&str".to_string(),
            value_preview: "\"1.0\"".to_string(),
        }],
    }
}

#[test]
fn test_schema_version() {
    assert_eq!(DOC_GENERATOR_SCHEMA_VERSION, "doc_generator.v1");
}

#[test]
fn test_new_generator_is_empty() {
    let gen = DocGenerator::new();
    assert!(gen.list_modules().is_empty());
    assert!(gen.schema_registry().is_empty());
}

#[test]
fn test_register_and_get_module() {
    let mut gen = DocGenerator::new();
    let md = sample_module();
    gen.register_module(md);
    assert!(gen.get_module("test_module").is_some());
    assert!(gen.get_module("missing").is_none());
}

#[test]
fn test_list_modules_sorted() {
    let mut gen = DocGenerator::new();
    gen.register_module(ModuleDoc {
        name: "beta".to_string(),
        description: String::new(),
        structs: vec![],
        functions: vec![],
        constants: vec![],
    });
    gen.register_module(ModuleDoc {
        name: "alpha".to_string(),
        description: String::new(),
        structs: vec![],
        functions: vec![],
        constants: vec![],
    });
    let names = gen.list_modules();
    assert_eq!(names.len(), 2);
}

#[test]
fn test_register_schema() {
    let mut gen = DocGenerator::new();
    gen.register_schema(SchemaRegistryEntry {
        schema_version: "test.v1".to_string(),
        module: "test_mod".to_string(),
        struct_name: "TestStruct".to_string(),
    });
    assert_eq!(gen.schema_registry().len(), 1);
    assert_eq!(gen.schema_registry()[0].schema_version, "test.v1");
}

#[test]
fn test_generate_module_docs_none_for_missing() {
    let gen = DocGenerator::new();
    assert!(gen.generate_module_docs("missing").is_none());
}

#[test]
fn test_generate_module_docs_contains_heading() {
    let mut gen = DocGenerator::new();
    gen.register_module(sample_module());
    let doc = gen.generate_module_docs("test_module").unwrap();
    assert!(doc.contains("# Module: test_module"));
    assert!(doc.contains("A test module."));
}

#[test]
fn test_generate_module_docs_contains_constants() {
    let mut gen = DocGenerator::new();
    gen.register_module(sample_module());
    let doc = gen.generate_module_docs("test_module").unwrap();
    assert!(doc.contains("## Constants"));
    assert!(doc.contains("VERSION"));
    assert!(doc.contains("&str"));
}

#[test]
fn test_generate_module_docs_contains_structs() {
    let mut gen = DocGenerator::new();
    gen.register_module(sample_module());
    let doc = gen.generate_module_docs("test_module").unwrap();
    assert!(doc.contains("## Structs"));
    assert!(doc.contains("MyStruct"));
    assert!(doc.contains("Debug, Clone"));
    assert!(doc.contains("| Field | Type | Public |"));
    assert!(doc.contains("id"));
}

#[test]
fn test_generate_module_docs_contains_functions() {
    let mut gen = DocGenerator::new();
    gen.register_module(sample_module());
    let doc = gen.generate_module_docs("test_module").unwrap();
    assert!(doc.contains("## Functions"));
    assert!(doc.contains("do_thing"));
    assert!(doc.contains("Result<()>"));
}

#[test]
fn test_generate_schema_registry_table() {
    let mut gen = DocGenerator::new();
    gen.register_schema(SchemaRegistryEntry {
        schema_version: "foo.v1".to_string(),
        module: "foo_mod".to_string(),
        struct_name: "Foo".to_string(),
    });
    let table = gen.generate_schema_registry();
    assert!(table.contains("# Schema Registry"));
    assert!(table.contains("foo.v1"));
    assert!(table.contains("foo_mod"));
    assert!(table.contains("Foo"));
}

#[test]
fn test_generate_api_reference_empty() {
    let gen = DocGenerator::new();
    let api = gen.generate_api_reference();
    assert!(api.contains("# API Reference"));
}

#[test]
fn test_generate_api_reference_multiple_modules() {
    let mut gen = DocGenerator::new();
    gen.register_module(sample_module());
    gen.register_module(ModuleDoc {
        name: "other".to_string(),
        description: "Other module.".to_string(),
        structs: vec![],
        functions: vec![],
        constants: vec![],
    });
    let api = gen.generate_api_reference();
    assert!(api.contains("test_module"));
    assert!(api.contains("other"));
}

#[test]
fn test_parse_module_from_source_basic() {
    let source = r#"
#[derive(Debug, Clone)]
pub struct Config {
    pub name: String,
    pub timeout: u64,
}

pub fn process(input: &str) -> Result<()> {
    Ok(())
}

pub const MAX_RETRIES: u32 = 3;
"#;
    let md = parse_module_from_source(source, "config_mod");
    assert_eq!(md.name, "config_mod");
    assert_eq!(md.structs.len(), 1);
    assert_eq!(md.structs[0].name, "Config");
    assert_eq!(md.structs[0].derives, vec!["Debug", "Clone"]);
    assert_eq!(md.structs[0].fields.len(), 2);
    assert_eq!(md.structs[0].fields[0].name, "name");
    assert_eq!(md.structs[0].fields[0].type_name, "String");
    assert_eq!(md.functions.len(), 1);
    assert_eq!(md.functions[0].name, "process");
    assert_eq!(md.constants.len(), 1);
    assert_eq!(md.constants[0].name, "MAX_RETRIES");
    assert_eq!(md.constants[0].type_name, "u32");
}

#[test]
fn test_parse_module_empty_source() {
    let md = parse_module_from_source("", "empty");
    assert_eq!(md.name, "empty");
    assert!(md.structs.is_empty());
    assert!(md.functions.is_empty());
    assert!(md.constants.is_empty());
}
