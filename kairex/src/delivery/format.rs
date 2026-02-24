use crate::config::SetupFormat;
use crate::llm::schemas::{AlertReport, EveningReport, MiddayReport, MorningReport, WeeklyReport};
use crate::llm::types::{ScorecardEntry, Setup};

// -- Premium (full) formatters --

pub fn format_morning(report: &MorningReport, sf: SetupFormat) -> String {
    let mut out = String::new();

    out.push_str("<b>Morning Pre-Market</b>\n");
    out.push_str(&format!(
        "<b>Regime:</b> {} (Day {})\n\n",
        html_escape(&report.regime_status),
        report.regime_duration_days
    ));

    out.push_str(&html_escape(&report.market_narrative));
    out.push_str("\n\n");

    for asset in &report.assets {
        out.push_str(&format!(
            "<b>{}</b> — {}\n",
            html_escape(&asset.symbol),
            html_escape(&asset.narrative)
        ));
    }

    if !report.setups.is_empty() {
        out.push_str(&format!("\n{}", format_setups(&report.setups, sf)));
    }

    if !report.regime_narrative.is_empty() {
        out.push_str(&format!("\n{}", html_escape(&report.regime_narrative)));
    }

    out
}

pub fn format_midday(report: &MiddayReport, sf: SetupFormat) -> String {
    let mut out = String::new();

    out.push_str("<b>Midday Update</b>\n\n");

    out.push_str(&html_escape(&report.morning_reference_narrative));
    out.push_str("\n\n");

    for asset in &report.assets {
        out.push_str(&format!(
            "<b>{}</b> — {}\n",
            html_escape(&asset.symbol),
            html_escape(&asset.narrative)
        ));
    }

    if !report.setups.is_empty() {
        out.push_str(&format!("\n{}", format_setups(&report.setups, sf)));
    }

    out.push_str(&format!("\n{}", html_escape(&report.market_narrative)));

    out
}

pub fn format_evening(report: &EveningReport, sf: SetupFormat) -> String {
    let mut out = String::new();

    out.push_str("<b>Evening Recap</b>\n");
    out.push_str(&format!(
        "<b>Regime:</b> {} (Day {})\n\n",
        html_escape(&report.regime_status),
        report.regime_duration_days
    ));

    out.push_str(&html_escape(&report.market_narrative));
    out.push_str("\n\n");

    if !report.scorecard.is_empty() {
        out.push_str("<b>Scorecard</b>\n\n");
        for entry in &report.scorecard {
            out.push_str(&format_scorecard_entry(entry));
            out.push('\n');
        }
    }

    if !report.setups.is_empty() {
        out.push_str(&format!("\n{}", format_setups(&report.setups, sf)));
    }

    out.push_str(&format!(
        "\n<b>Overnight:</b> {}",
        html_escape(&report.overnight_narrative)
    ));

    out
}

pub fn format_alert(report: &AlertReport, sf: SetupFormat) -> String {
    let mut out = String::new();

    out.push_str(&format!("<b>Alert: {}</b>\n\n", html_escape(&report.asset)));

    out.push_str(&html_escape(&report.trigger_summary));
    out.push_str("\n\n");

    out.push_str(&html_escape(&report.context_narrative));
    out.push_str("\n\n");

    out.push_str(&format!(
        "<b>Watch:</b> {}",
        html_escape(&report.watch_narrative)
    ));

    if !report.setups.is_empty() {
        out.push_str(&format!("\n\n{}", format_setups(&report.setups, sf)));
    }

    out
}

