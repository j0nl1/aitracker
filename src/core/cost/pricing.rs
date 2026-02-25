/// Per-model token pricing in dollars per token.
#[derive(Debug, Clone)]
pub struct ModelPricing {
    pub model: &'static str,
    pub input_per_token: f64,
    pub output_per_token: f64,
    pub cache_read_per_token: f64,
    pub cache_create_per_token: f64,
}

/// All known model pricing entries.
static PRICING_TABLE: &[ModelPricing] = &[
    ModelPricing {
        model: "claude-haiku-4-5",
        input_per_token: 1e-6,
        output_per_token: 5e-6,
        cache_read_per_token: 1e-7,
        cache_create_per_token: 1.25e-6,
    },
    ModelPricing {
        model: "claude-sonnet-4-5",
        input_per_token: 3e-6,
        output_per_token: 1.5e-5,
        cache_read_per_token: 3e-7,
        cache_create_per_token: 3.75e-6,
    },
    ModelPricing {
        model: "claude-sonnet-4",
        input_per_token: 3e-6,
        output_per_token: 1.5e-5,
        cache_read_per_token: 3e-7,
        cache_create_per_token: 3.75e-6,
    },
    ModelPricing {
        model: "claude-opus-4-5",
        input_per_token: 5e-6,
        output_per_token: 2.5e-5,
        cache_read_per_token: 5e-7,
        cache_create_per_token: 6.25e-6,
    },
    ModelPricing {
        model: "claude-opus-4-6",
        input_per_token: 5e-6,
        output_per_token: 2.5e-5,
        cache_read_per_token: 5e-7,
        cache_create_per_token: 6.25e-6,
    },
    ModelPricing {
        model: "claude-opus-4",
        input_per_token: 1.5e-5,
        output_per_token: 7.5e-5,
        cache_read_per_token: 1.5e-6,
        cache_create_per_token: 1.875e-5,
    },
    // GPT / Codex models
    ModelPricing {
        model: "gpt-5",
        input_per_token: 1.25e-6,
        output_per_token: 1e-5,
        cache_read_per_token: 1.25e-7,
        cache_create_per_token: 0.0,
    },
    ModelPricing {
        model: "gpt-5-codex",
        input_per_token: 1.25e-6,
        output_per_token: 1e-5,
        cache_read_per_token: 1.25e-7,
        cache_create_per_token: 0.0,
    },
    ModelPricing {
        model: "gpt-5.1",
        input_per_token: 1.25e-6,
        output_per_token: 1e-5,
        cache_read_per_token: 1.25e-7,
        cache_create_per_token: 0.0,
    },
    ModelPricing {
        model: "gpt-5.2",
        input_per_token: 1.75e-6,
        output_per_token: 1.4e-5,
        cache_read_per_token: 1.75e-7,
        cache_create_per_token: 0.0,
    },
    ModelPricing {
        model: "gpt-5.2-codex",
        input_per_token: 1.75e-6,
        output_per_token: 1.4e-5,
        cache_read_per_token: 1.75e-7,
        cache_create_per_token: 0.0,
    },
    ModelPricing {
        model: "gpt-5.3-codex",
        input_per_token: 1.75e-6,
        output_per_token: 1.4e-5,
        cache_read_per_token: 1.75e-7,
        cache_create_per_token: 0.0,
    },
];

/// Normalize a model name by stripping common prefixes and suffixes.
/// Examples:
///   "anthropic.claude-sonnet-4-5-v2:0" -> "claude-sonnet-4-5"
///   "claude-sonnet-4-5-20250514" -> "claude-sonnet-4-5"
fn normalize_model(model: &str) -> String {
    let mut name = model.to_string();

    // Strip "anthropic." prefix
    if let Some(stripped) = name.strip_prefix("anthropic.") {
        name = stripped.to_string();
    }

    // Strip Vertex/Bedrock suffixes like "-v2:0", ":0", "@001"
    if let Some(idx) = name.find(":") {
        name.truncate(idx);
    }
    if let Some(idx) = name.find("@") {
        name.truncate(idx);
    }
    // Strip "-v2" or similar version suffix at end
    if let Some(idx) = name.rfind("-v") {
        if name[idx + 2..].chars().all(|c| c.is_ascii_digit()) {
            name.truncate(idx);
        }
    }

    // Strip date suffixes like "-20250514"
    if name.len() > 9 {
        let tail = &name[name.len() - 9..];
        if tail.starts_with('-')
            && tail[1..].len() == 8
            && tail[1..].chars().all(|c| c.is_ascii_digit())
        {
            name.truncate(name.len() - 9);
        }
    }

    name
}

