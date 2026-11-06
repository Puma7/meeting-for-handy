use super::*;

#[test]
fn perfect_decisions_validate_literal_sources_and_durable_reopen() {
    let mut mismatches = Vec::new();
    for (quote, allowed) in [
        ("We must check that we decided to start the pilot.", false),
        (
            "The team must verify that we have decided to start the pilot.",
            false,
        ),
        ("We need to check that we agreed to start the pilot.", false),
        (
            "Please confirm that we have decided to start the pilot.",
            false,
        ),
        ("Check that we have decided to start the pilot.", false),
        (
            "Wir mussten prüfen, dass wir den Pilotstart beschlossen haben.",
            false,
        ),
        (
            "We have checked that we have decided to start the pilot.",
            true,
        ),
        (
            "We must check, but we have decided to start the pilot.",
            true,
        ),
        ("We must check. We have decided to start the pilot.", true),
        (
            "We have confirmed that May agreed to start the pilot.",
            true,
        ),
        ("We hope that we have decided to start the pilot.", false),
        ("We hope that we agreed to start the pilot.", false),
        (
            "Wir haben gehofft, dass der Pilotstart beschlossen wurde.",
            false,
        ),
        ("We proposed that the plan is agreed to.", false),
        ("We hoped, but we have decided to start the pilot.", true),
        ("Hope agreed to start the pilot.", true),
        ("The proposed plan is agreed to.", true),
        (
            "We have confirmed that we have decided to start the pilot.",
            true,
        ),
        ("Anna agreed to start the pilot.", true),
        ("The team agreed to start the pilot.", true),
        ("Anna plans to have agreed to start the pilot.", false),
        ("WE WANT IT AGREED TO.", false),
        ("The team may have agreed to start the pilot.", false),
        ("Anna must have agreed to start the pilot.", false),
        ("May agreed to start the pilot.", true),
        (
            "Wir haben den Pilotstart, wie besprochen, beschlossen.",
            true,
        ),
        (
            "Wir haben beraten, abgestimmt und den Pilotstart beschlossen.",
            true,
        ),
        ("Wir haben beschlossen, nichts mehr zu ändern.", true),
        (
            "Wir haben etwas vorgeschlagen, der Pilotstart wird morgen beschlossen.",
            false,
        ),
        ("We propose that we decide to start the pilot.", false),
        ("We want the plan to be agreed to.", false),
        ("Agreed to start the pilot.", true),
        ("Wir haben vor den Pilotstart beschlossen zu haben.", false),
        ("Wir haben zu Punkt 1 den Pilotstart beschlossen.", true),
        ("We have decided to release on Friday.", true),
        ("We've decided to release on Friday.", true),
        ("We’ve decided to release on Friday.", true),
        ("WE'VE DECIDED to release on Friday.", true),
        ("We have already decided to release on Friday.", true),
        ("We've just unanimously decided to release on Friday.", true),
        ("We had decided to release on Friday.", true),
        ("We have decided not to release on Friday.", true),
        ("We decided to release on Friday.", true),
        ("We have agreed to release on Friday.", true),
        ("We decided nothing.", false),
        ("Wir haben nichts entschieden.", false),
        ("We have decided none of the options.", false),
        ("We decided to do nothing.", true),
        ("Wir haben beschlossen, nichts zu ändern.", true),
        ("Wir haben beschlossen nichts zu ändern.", true),
        (
            "Wir müssen entscheiden, ob wir am Freitag veröffentlichen.",
            false,
        ),
        ("Wir möchten beschließen, den Bericht freizugeben.", false),
        ("Wir wollen den Pilotstart beschließen.", false),
        ("Wir versuchen, den Pilotstart zu beschließen.", false),
        (
            "Wir haben uns für die Veröffentlichung am Freitag entschieden.",
            true,
        ),
        ("Heute beschließen wir den Pilotstart.", true),
        (
            "Wir haben vorgeschlagen, dass der Pilotstart beschlossen wird.",
            false,
        ),
        (
            "Es ist wichtig, dass der Pilotstart beschlossen wird.",
            false,
        ),
        (
            "Wir haben vorgeschlagen dass der Pilotstart beschlossen wird.",
            false,
        ),
        (
            "Es ist wichtig dass der Pilotstart beschlossen wird.",
            false,
        ),
        ("Wir haben vorgeschlagen, dass wir beschließen.", false),
        ("Es ist wichtig dass wir beschließen.", false),
        ("Wir beschlossen den Pilotstart.", true),
        ("Wir entschieden uns für den Pilotstart.", true),
        (
            "Robin bestätigt: Wir haben den Pilotstart beschlossen.",
            true,
        ),
        (
            "Nach der Beratung haben wir den Pilotstart beschlossen.",
            true,
        ),
        (
            "Robin bestätigt, dass wir den Pilotstart beschlossen haben.",
            true,
        ),
        (
            "Es ist bestätigt, dass der Pilotstart beschlossen ist.",
            true,
        ),
        (
            "Wir haben den Pilotstart für Version 1.0 beschlossen.",
            true,
        ),
        ("We have not decided to release on Friday.", false),
        ("We haven't decided to release on Friday.", false),
        ("We haven’t decided to release on Friday.", false),
        ("We've never decided to release on Friday.", false),
        ("We have perhaps decided to release on Friday.", false),
        ("We will decide to release on Friday.", false),
        ("We've decided to release on Friday?", false),
        (
            "We have decided to release on Friday if approval arrives.",
            false,
        ),
        (
            "If approval arrives, we've decided to release on Friday.",
            false,
        ),
        (
            "We've decided to release on Friday unless the owner objects.",
            false,
        ),
        ("Weve decided to release on Friday.", false),
        ("archiveWe've decided to release on Friday.", false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (_writer, empty) = store::load(directory.path()).unwrap();
        let (mut next, mut req, mut answer) = setup();
        req.purpose = "facts".into();
        req.transcript_sources[0].text = quote.into();
        answer["changes"][0]["kind"] = json!("decision");
        answer["changes"][0]["status"] = json!("recorded");
        answer["changes"][0]["text"] = json!("Release decision");
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!(quote);
        let offered = quote_options(quote)
            .iter()
            .any(|q| validate::recorded_decision_quote_allowed(q));
        let result = validate::apply(&mut next, &req, &answer.to_string());
        if result.is_ok() != allowed || offered != allowed {
            mismatches.push(format!(
                "{quote}: allowed={allowed}, offered={offered}, result={result:?}"
            ));
            continue;
        }
        if allowed {
            next.revision = 1;
            store::append(
                directory.path(),
                &store::Delta::between(
                    &empty,
                    &next,
                    req.transcript_sources.clone(),
                    result.unwrap(),
                ),
            )
            .unwrap();
            let checkpoint =
                std::fs::read(directory.path().join("revision-00000001.json")).unwrap();
            let reopened = read_state(directory.path()).unwrap();
            assert_eq!(reopened.entities.len(), 1);
            assert_eq!(reopened.entities[0].status, "recorded");
            assert_eq!(reopened.entities[0].evidence[0].quote, quote);
            assert_eq!(reopened.originals[0].text, quote);
            assert_eq!(
                serde_json::to_vec(&reopened).unwrap(),
                serde_json::to_vec(&read_state(directory.path()).unwrap()).unwrap()
            );
            assert_eq!(
                std::fs::read(directory.path().join("revision-00000001.json")).unwrap(),
                checkpoint
            );
        } else {
            assert_eq!(
                serde_json::to_vec(&next).unwrap(),
                serde_json::to_vec(&empty).unwrap()
            );
            assert!(!directory.path().join("revision-00000001.json").exists());
        }
    }
    assert!(mismatches.is_empty(), "Decision mismatches: {mismatches:?}");
}

#[test]
fn contracted_perfect_completion_retains_literal_quotes_after_reopen() {
    let cases = [
        ("We must check that the report is complete.", false),
        ("We must check that the report has been approved.", false),
        ("The team had to confirm that the report is done.", false),
        ("We need to confirm that the report is done.", false),
        ("We have to check that the report is complete.", false),
        ("Please check that the report is complete.", false),
        ("Check that the report is complete.", false),
        ("Verify that the report is done.", false),
        (
            "Wir müssen prüfen, dass ich den Bericht abgeschlossen habe.",
            false,
        ),
        ("Wir mussten prüfen, der Bericht ist erledigt.", false),
        ("Bitte bestätigen, der Bericht ist erledigt.", false),
        ("We have checked that the report is complete.", true),
        ("We did verify that the report is done.", true),
        ("We must check, but the report is complete.", true),
        ("We must check. The report is complete.", true),
        ("The May report is complete.", true),
        ("We have confirmed that May's report is complete.", true),
        ("We hope the report is complete.", false),
        ("We hope that the report is done.", false),
        ("We are hoping the report is complete.", false),
        ("The team hopes that the report is done.", false),
        ("We hoped the report has been approved.", false),
        ("Wir hoffen, der Bericht ist erledigt.", false),
        ("Ich habe gehofft, der Bericht ist abgeschlossen.", false),
        ("We expect the report is complete.", false),
        ("We want to confirm that the report is done.", false),
        ("We plan to declare that the report is complete.", false),
        ("We hope we have sent the report.", false),
        (
            "Ich habe gehofft, dass ich den Bericht abgeschlossen habe.",
            false,
        ),
        ("We believe the report is done.", false),
        ("We have confirmed that the report is done.", true),
        ("We hoped, but the report is done.", true),
        ("We hoped. The report is complete.", true),
        ("The hope report is complete.", true),
        ("Hope's report is complete.", true),
        ("I have completed the proposed report.", true),
        ("I have sent the report, as expected.", true),
        ("The expected report is done.", true),
        ("Wir hofften, aber der Bericht ist erledigt.", true),
        (
            "Ich habe vorgeschlagen, den Bericht als abgeschlossen zu betrachten.",
            false,
        ),
        (
            "Ich habe vorgeschlagen, den Bericht als erledigt zu markieren.",
            false,
        ),
        (
            "Ich habe vorgeschlagen den Bericht als abgeschlossen zu betrachten.",
            false,
        ),
        (
            "Ich habe vorgeschlagen den Bericht als erledigt zu markieren.",
            false,
        ),
        ("Ich habe den Bericht vorgeschlagen und verschickt.", true),
        ("Ich habe den Bericht, zu den Akten, verschickt.", true),
        ("Ich habe den Bericht, wie besprochen, verschickt.", true),
        (
            "Ich habe den Bericht geprüft, korrigiert und verschickt.",
            true,
        ),
        ("Ich habe den Bericht, nach der Prüfung, verschickt.", true),
        (
            "Ich habe den Bericht geprüft, er wird morgen verschickt.",
            false,
        ),
        (
            "Ich habe vorgeschlagen, der Bericht wird nächste Woche verschickt.",
            false,
        ),
        (
            "Ich habe vorgeschlagen, dass der Bericht abgeschlossen wird.",
            false,
        ),
        (
            "Wir haben vorgeschlagen dass der Bericht abgeschlossen wird.",
            false,
        ),
        ("Ich habe vor, den Bericht abgeschlossen zu haben.", false),
        ("Wir haben vor den Bericht abgeschlossen zu haben.", false),
        (
            "Ich habe geplant den Bericht abgeschlossen zu haben.",
            false,
        ),
        (
            "Robin bestätigt, dass ich den Bericht abgeschlossen habe.",
            true,
        ),
        ("Robin bestätigt: Ich habe den Bericht abgeschlossen.", true),
        (
            "Ich habe den Bericht abgeschlossen um Zeit für andere Aufgaben zu haben.",
            true,
        ),
        ("Ich habe den Bericht für Version 1.0 abgeschlossen.", true),
        ("I have already completed the report.", true),
        ("I've just sent the report.", true),
        ("I've sent the report.", true),
        ("I’ve sent the report.", true),
        ("We've completed the report.", true),
        ("We’ve completed the report.", true),
        ("I have sent the report.", true),
        ("We have completed the report.", true),
        ("I have sent the report nonetheless.", true),
        ("Ich habe den Bericht abgeschlossen.", true),
        ("Ich habe den Bericht vollständig abgeschlossen.", true),
        ("I have completed the report fast.", true),
        ("I have completed the report fully.", true),
        ("Ich habe den Bericht zu 100 Prozent abgeschlossen.", true),
        ("I have completed 100% of the report.", true),
        ("I have completed 100.0 percent of the report.", true),
        ("Ich habe den Bericht zu 100,0 Prozent abgeschlossen.", true),
        ("I have completed 100 per cent of the report.", true),
        ("I have completed the percentage report.", true),
        ("I have completed the 50% discount report.", true),
        ("I have completed the 50-percent-discount report.", true),
        (
            "Ich habe den Bericht zur 50-Prozent-Rabattaktion abgeschlossen.",
            true,
        ),
        (
            "Ich habe den Bericht zur 50 Prozent Rabattaktion abgeschlossen.",
            true,
        ),
        ("I have completed the quarterly report.", true),
        ("I have completed the third report.", true),
        ("I have completed the half-year report.", true),
        ("I have completed the report on 11/5.", true),
        ("Ich habe den Bericht am 11/5 abgeschlossen.", true),
        ("I have completed the report on 2026/11/5.", true),
        ("I have completed the 2026/2027 budget report.", true),
        (
            "Ich habe den Haushaltsbericht 2026/2027 abgeschlossen.",
            true,
        ),
        ("I have completed the Q1/2026 budget report.", true),
        ("Ich habe den Haushaltsbericht Q1/2026 abgeschlossen.", true),
        ("I have completed report 1 of 2.", true),
        ("Ich habe Bericht 1 von 2 abgeschlossen.", true),
        ("I have completed 1/2 today.", false),
        ("Ich habe 1/2 abgeschlossen.", false),
        ("I have completed 1 of 2 today.", false),
        ("I have completed 2/2.", true),
        ("I have completed 2 of 2.", true),
        ("I have completed 1/2parts of the report.", false),
        ("Ich habe 1/2Teilen des Berichts abgeschlossen.", false),
        ("I have completed exactly 1/2 today.", false),
        ("Ich habe genau 1 von 2 abgeschlossen.", false),
        ("I have completed only 1/2 today.", false),
        ("Ich habe nur 1 von 2 abgeschlossen.", false),
        ("I have completed already exactly 1/2 today.", false),
        ("Ich habe bereits genau 1 von 2 abgeschlossen.", false),
        ("I have completed exactly 2/2 today.", true),
        ("Ich habe genau 2 von 2 abgeschlossen.", true),
        ("I have completed 1 out of 2 reports.", false),
        ("I have completed exactly 1 out of 2 reports.", false),
        ("I have completed 2 out of 2 reports.", true),
        ("I have completed 2 out of 2.", true),
        ("I have completed report 1 out of 2.", true),
        ("I have completed 2 of 2 parts of the report.", true),
        ("Ich habe 2 von 2 Teilen des Berichts abgeschlossen.", true),
        ("I have completed 2/2 parts of the report.", true),
        ("I have completed three thirds of the report.", true),
        ("Ich habe drei Drittel des Berichts abgeschlossen.", true),
        ("I have completed 100% today.", true),
        (
            "Ich habe den Bericht zu 100 Prozent heute abgeschlossen.",
            true,
        ),
        ("I have completed 50% today.", false),
        ("I have completed 50% really quickly.", false),
        ("I have completed the report 50% yesterday.", false),
        (
            "Ich habe den Bericht zu 50 Prozent heute abgeschlossen.",
            false,
        ),
        (
            "Ich habe den Bericht zu 50 Prozent schon abgeschlossen.",
            false,
        ),
        (
            "Ich habe den Bericht zu 50 Prozent mit kurzem Review abgeschlossen.",
            false,
        ),
        ("I have completed half of the report.", false),
        ("Ich habe den Bericht zur Hälfte abgeschlossen.", false),
        ("Ich habe den Bericht zur Haelfte abgeschlossen.", false),
        ("I have completed one third of the report.", false),
        ("Ich habe ein Drittel des Berichts abgeschlossen.", false),
        ("I have completed 1 of 2 parts of the report.", false),
        ("Ich habe 1 von 2 Teilen des Berichts abgeschlossen.", false),
        ("I have completed 1/2 of the report.", false),
        ("I have completed 50％ of the report.", false),
        ("I have completed 50٪ of the report.", false),
        ("Ich habe den Bericht zu 50 Prozent abgeschlossen.", false),
        ("I have completed 50% of the report.", false),
        ("Ich habe den Bericht zu 99,9 Prozent abgeschlossen.", false),
        ("I have completed 99.9 percent of the report.", false),
        ("I have completed 50 per cent of the report.", false),
        ("I have completed 0% of the report.", false),
        ("I have completed 101% of the report.", false),
        (
            "I have completed 99.9999999999999999999% of the report.",
            false,
        ),
        ("I have completed 90–100% of the report.", false),
        (
            "Ich habe den Bericht zu 90 bis 100 Prozent abgeschlossen.",
            false,
        ),
        (
            "I have completed between 50 and 100 percent of the report.",
            false,
        ),
        ("I have completed 50 of 100 percent of the report.", false),
        (
            "Ich habe den Bericht zu 50 von 100 Prozent abgeschlossen.",
            false,
        ),
        ("I have completed fifty percent of the report.", false),
        (
            "Ich habe den Bericht zu fünfzig Prozent abgeschlossen.",
            false,
        ),
        ("Ich habe den Bericht fast abgeschlossen.", false),
        ("Ich habe den Bericht beinahe abgeschlossen.", false),
        ("Ich habe den Bericht nahezu abgeschlossen.", false),
        ("Ich habe den Bericht teilweise abgeschlossen.", false),
        ("Ich habe den Bericht weitgehend abgeschlossen.", false),
        ("I have completed almost all of the report.", false),
        ("I have completed nearly all of the report.", false),
        ("I have completed the report partly.", false),
        ("I have completed the report partially.", false),
        ("I have completed the report mostly.", false),
        ("Ich habe nichts verschickt.", false),
        ("Wir haben nichts verschickt.", false),
        ("I've sent nothing.", false),
        ("I’ve sent nothing.", false),
        ("We have completed none of the reports.", false),
        ("We've completed none of the reports.", false),
        ("We’ve completed none of the reports.", false),
        ("I have sent neither report.", false),
        ("We've completed neither report.", false),
        ("We’ve completed neither report.", false),
        ("I've not sent the report.", false),
        ("I’ve never sent the report.", false),
        ("We've not completed the report.", false),
        ("We’ve never completed the report.", false),
        ("I haven't sent the report.", false),
        ("We haven’t completed the report.", false),
        ("I've sent the report if approval arrives.", false),
        ("We’ve completed the report?", false),
        ("I ve sent the report.", false),
        ("Ive sent the report.", false),
        ("archiveI've sent the report.", false),
    ];
    let mut mismatches = Vec::new();
    for (quote, allowed) in cases {
        let directory = tempfile::tempdir().unwrap();
        let (_writer, empty) = store::load(directory.path()).unwrap();
        let (mut seed, mut request, mut answer) = setup();
        request.transcript_sources[0].text = "I will send the report.".into();
        answer["changes"][0]["text"] = json!("Send the report");
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["evidence"][0]["quote"] = json!("I will send the report.");
        let events = validate::apply(&mut seed, &request, &answer.to_string()).unwrap();
        seed.revision = 1;
        store::append(
            directory.path(),
            &store::Delta::between(&empty, &seed, request.transcript_sources.clone(), events),
        )
        .unwrap();
        let seed = read_state(directory.path()).unwrap();
        let old_checkpoint =
            std::fs::read(directory.path().join("revision-00000001.json")).unwrap();
        let source = Source {
            id: "s_completion".into(),
            source: "microphone".into(),
            start_ms: 3000,
            end_ms: 6000,
            text: quote.into(),
        };
        let mut sources = seed.originals.clone();
        sources.push(source.clone());
        let mut completion = Request::new(
            "r_completion".into(),
            &seed.context,
            &seed,
            seed.entities.clone(),
            std::slice::from_ref(&source),
            sources,
        );
        completion.purpose = "tasks".into();
        answer["requestId"] = json!("r_completion");
        answer["changes"][0]["targetId"] = json!(seed.entities[0].id);
        answer["changes"][0]["targetRevision"] = json!(seed.entities[0].revision);
        answer["changes"][0]["text"] = json!(null);
        answer["changes"][0]["status"] = json!("completed");
        answer["changes"][0]["evidence"] = json!([{"sourceId":"s_completion","quote":quote}]);
        let mut next = seed.clone();
        let result = validate::apply(&mut next, &completion, &answer.to_string());
        if result.is_ok() != allowed {
            mismatches.push(format!("{quote}: allowed={allowed}, result={result:?}"));
            continue;
        }
        if allowed {
            next.revision += 1;
            store::append(
                directory.path(),
                &store::Delta::between(&seed, &next, vec![source], result.unwrap()),
            )
            .unwrap();
            let reopened = read_state(directory.path()).unwrap();
            assert_eq!(reopened.entities[0].status, "completed");
            assert_eq!(reopened.entities[0].text, seed.entities[0].text);
            assert!(seed.entities[0]
                .evidence
                .iter()
                .all(|e| reopened.entities[0].evidence.contains(e)));
            assert!(reopened.entities[0]
                .evidence
                .iter()
                .any(|e| e.source_id == "s_completion" && e.quote == quote));
            assert!(reopened
                .originals
                .iter()
                .any(|s| s.id == "s_completion" && s.text == quote));
            assert_eq!(
                serde_json::to_vec(&read_state(directory.path()).unwrap()).unwrap(),
                serde_json::to_vec(&reopened).unwrap()
            );
        } else {
            assert_eq!(
                serde_json::to_vec(&next).unwrap(),
                serde_json::to_vec(&seed).unwrap(),
                "{quote}"
            );
            assert!(!directory.path().join("revision-00000002.json").exists());
        }
        assert_eq!(
            std::fs::read(directory.path().join("revision-00000001.json")).unwrap(),
            old_checkpoint
        );
    }
    assert!(
        mismatches.is_empty(),
        "Completion admission mismatches: {mismatches:?}"
    );
}
