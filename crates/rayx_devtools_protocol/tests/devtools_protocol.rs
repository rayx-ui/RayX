//! The agent protocol: every golden message decodes into its type and encodes back to the same
//! bytes, the golden file covers every method, the docs carry the same file, an unknown method or
//! notification leaves the connection open, and a version mismatch names both versions.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeSet, HashMap};

use rayx_devtools_protocol::*;

/// Which frame a typed value is encoded into.
#[derive(Clone, Copy)]
enum Frame<'a> {
    Request(&'a Id),
    Notification,
    Response(&'a Id),
}

/// `raw` decoded as `T` and encoded back into `frame`, straight from the typed value.
fn encode_as<T: serde::de::DeserializeOwned + serde::Serialize>(
    frame: Frame<'_>,
    method: &str,
    raw: Params<'_>,
) -> String {
    let typed: T = raw
        .decode()
        .expect("the golden message decodes into its type");
    match frame {
        Frame::Request(id) => encode_request(id, method, &typed),
        Frame::Notification => encode_notification(method, &typed),
        Frame::Response(id) => encode_response(Some(id), Ok(&typed)),
    }
}

/// The parameters of `method` through their type.
fn params_again(frame: Frame<'_>, method: &str, params: Params<'_>) -> String {
    match method {
        methods::HELLO => encode_as::<Hello>(frame, method, params),
        methods::SURFACE_OPENED => encode_as::<SurfaceInfo>(frame, method, params),
        methods::SURFACE_CLOSED => encode_as::<SurfaceRef>(frame, method, params),
        methods::TREE_CHANGED => encode_as::<TreeChanged>(frame, method, params),
        methods::TREE => encode_as::<TreeParams>(frame, method, params),
        methods::TREE_WATCH => encode_as::<WatchParams>(frame, method, params),
        methods::NODE => encode_as::<NodeParams>(frame, method, params),
        methods::FIND => encode_as::<FindParams>(frame, method, params),
        methods::PICK => encode_as::<PickParams>(frame, method, params),
        methods::ACT => encode_as::<Act>(frame, method, params),
        methods::INPUT => encode_as::<Input>(frame, method, params),
        methods::WAIT => encode_as::<WaitParams>(frame, method, params),
        methods::SCREENSHOT => encode_as::<ScreenshotParams>(frame, method, params),
        methods::LAYER_PICK_MODE | methods::LAYER_PICK_MODE_REQUESTED => {
            encode_as::<PickMode>(frame, method, params)
        }
        methods::LAYER_SELECT => encode_as::<Selection>(frame, method, params),
        methods::LAYER_PICKED => encode_as::<Picked>(frame, method, params),
        methods::LAYER_NOTE => encode_as::<Note>(frame, method, params),
        methods::LOGS => encode_as::<LogsParams>(frame, method, params),
        methods::EXTENSION_LIST => encode_as::<Empty>(frame, method, params),
        methods::EXTENSION_CALL => encode_as::<ExtensionCall>(frame, method, params),
        other => panic!("no parameter type for `{other}`"),
    }
}

/// The result of `method` through its type.
fn result_again(id: &Id, method: &str, result: Params<'_>) -> String {
    let frame = Frame::Response(id);
    match method {
        methods::HELLO => encode_as::<Welcome>(frame, method, result),
        methods::TREE => encode_as::<TreeResult>(frame, method, result),
        methods::NODE => encode_as::<NodeResult>(frame, method, result),
        methods::FIND => encode_as::<FindResult>(frame, method, result),
        methods::PICK => encode_as::<PickResult>(frame, method, result),
        methods::SCREENSHOT => encode_as::<Image>(frame, method, result),
        methods::LOGS => encode_as::<LogsResult>(frame, method, result),
        methods::EXTENSION_LIST => encode_as::<ExtensionList>(frame, method, result),
        methods::EXTENSION_CALL => encode_as::<ExtensionResult>(frame, method, result),
        methods::TREE_WATCH
        | methods::ACT
        | methods::INPUT
        | methods::WAIT
        | methods::LAYER_PICK_MODE
        | methods::LAYER_SELECT => encode_as::<Done>(frame, method, result),
        other => panic!("no result type for `{other}`"),
    }
}

#[test]
fn every_golden_message_round_trips_byte_stably() {
    let lines: Vec<&str> = GOLDEN_MESSAGES.lines().collect();
    let mut pending: HashMap<Id, String> = HashMap::new();
    let mut seen = BTreeSet::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let again = match route(line) {
            Route::Request { id, method, params } => {
                seen.insert(method.to_owned());
                pending.insert(id.clone(), method.to_owned());
                params_again(Frame::Request(&id), method, params)
            }
            Route::Notification { method, params } => {
                seen.insert(method.to_owned());
                params_again(Frame::Notification, method, params)
            }
            Route::Response { id, outcome } => {
                let method = pending
                    .remove(&id)
                    .unwrap_or_else(|| panic!("line {}: a response to no request", index + 1));
                match outcome {
                    Ok(result) => result_again(&id, &method, result),
                    Err(error) => encode_response::<()>(Some(&id), Err(&error)),
                }
            }
            Route::Reply(reply) => {
                // An unknown method: the next line is the reply the receiver sends.
                index += 1;
                assert_eq!(
                    lines.get(index).copied(),
                    Some(reply.as_str()),
                    "line {index}"
                );
                line.to_owned()
            }
            Route::Ignore => panic!("line {}: a golden message is dropped", index + 1),
        };
        assert_eq!(
            again,
            line,
            "line {} changes when decoded and encoded",
            index + 1
        );
        index += 1;
    }
    assert!(
        pending.is_empty(),
        "requests without a response: {pending:?}"
    );
    let all: BTreeSet<String> = methods::ALL.iter().map(|m| (*m).to_owned()).collect();
    assert_eq!(seen, all, "the golden file has one message of every method");
}

