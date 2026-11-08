use super::*;

#[test]
fn perfect_decisions_validate_literal_sources_and_durable_reopen() {
    let mut mismatches = Vec::new();
    for (quote, allowed) in [
        ("We check that we have decided to start the pilot.", false),
        (
            "We did check that we have decided to start the pilot.",
            true,
        ),
        ("We denied that we have decided to start the pilot.", false),
        ("We deny that we agreed to start the pilot.", false),
        (
            "We disputed that we have decided to start the pilot.",
            false,
        ),
        (
            "We imagined that we have decided to start the pilot.",
            false,
        ),
        (
            "We confirmed that we denied that we have decided to start the pilot.",
            false,
        ),
        ("We verified that we have decided to start the pilot.", true),
        (
            "The members of the team confirmed that we have decided to start the pilot.",
            true,
        ),
        (
            "The members of the team must check that we have decided to start the pilot.",
            false,
        ),
        (
            "A team must verify that we decided to start the pilot.",
            false,
        ),
        (
            "All engineers must check that we have decided to start the pilot.",
            false,
        ),
        (
            "Our platform security team must check that we agreed to start the pilot.",
            false,
        ),
        (
            "Our platform security team has confirmed that we have decided to start the pilot.",
            true,
        ),
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
            let cited = &reopened.entities[0].evidence[0].quote;
            assert!(evidence_sentences(quote).any(|sentence| sentence == cited));
            assert!(validate::recorded_decision_quote_allowed(cited));
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
        ("We check that the report is complete.", false),
        ("We did check that the report is complete.", true),
        ("We denied that the report is complete.", false),
        ("We deny that the report is done.", false),
        ("We disputed that the report is complete.", false),
        ("We imagined that the report is done.", false),
        (
            "We confirmed that we denied that the report is complete.",
            false,
        ),
        ("We denied the report is complete.", false),
        (
            "Wir bestreiten, dass ich den Bericht abgeschlossen habe.",
            false,
        ),
        ("Wir haben bestritten, der Bericht ist erledigt.", false),
        ("We verified that the report is complete.", true),
        (
            "The members of the team confirmed that the report is done.",
            true,
        ),
        ("The denied report is complete.", true),
        ("We denied the claim, but the report is complete.", true),
        (
            "The members of the team must check that the report is complete.",
            false,
        ),
        ("A team must check that the report is complete.", false),
        ("All engineers must verify that the report is done.", false),
        (
            "Our platform security team must check that the report is complete.",
            false,
        ),
        (
            "Our platform security team has confirmed that the report is complete.",
            true,
        ),
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
        // Isolate terminal grammar, including standalone counts. The
        // production task phase has a separate association check.
        completion.purpose = "live_update".into();
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

#[test]
fn facts_frontier_survives_an_empty_tasks_phase_with_more_than_six_selected_facts() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut facts = HashSet::new();
        let mut queued_versions = Vec::new();
        let started = Instant::now();
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "synthetic request sequence did not finish"
            );
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            // Windows may inherit the listener's nonblocking mode. Accepted
            // synthetic HTTP streams use bounded blocking reads below.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 8192];
            let header = loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0 && bytes.len() < 500000);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(at) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let size = String::from_utf8_lossy(&bytes[..header])
                .lines()
                .filter_map(|l| l.split_once(':'))
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .unwrap()
                .1
                .trim()
                .parse::<usize>()
                .unwrap();
            while bytes.len() < header + size {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let body: serde_json::Value =
                serde_json::from_slice(&bytes[header..header + size]).unwrap();
            let req: serde_json::Value =
                serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
            if req["purpose"] == "facts" {
                for entity in req["state"].as_array().unwrap() {
                    if entity["kind"] == "fact" {
                        facts.insert(entity["id"].as_str().unwrap().to_owned());
                    }
                }
                if let Some(entity) = req["state"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["id"] == "e_fact_7")
                {
                    queued_versions.push((
                        entity["revision"].as_u64().unwrap(),
                        entity["text"].as_str().unwrap().to_owned(),
                    ));
                }
            }
            let changes = if req["purpose"] == "facts" && queued_versions.len() == 1 {
                let source = req["transcriptSources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["text"] == "Der Entwurf ist zur Prüfung bereit.")
                    .unwrap();
                assert!(req["window"]["requiredSourceIds"]
                    .as_array()
                    .unwrap()
                    .contains(&source["id"]));
                json!([{"proposalId":"p_seed","kind":"fact","targetId":"e_fact_7","targetRevision":1,"text":"Aktueller Entwurf","status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":source["id"],"quote":source["text"]}],"relatedEntityIds":[]}])
            } else {
                json!([])
            };
            let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":req["requestId"],"changes":changes,"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
            let payload = format!(
                "data: {}\n\ndata: {}\n\n",
                json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
            );
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
            if req["purpose"] == "reconcile" {
                break;
            }
        }
        (facts, queued_versions)
    });
    let dir = tempfile::tempdir().unwrap();
    let mut runner = WorkspaceRunner::open(dir.path()).unwrap();
    runner.set_context(Default::default()).unwrap();
    let (mut state, req, answer) = setup();
    validate::apply(&mut state, &req, &answer.to_string()).unwrap();
    let task = state.entities.remove(0);
    state.originals = req.transcript_sources.clone();
    for i in 0..8 {
        let mut fact = task.clone();
        fact.id = format!("e_fact_{i}");
        fact.kind = "fact".into();
        fact.status = "recorded".into();
        fact.due = None;
        state.overview_refs.push(fact.id.clone());
        state.entities.push(fact);
    }
    state.overview_refs.retain(|id| id.starts_with("e_fact_"));
    state.entities.push(task);
    let context = runner.context.lock().unwrap().clone();
    state.apply_context(&context).unwrap();
    runner
        .commit(
            state,
            req.transcript_sources,
            vec![],
            &AnalysisControl::default(),
            &context,
        )
        .unwrap();
    let client = TextClient::new(&crate::Connection {
        endpoint,
        model: "synthetic".into(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    })
    .unwrap();
    let pending = runner.pending(&[LiveText {
        key: "fresh".into(),
        track: "system".into(),
        start_ms: 3000,
        end_ms: 6000,
        text: "Der Entwurf ist zur Prüfung bereit.".into(),
    }]);
    runner
        .update(&client, &pending, &AnalysisControl::default(), "en")
        .unwrap();
    let (facts, versions) = server.join().unwrap();
    assert_eq!(facts, (0..8).map(|i| format!("e_fact_{i}")).collect());
    assert!(versions.len() >= 2);
    assert_eq!(versions[0].0, 1);
    assert!(
        versions[1..]
            .iter()
            .all(|(revision, text)| *revision == 2 && text == "Aktueller Entwurf"),
        "{versions:?}"
    );
}

