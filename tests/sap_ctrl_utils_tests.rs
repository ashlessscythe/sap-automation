use std::cell::RefCell;
use std::rc::Rc;
use windows::core::Result;

// Import our mock implementations
mod mocks;
use mocks::*;
use mocks::mock_utils::{mock_exist_ctrl, mock_hit_ctrl};

// We'll use our mock versions of the utility functions
// Instead of importing from sap_automation::utils::sap_ctrl_utils

// Helper function to create a mock session with components
fn create_mock_session() -> MockGuiSession {
    let mut session = MockGuiSession::new("Test Session");
    
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
    
    session
}

#[test]
fn test_exist_ctrl_found() -> Result<()> {
    let session = create_mock_session();
    
    // Test finding a component that exists
    let result = mock_exist_ctrl(&session, 0, "/usr/txtField", true)?;
    
    assert!(result.cband, "Component should be found");
    assert_eq!(result.ctext, "Test Text", "Text should match");
    assert_eq!(result.ctype, "GuiTextField", "Type should match");
    
    Ok(())
}

#[test]
fn test_exist_ctrl_not_found() -> Result<()> {
    let session = create_mock_session();
    
    // Test finding a component that doesn't exist
    let result = mock_exist_ctrl(&session, 0, "/usr/nonexistentField", true)?;
    
    assert!(!result.cband, "Component should not be found");
    assert_eq!(result.ctext, "", "Text should be empty");
    assert_eq!(result.ctype, "", "Type should be empty");
    
    Ok(())
}

#[test]
fn test_exist_ctrl_different_window() -> Result<()> {
    let session = create_mock_session();
    
    // Test finding a component in a different window
    let result = mock_exist_ctrl(&session, 1, "", true)?;
    
    assert!(result.cband, "Component should be found");
    assert_eq!(result.ctext, "Popup Window", "Text should match");
    assert_eq!(result.ctype, "GuiFrameWindow", "Type should match");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_get_text() -> Result<()> {
    let session = create_mock_session();
    
    // Test getting text from a text field
    let result = mock_hit_ctrl(&session, 0, "/usr/txtField", "Text", "Get", "")?;
    
    assert_eq!(result, "Test Text", "Text should match");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_set_text() -> Result<()> {
    let session = create_mock_session();
    
    // Test setting text on a text field
    let _ = mock_hit_ctrl(&session, 0, "/usr/txtField", "Text", "Set", "New Text")?;
    
    // Verify the text was set
    let component = session.find_by_id("wnd[0]/usr/txtField".to_string())?;
    let text = component.borrow().text.clone();
    assert_eq!(text, "New Text", "Text should be updated");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_press_button() -> Result<()> {
    let session = create_mock_session();
    
    // Test pressing a button
    // Note: We can't easily verify the button was pressed in our mock implementation,
    // but we can at least verify the function doesn't error
    let result = mock_hit_ctrl(&session, 0, "/tbar[0]/btn[0]", "Press", "", "")?;
    
    assert_eq!(result, "", "Result should be empty string");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_checkbox() -> Result<()> {
    let session = create_mock_session();
    
    // Test getting checkbox state
    let result = mock_hit_ctrl(&session, 0, "/usr/chkBox", "Selected", "Get", "")?;
    assert_eq!(result, "false", "Checkbox should be unchecked");
    
    // Test setting checkbox state
    let _ = mock_hit_ctrl(&session, 0, "/usr/chkBox", "Selected", "Set", "True")?;
    
    // Verify the checkbox state was set
    let component = session.find_by_id("wnd[0]/usr/chkBox".to_string())?;
    let selected = component.borrow().properties.get("selected").unwrap().clone();
    assert_eq!(selected, "true", "Checkbox should be checked");
    
    Ok(())
}

#[test]
fn test_hit_ctrl_statusbar() -> Result<()> {
    let session = create_mock_session();
    
    // Test getting text from statusbar
    let result = mock_hit_ctrl(&session, 0, "/sbar", "Text", "Get", "")?;
    
    assert_eq!(result, "Status: OK", "Statusbar text should match");
    
    Ok(())
}
