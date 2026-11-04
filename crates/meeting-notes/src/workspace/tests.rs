use super::*;
use serde_json::json;

fn setup() -> (WorkspaceState, Request, serde_json::Value) {
    let source = Source {
        id: "s_one".into(),
        source: "system".into(),
        start_ms: 0,
        end_ms: 3000,
        text: "Ich gebe den Entwurf morgen frei.".into(),
    };
    let state = WorkspaceState::default();
    let req = Request::new(
        "r_test".into(),
        &WorkspaceContext::default(),
        &state,
        vec![],
        std::slice::from_ref(&source),
        vec![source.clone()],
    );
    let answer = json!({"contractVersion":CONTRACT_VERSION,"requestId":"r_test","changes":[{"proposalId":"p_one","kind":"task","targetId":null,"targetRevision":null,"text":"Entwurf freigeben","status":"open","attention":"current","owner":null,"due":"morgen","evidence":[{"sourceId":"s_one","quote":"Ich gebe den Entwurf morgen frei."}],"relatedEntityIds":[]}],"overviewRefs":["p_one"],"unresolvedContextIds":[],"moreChangesPossible":false});
    (state, req, answer)
}

mod context_01;
mod context_02;
mod context_03;
mod inference_01;
mod inference_02;
mod inheritance_01;
mod persistence_01;
mod sources_01;
mod validation_01;