#[test]
fn a_request_snapshot_cannot_overwrite_a_newer_current_entity() {
    let (mut state, mut request, mut answer) = setup();
    validate::apply(&mut state, &request, &answer.to_string()).unwrap();
    request.state = state.entities.clone();
    answer["changes"][0]["targetId"] = json!(state.entities[0].id);
    answer["changes"][0]["targetRevision"] = json!(1);
    answer["changes"][0]["text"] = json!("Aktueller Entwurf");
    validate::apply(&mut state, &request, &answer.to_string()).unwrap();
    assert_eq!(state.entities[0].revision, 2);
    let bytes = serde_json::to_vec(&state).unwrap();
    answer["changes"][0]["text"] = json!("Überholter Entwurf");
    assert!(validate::apply(&mut state, &request, &answer.to_string()).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
}

/// Replay an actual bounded source window after withdrawal, beyond optional model seeds.
#[test]
fn withdrawn_decision_replay_keeps_identity_when_optional_seeds_hide_it() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::{Duration, Instant},
    };
    /// Persist validated synthetic windows through the production workspace writer.
    fn seed_window(
        runner: &mut WorkspaceRunner,
        texts: &[LiveText],
        mut changes: serde_json::Value,
    ) -> Vec<Source> {
        let context = runner.context.lock().unwrap().clone();
        let processed = live::pending(&[], texts);
        let fresh = processed
            .iter()
            .map(|s| Source {
                id: source_id(s),
                source: s.source.clone(),
                start_ms: s.start_ms,
                end_ms: s.end_ms,
                text: s.text.clone(),
            })
            .collect::<Vec<_>>();
        let mut originals = runner.state.originals.clone();
        originals.extend(fresh.clone());
        for change in changes.as_array_mut().unwrap() {
            for evidence in change["evidence"].as_array_mut().unwrap() {
                let quote = evidence["quote"].as_str().unwrap();
                evidence["sourceId"] =
                    json!(fresh.iter().find(|s| s.text.contains(quote)).unwrap().id);
            }
        }
        let mut next = runner.state.clone();
        next.apply_context(&context).unwrap();
        let mut request = Request::new(
            "r_seed".into(),
            &context,
            &next,
            next.entities.clone(),
            &fresh,
            originals.clone(),
        );
        request.purpose = "facts".into();
        let response = json!({"contractVersion":CONTRACT_VERSION,"requestId":request.request_id,"changes":changes,"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
        let events = validate::apply(&mut next, &request, &response.to_string()).unwrap();
        next.originals = originals;
        next.processed.extend(processed);
        runner
            .commit(
                next,
                fresh.clone(),
                events,
                &AnalysisControl::default(),
                &context,
            )
            .unwrap();
        fresh
    }
    for revoke_note in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut runner = WorkspaceRunner::open(directory.path()).unwrap();
        let mut initial = WorkspaceContext::default();
        if revoke_note {
            initial.shared_notes.push(SharedNote {
                id: "h_decision".into(),
                revision: 1,
                text: "Private synthetic released protocol context.".into(),
            });
        }
        runner.set_context(initial).unwrap();
        let first_quote = "We decided to release the report.";
        let other_quote = "We have decided to release the report.";
        let make_text = |index: u64, text: String| LiveText {
            key: format!("part-{index}"),
            track: "system".into(),
            start_ms: index * 3000,
            end_ms: (index + 1) * 3000,
            text,
        };
        let padding = "Background context. ".repeat(80);
        let mut first = format!("{first_quote} {other_quote} {padding}");
        first.truncate(1500);
        let mut texts = vec![make_text(0, first)];
        for index in 1..4 {
            let mut value = padding.clone();
            value.truncate(1500);
            texts.push(make_text(index, value));
        }
        let original = seed_window(&mut runner,&texts,json!([{"proposalId":"p_decision","kind":"decision","targetId":null,"targetRevision":null,"text":"Release the report","status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":"s_pending","quote":first_quote},{"sourceId":"s_pending","quote":other_quote}],"relatedEntityIds":[]}]))[0].clone();
        let decision_id = runner.state.entities[0].id.clone();
        let mut filler = padding.clone();
        filler.truncate(1500);
        let withdrawal = "We withdraw the release decision.";
        let later = vec![make_text(4, filler), make_text(5, withdrawal.into())];
        seed_window(
            &mut runner,
            &later,
            json!([{"proposalId":"p_withdraw","kind":"decision","targetId":decision_id,"targetRevision":1,"text":null,"status":"withdrawn","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":"s_pending","quote":withdrawal}],"relatedEntityIds":[]}]),
        );
        texts.extend(later);
        let facts = [
            "Budget discussed.",
            "Scope reviewed.",
            "Schedule reviewed.",
            "Testing reviewed.",
            "Costs reviewed.",
            "Risks reviewed.",
        ];
        let fact_text = make_text(6, facts.join(" "));
        let changes=facts.iter().enumerate().map(|(index,quote)|json!({"proposalId":format!("p_fact_{index}"),"kind":"fact","targetId":null,"targetRevision":null,"text":quote,"status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":"s_pending","quote":quote}],"relatedEntityIds":[]})).collect::<Vec<_>>();
        seed_window(
            &mut runner,
            std::slice::from_ref(&fact_text),
            json!(changes),
        );
        texts.push(fact_text);
        let before = runner.view();
        let withdrawn = before
            .entities
            .iter()
            .find(|e| e.id == decision_id)
            .unwrap()
            .clone();
        assert_eq!(withdrawn.status, "withdrawn");
        assert_eq!(withdrawn.evidence.len(), 2);
        let omitted = [first_quote, other_quote]
            .into_iter()
            .find(|quote| !withdrawn.evidence.iter().any(|e| e.quote == *quote))
            .unwrap();
        assert!(before
            .events
            .iter()
            .any(|event| event.entity_id == decision_id
                && event.status == "recorded"
                && event.evidence.iter().any(|e| e.quote == omitted)));
        let old_files = (1..=before.revision)
            .map(|revision| {
                let path = directory
                    .path()
                    .join(format!("revision-{revision:08}.json"));
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect::<Vec<_>>();
        let context = runner.context.lock().unwrap().clone();
        let mut forced_request = Request::new(
            "r_forced".into(),
            &context,
            &before,
            before.entities.clone(),
            std::slice::from_ref(&original),
            before.originals.clone(),
        );
        forced_request.purpose = "facts".into();
        let forced_answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_forced","changes":[{"proposalId":"p_forced","kind":"decision","targetId":decision_id,"targetRevision":withdrawn.revision,"text":null,"status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":original.id,"quote":omitted}],"relatedEntityIds":[]}],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
        let mut forced = before.clone();
        let forced_result =
            validate::apply(&mut forced, &forced_request, &forced_answer.to_string());
        let mut mismatches = Vec::new();
        if forced_result.is_ok() || forced.entities != before.entities {
            mismatches.push("an explicit model target reopened a withdrawn decision".to_owned());
        }
        let mut changed = context.clone();
        if revoke_note {
            changed.shared_notes.clear();
        }
        changed.agenda.push(Agenda {
            id: "e_release_review".into(),
            revision: 1,
            text: "Release review".into(),
        });
        let changed = update_context(directory.path(), &changed).unwrap().0;
        *runner.context.lock().unwrap() = changed;
        assert!(runner.context_dirty());
        assert_eq!(live::next_window(&runner.active_sources()).len(), 4);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server_original = original.clone();
        let server_decision = decision_id.clone();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut facts_seen = 0;
            loop {
                let mut stream = accept_synthetic_before(&listener, deadline).unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 8192];
                let header = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0 && bytes.len() < 500000);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(at) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                let size = String::from_utf8_lossy(&bytes[..header])
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                    .unwrap()
                    .1
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                while bytes.len() < header + size {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&bytes[header..header + size]).unwrap();
                let req: serde_json::Value =
                    serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
                let changes = if req["purpose"] == "facts" {
                    facts_seen += 1;
                    let selected = req["state"].as_array().unwrap();
                    assert_eq!(selected.len(), if revoke_note { 0 } else { 6 });
                    if revoke_note {
                        assert!(!req
                            .to_string()
                            .contains("Private synthetic released protocol context."));
                    }
                    assert!(selected
                        .iter()
                        .all(|e| e["kind"] == "fact" && e["id"] != server_decision));
                    assert!(!req["transcriptSources"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|s| s["text"].as_str().unwrap().contains("withdraw")));
                    json!([{"proposalId":"p_replay","kind":"decision","targetId":null,"targetRevision":null,"text":"Release the report","status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":server_original.id,"quote":omitted}],"relatedEntityIds":[]}])
                } else {
                    json!([])
                };
                let overview = if revoke_note && req["purpose"] == "facts" {
                    vec!["p_replay"]
                } else {
                    vec![]
                };
                let answer=json!({"contractVersion":CONTRACT_VERSION,"requestId":req["requestId"],"changes":changes,"overviewRefs":overview,"unresolvedContextIds":[],"moreChangesPossible":false}).to_string();
                let payload = format!(
                    "data: {}\n\ndata: {}\n\n",
                    json!({"choices":[{"delta":{"content":answer},"finish_reason":null}]}),
                    json!({"choices":[{"delta":{},"finish_reason":"stop"}]})
                );
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",payload.len()).unwrap();
                if req["purpose"] == "reconcile" {
                    break;
                }
            }
            facts_seen
        });
        let client = TextClient::new(&crate::Connection {
            endpoint,
            model: "synthetic".into(),
            mode: "local".into(),
            cloud_consent: false,
            api_key: String::new(),
        })
        .unwrap();
        runner
            .update(&client, &[], &AnalysisControl::default(), "en")
            .unwrap();
        assert_eq!(server.join().unwrap(), 1);
        let replayed = runner.view();
        if revoke_note {
            assert!(
                !replayed.overview_refs.contains(&decision_id),
                "A hidden replay alias resurfaced in the overview"
            );
        }
        if replayed
            .entities
            .iter()
            .filter(|e| e.kind == "decision")
            .count()
            != 1
            || replayed
                .entities
                .iter()
                .any(|e| e.kind == "decision" && e.status != "withdrawn")
        {
            mismatches
                .push("context replay republished a withdrawn decision as recorded".to_owned());
        }
        for (path, bytes) in old_files {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
        drop(runner);
        let reopened = WorkspaceRunner::open(directory.path()).unwrap().view();
        assert_eq!(reopened.entities, replayed.entities);
        assert_eq!(
            serde_json::to_vec(&reopened).unwrap(),
            serde_json::to_vec(&WorkspaceRunner::open(directory.path()).unwrap().view()).unwrap()
        );
        if revoke_note {
            assert!(reopened.context_excluded_ids.contains(&decision_id));
            let terminal_source = withdrawn
                .evidence
                .iter()
                .find(|e| e.quote == withdrawal)
                .unwrap()
                .source_id
                .clone();
            for invalidation in ["superseded", "missing", "changed"] {
                let mut invalid = reopened.clone();
                match invalidation {
                    "superseded" => invalid.superseded_sources.push(terminal_source.clone()),
                    "missing" => invalid.originals.retain(|s| s.id != terminal_source),
                    _ => {
                        invalid
                            .originals
                            .iter_mut()
                            .find(|s| s.id == terminal_source)
                            .unwrap()
                            .text = "Replaced synthetic terminal source.".into()
                    }
                }
                let mut request = Request::new(
                    "r_forced".into(),
                    &invalid.context,
                    &invalid,
                    vec![],
                    std::slice::from_ref(&original),
                    invalid.originals.clone(),
                );
                request.purpose = "facts".into();
                let mut proposal = forced_answer.clone();
                proposal["changes"][0]["targetId"] = json!(null);
                proposal["changes"][0]["targetRevision"] = json!(null);
                proposal["changes"][0]["text"] = json!("Release the report");
                let count = invalid.entities.len();
                let events =
                    validate::apply(&mut invalid, &request, &proposal.to_string()).unwrap();
                assert_eq!(events.len(), 1, "{invalidation}");
                assert_eq!(invalid.entities.len(), count + 1);
                assert_eq!(
                    invalid
                        .entities
                        .iter()
                        .find(|e| e.id == decision_id)
                        .unwrap(),
                    &withdrawn
                );
            }
        }
        let mut fresh = original.clone();
        fresh.id = "s_independent_decision".into();
        fresh.start_ms = 24000;
        fresh.end_ms = 27000;
        let mut all = before.originals.clone();
        all.push(fresh.clone());
        let mut request = Request::new(
            "r_fresh".into(),
            &context,
            &before,
            before.entities.clone(),
            std::slice::from_ref(&fresh),
            all,
        );
        request.purpose = "facts".into();
        let answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_fresh","changes":[{"proposalId":"p_fresh","kind":"decision","targetId":null,"targetRevision":null,"text":"Release the report","status":"recorded","attention":"normal","owner":null,"due":null,"evidence":[{"sourceId":fresh.id,"quote":omitted}],"relatedEntityIds":[]}],"overviewRefs":[],"unresolvedContextIds":[],"moreChangesPossible":false});
        let mut independent = before.clone();
        validate::apply(&mut independent, &request, &answer.to_string()).unwrap();
        assert_eq!(
            independent
                .entities
                .iter()
                .filter(|e| e.kind == "decision" && e.status == "recorded")
                .count(),
            1
        );
        assert_eq!(
            independent
                .entities
                .iter()
                .find(|e| e.id == decision_id)
                .unwrap(),
            &withdrawn
        );
        assert!(
            mismatches.is_empty(),
            "Withdrawn decision regressions: {mismatches:?}"
        );
    }
}

