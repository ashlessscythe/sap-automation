//! In-process mock of the [`SapSession`] / [`SapComponent`] traits.
//!
//! The mock has two responsibilities:
//!
//! 1. Provide enough state for tests to drive simple flows that only depend on
//!    `SapSession::find_by_id`, `start_transaction`, `end_transaction`,
//!    component text, selection state, and the like.
//! 2. Record every interesting interaction in [`MockEvent`] order so tests can
//!    assert "the date field was set to X then the run button was pressed".
//!
//! The trait API takes `&self`, so all internal state goes through
//! `Rc<RefCell<...>>` and is shared between the session and the components
//! it hands out. Components push events through a clone of the session's
//! recorder.
//!
//! Production code never imports this module, so the lib-level build flags
//! everything as dead. The integration tests in `tests/mock_session_tests.rs`
//! exercise the surface; suppress the dead-code warnings here to keep the
//! lib build clean.

#![allow(dead_code)]

use crate::utils::sap_interfaces::{SapComponent, SapComponentFactory, SapSession, SapSessionInfo};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use windows::core::{Error, Result, HRESULT};

/// One observable interaction with the mock. The variants intentionally cover
/// only the trait surface; if a test needs something more exotic (e.g. a
/// `find_by_id` for a control that doesn't exist) it can read the events.
#[derive(Debug, Clone, PartialEq)]
pub enum MockEvent {
    FindById(String),
    SetText { id: String, value: String },
    Press(String),
    Select(String),
    SetSelected { id: String, value: bool },
    SetFocus(String),
    StartTransaction(String),
    EndTransaction,
}

/// Mock component for testing. Backed by `Rc<RefCell<MockComponent>>` so the
/// session and any components it has handed out share the same underlying
/// state.
#[derive(Debug, Clone)]
pub struct MockComponent {
    pub id: String,
    pub name: String,
    pub r_type: String,
    pub text: String,
    pub properties: HashMap<String, String>,
    pub children: Vec<Rc<RefCell<MockComponent>>>,
}

impl MockComponent {
    pub fn new(id: &str, name: &str, r_type: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            r_type: r_type.to_string(),
            text: String::new(),
            properties: HashMap::new(),
            children: Vec::new(),
        }
    }

    pub fn add_child(&mut self, child: Rc<RefCell<MockComponent>>) {
        self.children.push(child);
    }
}

/// Implementation of [`SapComponent`] for mock components. Mutating calls are
/// recorded in the shared [`MockEvent`] log so tests can assert ordering.
pub struct MockSapComponent {
    component: Rc<RefCell<MockComponent>>,
    events: Rc<RefCell<Vec<MockEvent>>>,
}

