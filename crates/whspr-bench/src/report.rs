use serde::Serialize;
use std::collections::BTreeMap;

/// Results for a single test case.
#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    pub file: String,
    pub criterion: String,
    pub wer: f32,
    pub cer: f32,
}

/// Aggregated results for a criterion group (e.g., "F", "AM").
#[derive(Debug, Clone, Serialize)]
pub struct GroupResult {
    pub prefix: String,
    pub case_count: usize,
    pub mean_wer: f32,
    pub mean_cer: f32,
}

/// Aggregated results across all cases.
#[derive(Debug, Clone, Serialize)]
pub struct AggregateResult {
    pub total_cases: usize,
    pub mean_wer: f32,
    pub mean_cer: f32,
}

/// Full benchmark report.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub cases: Vec<CaseResult>,
    pub groups: Vec<GroupResult>,
    pub aggregate: AggregateResult,
}

impl Report {
    /// Construct a report from case results.
    pub fn from_case_results(cases: Vec<CaseResult>) -> Self {
        let total_cases = cases.len();

        // Group by criterion prefix
        let mut groups: BTreeMap<String, Vec<&CaseResult>> = BTreeMap::new();
        for case in &cases {
            let prefix = case.criterion.split('-').next().unwrap_or(&case.criterion).to_string();
            groups.entry(prefix).or_default().push(case);
        }

        // Compute group stats
        let mut group_results = Vec::new();
        for (prefix, group_cases) in groups {
            let count = group_cases.len();
            let mean_wer = group_cases.iter().map(|c| c.wer).sum::<f32>() / count as f32;
            let mean_cer = group_cases.iter().map(|c| c.cer).sum::<f32>() / count as f32;

            group_results.push(GroupResult {
                prefix,
                case_count: count,
                mean_wer,
                mean_cer,
            });
        }

        // Compute aggregate stats
        let mean_wer = if total_cases > 0 {
            cases.iter().map(|c| c.wer).sum::<f32>() / total_cases as f32
        } else {
            0.0
        };
        let mean_cer = if total_cases > 0 {
            cases.iter().map(|c| c.cer).sum::<f32>() / total_cases as f32
        } else {
            0.0
        };

        let aggregate = AggregateResult {
            total_cases,
            mean_wer,
            mean_cer,
        };

        Report {
            cases,
            groups: group_results,
            aggregate,
        }
    }

    /// Format the report as plain text.
    pub fn format_text(&self) -> String {
        let mut output = String::new();

        output.push_str("=== Per-Case Results ===\n");
        output.push_str("File                              Criterion  WER      CER\n");
        output.push_str("---                               ---------  ---      ---\n");

        for case in &self.cases {
            output.push_str(&format!(
                "{:<32} {:<10} {:.4}   {:.4}\n",
                case.file, case.criterion, case.wer, case.cer
            ));
        }

        output.push_str("\n=== Per-Criterion-Group Results ===\n");
        output.push_str("Prefix  Cases  Mean WER  Mean CER\n");
        output.push_str("------  -----  --------  --------\n");

        for group in &self.groups {
            output.push_str(&format!(
                "{:<6}  {:<5}  {:.4}     {:.4}\n",
                group.prefix, group.case_count, group.mean_wer, group.mean_cer
            ));
        }

        output.push_str("\n=== Aggregate Results ===\n");
        output.push_str(&format!("Total Cases: {}\n", self.aggregate.total_cases));
        output.push_str(&format!("Mean WER:    {:.4}\n", self.aggregate.mean_wer));
        output.push_str(&format!("Mean CER:    {:.4}\n", self.aggregate.mean_cer));

        output
    }

    /// Format the report as JSON.
    pub fn format_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_from_case_results() {
        let cases = vec![
            CaseResult {
                file: "test1.wav".to_string(),
                criterion: "F-01".to_string(),
                wer: 0.1,
                cer: 0.05,
            },
            CaseResult {
                file: "test2.wav".to_string(),
                criterion: "F-02".to_string(),
                wer: 0.2,
                cer: 0.1,
            },
            CaseResult {
                file: "test3.wav".to_string(),
                criterion: "AM-01".to_string(),
                wer: 0.3,
                cer: 0.15,
            },
        ];

        let report = Report::from_case_results(cases);

        assert_eq!(report.aggregate.total_cases, 3);
        assert!(report.aggregate.mean_wer > 0.19 && report.aggregate.mean_wer < 0.21);
        assert!(report.aggregate.mean_cer > 0.09 && report.aggregate.mean_cer < 0.11);

        // Check groups
        assert_eq!(report.groups.len(), 2);

        let f_group = report.groups.iter().find(|g| g.prefix == "F").unwrap();
        assert_eq!(f_group.case_count, 2);

        let am_group = report
            .groups
            .iter()
            .find(|g| g.prefix == "AM")
            .unwrap();
        assert_eq!(am_group.case_count, 1);
    }

    #[test]
    fn test_report_format_text() {
        let cases = vec![CaseResult {
            file: "test.wav".to_string(),
            criterion: "F-01".to_string(),
            wer: 0.5,
            cer: 0.25,
        }];

        let report = Report::from_case_results(cases);
        let text = report.format_text();

        assert!(text.contains("Per-Case Results"));
        assert!(text.contains("test.wav"));
        assert!(text.contains("F-01"));
        assert!(text.contains("0.5000"));
        assert!(text.contains("0.2500"));
        assert!(text.contains("Aggregate Results"));
        assert!(text.contains("Total Cases: 1"));
    }

    #[test]
    fn test_report_format_json() {
        let cases = vec![CaseResult {
            file: "test.wav".to_string(),
            criterion: "F-01".to_string(),
            wer: 0.5,
            cer: 0.25,
        }];

        let report = Report::from_case_results(cases);
        let json = report.format_json().unwrap();

        // Parse as JSON to verify it's valid and contains expected fields
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("cases").is_some());
        assert_eq!(parsed["cases"][0]["file"], "test.wav");
        assert_eq!(parsed["cases"][0]["wer"], 0.5);
        assert_eq!(parsed["cases"][0]["cer"], 0.25);
    }
}