#[test]
fn terminal_replay_uses_immutable_promise_history_after_evidence_minimization() {
    let mut duplicates = Vec::new();
    for terminal in ["completed", "withdrawn"] {
        let directory = tempfile::tempdir().unwrap();
        let (_writer, empty) = store::load(directory.path()).unwrap();
        let (mut state, mut request, mut answer) = setup();
        request.purpose = "tasks".into();
        request.transcript_sources[0].text = "I will send the report tomorrow.".into();
        let alternative = Source {
            id: "s_other_promise".into(),
            source: "microphone".into(),
            start_ms: 3000,
            end_ms: 6000,
            text: "I promise to send the report tomorrow.".into(),
        };
        request.transcript_sources.push(alternative.clone());
        answer["changes"][0]["text"] = json!("Send the report");
        answer["changes"][0]["due"] = json!("tomorrow");
        answer["changes"][0]["evidence"] = json!([{"sourceId":"s_one","quote":"I will send the report tomorrow."},{"sourceId":alternative.id,"quote":alternative.text}]);
        let events = validate::apply(&mut state, &request, &answer.to_string()).unwrap();
        assert_eq!(state.entities[0].evidence.len(), 2);
        state.revision = 1;
        store::append(
            directory.path(),
            &store::Delta::between(&empty, &state, request.transcript_sources.clone(), events),
        )
        .unwrap();
        let seed = read_state(directory.path()).unwrap();
        let done = Source {
            id: "s_terminal".into(),
            source: "system".into(),
            start_ms: 6000,
            end_ms: 9000,
            text: if terminal == "completed" {
                "I have sent the report."
            } else {
                "I withdraw the report task."
            }
            .into(),
        };
        let mut sources = seed.originals.clone();
        sources.push(done.clone());
        let mut completion = Request::new(
            "r_terminal".into(),
            &seed.context,
            &seed,
            seed.entities.clone(),
            std::slice::from_ref(&done),
            sources,
        );
        completion.purpose = "tasks".into();
        answer["requestId"] = json!("r_terminal");
        answer["changes"][0]["targetId"] = json!(seed.entities[0].id);
        answer["changes"][0]["targetRevision"] = json!(seed.entities[0].revision);
        answer["changes"][0]["text"] = json!(null);
        answer["changes"][0]["due"] = json!(null);
        answer["changes"][0]["status"] = json!(terminal);
        answer["changes"][0]["evidence"] = json!([{"sourceId":done.id,"quote":done.text}]);
        let mut next = seed.clone();
        let events = validate::apply(&mut next, &completion, &answer.to_string()).unwrap();
        next.revision = 2;
        store::append(
            directory.path(),
            &store::Delta::between(&seed, &next, vec![done], events),
        )
        .unwrap();
        let terminal_state = read_state(directory.path()).unwrap();
        assert_eq!(terminal_state.entities[0].evidence.len(), 2);
        let omitted = seed.entities[0]
            .evidence
            .iter()
            .find(|e| !terminal_state.entities[0].evidence.contains(e))
            .unwrap()
            .clone();
        assert!(terminal_state
            .events
            .iter()
            .any(|e| e.entity_id == seed.entities[0].id
                && e.revision == 1
                && e.evidence.contains(&omitted)));
        let old_files = (1..=2)
            .map(|i| {
                let path = directory.path().join(format!("revision-{i:08}.json"));
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect::<Vec<_>>();
        let source = terminal_state
            .originals
            .iter()
            .find(|s| s.id == omitted.source_id)
            .unwrap()
            .clone();
        let mut replay = Request::new(
            "r_replay".into(),
            &terminal_state.context,
            &terminal_state,
            terminal_state.entities.clone(),
            std::slice::from_ref(&source),
            terminal_state.originals.clone(),
        );
        replay.purpose = "tasks".into();
        answer["requestId"] = json!("r_replay");
        answer["changes"][0]["targetId"] = json!(null);
        answer["changes"][0]["targetRevision"] = json!(null);
        answer["changes"][0]["text"] = json!("Send the report");
        answer["changes"][0]["status"] = json!("open");
        answer["changes"][0]["evidence"] =
            json!([{"sourceId":omitted.source_id,"quote":omitted.quote}]);
        let mut replayed = terminal_state.clone();
        let events = validate::apply(&mut replayed, &replay, &answer.to_string()).unwrap();
        if replayed.entities != terminal_state.entities || !events.is_empty() {
            duplicates.push(format!("{terminal}: minimized current evidence lost the original promise identity; entities={}, events={}",replayed.entities.len(),events.len()));
        }
        replayed.revision += 1;
        store::append(
            directory.path(),
            &store::Delta::between(&terminal_state, &replayed, vec![], events),
        )
        .unwrap();
        let reopened = read_state(directory.path()).unwrap();
        assert_eq!(reopened.entities, replayed.entities);
        assert_eq!(
            serde_json::to_vec(&reopened).unwrap(),
            serde_json::to_vec(&read_state(directory.path()).unwrap()).unwrap()
        );
        let fresh = Source {
            id: "s_fresh_promise".into(),
            source: "microphone".into(),
            start_ms: 9000,
            end_ms: 12000,
            text: source.text.clone(),
        };
        let mut sources = reopened.originals.clone();
        sources.push(fresh.clone());
        let mut req = Request::new(
            "r_fresh".into(),
            &reopened.context,
            &reopened,
            reopened.entities.clone(),
            std::slice::from_ref(&fresh),
            sources,
        );
        req.purpose = "tasks".into();
        answer["requestId"] = json!("r_fresh");
        answer["changes"][0]["evidence"] = json!([{"sourceId":fresh.id,"quote":fresh.text}]);
        let mut renewed = reopened.clone();
        validate::apply(&mut renewed, &req, &answer.to_string()).unwrap();
        assert_eq!(
            renewed
                .entities
                .iter()
                .filter(|e| e.kind == "task" && e.status == "open")
                .count(),
            1
        );
        assert_eq!(
            renewed
                .entities
                .iter()
                .find(|e| e.id == terminal_state.entities[0].id)
                .unwrap(),
            &terminal_state.entities[0]
        );
        for (path, bytes) in old_files {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }
    assert!(
        duplicates.is_empty(),
        "Historical replay duplicates: {duplicates:?}"
    );
}
