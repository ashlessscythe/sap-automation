use crate::utils::sap_interfaces::{SapComponent, SapComponentFactory, SapSession, SapSessionInfo};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use windows::core::{Error, Result, HRESULT};

/// Mock component for testing
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

/// Implementation of SapComponent for mock components
pub struct MockSapComponent {
    component: Rc<RefCell<MockComponent>>,
}

impl MockSapComponent {
    pub fn new(component: Rc<RefCell<MockComponent>>) -> Self {
        Self { component }
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
        self.component.borrow_mut().text = text;
        Ok(())
    }

    fn set_focus(&self) -> Result<()> {
        // In a mock, we don't need to do anything for set_focus
        Ok(())
    }

    fn press(&self) -> Result<()> {
        // In a mock, we don't need to do anything for press
        Ok(())
    }

    fn select(&self) -> Result<()> {
        // In a mock, we don't need to do anything for select
        Ok(())
    }

    fn selected(&self) -> Result<bool> {
        // Get the selected state from properties
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
        // Set the selected state in properties
        self.component
            .borrow_mut()
            .properties
            .insert("selected".to_string(), selected.to_string());
        Ok(())
    }

    fn maximize(&self) -> Result<()> {
        // In a mock, we don't need to do anything for maximize
        Ok(())
    }
}

/// Implementation of SapSessionInfo for mock session info
pub struct MockSapSessionInfo {
    transaction: String,
}

impl MockSapSessionInfo {
    pub fn new(transaction: &str) -> Self {
        Self {
            transaction: transaction.to_string(),
        }
    }
}

impl SapSessionInfo for MockSapSessionInfo {
    fn transaction(&self) -> Result<String> {
        Ok(self.transaction.clone())
    }
}

/// Implementation of SapSession for mock session
pub struct MockSapSession {
    name: String,
    components: HashMap<String, Rc<RefCell<MockComponent>>>,
    current_transaction: String,
}

impl MockSapSession {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            components: HashMap::new(),
            current_transaction: "S000".to_string(), // Default to login screen
        }
    }

    pub fn add_component(&mut self, id: &str, component: Rc<RefCell<MockComponent>>) {
        self.components.insert(id.to_string(), component);
    }

    pub fn set_transaction(&mut self, transaction: &str) {
        self.current_transaction = transaction.to_string();
    }
}

impl SapSession for MockSapSession {
    fn find_by_id(&self, id: String) -> Result<Box<dyn SapComponent>> {
        if let Some(component) = self.components.get(&id) {
            return Ok(Box::new(MockSapComponent::new(component.clone())));
        }

        // If not found, return an error
        Err(Error::new(
            HRESULT(-2147467259),
            "Component not found".into(),
        ))
    }

    fn info(&self) -> Result<Box<dyn SapSessionInfo>> {
        Ok(Box::new(MockSapSessionInfo::new(&self.current_transaction)))
    }

    fn start_transaction(&self, _transaction: String) -> Result<()> {
        // In a real implementation, this would update self.current_transaction
        // But since self is not mutable here, we'll just return Ok
        Ok(())
    }

    fn end_transaction(&self) -> Result<()> {
        // In a real implementation, this would reset self.current_transaction to "S000"
        // But since self is not mutable here, we'll just return Ok
        Ok(())
    }
}

/// Factory for creating mock SAP components
pub struct MockSapComponentFactory {
    // This would typically hold configuration for creating mock components
}

impl Default for MockSapComponentFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl MockSapComponentFactory {
    pub fn new() -> Self {
        Self {}
    }
}

impl SapComponentFactory for MockSapComponentFactory {
    fn create_session(&self, name: &str) -> Box<dyn SapSession> {
        Box::new(MockSapSession::new(name))
    }
}

/// Helper function to create a mock session with some default components
pub fn create_test_session() -> Box<dyn SapSession> {
    let mut session = MockSapSession::new("Test Session");

    // Add a text field
    let text_field = Rc::new(RefCell::new(MockComponent {
        id: "wnd[0]/usr/txtField".to_string(),
        name: "txtField".to_string(),
        r_type: "GuiTextField".to_string(),
        text: "Test Text".to_string(),
        children: Vec::new(),
        properties: Default::default(),
    }));
    session.add_component("wnd[0]/usr/txtField", text_field);

    // Add a button
    let button = Rc::new(RefCell::new(MockComponent {
        id: "wnd[0]/tbar[0]/btn[0]".to_string(),
        name: "btn[0]".to_string(),
        r_type: "GuiButton".to_string(),
        text: "Press Me".to_string(),
        children: Vec::new(),
        properties: Default::default(),
    }));
    session.add_component("wnd[0]/tbar[0]/btn[0]", button);

    // Add a checkbox
    let mut properties = std::collections::HashMap::new();
    properties.insert("selected".to_string(), "false".to_string());
    let checkbox = Rc::new(RefCell::new(MockComponent {
        id: "wnd[0]/usr/chkBox".to_string(),
        name: "chkBox".to_string(),
        r_type: "GuiCheckBox".to_string(),
        text: "Check Me".to_string(),
        children: Vec::new(),
        properties,
    }));
    session.add_component("wnd[0]/usr/chkBox", checkbox);

    // Add a statusbar
    let statusbar = Rc::new(RefCell::new(MockComponent {
        id: "wnd[0]/sbar".to_string(),
        name: "sbar".to_string(),
        r_type: "GuiStatusbar".to_string(),
        text: "Status: OK".to_string(),
        children: Vec::new(),
        properties: Default::default(),
    }));
    session.add_component("wnd[0]/sbar", statusbar);

    // Add a window
    let window = Rc::new(RefCell::new(MockComponent {
        id: "wnd[1]".to_string(),
        name: "wnd[1]".to_string(),
        r_type: "GuiFrameWindow".to_string(),
        text: "Popup Window".to_string(),
        children: Vec::new(),
        properties: Default::default(),
    }));
    session.add_component("wnd[1]", window);

    Box::new(session)
}
