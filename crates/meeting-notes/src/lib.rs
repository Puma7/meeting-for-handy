//! Source-linked text analysis, separated into data, validation, transport and orchestration.
mod control;
pub mod diagnostics;
mod listing;
pub mod live;
mod markdown;
mod models;
pub mod name_candidates;
pub mod residency;
mod service;
mod templates;
mod transport;
mod validation;
pub use control::{is_cancelled, new_job_id, AnalysisControl, AnalysisProgress};
pub use listing::{
    is_run_id, list_analyses, skip_reason, AnalysisListing, ListedAnalysis, PendingListing,
    SkipReason, SkippedAnalysis,
};
pub use markdown::render_markdown;
pub use models::{
    Analysis, CheckedAnswer, Connection, Evidence, Fact, Kind, PendingAnalysis, RejectionReason,
    Rejections, Section, Source, Step, Template, RESULT_SCHEMA,
};
pub use service::{
    analyze, analyze_controlled, analyze_controlled_resident, read_analysis, read_pending_analysis,
    read_snapshot, render_text, source_hash, UnsupportedFormat,
};
pub use templates::{validate_library, TemplateLibrary, TemplateStore};
pub use transport::{credential_account, local_json_lines, local_model_ids, TextClient};
pub use validation::{validate_answer, validate_template, MAX_ITEMS_PER_ANSWER};
pub const PROMPT_VERSION: &str = "handymeeting-rust-notes-3";

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn config(endpoint: &str) -> Connection {
        Connection {
            endpoint: endpoint.into(),
            model: "local-test".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        }
    }
    fn sources() -> Vec<Source> {
        vec![Source {
            id: "S00001".into(),
            source: "system".into(),
            start_ms: 0,
            end_ms: 1000,
            text: "Anna schreibt den Bericht bis Freitag.".into(),
        }]
    }
    #[test]
    fn local_rejects_remote_credentials_and_redirectable_url_parts() {
        for url in [
            "https://example.com/v1/",
            "http://127.0.0.1@evil.test/",
            "http://127.0.0.1/v1/?key=secret",
            "file:///tmp/model",
        ] {
            assert!(TextClient::new(&config(url)).is_err());
        }
    }
    #[test]
    fn cloud_needs_per_run_consent_and_https() {
        let mut c = config("https://api.example.com/v1/");
        c.mode = "cloud".into();
        assert!(TextClient::new(&c).is_err());
        c.cloud_consent = true;
        assert!(TextClient::new(&c).is_ok());
        c.endpoint = "http://api.example.com/v1/".into();
        assert!(TextClient::new(&c).is_err());
    }
    const QUOTE: &str = "Anna schreibt den Bericht bis Freitag.";
    fn item(text: &str, owner: &str, due: &str, evidence: &str) -> String {
        format!(r#"{{"text":"{text}","owner":{owner},"due":{due},"evidence":[{evidence}]}}"#)
    }
    fn valid_evidence() -> String {
        format!(r#"{{"sourceId":"S00001","quote":"{QUOTE}"}}"#)
    }
    fn answer(items: &[String]) -> String {
        format!(r#"{{"items":[{}]}}"#, items.join(","))
    }
    fn only(reason: RejectionReason, count: usize) -> Rejections {
        Rejections::from([(reason, count)])
    }
    #[test]
    fn non_literal_quote_or_unknown_source_is_discarded_never_kept() {
        for evidence in [
            r#"{"sourceId":"S00002","quote":"Anna schreibt"}"#,
            r#"{"sourceId":"S00001","quote":"Anna prüft den Bericht"}"#,
            r#"{"sourceId":"S00001","quote":"Anna"}"#,
        ] {
            let json = answer(&[item("Bericht", "null", "null", evidence)]);
            let checked = validate_answer(&json, &sources(), &Kind::Summary, "t").unwrap();
            assert!(checked.items.is_empty());
            assert_eq!(checked.rejected, only(RejectionReason::QuoteNotLiteral, 1));
        }
        // One literal quote does not rescue an item that also cites an invented one.
        let mixed = format!(
            r#"{},{{"sourceId":"S00001","quote":"Anna prüft den Bericht"}}"#,
            valid_evidence()
        );
        let json = answer(&[item("Bericht", "null", "null", &mixed)]);
        let checked = validate_answer(&json, &sources(), &Kind::Summary, "t").unwrap();
        assert!(checked.items.is_empty());
        assert_eq!(checked.rejected, only(RejectionReason::QuoteNotLiteral, 1));
    }
    #[test]
    fn one_invalid_statement_no_longer_discards_the_valid_ones() {
        let json = answer(&[
            item("Erste", "null", "null", &valid_evidence()),
            item(
                "Erfunden",
                "null",
                "null",
                r#"{"sourceId":"S00001","quote":"Ben schreibt die Abrechnung."}"#,
            ),
            item("Zweite", "null", "null", &valid_evidence()),
        ]);
        let checked = validate_answer(&json, &sources(), &Kind::Summary, "t").unwrap();
        let kept: Vec<_> = checked.items.iter().map(|f| f.text.as_str()).collect();
        assert_eq!(kept, ["Erste", "Zweite"]);
        assert_eq!(checked.rejected, only(RejectionReason::QuoteNotLiteral, 1));
        for fact in &checked.items {
            assert!(fact.evidence.iter().all(|e| QUOTE.contains(&e.quote)));
        }
    }
    #[test]
    fn invented_owner_is_discarded_and_duplicate_field_still_fails_hard() {
        let json = answer(&[item("Bericht", r#""Ben""#, "null", &valid_evidence())]);
        let checked = validate_answer(&json, &sources(), &Kind::Tasks, "t").unwrap();
        assert!(checked.items.is_empty());
        assert_eq!(
            checked.rejected,
            only(RejectionReason::UnverifiedOwnerOrDue, 1)
        );
        assert!(validate_answer(
            r#"{"items":[],"items":[]}"#,
            &sources(),
            &Kind::Summary,
            "t"
        )
        .is_err());
        assert!(
            validate_answer(r#"{"items":[]} trailing"#, &sources(), &Kind::Summary, "t").is_err()
        );
        assert!(
            validate_answer(r#"{"items":[],"extra":1}"#, &sources(), &Kind::Summary, "t").is_err()
        );
        assert!(validate_answer(r#"[]"#, &sources(), &Kind::Summary, "t").is_err());
    }
    #[test]
    fn missing_nullable_fields_are_not_silently_accepted() {
        let json = r#"{"items":[{"text":"Bericht","evidence":[{"sourceId":"S00001","quote":"Anna schreibt den Bericht bis Freitag."}]}]}"#;
        assert!(validate_answer(json, &sources(), &Kind::Tasks, "t").is_err());
    }
    #[test]
    fn excess_statements_and_evidence_are_discarded_and_counted() {
        let mut items: Vec<_> = (0..23)
            .map(|i| {
                item(
                    &format!("Aussage {}", char::from(b'a' + i)),
                    "null",
                    "null",
                    &valid_evidence(),
                )
            })
            .collect();
        let seven = vec![valid_evidence(); 7].join(",");
        items.insert(3, item("Zu viele Belege", "null", "null", &seven));
        items.push(item("Ohne Beleg", "null", "null", ""));
        let checked = validate_answer(&answer(&items), &sources(), &Kind::Summary, "t").unwrap();
        assert_eq!(checked.items.len(), MAX_ITEMS_PER_ANSWER);
        assert_eq!(checked.items[0].text, "Aussage a");
        assert_eq!(checked.items[19].text, "Aussage t");
        assert_eq!(
            checked.rejected,
            Rejections::from([
                (RejectionReason::EvidenceCount, 2),
                (RejectionReason::ItemLimit, 3)
            ])
        );
    }
    #[test]
    fn valid_task_and_plain_text_export() {
        let json = answer(&[item(
            "<script>Bericht</script>",
            r#""Anna""#,
            r#""Freitag""#,
            &valid_evidence(),
        )]);
        let checked = validate_answer(&json, &sources(), &Kind::Tasks, "t").unwrap();
        assert_eq!(checked.items.len(), 1);
        assert!(checked.rejected.is_empty());
        let decisions = validate_answer(&json, &sources(), &Kind::Decisions, "t").unwrap();
        assert!(decisions.items.is_empty());
        assert_eq!(
            decisions.rejected,
            only(RejectionReason::OwnerOrDueOutsideTasks, 1)
        );
    }
    #[test]
    fn name_candidate_rules_still_apply_to_each_kept_item() {
        let json = answer(&[
            item("Anna", "null", "null", &valid_evidence()),
            item("Bernd", "null", "null", &valid_evidence()),
        ]);
        let checked = validate_answer(
            &json,
            &sources(),
            &Kind::Summary,
            name_candidates::TEMPLATE_ID,
        )
        .unwrap();
        assert_eq!(checked.items.len(), 1);
        assert_eq!(checked.items[0].text, "Anna");
        assert_eq!(
            checked.rejected,
            only(RejectionReason::InvalidNameCandidate, 1)
        );
        // Other templates are not restricted by the name rules.
        let other = validate_answer(&json, &sources(), &Kind::Summary, "t").unwrap();
        assert_eq!(other.items.len(), 2);
    }
    #[test]
    fn redirects_are_not_followed_and_no_proxy_is_used() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0; 8192];
            let _ = stream.read(&mut buffer);
            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: https://example.com/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let client = TextClient::new(&config(&format!("http://{address}/v1/"))).unwrap();
        assert!(client.models().is_err());
        thread.join().unwrap();
    }
}