impl MockSapComponent {
    pub fn new(component: Rc<RefCell<MockComponent>>) -> Self {
        Self {
            component,
            events: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Construct a component that pushes events into the supplied recorder.
    /// Used by [`MockSapSession::find_by_id`] so component events flow into
    /// the same log as session events.
    pub fn with_events(
        component: Rc<RefCell<MockComponent>>,
        events: Rc<RefCell<Vec<MockEvent>>>,
    ) -> Self {
        Self { component, events }
    }

    fn id(&self) -> String {
        self.component.borrow().id.clone()
    }
}

impl SapComponent for MockSapComponent {
    fn r_type(&self) -> Result<String> {
        Ok(self.component.borrow().r_type.clone())
    }

    fn name(&self) -> Result<String> {
        Ok(self.component.borrow().name.clone())
    }

    fn get_text(&self) -> Result<String> {
        Ok(self.component.borrow().text.clone())
    }

    fn set_text(&self, text: String) -> Result<()> {
        self.events.borrow_mut().push(MockEvent::SetText {
            id: self.id(),
            value: text.clone(),
        });
        self.component.borrow_mut().text = text;
        Ok(())
    }

    fn set_focus(&self) -> Result<()> {
        self.events.borrow_mut().push(MockEvent::SetFocus(self.id()));
        Ok(())
    }

    fn press(&self) -> Result<()> {
        self.events.borrow_mut().push(MockEvent::Press(self.id()));
        Ok(())
    }

    fn select(&self) -> Result<()> {
        self.events.borrow_mut().push(MockEvent::Select(self.id()));
        Ok(())
    }

    fn selected(&self) -> Result<bool> {
        let selected = self
            .component
            .borrow()
            .properties
            .get("selected")
            .map(|s| s == "true")
            .unwrap_or(false);
        Ok(selected)
    }

    fn set_selected(&self, selected: bool) -> Result<()> {
        self.events.borrow_mut().push(MockEvent::SetSelected {
            id: self.id(),
            value: selected,
        });
        self.component
            .borrow_mut()
            .properties
            .insert("selected".to_string(), selected.to_string());
        Ok(())
    }

    fn maximize(&self) -> Result<()> {
        Ok(())
    }
}

/// Implementation of [`SapSessionInfo`] for mock session info.
pub struct MockSapSessionInfo {
    transaction: Rc<RefCell<String>>,
}

impl MockSapSessionInfo {
    pub fn new(transaction: Rc<RefCell<String>>) -> Self {
        Self { transaction }
    }
}

impl SapSessionInfo for MockSapSessionInfo {
    fn transaction(&self) -> Result<String> {
        Ok(self.transaction.borrow().clone())
    }
}

/// Implementation of [`SapSession`] for mock session.
///
/// `current_transaction` and `events` are kept in `Rc<RefCell<_>>` so the
/// trait's `&self` methods can still mutate observable state and the same
/// log is shared with every component returned from `find_by_id`.
pub struct MockSapSession {
    #[allow(dead_code)]
    name: String,
    components: HashMap<String, Rc<RefCell<MockComponent>>>,
    current_transaction: Rc<RefCell<String>>,
    events: Rc<RefCell<Vec<MockEvent>>>,
}

impl MockSapSession {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            components: HashMap::new(),
            current_transaction: Rc::new(RefCell::new("S000".to_string())),
            events: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn add_component(&mut self, id: &str, component: Rc<RefCell<MockComponent>>) {
        self.components.insert(id.to_string(), component);
    }

    /// Force the current transaction without going through
    /// `start_transaction` (which would generate a `MockEvent`). Useful for
    /// arranging the initial fixture state.
    pub fn set_transaction(&self, transaction: &str) {
        *self.current_transaction.borrow_mut() = transaction.to_string();
    }

    /// Snapshot the current event log.
    pub fn events(&self) -> Vec<MockEvent> {
        self.events.borrow().clone()
    }

    /// Drop every recorded event. Handy between fixture setup and the actual
    /// "act" step of a test.
    pub fn events_clear(&self) {
        self.events.borrow_mut().clear();
    }

    // ---- Ergonomic builders. Each adds a typed component under `id` and
    // returns &mut Self so tests can chain. ----

    pub fn add_text_field(&mut self, id: &str, initial: &str) -> &mut Self {
        let name = leaf_name(id);
        let mut comp = MockComponent::new(id, &name, "GuiTextField");
        comp.text = initial.to_string();
        self.add_component(id, Rc::new(RefCell::new(comp)));
        self
    }

    pub fn add_button(&mut self, id: &str, label: &str) -> &mut Self {
        let name = leaf_name(id);
        let mut comp = MockComponent::new(id, &name, "GuiButton");
        comp.text = label.to_string();
        self.add_component(id, Rc::new(RefCell::new(comp)));
        self
    }

    pub fn add_checkbox(&mut self, id: &str, selected: bool) -> &mut Self {
        let name = leaf_name(id);
        let mut comp = MockComponent::new(id, &name, "GuiCheckBox");
        comp.properties
            .insert("selected".to_string(), selected.to_string());
        self.add_component(id, Rc::new(RefCell::new(comp)));
        self
    }

    pub fn add_statusbar(&mut self, id: &str, msg: &str) -> &mut Self {
        let name = leaf_name(id);
        let mut comp = MockComponent::new(id, &name, "GuiStatusbar");
        comp.text = msg.to_string();
        self.add_component(id, Rc::new(RefCell::new(comp)));
        self
    }

    pub fn add_window(&mut self, id: &str, title: &str) -> &mut Self {
        let name = leaf_name(id);
        let mut comp = MockComponent::new(id, &name, "GuiFrameWindow");
        comp.text = title.to_string();
        self.add_component(id, Rc::new(RefCell::new(comp)));
        self
    }
}

impl SapSession for MockSapSession {
    fn find_by_id(&self, id: String) -> Result<Box<dyn SapComponent>> {
        self.events
            .borrow_mut()
            .push(MockEvent::FindById(id.clone()));
        if let Some(component) = self.components.get(&id) {
            return Ok(Box::new(MockSapComponent::with_events(
                component.clone(),
                self.events.clone(),
            )));
        }

        Err(Error::new(
            HRESULT(-2147467259),
            "Component not found".into(),
        ))
    }

    fn info(&self) -> Result<Box<dyn SapSessionInfo>> {
        Ok(Box::new(MockSapSessionInfo::new(
            self.current_transaction.clone(),
        )))
    }

    fn start_transaction(&self, transaction: String) -> Result<()> {
        self.events
            .borrow_mut()
            .push(MockEvent::StartTransaction(transaction.clone()));
        *self.current_transaction.borrow_mut() = transaction;
        Ok(())
    }

    fn end_transaction(&self) -> Result<()> {
        self.events.borrow_mut().push(MockEvent::EndTransaction);
        *self.current_transaction.borrow_mut() = "S000".to_string();
        Ok(())
    }
}

/// Factory for creating mock SAP components.
pub struct MockSapComponentFactory;

impl Default for MockSapComponentFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl MockSapComponentFactory {
    pub fn new() -> Self {
        Self
    }
}

impl SapComponentFactory for MockSapComponentFactory {
    fn create_session(&self, name: &str) -> Box<dyn SapSession> {
        Box::new(MockSapSession::new(name))
    }
}

/// Take the last `/segment` of an SAP-style id (`wnd[0]/usr/txtField` → `txtField`).
fn leaf_name(id: &str) -> String {
    id.rsplit('/').next().unwrap_or(id).to_string()
}

/// Helper function to create a mock session with some default components.
/// Implemented on top of the new builders to keep one source of truth.
pub fn create_test_session() -> Box<dyn SapSession> {
    let mut session = MockSapSession::new("Test Session");
    session
        .add_text_field("wnd[0]/usr/txtField", "Test Text")
        .add_button("wnd[0]/tbar[0]/btn[0]", "Press Me")
        .add_checkbox("wnd[0]/usr/chkBox", false)
        .add_statusbar("wnd[0]/sbar", "Status: OK")
        .add_window("wnd[1]", "Popup Window");
    Box::new(session)
}
