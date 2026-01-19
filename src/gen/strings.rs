use super::{generate_from_schema, Generate};
use serde_json::{json, Value};

pub struct TextGenerator {
    min_size: usize,
    max_size: Option<usize>,
}

impl TextGenerator {
    pub fn with_min_size(mut self, min: usize) -> Self {
        self.min_size = min;
        self
    }

    pub fn with_max_size(mut self, max: usize) -> Self {
        self.max_size = Some(max);
        self
    }
}

impl Generate<String> for TextGenerator {
    fn generate(&self) -> String {
        generate_from_schema(&self.schema().unwrap())
    }

    fn schema(&self) -> Option<Value> {
        let mut schema = json!({"type": "string"});

        if self.min_size > 0 {
            schema["minLength"] = json!(self.min_size);
        }

        if let Some(max) = self.max_size {
            schema["maxLength"] = json!(max);
        }

        Some(schema)
    }
}

pub fn text() -> TextGenerator {
    TextGenerator {
        min_size: 0,
        max_size: None,
    }
}

pub struct RegexGenerator {
    pattern: String,
}

impl Generate<String> for RegexGenerator {
    fn generate(&self) -> String {
        generate_from_schema(&self.schema().unwrap())
    }

    fn schema(&self) -> Option<Value> {
        Some(json!({
            "type": "string",
            "pattern": self.pattern
        }))
    }
}

pub fn from_regex(pattern: &str) -> RegexGenerator {
    let anchored = if pattern.starts_with('^') && pattern.ends_with('$') {
        pattern.to_string()
    } else if pattern.starts_with('^') {
        format!("{}$", pattern)
    } else if pattern.ends_with('$') {
        format!("^{}", pattern)
    } else {
        format!("^{}$", pattern)
    };

    RegexGenerator { pattern: anchored }
}