#[test]
fn the_docs_carry_the_embedded_golden_file() {
    let docs = include_str!("../../../docs/devtools/messages.jsonl");
    assert_eq!(docs, GOLDEN_MESSAGES);
}

#[test]
fn an_unknown_method_is_answered_and_an_unknown_notification_dropped() {
    let Route::Reply(reply) = route(r#"{"jsonrpc":"2.0","id":5,"method":"nope","params":{}}"#)
    else {
        panic!("an unknown request gets a reply");
    };
    assert_eq!(
        reply,
        r#"{"jsonrpc":"2.0","id":5,"error":{"code":-32601,"message":"no method `nope` here"}}"#
    );
    assert!(matches!(
        route(r#"{"jsonrpc":"2.0","method":"nope.changed","params":{}}"#),
        Route::Ignore
    ));
    let Route::Reply(reply) = route("not json") else {
        panic!("a frame that is not JSON gets a reply");
    };
    assert!(reply.starts_with(r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"#));
    // The connection reads on: the next frame routes as usual.
    assert!(matches!(
        route(r#"{"jsonrpc":"2.0","id":6,"method":"tree"}"#),
        Route::Request { method: "tree", .. }
    ));
}

#[test]
fn parameters_that_do_not_fit_are_invalid_params() {
    let Route::Request { params, .. } =
        route(r#"{"jsonrpc":"2.0","id":1,"method":"node","params":{"id":"fifteen"}}"#)
    else {
        panic!("a request");
    };
    let error = params.decode::<NodeParams>().unwrap_err();
    assert_eq!(error.code, codes::INVALID_PARAMS);
    // Absent parameters decode as `{}`.
    let Route::Request { params, .. } = route(r#"{"jsonrpc":"2.0","id":2,"method":"tree"}"#) else {
        panic!("a request");
    };
    assert_eq!(
        params.decode::<TreeParams>().unwrap(),
        TreeParams::default()
    );
}

#[test]
fn a_peer_ignores_fields_it_does_not_know() {
    let hello: Hello = serde_json::from_str(
        r#"{"protocolVersion":"1.3","appId":"a","appName":"A","platform":"linux","buildId":"b","surfaces":[],"later":{"x":1}}"#,
    )
    .unwrap();
    assert_eq!(hello.credential, "");
    assert_eq!(hello.root, None);
    // A newer minor version is accepted.
    assert!(PROTOCOL_VERSION.check(hello.protocol_version).is_ok());
}

#[test]
fn a_version_mismatch_names_both_versions() {
    let theirs = Version { major: 2, minor: 0 };
    let mismatch = PROTOCOL_VERSION.check(theirs).unwrap_err();
    let error = mismatch.to_error();
    assert_eq!(error.code, codes::VERSION);
    assert!(error.message.contains("2.0") && error.message.contains("1.0"));
    assert_eq!(error.kind(), Some("version"));
    assert_eq!(error.data.as_ref().unwrap()["theirs"], "2.0");
    assert_eq!(error.data.as_ref().unwrap()["ours"], "1.0");
    assert!("1".parse::<Version>().is_err());
}

#[test]
fn the_protocol_says_no_product_word() {
    // Neutral names: a marker is a marker, a note is a note.
    let words = [
        "annotation",
        "preview",
        "account",
        "licen",
        "entitle",
        "sketch",
    ];
    let text = GOLDEN_MESSAGES.to_lowercase();
    for word in words {
        assert!(!text.contains(word), "the golden messages say `{word}`");
    }
    for method in methods::ALL {
        for word in words {
            assert!(!method.contains(word), "method `{method}` says `{word}`");
        }
    }
}