pub fn format_weekly(report: &WeeklyReport, sf: SetupFormat) -> String {
    let mut out = String::new();

    out.push_str("<b>Weekly Briefing</b>\n");
    out.push_str(&format!(
        "<b>Regime:</b> {} (Day {})\n\n",
        html_escape(&report.regime_status),
        report.regime_duration_days
    ));

    out.push_str(&html_escape(&report.week_narrative));
    out.push_str("\n\n");

    // Scorecard summary
    let ss = &report.scorecard_summary;
    out.push_str("<b>Week Scorecard</b>\n");
    out.push_str(&format!(
        "{} setups: {} triggered ({:.0}% hit rate), {} invalidated, {} expired\n",
        ss.total_setups,
        ss.triggered,
        ss.hit_rate * 100.0,
        ss.invalidated,
        ss.expired
    ));
    if !ss.by_confidence.is_empty() {
        for bucket in &ss.by_confidence {
            out.push_str(&format!(
                "  {} ({}): {:.0}% hit rate\n",
                html_escape(&bucket.level),
                bucket.count,
                bucket.hit_rate * 100.0
            ));
        }
    }
    out.push_str(&format!("{}\n\n", html_escape(&ss.narrative)));

    // Assets
    for asset in &report.assets {
        out.push_str(&format!(
            "<b>{}</b> — {}\n",
            html_escape(&asset.symbol),
            html_escape(&asset.narrative)
        ));
    }

    // Regime assessment
    out.push_str(&format!(
        "\n<b>Regime Assessment:</b> {}\n\n",
        html_escape(&report.regime_assessment)
    ));
    out.push_str(&format!(
        "<b>What Would Change My Mind:</b> {}\n",
        html_escape(&report.what_would_change_my_mind)
    ));

    // Setups
    if !report.setups.is_empty() {
        out.push_str(&format!("\n{}", format_setups(&report.setups, sf)));
    }

    // Notebook
    out.push_str("\n<b>Notebook</b>\n");
    if !report.notebook.beliefs.is_empty() {
        out.push_str("<b>Beliefs:</b>\n");
        for belief in &report.notebook.beliefs {
            out.push_str(&format!("- {}\n", html_escape(belief)));
        }
    }
    if !report.notebook.biases.is_empty() {
        out.push_str("<b>Biases:</b>\n");
        for bias in &report.notebook.biases {
            out.push_str(&format!("- {}\n", html_escape(bias)));
        }
    }
    if !report.notebook.hypotheses.is_empty() {
        out.push_str("<b>Hypotheses:</b>\n");
        for h in &report.notebook.hypotheses {
            out.push_str(&format!("- {}\n", html_escape(h)));
        }
    }

    out
}

// -- Condensed formatters (for free channel) --

pub fn format_morning_condensed(report: &MorningReport, _sf: SetupFormat) -> String {
    let mut out = String::new();

    out.push_str("<b>Morning Pre-Market</b>\n");
    out.push_str(&format!(
        "<b>Regime:</b> {} (Day {})\n\n",
        html_escape(&report.regime_status),
        report.regime_duration_days
    ));

    out.push_str(&html_escape(&report.market_narrative));

    if !report.setups.is_empty() {
        let assets: Vec<&str> = report
            .setups
            .iter()
            .map(|s| s.asset.as_str())
            .collect::<Vec<_>>();
        let unique: Vec<&str> = {
            let mut v = assets;
            v.dedup();
            v
        };
        out.push_str(&format!(
            "\n\n{} setup{} across {}.",
            report.setups.len(),
            if report.setups.len() == 1 { "" } else { "s" },
            unique.join(", ")
        ));
    }

    out
}

pub fn format_evening_condensed(report: &EveningReport, _sf: SetupFormat) -> String {
    let mut out = String::new();

    out.push_str("<b>Evening Recap</b>\n");
    out.push_str(&format!(
        "<b>Regime:</b> {} (Day {})\n\n",
        html_escape(&report.regime_status),
        report.regime_duration_days
    ));

    if !report.scorecard.is_empty() {
        out.push_str("<b>Scorecard</b>\n\n");
        for entry in &report.scorecard {
            out.push_str(&format_scorecard_entry(entry));
            out.push('\n');
        }
    }

    out.push_str(&format!(
        "\n<b>Overnight:</b> {}",
        html_escape(&report.overnight_narrative)
    ));

    out
}

pub fn format_alert_condensed(report: &AlertReport, _sf: SetupFormat) -> String {
    let mut out = String::new();

    out.push_str(&format!("<b>Alert: {}</b>\n\n", html_escape(&report.asset)));

    out.push_str(&html_escape(&report.trigger_summary));
    out.push_str("\n\n");

    out.push_str(&html_escape(&report.context_narrative));
    out.push_str("\n\n");

    out.push_str(&format!(
        "<b>Watch:</b> {}",
        html_escape(&report.watch_narrative)
    ));

    out
}

