//! Tests for the mock SAP layer in `src/utils/sap_mock_impl.rs`.
//!
//! The mock has two jobs: provide enough state for tests to drive simple
//! flows (transactions, components, selection), and record every observable
//! interaction in a `MockEvent` log. These tests lock both behaviors in.

use sap_automation::utils::sap_interfaces::SapSession;
use sap_automation::utils::sap_mock_impl::{create_test_session, MockEvent, MockSapSession};

// ---------- transactions ----------

#[test]
fn set_transaction_is_visible_to_info() {
    let session = MockSapSession::new("s");
    session.set_transaction("VA01");
    assert_eq!(session.info().unwrap().transaction().unwrap(), "VA01");
}

#[test]
fn start_transaction_mutates_state_and_records_event() {
    let session = MockSapSession::new("s");
    session.start_transaction("VL06O".to_string()).unwrap();
    assert_eq!(session.info().unwrap().transaction().unwrap(), "VL06O");
    assert_eq!(
        session.events(),
        vec![MockEvent::StartTransaction("VL06O".to_string())]
    );
}

#[test]
fn end_transaction_resets_to_s000() {
    let session = MockSapSession::new("s");
    session.start_transaction("VT11".to_string()).unwrap();
    session.end_transaction().unwrap();
    assert_eq!(session.info().unwrap().transaction().unwrap(), "S000");
    assert_eq!(
        session.events(),
        vec![
            MockEvent::StartTransaction("VT11".to_string()),
            MockEvent::EndTransaction
        ]
    );
}

// ---------- find_by_id ----------

#[test]
fn find_by_id_unknown_returns_err() {
    let session = MockSapSession::new("s");
    let res = session.find_by_id("does/not/exist".to_string());
    assert!(res.is_err());
}

#[test]
fn find_by_id_records_event_even_for_misses() {
    let session = MockSapSession::new("s");
    let _ = session.find_by_id("missing".to_string());
    assert_eq!(
        session.events(),
        vec![MockEvent::FindById("missing".to_string())]
    );
}

// ---------- text fields ----------

#[test]
fn text_field_set_and_get_round_trips() {
    let mut session = MockSapSession::new("s");
    session.add_text_field("wnd[0]/usr/txtFoo", "");

    let comp = session.find_by_id("wnd[0]/usr/txtFoo".to_string()).unwrap();
    comp.set_text("hello".to_string()).unwrap();
    assert_eq!(comp.get_text().unwrap(), "hello");
}

#[test]
fn text_field_initial_value_is_observable() {
    let mut session = MockSapSession::new("s");
    session.add_text_field("wnd[0]/usr/txtFoo", "preset");
    let comp = session.find_by_id("wnd[0]/usr/txtFoo".to_string()).unwrap();
    assert_eq!(comp.get_text().unwrap(), "preset");
}

// ---------- checkboxes ----------

#[test]
fn checkbox_round_trip_and_records_set_selected() {
    let mut session = MockSapSession::new("s");
    session.add_checkbox("wnd[0]/usr/chkFlag", false);

    let comp = session.find_by_id("wnd[0]/usr/chkFlag".to_string()).unwrap();
    assert!(!comp.selected().unwrap());
    comp.set_selected(true).unwrap();
    assert!(comp.selected().unwrap());

    assert_eq!(
        session.events(),
        vec![
            MockEvent::FindById("wnd[0]/usr/chkFlag".to_string()),
            MockEvent::SetSelected {
                id: "wnd[0]/usr/chkFlag".to_string(),
                value: true,
            }
        ]
    );
}

// ---------- buttons ----------

#[test]
fn button_press_is_recorded() {
    let mut session = MockSapSession::new("s");
    session.add_button("wnd[0]/tbar[0]/btn[8]", "Run");

    let btn = session
        .find_by_id("wnd[0]/tbar[0]/btn[8]".to_string())
        .unwrap();
    btn.press().unwrap();
    btn.set_focus().unwrap();
    btn.select().unwrap();

    let events = session.events();
    assert!(events.contains(&MockEvent::Press("wnd[0]/tbar[0]/btn[8]".to_string())));
    assert!(events.contains(&MockEvent::SetFocus("wnd[0]/tbar[0]/btn[8]".to_string())));
    assert!(events.contains(&MockEvent::Select("wnd[0]/tbar[0]/btn[8]".to_string())));
}

// ---------- recorded ordering ----------

#[test]
fn recorded_events_preserve_call_order() {
    let mut session = MockSapSession::new("s");
    session.add_text_field("wnd[0]/usr/txtA", "");
    session.add_button("wnd[0]/tbar[0]/btn[0]", "");

    let txt = session.find_by_id("wnd[0]/usr/txtA".to_string()).unwrap();
    txt.set_text("v1".to_string()).unwrap();
    let btn = session
        .find_by_id("wnd[0]/tbar[0]/btn[0]".to_string())
        .unwrap();
    btn.press().unwrap();

    assert_eq!(
        session.events(),
        vec![
            MockEvent::FindById("wnd[0]/usr/txtA".to_string()),
            MockEvent::SetText {
                id: "wnd[0]/usr/txtA".to_string(),
                value: "v1".to_string(),
            },
            MockEvent::FindById("wnd[0]/tbar[0]/btn[0]".to_string()),
            MockEvent::Press("wnd[0]/tbar[0]/btn[0]".to_string()),
        ]
    );
}

#[test]
fn events_clear_resets_log() {
    let mut session = MockSapSession::new("s");
    session.add_button("wnd[0]/tbar[0]/btn[0]", "");
    let _ = session.find_by_id("wnd[0]/tbar[0]/btn[0]".to_string());
    assert!(!session.events().is_empty());
    session.events_clear();
    assert!(session.events().is_empty());
}

// ---------- create_test_session smoke ----------

#[test]
fn create_test_session_provides_default_components() {
    let session = create_test_session();

    // Each id should resolve.
    for id in [
        "wnd[0]/usr/txtField",
        "wnd[0]/tbar[0]/btn[0]",
        "wnd[0]/usr/chkBox",
        "wnd[0]/sbar",
        "wnd[1]",
    ] {
        assert!(
            session.find_by_id(id.to_string()).is_ok(),
            "expected default mock session to expose {id}"
        );
    }
}
