use crate::config::{FormatMode, FreeChannelConfig, RouteMode};
use crate::llm::types::Significance;
use crate::llm::ReportType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    Skip,
    Send(FormatMode),
}

pub struct FreeChannelRouter {
    config: FreeChannelConfig,
}

impl FreeChannelRouter {
    pub fn new(config: FreeChannelConfig) -> Self {
        Self { config }
    }

    pub fn evaluate(&self, report_type: ReportType, significance: &Significance) -> RouteDecision {
        let section_name = match report_type {
            ReportType::Morning => "morning_report",
            ReportType::Evening => "evening_recap",
            ReportType::Weekly => "weekly_scorecard",
            ReportType::Alert => "alerts",
            ReportType::Midday => return RouteDecision::Skip,
        };

        let route_config = match self.config.routes.get(section_name) {
            Some(rc) => rc,
            None => return RouteDecision::Skip,
        };

        match route_config.route {
            RouteMode::Always => RouteDecision::Send(route_config.format),
            RouteMode::Never => RouteDecision::Skip,
            RouteMode::Threshold => {
                if evaluate_rules(&route_config.rules, significance) {
                    RouteDecision::Send(route_config.format)
                } else {
                    RouteDecision::Skip
                }
            }
        }
    }
}

/// Evaluate routing rules against significance. Rules are OR'd — any match means route.
fn evaluate_rules(rules: &[crate::config::RouteRule], significance: &Significance) -> bool {
    rules.iter().any(|rule| evaluate_rule(rule, significance))
}

fn evaluate_rule(rule: &crate::config::RouteRule, significance: &Significance) -> bool {
    match rule.op.as_str() {
        ">" => {
            if let Some(ref field) = rule.field {
                let val = lookup_field(field, significance);
                val > rule.value
            } else {
                false
            }
        }
        "all_above" => {
            if let Some(ref fields) = rule.fields {
                fields
                    .iter()
                    .all(|f| lookup_field(f, significance) > rule.value)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn lookup_field(field: &str, significance: &Significance) -> f64 {
    match field {
        "magnitude" => significance.magnitude,
        "surprise" => significance.surprise,
        "regime_relevance" => significance.regime_relevance,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FreeChannelConfig;
    use std::path::PathBuf;

    fn load_router() -> FreeChannelRouter {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("tests/fixtures/config/free_channel.toml");
        let config = FreeChannelConfig::load(&path).unwrap();
        FreeChannelRouter::new(config)
    }

    fn sig(magnitude: f64, surprise: f64, regime_relevance: f64) -> Significance {
        Significance {
            magnitude,
            surprise,
            regime_relevance,
        }
    }

    #[test]
    fn evening_always_routes_condensed() {
        let router = load_router();
        let result = router.evaluate(ReportType::Evening, &sig(0.1, 0.1, 0.1));
        assert_eq!(result, RouteDecision::Send(FormatMode::Condensed));
    }

    #[test]
    fn weekly_always_routes_pass_through() {
        let router = load_router();
        let result = router.evaluate(ReportType::Weekly, &sig(0.1, 0.1, 0.1));
        assert_eq!(result, RouteDecision::Send(FormatMode::PassThrough));
    }

    #[test]
    fn midday_always_skips() {
        let router = load_router();
        let result = router.evaluate(ReportType::Midday, &sig(0.9, 0.9, 0.9));
        assert_eq!(result, RouteDecision::Skip);
    }

    #[test]
    fn morning_high_magnitude_routes() {
        let router = load_router();
        // magnitude 0.8 > 0.7 threshold
        let result = router.evaluate(ReportType::Morning, &sig(0.8, 0.1, 0.1));
        assert_eq!(result, RouteDecision::Send(FormatMode::Condensed));
    }

    #[test]
    fn morning_all_above_routes() {
        let router = load_router();
        // surprise 0.6 > 0.5 AND regime_relevance 0.6 > 0.5
        let result = router.evaluate(ReportType::Morning, &sig(0.3, 0.6, 0.6));
        assert_eq!(result, RouteDecision::Send(FormatMode::Condensed));
    }

    #[test]
    fn morning_below_thresholds_skips() {
        let router = load_router();
        // magnitude 0.3 < 0.7, surprise 0.6 but regime_relevance 0.4 < 0.5
        let result = router.evaluate(ReportType::Morning, &sig(0.3, 0.6, 0.4));
        assert_eq!(result, RouteDecision::Skip);
    }

    #[test]
    fn alert_high_magnitude_routes() {
        let router = load_router();
        // magnitude 0.85 > 0.8 threshold
        let result = router.evaluate(ReportType::Alert, &sig(0.85, 0.1, 0.1));
        assert_eq!(result, RouteDecision::Send(FormatMode::Condensed));
    }

    #[test]
    fn alert_below_thresholds_skips() {
        let router = load_router();
        // magnitude 0.5 < 0.8, and for all_above: both need > 0.6 but surprise is only 0.5
        let result = router.evaluate(ReportType::Alert, &sig(0.5, 0.5, 0.5));
        assert_eq!(result, RouteDecision::Skip);
    }
}