// -- Shared helpers --

fn format_setups(setups: &[Setup], sf: SetupFormat) -> String {
    let mut out = String::from("<b>Setups</b>\n\n");
    for (i, setup) in setups.iter().enumerate() {
        out.push_str(&format_setup(setup, sf));
        if i < setups.len() - 1 {
            out.push('\n');
        }
    }
    out
}

fn format_setup(setup: &Setup, sf: SetupFormat) -> String {
    match sf {
        SetupFormat::DetailLine => format_setup_detail_line(setup),
        SetupFormat::InlineCompact => format_setup_inline_compact(setup),
        SetupFormat::Card => format_setup_card(setup),
    }
}

fn format_setup_detail_line(setup: &Setup) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "{} {} | <code>{}</code>\n",
        html_escape(&setup.asset),
        capitalize(&setup.direction),
        format_price(setup.trigger_level)
    ));

    let mut targets = Vec::new();
    if let Some(target) = setup.target_level {
        targets.push(format!("<code>{}</code>", format_price(target)));
    }
    if let Some(inv) = setup.invalidation_level {
        targets.push(format!("<code>{}</code>", format_price(inv)));
    }

    if !targets.is_empty() {
        out.push_str(&format!("\u{2192} {}", targets.join(" / ")));
        if let Some(conf) = setup.confidence {
            out.push_str(&format!(" | {:.0}%", conf * 100.0));
        }
        out.push('\n');
    } else if let Some(conf) = setup.confidence {
        out.push_str(&format!("{:.0}%\n", conf * 100.0));
    }

    out.push_str(&format!("{}\n", html_escape(&setup.narrative)));
    out
}

fn format_setup_inline_compact(setup: &Setup) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "{} {} <code>{}</code>",
        html_escape(&setup.asset),
        capitalize(&setup.direction),
        format_price(setup.trigger_level)
    ));

    if let Some(target) = setup.target_level {
        out.push_str(&format!(" \u{2192} <code>{}</code>", format_price(target)));
    }
    if let Some(inv) = setup.invalidation_level {
        out.push_str(&format!(" (inv <code>{}</code>)", format_price(inv)));
    }
    if let Some(conf) = setup.confidence {
        out.push_str(&format!(" {:.0}%", conf * 100.0));
    }
    out.push('\n');

    out.push_str(&format!("{}\n", html_escape(&setup.narrative)));
    out
}

fn format_setup_card(setup: &Setup) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "<b>{} {}</b>\n",
        html_escape(&setup.asset),
        capitalize(&setup.direction)
    ));
    out.push_str(&format!(
        "Trigger: <code>{}</code>\n",
        format_price(setup.trigger_level)
    ));

    let mut detail_parts = Vec::new();
    if let Some(target) = setup.target_level {
        detail_parts.push(format!("Target: <code>{}</code>", format_price(target)));
    }
    if let Some(inv) = setup.invalidation_level {
        detail_parts.push(format!("Invalid: <code>{}</code>", format_price(inv)));
    }
    if !detail_parts.is_empty() {
        out.push_str(&format!("{}\n", detail_parts.join(" | ")));
    }

    if let Some(conf) = setup.confidence {
        out.push_str(&format!("Confidence: {:.0}%\n", conf * 100.0));
    }

    out.push_str(&format!("{}\n", html_escape(&setup.narrative)));
    out
}

fn format_scorecard_entry(entry: &ScorecardEntry) -> String {
    let mut out = String::new();

    let price_str = match entry.outcome_price {
        Some(p) => format!(" at <code>{}</code>", format_price(p)),
        None => String::new(),
    };

    out.push_str(&format!(
        "{} {} — {} ({}){}\n",
        html_escape(&entry.asset),
        capitalize(&entry.direction),
        html_escape(&entry.outcome),
        html_escape(&entry.assessment),
        price_str
    ));

    if let Some(ref reason) = entry.miss_reason {
        out.push_str(&format!("  Miss: {}\n", html_escape(reason)));
    }

    out.push_str(&html_escape(&entry.narrative));
    out
}

