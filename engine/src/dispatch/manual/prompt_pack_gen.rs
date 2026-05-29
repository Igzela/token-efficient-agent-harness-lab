use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PROMPT_PACK_SCHEMA_VERSION: &str = "prompt_pack.v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PromptPack {
    pub schema_version: String,
    pub pack_id: String,
    pub dispatch_id: String,
    pub task_description: String,
    pub context: String,
    pub instructions: String,
    pub output_format: Option<String>,
    pub constraints: Vec<String>,
    pub generated_at: String,
}

pub struct PromptPackGenerator;

impl Default for PromptPackGenerator {
    fn default() -> Self {
        Self
    }
}

impl PromptPackGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(
        &self,
        dispatch_id: &str,
        task_description: &str,
        context: &str,
        instructions: &str,
        output_format: Option<&str>,
        constraints: Vec<String>,
    ) -> PromptPack {
        PromptPack {
            schema_version: PROMPT_PACK_SCHEMA_VERSION.to_string(),
            pack_id: format!("pp-{}", &Uuid::new_v4().to_string().replace('-', "")[..12]),
            dispatch_id: dispatch_id.to_string(),
            task_description: task_description.to_string(),
            context: context.to_string(),
            instructions: instructions.to_string(),
            output_format: output_format.map(|s| s.to_string()),
            constraints,
            generated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    pub fn render_text(pack: &PromptPack) -> String {
        let mut parts = vec![
            format!("# Task\n{}", pack.task_description),
            format!("# Context\n{}", pack.context),
            format!("# Instructions\n{}", pack.instructions),
        ];
        if let Some(fmt) = &pack.output_format {
            parts.push(format!("# Output Format\n{}", fmt));
        }
        if !pack.constraints.is_empty() {
            parts.push(format!("# Constraints\n{}", pack.constraints.join("\n- ")));
        }
        parts.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_pack() {
        let gen = PromptPackGenerator::new();
        let p = gen.generate(
            "d1",
            "do thing",
            "ctx",
            "inst",
            Some("json"),
            vec!["c1".into()],
        );
        assert_eq!(p.dispatch_id, "d1");
        assert!(p.pack_id.starts_with("pp-"));
        assert_eq!(p.output_format, Some("json".to_string()));
    }

    #[test]
    fn render_text_includes_sections() {
        let gen = PromptPackGenerator::new();
        let p = gen.generate("d1", "task", "ctx", "inst", None, vec![]);
        let text = PromptPackGenerator::render_text(&p);
        assert!(text.contains("# Task"));
        assert!(text.contains("# Context"));
    }

    #[test]
    fn render_with_constraints() {
        let gen = PromptPackGenerator::new();
        let p = gen.generate(
            "d1",
            "t",
            "c",
            "i",
            None,
            vec!["rule1".into(), "rule2".into()],
        );
        let text = PromptPackGenerator::render_text(&p);
        assert!(text.contains("rule1"));
    }

    #[test]
    fn pack_serializes() {
        let p = PromptPackGenerator::new().generate("d1", "t", "c", "i", None, vec![]);
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["schema_version"], PROMPT_PACK_SCHEMA_VERSION);
    }

    #[test]
    fn pack_ids_unique() {
        let p1 = PromptPackGenerator::new().generate("d1", "t", "c", "i", None, vec![]);
        let p2 = PromptPackGenerator::new().generate("d2", "t", "c", "i", None, vec![]);
        assert_ne!(p1.pack_id, p2.pack_id);
    }
}