/// Look up pricing for a model name. Returns None if unknown.
pub fn lookup(model: &str) -> Option<&'static ModelPricing> {
    let normalized = normalize_model(model);
    PRICING_TABLE
        .iter()
        .find(|p| p.model == normalized)
}

/// Calculate cost for given token counts.
pub fn calculate_cost(
    pricing: &ModelPricing,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
) -> (f64, f64, f64, f64) {
    let input_cost = input_tokens as f64 * pricing.input_per_token;
    let output_cost = output_tokens as f64 * pricing.output_per_token;
    let cache_read_cost = cache_read_tokens as f64 * pricing.cache_read_per_token;
    let cache_creation_cost = cache_creation_tokens as f64 * pricing.cache_create_per_token;
    (input_cost, output_cost, cache_read_cost, cache_creation_cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_anthropic_prefix() {
        assert_eq!(normalize_model("anthropic.claude-sonnet-4-5"), "claude-sonnet-4-5");
    }

    #[test]
    fn normalize_strips_date_suffix() {
        assert_eq!(normalize_model("claude-sonnet-4-5-20250514"), "claude-sonnet-4-5");
    }

    #[test]
    fn normalize_strips_vertex_suffix() {
        assert_eq!(normalize_model("claude-sonnet-4-5-v2:0"), "claude-sonnet-4-5");
    }

    #[test]
    fn normalize_strips_at_suffix() {
        assert_eq!(normalize_model("claude-opus-4-5@001"), "claude-opus-4-5");
    }

    #[test]
    fn normalize_combined() {
        assert_eq!(
            normalize_model("anthropic.claude-haiku-4-5-20250514-v2:0"),
            "claude-haiku-4-5"
        );
    }

    #[test]
    fn normalize_passthrough() {
        assert_eq!(normalize_model("claude-opus-4-6"), "claude-opus-4-6");
    }

    #[test]
    fn lookup_known_model() {
        let p = lookup("claude-sonnet-4-5").unwrap();
        assert!((p.input_per_token - 3e-6).abs() < 1e-12);
        assert!((p.output_per_token - 1.5e-5).abs() < 1e-12);
    }

    #[test]
    fn lookup_with_prefix_and_suffix() {
        let p = lookup("anthropic.claude-opus-4-6-20250514").unwrap();
        assert!((p.input_per_token - 5e-6).abs() < 1e-12);
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert!(lookup("gpt-4o").is_none());
    }

    #[test]
    fn lookup_gpt5() {
        let p = lookup("gpt-5").unwrap();
        assert!((p.input_per_token - 1.25e-6).abs() < 1e-12);
        assert!((p.output_per_token - 1e-5).abs() < 1e-12);
    }

    #[test]
    fn lookup_gpt5_2() {
        let p = lookup("gpt-5.2").unwrap();
        assert!((p.input_per_token - 1.75e-6).abs() < 1e-12);
    }

    #[test]
    fn lookup_gpt5_3_codex() {
        let p = lookup("gpt-5.3-codex").unwrap();
        assert!((p.input_per_token - 1.75e-6).abs() < 1e-12);
        assert!((p.output_per_token - 1.4e-5).abs() < 1e-12);
    }

    #[test]
    fn normalize_vertex_model_strips_at() {
        // normalize_model already handles @ in model names (used by Vertex AI)
        assert_eq!(normalize_model("claude-opus-4-5@20251101"), "claude-opus-4-5");
    }

    #[test]
    fn calculate_cost_basic() {
        let p = lookup("claude-sonnet-4-5").unwrap();
        let (ic, oc, crc, ccc) = calculate_cost(p, 1_000_000, 100_000, 500_000, 50_000);
        assert!((ic - 3.0).abs() < 1e-6);      // 1M * 3e-6
        assert!((oc - 1.5).abs() < 1e-6);      // 100K * 1.5e-5
        assert!((crc - 0.15).abs() < 1e-6);    // 500K * 3e-7
        assert!((ccc - 0.1875).abs() < 1e-6);  // 50K * 3.75e-6
    }
}