pub fn format_price(price: f64) -> String {
    if price >= 1.0 {
        // Standard comma-separated format with 2 decimals
        let formatted = format!("{:.2}", price);
        add_thousands_separator(&formatted)
    } else {
        // Sub-dollar: show 4 decimal places
        let formatted = format!("{:.4}", price);
        add_thousands_separator(&formatted)
    }
}

fn add_thousands_separator(s: &str) -> String {
    let parts: Vec<&str> = s.splitn(2, '.').collect();
    let integer = parts[0];

    let negative = integer.starts_with('-');
    let digits = if negative { &integer[1..] } else { integer };

    let mut result = String::new();
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    let mut grouped: String = result.chars().rev().collect();
    if negative {
        grouped.insert(0, '-');
    }

    if parts.len() > 1 {
        format!("${}.{}", grouped, parts[1])
    } else {
        format!("${}", grouped)
    }
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn load_fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
        let path = format!(
            "{}/tests/fixtures/llm/{name}",
            env!("CARGO_MANIFEST_DIR").trim_end_matches("/kairex")
        );
        let json =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("failed to parse {path}: {e}"))
    }

    #[test]
    fn format_morning_report_nonempty() {
        let report: MorningReport = load_fixture("morning_report.json");
        let html = format_morning(&report, SetupFormat::DetailLine);
        assert!(!html.is_empty());
        assert!(html.contains("<b>Morning Pre-Market</b>"));
        assert!(html.contains("range_bound"));
        assert!(html.contains("BTCUSDT"));
        assert!(html.contains("ETHUSDT"));
    }

    #[test]
    fn format_midday_report_nonempty() {
        let report: MiddayReport = load_fixture("midday_report.json");
        let html = format_midday(&report, SetupFormat::DetailLine);
        assert!(!html.is_empty());
        assert!(html.contains("<b>Midday Update</b>"));
        assert!(html.contains("ETHUSDT"));
    }

    #[test]
    fn format_evening_report_nonempty() {
        let report: EveningReport = load_fixture("evening_report.json");
        let html = format_evening(&report, SetupFormat::DetailLine);
        assert!(!html.is_empty());
        assert!(html.contains("<b>Evening Recap</b>"));
        assert!(html.contains("<b>Scorecard</b>"));
        assert!(html.contains("Overnight:"));
    }

    #[test]
    fn format_alert_report_nonempty() {
        let report: AlertReport = load_fixture("alert_report.json");
        let html = format_alert(&report, SetupFormat::DetailLine);
        assert!(!html.is_empty());
        assert!(html.contains("<b>Alert: ETHUSDT</b>"));
        assert!(html.contains("Watch:"));
    }

    #[test]
    fn format_weekly_report_nonempty() {
        let report: WeeklyReport = load_fixture("weekly_report.json");
        let html = format_weekly(&report, SetupFormat::DetailLine);
        assert!(!html.is_empty());
        assert!(html.contains("<b>Weekly Briefing</b>"));
        assert!(html.contains("Week Scorecard"));
        assert!(html.contains("Notebook"));
        assert!(html.contains("Beliefs:"));
    }

    #[test]
    fn condensed_shorter_than_full() {
        let morning: MorningReport = load_fixture("morning_report.json");
        let full = format_morning(&morning, SetupFormat::DetailLine);
        let condensed = format_morning_condensed(&morning, SetupFormat::DetailLine);
        assert!(condensed.len() < full.len());

        let evening: EveningReport = load_fixture("evening_report.json");
        let full = format_evening(&evening, SetupFormat::DetailLine);
        let condensed = format_evening_condensed(&evening, SetupFormat::DetailLine);
        assert!(condensed.len() < full.len());

        let alert: AlertReport = load_fixture("alert_report.json");
        let full = format_alert(&alert, SetupFormat::DetailLine);
        let condensed = format_alert_condensed(&alert, SetupFormat::DetailLine);
        assert!(condensed.len() < full.len());
    }

    #[test]
    fn price_formatting() {
        assert_eq!(format_price(70000.0), "$70,000.00");
        assert_eq!(format_price(1880.0), "$1,880.00");
        assert_eq!(format_price(0.073), "$0.0730");
        assert_eq!(format_price(144.0), "$144.00");
        assert_eq!(format_price(1838.0), "$1,838.00");
    }

    #[test]
    fn html_escape_special_chars() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("price < $100"), "price &lt; $100");
        assert_eq!(html_escape("no special chars"), "no special chars");
    }

    #[test]
    fn setup_format_detail_line() {
        let setup = make_test_setup();
        let html = format_setup(&setup, SetupFormat::DetailLine);
        assert!(html.contains("\u{2192}")); // →
        assert!(html.contains("$1,880.00"));
        assert!(html.contains("72%"));
    }

    #[test]
    fn setup_format_inline_compact() {
        let setup = make_test_setup();
        let html = format_setup(&setup, SetupFormat::InlineCompact);
        assert!(html.contains("inv"));
        assert!(html.contains("$1,880.00"));
    }

    #[test]
    fn setup_format_card() {
        let setup = make_test_setup();
        let html = format_setup(&setup, SetupFormat::Card);
        assert!(html.contains("Trigger:"));
        assert!(html.contains("Target:"));
        assert!(html.contains("Invalid:"));
        assert!(html.contains("Confidence:"));
    }

    #[test]
    fn all_three_styles_produce_output_for_same_setup() {
        let setup = make_test_setup();
        let detail = format_setup(&setup, SetupFormat::DetailLine);
        let inline = format_setup(&setup, SetupFormat::InlineCompact);
        let card = format_setup(&setup, SetupFormat::Card);

        assert!(!detail.is_empty());
        assert!(!inline.is_empty());
        assert!(!card.is_empty());

        // All should contain the asset and price
        for html in &[&detail, &inline, &card] {
            assert!(html.contains("ETHUSDT"));
            assert!(html.contains("$1,880.00"));
        }
    }

    #[test]
    fn setup_without_optional_fields() {
        let setup = Setup {
            asset: "BTCUSDT".into(),
            direction: "long".into(),
            trigger_condition: "price_above".into(),
            trigger_level: 70000.0,
            trigger_field: None,
            target_level: None,
            invalidation_level: None,
            confidence: None,
            timeframe: None,
            narrative: "Breakout setup.".into(),
        };

        let detail = format_setup(&setup, SetupFormat::DetailLine);
        assert!(detail.contains("BTCUSDT"));
        assert!(detail.contains("$70,000.00"));
        assert!(detail.contains("Breakout setup."));

        let card = format_setup(&setup, SetupFormat::Card);
        assert!(card.contains("Trigger:"));
        assert!(!card.contains("Target:"));
        assert!(!card.contains("Confidence:"));
    }

    #[test]
    fn morning_condensed_shows_setup_count() {
        let report: MorningReport = load_fixture("morning_report.json");
        let condensed = format_morning_condensed(&report, SetupFormat::DetailLine);
        assert!(condensed.contains("2 setups across"));
        // Should NOT contain individual asset narratives
        assert!(!condensed.contains("Holding the range"));
    }

    #[test]
    fn scorecard_entry_formatting() {
        let entry = ScorecardEntry {
            asset: "ETHUSDT".into(),
            direction: "short".into(),
            trigger_level: 1880.0,
            outcome: "triggered".into(),
            outcome_price: Some(1838.0),
            assessment: "hit".into(),
            miss_reason: None,
            narrative: "Clean move.".into(),
        };

        let html = format_scorecard_entry(&entry);
        assert!(html.contains("ETHUSDT"));
        assert!(html.contains("triggered"));
        assert!(html.contains("hit"));
        assert!(html.contains("$1,838.00"));
        assert!(!html.contains("Miss:"));
    }

    fn make_test_setup() -> Setup {
        Setup {
            asset: "ETHUSDT".into(),
            direction: "short".into(),
            trigger_condition: "price_below".into(),
            trigger_level: 1880.0,
            trigger_field: None,
            target_level: Some(1820.0),
            invalidation_level: Some(1950.0),
            confidence: Some(0.72),
            timeframe: Some("intraday".into()),
            narrative: "Funding creep + liquidation cluster at $1,880.".into(),
        }
    }
}
