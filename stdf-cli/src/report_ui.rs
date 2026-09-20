// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
/// Inline shared presentation controls before report scripts, without touching evidence.
pub fn decorate(html: String) -> String {
    let settings = include_str!("report_ui/settings.html").replace(
        "__REPORT_TRANSLATIONS__",
        include_str!("report_ui/translations.json"),
    );
    html.replacen("<body>", &format!("<body>{settings}"), 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_languages_preserve_message_parameters() {
        let dict: serde_json::Value =
            serde_json::from_str(include_str!("report_ui/translations.json")).unwrap();
        let parameters = regex::Regex::new(r"\{\w+\}").unwrap();
        for (key, translations) in dict.as_object().unwrap() {
            let values = translations.as_array().unwrap();
            assert_eq!(values.len(), 5, "{key}");
            let keys = |s: &str| {
                parameters
                    .find_iter(s)
                    .map(|m| m.as_str().to_owned())
                    .collect::<std::collections::BTreeSet<_>>()
            };
            for value in values {
                let value = value.as_str().unwrap();
                assert!(!value.is_empty(), "{key}");
                assert_eq!(keys(key), keys(value), "{key}: {value}");
            }
        }
    }

    #[test]
    fn presentation_injection_preserves_raw_evidence_and_stream_marker() {
        let payload = r#"<script type="application/json" id="evidence">{"pattern":"Pass","value":null}</script>"#;
        let html = decorate(format!("<html><body>{payload}__TRACE_DATA__</body></html>"));
        assert!(html.contains(payload));
        assert_eq!(html.matches("__TRACE_DATA__").count(), 1);
        assert_eq!(html.matches("id=\"report-settings\"").count(), 1);
        assert!(html.find("window.ReportUI=").unwrap() < html.find(payload).unwrap());
    }
}
