use crate::types::Rule;

/// Layer 3: learned compression rules.
///
/// These rules are discovered at runtime through feedback and stored in the
/// database. Only rules that meet the confidence threshold and have enough
/// samples are applied.
#[derive(Debug, Clone)]
pub struct LearnedLayer {
    rules: Vec<Rule>,
    confidence_threshold: f64,
    min_samples: i64,
}

impl LearnedLayer {
    /// Create a new learned layer with the given rules and filtering parameters.
    pub fn new(rules: Vec<Rule>, confidence_threshold: f64, min_samples: i64) -> Self {
        Self {
            rules,
            confidence_threshold,
            min_samples,
        }
    }

    /// Return only rules that meet both the confidence threshold and minimum
    /// sample requirements.
    pub fn active_rules(&self) -> Vec<Rule> {
        self.rules
            .iter()
            .filter(|r| {
                r.enabled
                    && r.confidence >= self.confidence_threshold
                    && r.samples >= self.min_samples
            })
            .cloned()
            .collect()
    }

    /// Update a rule's confidence using exponential moving average (EMA).
    ///
    /// Formula: `confidence = alpha * new_score + (1 - alpha) * old_confidence`
    ///
    /// Also increments the sample count by 1.
    pub fn update_confidence(rule: &mut Rule, new_score: f64, alpha: f64) {
        rule.confidence = alpha * new_score + (1.0 - alpha) * rule.confidence;
        rule.samples += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RuleLayer;

    fn make_rule(confidence: f64, samples: i64, enabled: bool) -> Rule {
        Rule {
            id: "test".to_string(),
            pattern: "some pattern".to_string(),
            replacement: "short".to_string(),
            layer: RuleLayer::Learned,
            domain: None,
            confidence,
            samples,
            enabled,
            priority: 50,
        }
    }

    #[test]
    fn test_active_rules_filters_correctly() {
        let rules = vec![
            make_rule(0.9, 20, true),  // passes
            make_rule(0.5, 20, true),  // low confidence
            make_rule(0.9, 5, true),   // low samples
            make_rule(0.9, 20, false), // disabled
        ];
        let layer = LearnedLayer::new(rules, 0.8, 10);
        let active = layer.active_rules();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].confidence, 0.9);
    }

    #[test]
    fn test_update_confidence_ema() {
        let mut rule = make_rule(0.8, 10, true);
        // alpha=0.1, new_score=1.0
        // new confidence = 0.1 * 1.0 + 0.9 * 0.8 = 0.1 + 0.72 = 0.82
        LearnedLayer::update_confidence(&mut rule, 1.0, 0.1);
        assert!((rule.confidence - 0.82).abs() < 1e-10);
        assert_eq!(rule.samples, 11);
    }

    #[test]
    fn test_update_confidence_bad_score() {
        let mut rule = make_rule(0.8, 10, true);
        // alpha=0.1, new_score=0.0
        // new confidence = 0.1 * 0.0 + 0.9 * 0.8 = 0.72
        LearnedLayer::update_confidence(&mut rule, 0.0, 0.1);
        assert!((rule.confidence - 0.72).abs() < 1e-10);
        assert_eq!(rule.samples, 11);
    }
}
